// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! IMA ADPCM: four bits a sample, each frame on its own — it starts with
//! the predictor and the step index, so a frame lost is a frame of
//! silence, not a voice garbled until the next one.
//!
//! A frame: the predictor (i16, little-endian), the step index, a byte of
//! padding, then two samples a byte, the first in the low four bits.

const INDEX: [i8; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];
const STEP: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73, 80, 88, 97, 107,
    118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449, 494, 544, 598, 658, 724, 796, 876, 963,
    1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272, 2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894,
    6484, 7132, 7845, 8630, 9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794,
    32767,
];

/// The bytes of a frame of `samples` samples.
pub const fn frame_bytes(samples: usize) -> usize {
    4 + samples.div_ceil(2)
}

/// The state the encoder and the decoder keep alike.
#[derive(Clone, Copy, Default)]
struct State {
    predictor: i32,
    index: i32,
}

impl State {
    /// Takes in a code: the sample it stands for.
    fn decode(&mut self, code: u8) -> i16 {
        let step = STEP[self.index as usize];
        let mut diff = step >> 3;
        if code & 4 != 0 {
            diff += step;
        }
        if code & 2 != 0 {
            diff += step >> 1;
        }
        if code & 1 != 0 {
            diff += step >> 2;
        }
        self.predictor = if code & 8 != 0 { self.predictor - diff } else { self.predictor + diff }.clamp(-32768, 32767);
        self.index = (self.index + i32::from(INDEX[usize::from(code & 15)])).clamp(0, 88);
        self.predictor as i16
    }

    fn encode(&mut self, sample: i16) -> u8 {
        let step = STEP[self.index as usize];
        let mut diff = i32::from(sample) - self.predictor;
        let mut code = 0u8;
        if diff < 0 {
            code = 8;
            diff = -diff;
        }
        let mut step_part = step;
        for bit in [4u8, 2, 1] {
            if diff >= step_part {
                code |= bit;
                diff -= step_part;
            }
            step_part >>= 1;
        }
        self.decode(code);
        code
    }
}

/// `samples` as a frame.
pub fn encode(samples: &[i16]) -> Vec<u8> {
    let first = samples.first().copied().unwrap_or(0);
    // The index that suits the start: the step nearest the first change.
    let change = samples.get(1).map_or(0, |s| (i32::from(*s) - i32::from(first)).abs());
    let index = STEP.iter().position(|s| *s >= change).unwrap_or(88).min(60) as i32;
    let mut state = State { predictor: i32::from(first), index };
    let mut out = Vec::with_capacity(frame_bytes(samples.len()));
    out.extend(first.to_le_bytes());
    out.push(index as u8);
    out.push(0);
    for pair in samples.chunks(2) {
        let low = state.encode(pair[0]);
        let high = pair.get(1).map_or(0, |s| state.encode(*s));
        out.push(low | (high << 4));
    }
    out
}

/// The samples of a frame; `None` if it is not one.
pub fn decode(frame: &[u8], samples: usize) -> Option<Vec<i16>> {
    if frame.len() != frame_bytes(samples) || frame[2] > 88 {
        return None;
    }
    let mut state =
        State { predictor: i32::from(i16::from_le_bytes([frame[0], frame[1]])), index: i32::from(frame[2]) };
    let mut out = Vec::with_capacity(samples);
    for byte in &frame[4..] {
        for code in [byte & 15, byte >> 4] {
            if out.len() < samples {
                out.push(state.decode(code));
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_voice_comes_back_close_to_what_was_said() {
        let said: Vec<i16> = (0..320)
            .map(|n| {
                let t = n as f32 / 16_000.0;
                ((t * 220.0 * std::f32::consts::TAU).sin() * 9_000.0
                    + (t * 1_250.0 * std::f32::consts::TAU).sin() * 3_000.0) as i16
            })
            .collect();
        let frame = encode(&said);
        assert_eq!(frame.len(), frame_bytes(320));
        assert_eq!(frame.len(), 164, "a quarter of the bytes, and a header");
        let heard = decode(&frame, 320).unwrap();
        let error: f64 = said.iter().zip(&heard).map(|(a, b)| (f64::from(*a) - f64::from(*b)).powi(2)).sum::<f64>();
        let signal: f64 = said.iter().map(|a| f64::from(*a).powi(2)).sum::<f64>();
        let snr = 10.0 * (signal / error).log10();
        assert!(snr > 20.0, "signal to noise {snr:.1} dB");
        assert!(decode(&frame[..100], 320).is_none());
        assert_eq!(decode(&encode(&[0; 320]), 320).unwrap(), vec![0; 320]);
    }
}
