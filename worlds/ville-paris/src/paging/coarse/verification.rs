// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::unpack;
use flate2::{Compression, write::ZlibEncoder};
use std::io::Write;

fn field(value: &[u8]) -> Vec<u8> {
    [(value.len() as u32).to_le_bytes().as_slice(), value].concat()
}

fn envelope(meta: &[u8], geo: &[u8], json: usize, bin: usize) -> Vec<u8> {
    let body = [field(meta), field(geo)].concat();
    let mut stream = ZlibEncoder::new(Vec::new(), Compression::default());
    stream.write_all(&body).unwrap();
    [
        b"OVC2".as_slice(),
        &(body.len() as u32).to_le_bytes(),
        &(json as u32).to_le_bytes(),
        &(bin as u32).to_le_bytes(),
        &stream.finish().unwrap(),
    ]
    .concat()
}

#[test]
fn reconstructs_json_lexemes_and_opaque_binary_without_rounding() {
    let text = br#"{"x":-0.00e0}"#;
    let bin = [0x01, 0x02, 0x03, 0xff, 0xf3, 0x40, 0x00, 0x00];
    let input = envelope(&[b"\0", text.as_slice()].concat(), &[b"\0", bin.as_slice()].concat(), text.len(), bin.len());
    let raw = unpack(&input).unwrap();
    let padded = text.len().next_multiple_of(4);
    assert_eq!(&raw[20..20 + text.len()], text);
    assert!(raw[20 + text.len()..20 + padded].iter().all(|&b| b == b' '));
    assert_eq!(&raw[28 + padded..], &bin);
}

#[test]
fn inline_columns_reconstruct_the_original_and_require_exact_length() {
    let baseline = envelope(b"\0{}", b"\0abcd", 2, 4);
    let mut inline = baseline[..16].to_vec();
    inline[..4].copy_from_slice(b"OVCU");
    inline.extend(field(b"\0{}"));
    inline.extend(field(b"\0abcd"));
    assert_eq!(unpack(&inline).unwrap(), unpack(&baseline).unwrap());
    assert!(unpack(&inline[..inline.len() - 1]).is_err());
    inline.push(0);
    assert!(unpack(&inline).is_err());
}

#[test]
fn rejects_truncation_excess_data_wrong_tags_and_declared_expansion() {
    let base = envelope(b"\0{}", b"\0abcd", 2, 4);
    let mut huge = base.clone();
    huge[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    for invalid in [
        base[..base.len() - 1].to_vec(),
        [base.as_slice(), b"x"].concat(),
        huge,
        envelope(b"\0{}", b"\xf0abcd", 2, 4),
        envelope(b"\0{}", b"\0abcd", 1, 4),
        envelope(b"\x01\x00\x00\x00\xff", b"\0", 2, 0),
    ] {
        assert!(unpack(&invalid).is_err());
    }
}

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
fn cpu_ns() -> u64 {
    use std::ffi::{c_int, c_long};
    #[repr(C)]
    struct Time {
        sec: c_long,
        ns: c_long,
    }
    unsafe extern "C" {
        fn clock_gettime(clk: c_int, target: *mut Time) -> c_int;
    }
    let mut t = Time { sec: 0, ns: 0 };
    // SAFETY: Linux LP64 clock 3 is a thread CPU clock and timespec contains two longs.
    assert_eq!(unsafe { clock_gettime(3, &mut t) }, 0);
    t.sec as u64 * 1_000_000_000 + t.ns as u64
}

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
#[test]
#[ignore = "measured in a pinned process by check-coarse-storage.py"]
fn measure_native_decoders() {
    use crate::{bench::sha, paging::models};
    use std::{env, fs, hint::black_box, time::Instant};
    let old = fs::read(env::var_os("OSAGE_COARSE_OLD").unwrap()).unwrap();
    let new = fs::read(env::var_os("OSAGE_COARSE_NEW").unwrap()).unwrap();
    let mode = env::var("OSAGE_COARSE_MODE").unwrap();
    let iterations: usize = env::var("OSAGE_COARSE_ROUNDS").unwrap().parse().unwrap();
    let peak = || {
        fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .find_map(|s| s.strip_prefix("VmHWM:")?.split_whitespace().next()?.parse::<u64>().ok())
            .unwrap()
    };
    let baseline = peak();
    let mut samples = Vec::new();
    let mut digest = None;
    for i in 0..iterations {
        let ordered = if i % 2 == 0 { ["ovc1", "ovc2"] } else { ["ovc2", "ovc1"] };
        for method in ordered {
            if mode != "paired" && mode != method {
                continue;
            }
            let wall = Instant::now();
            let clock = cpu_ns();
            let result =
                if method == "ovc1" { models::unpack(black_box(&old), b"OVC1") } else { unpack(black_box(&new)) }
                    .unwrap();
            let elapsed = cpu_ns() - clock;
            let wall_ns = wall.elapsed().as_nanos();
            let fingerprint = sha(black_box(&result));
            assert_eq!(digest.get_or_insert_with(|| fingerprint.clone()), &fingerprint);
            samples.push(serde_json::json!({"method":method,"round":i,"cpu_ns":elapsed,"wall_ns":wall_ns}));
        }
    }
    println!(
        "OSAGE_COARSE_MEASUREMENT {}",
        serde_json::json!({"decoded_sha256":digest,"baseline_peak_rss_kib":baseline,"peak_rss_kib":peak(),"samples":samples})
    );
}
