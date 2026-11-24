// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! A sound seen as a sum of sines: the fast Fourier transform (radix 2), the
//! Hann window, and frames taken along a signal.
//!
//! Every measure of this crate that speaks of frequency goes through
//! [`Stft`], so that they all cut a signal the same way and their times
//! agree.

use std::{
    f64::consts::TAU,
    ops::{Add, Mul, Sub},
};

/// A complex number.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Complex {
    pub re: f64,
    pub im: f64,
}

impl Complex {
    pub const ZERO: Complex = Complex { re: 0.0, im: 0.0 };

    pub fn new(re: f64, im: f64) -> Complex {
        Complex { re, im }
    }

    /// The point of the unit circle at `angle` radians.
    pub fn turn(angle: f64) -> Complex {
        let (im, re) = angle.sin_cos();
        Complex { re, im }
    }

    pub fn conj(self) -> Complex {
        Complex { re: self.re, im: -self.im }
    }

    /// The square of its length.
    pub fn power(self) -> f64 {
        self.re * self.re + self.im * self.im
    }

    pub fn abs(self) -> f64 {
        self.power().sqrt()
    }

    pub fn scale(self, by: f64) -> Complex {
        Complex { re: self.re * by, im: self.im * by }
    }
}

impl Add for Complex {
    type Output = Complex;

    fn add(self, other: Complex) -> Complex {
        Complex { re: self.re + other.re, im: self.im + other.im }
    }
}

impl Sub for Complex {
    type Output = Complex;

    fn sub(self, other: Complex) -> Complex {
        Complex { re: self.re - other.re, im: self.im - other.im }
    }
}

impl Mul for Complex {
    type Output = Complex;

    fn mul(self, other: Complex) -> Complex {
        Complex { re: self.re * other.re - self.im * other.im, im: self.re * other.im + self.im * other.re }
    }
}

/// The transform of one size, a power of two: its turns and its reordering
/// are computed once, then used for every frame.
#[derive(Clone, Debug)]
pub struct Fft {
    size: usize,
    /// The turns of each pass, one pass after the other: the pass that joins
    /// blocks of `half` values has `e^(−πi·n/half)` for n under `half`,
    /// starting at index `half − 1`. In a row, so that a pass reads them as
    /// it reads the data.
    turns: Vec<Complex>,
    /// The pairs to exchange before the passes (bit reversal).
    swaps: Vec<(usize, usize)>,
}

impl Fft {
    /// # Panics
    /// When `size` is not a power of two.
    pub fn new(size: usize) -> Fft {
        assert!(size.is_power_of_two(), "the size of a transform is a power of two, not {size}");
        let mut turns = Vec::with_capacity(size);
        let mut half = 1;
        while half < size {
            turns.extend((0..half).map(|n| Complex::turn(-TAU * n as f64 / (2 * half) as f64)));
            half *= 2;
        }
        let bits = size.trailing_zeros();
        let mut swaps = Vec::new();
        if bits > 0 {
            for i in 0..size {
                let j = i.reverse_bits() >> (usize::BITS - bits);
                if i < j {
                    swaps.push((i, j));
                }
            }
        }
        Fft { size, turns, swaps }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    /// `X[k] = Σ x[n]·e^(−2πi·kn/N)`, in place.
    pub fn forward(&self, data: &mut [Complex]) {
        assert_eq!(data.len(), self.size, "a transform of {} values was given {}", self.size, data.len());
        for &(a, b) in &self.swaps {
            data.swap(a, b);
        }
        // The first pass turns by nothing.
        for pair in data.as_chunks_mut::<2>().0 {
            (pair[0], pair[1]) = (pair[0] + pair[1], pair[0] - pair[1]);
        }
        let mut half = 2;
        while half < self.size {
            let turns = &self.turns[half - 1..2 * half - 1];
            for block in data.chunks_exact_mut(2 * half) {
                let (low, high) = block.split_at_mut(half);
                for ((a, b), turn) in low.iter_mut().zip(high).zip(turns) {
                    let turned = *b * *turn;
                    (*a, *b) = (*a + turned, *a - turned);
                }
            }
            half *= 2;
        }
    }

    /// `x[n] = (1/N)·Σ X[k]·e^(+2πi·kn/N)`, in place: what [`Fft::forward`]
    /// did, undone.
    pub fn inverse(&self, data: &mut [Complex]) {
        // Turning the other way is turning the mirror image, and mirroring back.
        for value in data.iter_mut() {
            *value = value.conj();
        }
        self.forward(data);
        let scale = 1.0 / self.size as f64;
        for value in data.iter_mut() {
            *value = value.conj().scale(scale);
        }
    }
}

/// The Hann window of `size` points, of the periodic kind: windows half a
/// size apart sum to one.
pub fn hann(size: usize) -> Vec<f64> {
    (0..size).map(|n| 0.5 - 0.5 * (TAU * n as f64 / size.max(1) as f64).cos()).collect()
}

/// The power of two nearest to `seconds` of sound at `rate`, for a frame:
/// 2048 samples for 50 ms at 44.1 and at 48 kHz.
pub fn frame_size(rate: u32, seconds: f64) -> usize {
    let wanted = (f64::from(rate) * seconds).max(2.0);
    1usize << (wanted.log2().round() as u32).clamp(1, 24)
}

/// How much of a window lay on the signal: a frame that reaches past either
/// end of the signal is weighed by the part that holds sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weight {
    /// The sum of the window over the samples that exist.
    pub sum: f64,
    /// The sum of its square over them.
    pub squares: f64,
}

/// Frames along a signal, each under a Hann window, each transformed.
///
/// Two layouts are offered. *Steps* are centred: step `i` looks at the signal
/// around sample `i·hop + hop/2`, whatever the frame's size, so that measures
/// made with frames of different sizes speak of the same instants; the first
/// and last frames reach outside the signal, and see its edge as a sound
/// switched on or off. *Covering frames* are a quarter of a frame apart and
/// start before the signal and end after it, so that every sample of a file
/// counts alike in an average over it.
#[derive(Clone, Debug)]
pub struct Stft {
    /// Half the frame's size: a frame of real samples is transformed as half
    /// as many complex values, which is half the work.
    fft: Fft,
    window: Vec<f64>,
    /// The window's sum and the sum of its square, over a whole frame.
    whole: Weight,
    hop: usize,
    /// The frame under its window.
    frame: Vec<f64>,
    work: Vec<Complex>,
    spectrum: Vec<Complex>,
    /// `e^(−2πi·k/size)` for each bin.
    turns: Vec<Complex>,
}

impl Stft {
    /// Frames of `size` samples (a power of two, 2 at least), `hop` samples
    /// apart.
    pub fn new(size: usize, hop: usize) -> Stft {
        assert!(size >= 2, "a frame holds two samples at least");
        let window = hann(size);
        Stft {
            fft: Fft::new(size / 2),
            whole: Weight { sum: window.iter().sum(), squares: window.iter().map(|w| w * w).sum() },
            window,
            hop: hop.max(1),
            frame: vec![0.0; size],
            work: vec![Complex::ZERO; size / 2],
            spectrum: vec![Complex::ZERO; size / 2 + 1],
            turns: (0..=size / 2).map(|k| Complex::turn(-TAU * k as f64 / size as f64)).collect(),
        }
    }

    pub fn size(&self) -> usize {
        self.window.len()
    }

    pub fn hop(&self) -> usize {
        self.hop
    }

    /// Frequencies a frame is seen at: from 0 to half the rate, both included.
    pub fn bins(&self) -> usize {
        self.size() / 2 + 1
    }

    /// Steps along a signal of `samples` samples: one for each whole hop, and
    /// one at least for any sound at all.
    pub fn steps(&self, samples: usize) -> usize {
        if samples == 0 { 0 } else { (samples / self.hop).max(1) }
    }

    /// The sample a step is centred on.
    pub fn centre(&self, step: usize) -> usize {
        step * self.hop + self.hop / 2
    }

    /// Where the frame of a step starts: before the signal, for the first.
    pub fn start(&self, step: usize) -> isize {
        self.centre(step) as isize - (self.size() / 2) as isize
    }

    /// Where the frames start that together weigh every sample of a signal of
    /// `samples` samples alike: a quarter of a frame apart, where the squares
    /// of Hann windows sum to the same everywhere, from the frame that holds
    /// only the signal's first samples to the one that holds only its last.
    pub fn covering(&self, samples: usize) -> impl Iterator<Item = isize> + use<> {
        let size = self.size();
        let quarter = (size / 4).max(1);
        let first = quarter as isize - size as isize;
        let count = if samples == 0 { 0 } else { (samples + size - quarter).div_ceil(quarter) };
        (0..count).map(move |frame| first + (frame * quarter) as isize)
    }

    /// The spectrum of the frame that starts at sample `start` — which may
    /// be before the signal, or end after it: [`Stft::bins`] values, and how
    /// much of the window lay on the signal.
    pub fn spectrum(&mut self, signal: &[f32], start: isize) -> (&[Complex], Weight) {
        let size = self.window.len() as isize;
        // The part of the frame that lies on the signal.
        let from = (-start).clamp(0, size) as usize;
        let to = (signal.len() as isize - start).clamp(0, size) as usize;
        let mut weight = self.whole;
        self.frame.fill(0.0);
        if from < to {
            let at = (start + from as isize) as usize;
            let samples = &signal[at..at + (to - from)];
            let window = &self.window[from..to];
            for ((slot, w), sample) in self.frame[from..to].iter_mut().zip(window).zip(samples) {
                *slot = f64::from(*sample) * w;
            }
            if to - from < self.window.len() {
                weight = Weight { sum: window.iter().sum(), squares: window.iter().map(|w| w * w).sum() };
            }
        } else {
            weight = Weight { sum: 0.0, squares: 0.0 };
        }
        // Even samples as real parts, odd ones as imaginary parts: one
        // transform of half the size holds the transforms of both, E and O,
        // as E + i·O; and the frame's own is E[k] + e^(−2πi·k/size)·O[k].
        for (slot, pair) in self.work.iter_mut().zip(self.frame.as_chunks::<2>().0) {
            *slot = Complex { re: pair[0], im: pair[1] };
        }
        self.fft.forward(&mut self.work);
        let half = self.work.len();
        for (k, (slot, turn)) in self.spectrum.iter_mut().zip(&self.turns).enumerate() {
            let (a, b) = (self.work[k % half], self.work[(half - k) % half].conj());
            let (even, odd) = (a + b, a - b);
            let odd = Complex { re: odd.im, im: -odd.re };
            *slot = (even + odd * *turn).scale(0.5);
        }
        (&self.spectrum, weight)
    }

    /// The same frame as mean squares: `out[k]` is the part of the frame's
    /// mean square (under the window) that lies at frequency `k·rate/size`,
    /// so that the bins together hold the frame's whole mean square. Returns
    /// how much of the window lay on the signal.
    pub fn power(&mut self, signal: &[f32], start: isize, out: &mut Vec<f64>) -> Weight {
        let size = self.size();
        let (spectrum, weight) = self.spectrum(signal, start);
        out.clear();
        if weight.squares <= 0.0 {
            out.resize(spectrum.len(), 0.0);
            return weight;
        }
        // Parseval: the squares of all `size` bins sum to `size` times the
        // squares of the samples. Bins between 0 and half the rate stand for
        // themselves and for their mirror.
        let scale = 1.0 / (size as f64 * weight.squares);
        out.extend(spectrum.iter().enumerate().map(|(k, value)| {
            let both = if k == 0 || 2 * k == size { 1.0 } else { 2.0 };
            both * value.power() * scale
        }));
        weight
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Noise, sine};

    fn noise(seed: u32, count: usize) -> Vec<Complex> {
        let mut noise = Noise::new(seed);
        (0..count).map(|_| Complex::new(f64::from(noise.next()), f64::from(noise.next()))).collect()
    }

    #[test]
    fn the_fast_transform_agrees_with_the_sum_it_stands_for() {
        let signal = noise(7, 64);
        let mut fast = signal.clone();
        Fft::new(64).forward(&mut fast);
        for (k, value) in fast.iter().enumerate() {
            let mut slow = Complex::ZERO;
            for (n, x) in signal.iter().enumerate() {
                slow = slow + *x * Complex::turn(-TAU * (k * n) as f64 / 64.0);
            }
            assert!((*value - slow).abs() < 1e-10, "bin {k}: {value:?} against {slow:?}");
        }
    }

    #[test]
    fn forward_then_inverse_gives_the_signal_back() {
        for size in [1, 2, 8, 1024] {
            let signal = noise(3, size);
            let fft = Fft::new(size);
            let mut data = signal.clone();
            fft.forward(&mut data);
            fft.inverse(&mut data);
            for (a, b) in signal.iter().zip(&data) {
                assert!((*a - *b).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn the_transform_keeps_the_energy() {
        let signal = noise(11, 512);
        let mut data = signal.clone();
        Fft::new(512).forward(&mut data);
        let before: f64 = signal.iter().map(|x| x.power()).sum();
        let after: f64 = data.iter().map(|x| x.power()).sum();
        assert!((after / 512.0 - before).abs() < 1e-9 * before);
    }

    #[test]
    fn a_sine_on_a_bin_lands_in_that_bin_alone() {
        let mut data: Vec<Complex> =
            (0..256).map(|n| Complex::new(0.5 * (TAU * 19.0 * n as f64 / 256.0).cos(), 0.0)).collect();
        Fft::new(256).forward(&mut data);
        for (k, value) in data.iter().enumerate() {
            // Half the amplitude at the bin, half at its mirror.
            let expected = if k == 19 || k == 256 - 19 { 0.25 * 256.0 } else { 0.0 };
            assert!((value.abs() - expected).abs() < 1e-9, "bin {k}: {}", value.abs());
        }
    }

    #[test]
    #[should_panic(expected = "power of two")]
    fn a_size_that_is_no_power_of_two_is_refused() {
        let _ = Fft::new(1000);
    }

    #[test]
    fn hann_windows_half_a_size_apart_sum_to_one() {
        let window = hann(64);
        assert_eq!(window[0], 0.0);
        assert!((window[32] - 1.0).abs() < 1e-15);
        for n in 0..32 {
            assert!((window[n] + window[n + 32] - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn a_frame_is_the_power_of_two_nearest_the_time_asked() {
        assert_eq!(frame_size(48_000, 0.05), 2_048);
        assert_eq!(frame_size(44_100, 0.05), 2_048);
        assert_eq!(frame_size(48_000, 0.17), 8_192);
        assert_eq!(frame_size(48_000, 0.68), 32_768);
    }

    #[test]
    fn the_bins_of_a_frame_hold_its_whole_mean_square() {
        let mut noise = Noise::new(5);
        let signal: Vec<f32> = (0..4_096).map(|_| noise.next() * 0.3).collect();
        let mut stft = Stft::new(1_024, 512);
        let window = hann(1_024);
        let mut power = Vec::new();
        for start in [0isize, 512, 3_072, -300, 3_700] {
            stft.power(&signal, start, &mut power);
            assert_eq!(power.len(), 513);
            let (mut squares, mut weights) = (0.0, 0.0);
            for (n, w) in window.iter().enumerate() {
                let at = start + n as isize;
                if at >= 0 && (at as usize) < signal.len() {
                    squares += (f64::from(signal[at as usize]) * w).powi(2);
                    weights += w * w;
                }
            }
            let total: f64 = power.iter().sum();
            assert!((total - squares / weights).abs() < 1e-12, "frame at {start}");
        }
    }

    #[test]
    fn a_frame_is_transformed_as_its_samples_under_the_window_would_be() {
        let mut noise = Noise::new(13);
        let signal: Vec<f32> = (0..700).map(|_| noise.next()).collect();
        for size in [2usize, 4, 256] {
            let (mut stft, fft, window) = (Stft::new(size, size / 2), Fft::new(size), hann(size));
            for start in [0isize, 100, -(size as isize) / 2, 700 - size as isize / 2, -5_000, 5_000] {
                let mut plain: Vec<Complex> = (0..size as isize)
                    .map(|n| {
                        usize::try_from(start + n).ok().and_then(|at| signal.get(at)).map_or(0.0, |&s| f64::from(s))
                    })
                    .zip(&window)
                    .map(|(s, w)| Complex::new(s * w, 0.0))
                    .collect();
                fft.forward(&mut plain);
                let (spectrum, _) = stft.spectrum(&signal, start);
                assert_eq!(spectrum.len(), size / 2 + 1);
                for (k, value) in spectrum.iter().enumerate() {
                    assert!((*value - plain[k]).abs() < 1e-12, "size {size}, frame at {start}, bin {k}");
                }
            }
        }
    }

    #[test]
    fn a_sine_reads_half_its_amplitude_squared_wherever_it_falls_between_bins() {
        let mut stft = Stft::new(2_048, 1_024);
        let mut power = Vec::new();
        for hz in [1_000.0, 1_007.3, 1_011.7] {
            let signal = sine(48_000, hz, 0.4, 0.5);
            stft.power(&signal, 4_096, &mut power);
            let total: f64 = power.iter().sum();
            assert!((total - 0.08).abs() < 2e-4, "{hz} Hz reads {total}");
        }
    }

    #[test]
    fn steps_are_centred_on_the_same_samples_whatever_the_frame() {
        let (short, long) = (Stft::new(256, 100), Stft::new(4_096, 100));
        assert_eq!(short.steps(1_050), 10);
        assert_eq!(long.steps(1_050), 10);
        assert_eq!(short.steps(30), 1);
        assert_eq!(short.steps(0), 0);
        for step in 0..10 {
            assert_eq!(short.centre(step), long.centre(step));
            assert_eq!(short.start(step) + 128, long.start(step) + 2_048);
        }
    }

    #[test]
    fn covering_frames_weigh_every_sample_alike() {
        for (size, samples) in [(256usize, 1_050usize), (4_096, 1_050), (4, 9), (2, 5), (64, 1)] {
            let stft = Stft::new(size, size / 2);
            let window = hann(size);
            let mut weighed = vec![0.0f64; samples];
            let starts: Vec<isize> = stft.covering(samples).collect();
            for &start in &starts {
                for (n, w) in window.iter().enumerate() {
                    let at = start + n as isize;
                    if (0..samples as isize).contains(&at) {
                        weighed[at as usize] += w * w;
                    }
                }
            }
            // The squares of Hann windows a quarter apart sum to 1.5 (to 1 for a frame of two samples).
            let all = if size == 2 { 1.0 } else { 1.5 };
            assert!(weighed.iter().all(|sum| (sum - all).abs() < 1e-12), "{size} over {samples}: {weighed:?}");
            // None lies wholly outside the signal.
            assert!(starts.iter().all(|&start| start + size as isize > 0 && start < samples as isize));
        }
        assert_eq!(Stft::new(256, 128).covering(0).count(), 0);
    }
}
