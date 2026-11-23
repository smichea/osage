// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Everything this crate measures of a stereo file, gathered: a table a
//! person reads in a terminal, and the same figures as JSON for a receipt.

use crate::{
    bands::{Band, OCTAVE_NAMES, band_levels, octaves},
    cues::{self, FINE_STRUCTURE},
    level::{self, Clipping, amplitude_db, db},
    loudness::loudness,
    wav::Audio,
};
use std::fmt::{self, Write as _};

/// The band of the level difference called low, Hz: under 500 Hz a head
/// hardly shadows, and a difference there is a matter of distance. From
/// 50 Hz only: under it, a frame of 50 ms cannot tell a sound from an
/// offset, and an offset in one channel would read as a difference.
pub const LOW: Band = Band::new(50.0, 500.0);
/// The band of the level difference called mid, Hz.
pub const MID: Band = Band::new(500.0, 2_000.0);
/// The band of the level difference called high, Hz: where a head shadows
/// most.
pub const HIGH: Band = Band::new(2_000.0, 20_000.0);

/// What is measured of one channel.
#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    /// The largest sample, dB of full scale.
    pub sample_peak_db: f64,
    /// The largest value between samples, dBTP ([`level::true_peak`]).
    pub true_peak_db: f64,
    /// The level of the whole channel, dB ([`level::rms`]).
    pub rms_db: f64,
    /// The mean of its samples, of full scale ([`level::dc`]).
    pub dc: f64,
    /// Its samples at the limits of a 16-bit file.
    pub clipping: Clipping,
    /// Its level in each octave band from 31.5 Hz to 16 kHz, dB.
    pub octaves_db: Vec<f64>,
}

/// What is measured of one second of the file.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Second {
    /// When the second starts, seconds.
    pub time: f64,
    /// The level of each channel over the second, dB.
    pub left_db: f64,
    pub right_db: f64,
    /// The level difference in [`LOW`], [`MID`] and [`HIGH`], dB: positive
    /// when the right channel is louder.
    pub ild_low_db: f64,
    pub ild_mid_db: f64,
    pub ild_high_db: f64,
    /// The time difference in [`FINE_STRUCTURE`], microseconds: positive
    /// when the right channel leads.
    pub itd_us: f64,
    /// The coherence there, 0…1.
    pub coherence: f64,
    /// The broadband time difference by [`cues::gcc_phat`], microseconds,
    /// same sign: a second opinion on [`Second::itd_us`].
    pub phat_itd_us: f64,
}

/// What is measured of a stereo file ([`measure`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    /// Samples a second.
    pub rate: u32,
    /// The channels the file has. A file of one channel is measured as the
    /// same sound in both ears; of a file of more, the first two are.
    pub channels: usize,
    /// Samples in each channel.
    pub frames: usize,
    /// Its length, seconds.
    pub seconds: f64,
    pub left: Channel,
    pub right: Channel,
    /// The higher of the two channels' true peaks, dBTP: what must stay
    /// under −1 for the file to be safe to play and to encode.
    pub true_peak_db: f64,
    /// Integrated loudness, LKFS ([`loudness`]); `None` for silence or for
    /// less than 400 ms.
    pub loudness_lkfs: Option<f64>,
    /// How alike the two channels are, −1…1 ([`level::correlation`]).
    pub correlation: f64,
    /// Second by second.
    pub per_second: Vec<Second>,
}

fn channel(signal: &[f32], rate: u32) -> Channel {
    Channel {
        sample_peak_db: amplitude_db(level::sample_peak(signal)),
        true_peak_db: amplitude_db(level::true_peak(signal)),
        rms_db: db(level::mean_square(signal)),
        dc: level::dc(signal),
        clipping: level::clipping(signal),
        octaves_db: band_levels(signal, rate, &octaves()),
    }
}

/// Everything this crate measures of a stereo file: its length, its peaks
/// and whether it clips, its loudness, its spectrum by octaves, and second
/// by second where its sound is.
pub fn measure(audio: &Audio) -> Report {
    let nothing: &[f32] = &[];
    let left = audio.channels.first().map_or(nothing, Vec::as_slice);
    let right = audio.channels.get(1).map_or(left, Vec::as_slice);
    let frames = left.len().min(right.len());
    let (left, right) = (&left[..frames], &right[..frames]);
    let (of_left, of_right) = (channel(left, audio.rate), channel(right, audio.rate));
    let rate = audio.rate.max(1);

    let (bands, broad) = cues::analyse(left, right, rate, &[LOW, MID, HIGH, FINE_STRUCTURE], 1.0, true);
    let per_second = (0..broad.len())
        .map(|second| {
            let from = (second * rate as usize).min(frames);
            let to = ((second + 1) * rate as usize).min(frames);
            Second {
                time: second as f64,
                left_db: db(level::mean_square(&left[from..to])),
                right_db: db(level::mean_square(&right[from..to])),
                ild_low_db: bands[0][second].ild_db,
                ild_mid_db: bands[1][second].ild_db,
                ild_high_db: bands[2][second].ild_db,
                itd_us: bands[3][second].itd_us,
                coherence: bands[3][second].coherence,
                phat_itd_us: broad[second].itd_us,
            }
        })
        .collect();

    Report {
        rate: audio.rate,
        channels: audio.channels.len(),
        frames,
        seconds: frames as f64 / f64::from(rate),
        true_peak_db: of_left.true_peak_db.max(of_right.true_peak_db),
        loudness_lkfs: loudness(&[left, right], rate),
        correlation: level::correlation(left, right),
        left: of_left,
        right: of_right,
        per_second,
    }
}

/// A number as JSON has it: `null` for what is not one.
fn number(value: f64, decimals: usize) -> String {
    if value.is_finite() { format!("{value:.decimals$}") } else { "null".to_owned() }
}

fn list(values: impl IntoIterator<Item = String>) -> String {
    format!("[{}]", values.into_iter().collect::<Vec<_>>().join(", "))
}

impl Report {
    /// The report as JSON, for a receipt: the same figures as the table,
    /// with more decimals. Levels in dB, times in seconds, time differences
    /// in microseconds; pairs are `[left, right]`.
    pub fn to_json(&self) -> String {
        let pair = |of: &dyn Fn(&Channel) -> String| list([of(&self.left), of(&self.right)]);
        let mut out = String::from("{\n");
        let mut field = |name: &str, value: String| {
            let _ = writeln!(out, "  \"{name}\": {value},");
        };
        field("seconds", number(self.seconds, 6));
        field("rate", self.rate.to_string());
        field("channels", self.channels.to_string());
        field("frames", self.frames.to_string());
        field("sample_peak_db", pair(&|channel| number(channel.sample_peak_db, 3)));
        field("true_peak_db", pair(&|channel| number(channel.true_peak_db, 3)));
        field("true_peak_dbtp", number(self.true_peak_db, 3));
        field("loudness_lkfs", self.loudness_lkfs.map_or("null".to_owned(), |loudness| number(loudness, 3)));
        field("rms_db", pair(&|channel| number(channel.rms_db, 3)));
        field("dc", pair(&|channel| number(channel.dc, 7)));
        field("clipped_samples", pair(&|channel| channel.clipping.samples.to_string()));
        field("longest_clipped_run", pair(&|channel| channel.clipping.longest_run.to_string()));
        field("correlation", number(self.correlation, 4));
        field("octave_hz", list(OCTAVE_NAMES.iter().map(|name| number(*name, 1))));
        field("octaves_db", pair(&|channel| list(channel.octaves_db.iter().map(|level| number(*level, 2)))));
        out.push_str("  \"per_second\": [");
        for (n, second) in self.per_second.iter().enumerate() {
            let _ = write!(
                out,
                "{}\n    {{\"time\": {}, \"left_db\": {}, \"right_db\": {}, \"ild_low_db\": {}, \"ild_mid_db\": {}, \
                 \"ild_high_db\": {}, \"itd_us\": {}, \"coherence\": {}, \"phat_itd_us\": {}}}",
                if n == 0 { "" } else { "," },
                number(second.time, 0),
                number(second.left_db, 2),
                number(second.right_db, 2),
                number(second.ild_low_db, 2),
                number(second.ild_mid_db, 2),
                number(second.ild_high_db, 2),
                number(second.itd_us, 1),
                number(second.coherence, 3),
                number(second.phat_itd_us, 1),
            );
        }
        out.push_str(if self.per_second.is_empty() { "]\n}\n" } else { "\n  ]\n}\n" });
        out
    }
}

/// The name of an octave band as a column head: 31.5, 63… 1k… 16k.
fn octave_name(hz: f64) -> String {
    if hz >= 1_000.0 { format!("{}k", hz / 1_000.0) } else { format!("{hz}") }
}

impl fmt::Display for Report {
    /// A table for a terminal, 80 columns wide.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (l, r) = (&self.left, &self.right);
        writeln!(
            f,
            "{:.3} s at {} Hz, {} channel{}, {} samples a channel",
            self.seconds,
            self.rate,
            self.channels,
            if self.channels == 1 { "" } else { "s" },
            self.frames
        )?;
        writeln!(f)?;
        writeln!(f, "{:<26}{:>12}{:>12}", "", "left", "right")?;
        writeln!(f, "{:<26}{:>12.2}{:>12.2}", "sample peak, dBFS", l.sample_peak_db, r.sample_peak_db)?;
        writeln!(f, "{:<26}{:>12.2}{:>12.2}", "true peak, dBTP", l.true_peak_db, r.true_peak_db)?;
        writeln!(f, "{:<26}{:>12.2}{:>12.2}", "level (RMS), dB", l.rms_db, r.rms_db)?;
        writeln!(f, "{:<26}{:>+12.6}{:>+12.6}", "offset (DC)", l.dc, r.dc)?;
        writeln!(f, "{:<26}{:>12}{:>12}", "samples at the limits", l.clipping.samples, r.clipping.samples)?;
        writeln!(f, "{:<26}{:>12}{:>12}", "longest run of them", l.clipping.longest_run, r.clipping.longest_run)?;
        writeln!(f)?;
        match self.loudness_lkfs {
            Some(loudness) => writeln!(f, "{:<26}{:>12.2}", "loudness, LKFS", loudness)?,
            None => writeln!(f, "{:<26}{:>12}", "loudness, LKFS", "none")?,
        }
        writeln!(f, "{:<26}{:>12.2}", "true peak, dBTP", self.true_peak_db)?;
        writeln!(f, "{:<26}{:>+12.3}", "correlation", self.correlation)?;
        writeln!(f)?;
        write!(f, "{:<10}", "octave")?;
        for name in OCTAVE_NAMES {
            write!(f, "{:>7}", octave_name(name))?;
        }
        writeln!(f)?;
        for (name, channel) in [("left, dB", l), ("right, dB", r)] {
            write!(f, "{name:<10}")?;
            for level in &channel.octaves_db {
                write!(f, "{level:>7.1}")?;
            }
            writeln!(f)?;
        }
        writeln!(f)?;
        writeln!(f, "Second by second. Differences are right minus left: positive when the right")?;
        writeln!(f, "channel is louder (dB), or leads (µs). Low is 50 to 500 Hz, mid 500 Hz to")?;
        writeln!(f, "2 kHz, high above; times and coherence are read from 100 Hz to 1.5 kHz.")?;
        writeln!(
            f,
            "{:>5}{:>8}{:>8}{:>8}{:>8}{:>8}{:>9}{:>7}{:>9}",
            "s", "left", "right", "low", "mid", "high", "time", "coh.", "PHAT"
        )?;
        writeln!(f, "{:>5}{:>8}{:>8}{:>8}{:>8}{:>8}{:>9}{:>7}{:>9}", "", "dB", "dB", "dB", "dB", "dB", "µs", "", "µs")?;
        for second in &self.per_second {
            writeln!(
                f,
                "{:>5.0}{:>8.1}{:>8.1}{:>+8.1}{:>+8.1}{:>+8.1}{:>+9.0}{:>7.2}{:>+9.0}",
                second.time,
                second.left_db,
                second.right_db,
                second.ild_low_db,
                second.ild_mid_db,
                second.ild_high_db,
                second.itd_us,
                second.coherence,
                second.phat_itd_us
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{late, sine, white};

    /// Three seconds of a source on the left: the right channel is a copy
    /// 0.5 ms late and 6 dB lower.
    fn on_the_left() -> Audio {
        let left = white(70, 48_000, 3.0, 0.5);
        let right = late(&left, 24, 0.5);
        Audio { rate: 48_000, channels: vec![left, right] }
    }

    /// Whether `text` is well-formed JSON: a small reader that knows the
    /// whole grammar but keeps nothing.
    fn is_json(text: &str) -> bool {
        fn blank(s: &[u8], mut at: usize) -> usize {
            while at < s.len() && s[at].is_ascii_whitespace() {
                at += 1;
            }
            at
        }
        fn string(s: &[u8], at: usize) -> Option<usize> {
            let mut at = (s.get(at) == Some(&b'"')).then_some(at + 1)?;
            while *s.get(at)? != b'"' {
                at += if s[at] == b'\\' { 2 } else { 1 };
            }
            Some(at + 1)
        }
        fn value(s: &[u8], at: usize) -> Option<usize> {
            let at = blank(s, at);
            match *s.get(at)? {
                b'{' | b'[' => {
                    let (object, close) = (s[at] == b'{', if s[at] == b'{' { b'}' } else { b']' });
                    let mut at = blank(s, at + 1);
                    if s.get(at) == Some(&close) {
                        return Some(at + 1);
                    }
                    loop {
                        if object {
                            at = blank(s, string(s, blank(s, at))?);
                            at = (s.get(at) == Some(&b':')).then_some(at + 1)?;
                        }
                        at = blank(s, value(s, at)?);
                        match *s.get(at)? {
                            b',' => at += 1,
                            found if found == close => return Some(at + 1),
                            _ => return None,
                        }
                    }
                }
                b'"' => string(s, at),
                _ => {
                    let end = (at..s.len())
                        .find(|&n| !matches!(s[n], b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'a'..=b'z'))
                        .unwrap_or(s.len());
                    let word = std::str::from_utf8(&s[at..end]).ok()?;
                    (matches!(word, "null" | "true" | "false") || word.parse::<f64>().is_ok_and(f64::is_finite))
                        .then_some(end)
                }
            }
        }
        value(text.as_bytes(), 0).is_some_and(|end| blank(text.as_bytes(), end) == text.len())
    }

    #[test]
    fn a_report_tells_a_source_on_the_left_by_negative_differences_every_second() {
        let report = measure(&on_the_left());
        assert_eq!((report.rate, report.channels, report.frames), (48_000, 2, 144_000));
        assert!((report.seconds - 3.0).abs() < 1e-12);
        assert_eq!(report.per_second.len(), 3);
        for (n, second) in report.per_second.iter().enumerate() {
            assert_eq!(second.time, n as f64);
            for difference in [second.ild_low_db, second.ild_mid_db, second.ild_high_db] {
                assert!((difference + 6.02).abs() < 0.1, "{second:?}");
            }
            assert!((second.itd_us + 500.0).abs() < 1.0 && (second.phat_itd_us + 500.0).abs() < 1.0, "{second:?}");
            assert!(second.coherence > 0.99, "{second:?}");
            // Uniform noise of ±0.5: a mean square of 1/12, a quarter of it on the right.
            assert!(
                (second.left_db - db(1.0 / 12.0)).abs() < 0.2 && (second.left_db - second.right_db - 6.02).abs() < 0.1
            );
        }
        assert!((report.left.rms_db - report.right.rms_db - 6.02).abs() < 0.01);
        assert!((report.left.sample_peak_db - amplitude_db(0.5)).abs() < 0.01);
        assert_eq!(report.true_peak_db, report.left.true_peak_db);
        assert!(report.true_peak_db >= report.left.sample_peak_db);
        assert!(report.left.dc.abs() < 0.01);
        assert_eq!(report.left.clipping, Clipping::default());
        assert_eq!(report.left.octaves_db.len(), 10);
        // At 24 samples late a white noise is no longer like itself.
        assert!(report.correlation.abs() < 0.02);
        // Loudness: the sum of both channels' weighed mean squares.
        let loudness = report.loudness_lkfs.unwrap();
        assert!(loudness > -12.0 && loudness < -6.0, "{loudness} LKFS");
    }

    #[test]
    fn a_report_counts_the_samples_a_16_bit_file_cut() {
        let cut: Vec<f32> = sine(48_000, 480.0, 2.0, 1.0).iter().map(|s| s.clamp(-1.0, 1.0)).collect();
        let clean = sine(48_000, 480.0, 0.5, 1.0);
        let report = measure(&Audio { rate: 48_000, channels: vec![clean, cut] });
        assert_eq!(report.left.clipping.samples, 0);
        assert!(report.right.clipping.samples > 30_000 && report.right.clipping.longest_run >= 32);
        assert!(report.right.sample_peak_db.abs() < 1e-9 && report.true_peak_db > 0.0);
        assert!((report.correlation - 0.974).abs() < 0.01);
        // Cut at half its height, a sine keeps a fundamental of 1.218: 7.73 dB
        // above the clean one. What the cut adds is higher, where the clean
        // sine has nothing but the click of its start and of its end.
        let second = report.per_second[0];
        assert!((second.ild_low_db - 7.73).abs() < 0.05, "{second:?}");
        assert!(second.ild_high_db > 40.0, "{second:?}");
    }

    #[test]
    fn the_json_is_well_formed_and_holds_the_figures_of_the_table() {
        let report = measure(&on_the_left());
        let json = report.to_json();
        assert!(is_json(&json), "{json}");
        for field in [
            "\"seconds\": 3.000000",
            "\"rate\": 48000",
            "\"channels\": 2",
            "\"frames\": 144000",
            "\"clipped_samples\": [0, 0]",
            "\"octave_hz\": [31.5, 63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0]",
            "\"time\": 2, \"left_db\": ",
        ] {
            assert!(json.contains(field), "no {field} in {json}");
        }
        assert!(json.contains(&format!("\"loudness_lkfs\": {:.3}", report.loudness_lkfs.unwrap())));
        assert!(json.contains(&format!("\"true_peak_dbtp\": {:.3}", report.true_peak_db)));
        assert!(json.contains(&format!("\"itd_us\": {:.1}", report.per_second[1].itd_us)));
        // The reader used here does tell what is not JSON.
        for broken in ["{\"a\": 1,}", "{\"a\": NaN}", "[1 2]", "{\"a\" 1}", "{\"a\": 1} x", "{\"a\": [1, 2}"] {
            assert!(!is_json(broken), "{broken}");
        }
        assert!(is_json("{\"a\": [], \"b\": {}, \"c\": [null, -1.5e3, \"x\\\"y\"]}"));
    }

    #[test]
    fn the_table_fits_a_terminal_of_80_columns_and_has_a_line_a_second() {
        let table = measure(&on_the_left()).to_string();
        for line in table.lines() {
            assert!(line.chars().count() <= 80, "{} columns: {line}", line.chars().count());
        }
        assert!(table.starts_with("3.000 s at 48000 Hz, 2 channels, 144000 samples a channel\n"), "{table}");
        assert!(table.contains("loudness, LKFS") && table.contains("   31.5     63    125"), "{table}");
        let seconds = table.lines().filter(|line| line.contains("-500")).count();
        assert_eq!(seconds, 3, "{table}");
    }

    #[test]
    fn a_file_of_one_channel_is_measured_as_the_same_sound_in_both_ears() {
        let report = measure(&Audio { rate: 44_100, channels: vec![white(71, 44_100, 2.0, 0.3)] });
        assert_eq!((report.channels, report.frames, report.per_second.len()), (1, 88_200, 2));
        assert_eq!(report.left, report.right);
        assert!((report.correlation - 1.0).abs() < 1e-12);
        for second in &report.per_second {
            assert_eq!((second.ild_low_db, second.ild_mid_db, second.ild_high_db, second.itd_us), (0.0, 0.0, 0.0, 0.0));
            assert!((second.coherence - 1.0).abs() < 1e-9);
        }
        assert!(report.loudness_lkfs.is_some());
    }

    #[test]
    fn silence_and_a_file_of_nothing_are_measured_without_a_fault() {
        let silent = measure(&Audio { rate: 48_000, channels: vec![vec![0.0; 60_000], vec![0.0; 60_000]] });
        assert_eq!((silent.loudness_lkfs, silent.correlation, silent.true_peak_db), (None, 0.0, -180.0));
        // A second and a quarter: two lines, the last of what is left.
        assert_eq!(silent.per_second.len(), 2);
        assert_eq!(silent.per_second[0].coherence, 0.0);
        assert!(is_json(&silent.to_json()), "{}", silent.to_json());
        assert!(silent.to_string().contains("none"));
        let empty = measure(&Audio { rate: 48_000, channels: Vec::new() });
        assert_eq!((empty.frames, empty.seconds, empty.per_second.len()), (0, 0.0, 0));
        assert!(is_json(&empty.to_json()), "{}", empty.to_json());
        assert!(!empty.to_string().is_empty());
    }

    /// Not a test of what is measured but of how long it takes: run with
    /// `cargo test --release -- --ignored --nocapture`.
    #[test]
    #[ignore = "a measure of time, not of correctness"]
    fn measuring_45_seconds_of_stereo_takes_a_few_seconds() {
        let audio = Audio { rate: 48_000, channels: vec![white(72, 48_000, 45.0, 0.3), white(73, 48_000, 45.0, 0.3)] };
        let start = std::time::Instant::now();
        let report = measure(&audio);
        let whole = start.elapsed();
        let start = std::time::Instant::now();
        let frames = cues::cues(&audio.channels[0], &audio.channels[1], 48_000, &cues::cue_bands(), 0.0);
        let by_frame = start.elapsed();
        let start = std::time::Instant::now();
        let bands = crate::bands::band_levels_over_time(&audio.channels[0], 48_000, &octaves(), 0.05);
        let over_time = start.elapsed();
        eprintln!(
            "measure: {whole:?}; cues by frame in 7 octaves: {by_frame:?}; octaves every 50 ms, one channel: {over_time:?}"
        );
        assert_eq!((report.per_second.len(), frames[0].len(), bands.times.len()), (45, 2_109, 900));
        assert!(whole.as_secs_f64() < 30.0);
    }
}
