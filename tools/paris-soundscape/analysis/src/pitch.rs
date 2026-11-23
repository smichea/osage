// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The pitch of a steady tone followed over time, to read a Doppler shift:
//! a source that passes is heard higher as it comes and lower as it goes,
//! by a ratio its speed fixes.

use crate::{
    bands::Band,
    fft::{Stft, frame_size},
    level::db,
};

/// The length of the frames a tone is looked for in, seconds: 8 192 samples
/// at 44.1 and at 48 kHz, bins 5.9 Hz apart. Longer would be finer in pitch
/// but would blur a pitch that moves.
pub const TONE_FRAME_S: f64 = 0.17;
/// How far under the strongest instant of a track a tone may be and still
/// count as present ([`tone_shift`]), dB.
pub const TONE_PRESENT_DB: f64 = 30.0;

/// The strongest tone of a band at an instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tone {
    /// Seconds.
    pub time: f64,
    /// Its frequency, Hz.
    pub hz: f64,
    /// Its level, dB: −3.01 for a sine of full scale. Low when the band
    /// holds no tone, in which case the frequency means nothing.
    pub level_db: f64,
}

/// The strongest tone of `band` along a signal, one value every `step_s`
/// seconds (0.05, say).
///
/// Each frame's spectrum is searched for its highest peak in the band; a
/// parabola through the peak's bin and its two neighbours, in decibels,
/// places the tone between bins. For a steady sine this is true to 0.016
/// of a bin: 0.1 Hz, which is 1.6 cents at 100 Hz and a sixth of a cent at
/// 1 kHz. The tone's level is the sum of the five bins its window spreads
/// it over.
pub fn tone_track(signal: &[f32], rate: u32, band: &Band, step_s: f64) -> Vec<Tone> {
    let hop = ((step_s * f64::from(rate)).round() as usize).max(1);
    let size = frame_size(rate, TONE_FRAME_S).max((2 * hop).next_power_of_two());
    let hz_a_bin = f64::from(rate) / size as f64;
    // A peak needs a neighbour on each side.
    let first = ((band.low / hz_a_bin).ceil().max(1.0)) as usize;
    let last = ((band.high / hz_a_bin).floor().max(0.0) as usize).min(size / 2 - 1);
    if first > last {
        return Vec::new();
    }
    let mut stft = Stft::new(size, hop);
    let mut track = Vec::with_capacity(stft.steps(signal.len()));
    for step in 0..stft.steps(signal.len()) {
        let time = stft.centre(step) as f64 / f64::from(rate);
        let (spectrum, weight) = stft.spectrum(signal, stft.start(step));
        let power = |k: usize| spectrum[k].power();
        // The highest bin that stands above both its neighbours; failing
        // one (the band is the slope of a tone outside it), the highest.
        let mut top: Option<usize> = None;
        let mut highest = first;
        for k in first..=last {
            if power(k) > power(highest) {
                highest = k;
            }
            if power(k) > power(k - 1) && power(k) >= power(k + 1) && top.is_none_or(|top| power(k) > power(top)) {
                top = Some(k);
            }
        }
        let k = top.unwrap_or(highest);
        let (before, at, after) = (db(power(k - 1)), db(power(k)), db(power(k + 1)));
        let bend = before - 2.0 * at + after;
        let offset = if top.is_some() && bend < 0.0 { (0.5 * (before - after) / bend).clamp(-0.5, 0.5) } else { 0.0 };
        // Mean squares, as in `Stft::power`.
        let spread: f64 = (k.saturating_sub(2).max(1)..=(k + 2).min(size / 2 - 1)).map(power).sum();
        let level_db = db(2.0 * spread / (size as f64 * weight.squares).max(1e-300));
        track.push(Tone { time, hz: (k as f64 + offset) * hz_a_bin, level_db });
    }
    track
}

/// A pitch before and after an instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shift {
    /// The pitch before, Hz: the median of the frames.
    pub before_hz: f64,
    /// The pitch after, Hz.
    pub after_hz: f64,
    /// After over before: under 1 for a source that passes.
    pub ratio: f64,
    /// The same in cents, hundredths of a semitone: 1200·log2(ratio).
    pub cents: f64,
    /// How steady the pitch was on the less steady side, cents: the range
    /// that holds the middle half of its frames. A few cents for a tone,
    /// hundreds when the band held only noise.
    pub spread_cents: f64,
}

/// The value a `part` (0…1) of the sorted `values` lie under.
fn quantile(values: &[f64], part: f64) -> f64 {
    let at = part * (values.len() - 1) as f64;
    let (under, over) = (at.floor() as usize, at.ceil() as usize);
    values[under] + (values[over] - values[under]) * (at - under as f64)
}

/// The pitch of a track ([`tone_track`]) before and after `instant`, as a
/// ratio and in cents: the Doppler shift of a source that is nearest at
/// that instant.
///
/// Frames less than `guard_s` seconds from the instant are left out — the
/// pitch slides there —, and so are frames where the tone is more than
/// [`TONE_PRESENT_DB`] under its strongest. `None` when a side has no frame
/// left.
///
/// A source of speed `v` heard far before and far after it passes shifts by
/// `(c − v)/(c + v)`: −102 cents at 36 km/h. Nearer the passage the shift
/// is smaller: the speed that counts is the part of it along the line to
/// the listener.
pub fn tone_shift(track: &[Tone], instant: f64, guard_s: f64) -> Option<Shift> {
    let strongest = track.iter().map(|tone| tone.level_db).fold(f64::NEG_INFINITY, f64::max);
    let side = |keep: &dyn Fn(f64) -> bool| {
        let mut pitches: Vec<f64> = track
            .iter()
            .filter(|tone| keep(tone.time) && tone.level_db >= strongest - TONE_PRESENT_DB && tone.hz > 0.0)
            .map(|tone| tone.hz)
            .collect();
        pitches.sort_by(f64::total_cmp);
        if pitches.is_empty() {
            return None;
        }
        let spread = 1_200.0 * (quantile(&pitches, 0.75) / quantile(&pitches, 0.25)).log2();
        Some((quantile(&pitches, 0.5), spread))
    };
    let (before_hz, spread_before) = side(&|time| time <= instant - guard_s)?;
    let (after_hz, spread_after) = side(&|time| time >= instant + guard_s)?;
    let ratio = after_hz / before_hz;
    Some(Shift {
        before_hz,
        after_hz,
        ratio,
        cents: 1_200.0 * ratio.log2(),
        spread_cents: spread_before.max(spread_after),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{sine, white};
    use std::f64::consts::TAU;

    fn cents(hz: f64, of: f64) -> f64 {
        1_200.0 * (hz / of).log2()
    }

    #[test]
    fn a_steady_tone_is_read_to_a_cent_and_at_its_level() {
        for rate in [44_100, 48_000] {
            for hz in [220.0, 441.3, 1_003.1, 3_217.9] {
                let track = tone_track(&sine(rate, hz, 0.2, 1.0), rate, &Band::new(hz * 0.8, hz * 1.25), 0.05);
                assert_eq!(track.len(), 20);
                // Away from the ends, where the frame reaches outside the signal.
                for tone in &track[2..18] {
                    assert!(cents(tone.hz, hz).abs() < 1.0, "{} Hz for {hz} Hz at {rate} Hz", tone.hz);
                    assert!((tone.level_db - db(0.02)).abs() < 0.02, "{} dB", tone.level_db);
                }
            }
        }
    }

    #[test]
    fn a_low_tone_is_read_to_a_few_cents() {
        let track = tone_track(&sine(48_000, 103.7, 0.2, 1.0), 48_000, &Band::new(80.0, 130.0), 0.05);
        for tone in &track[2..18] {
            assert!(cents(tone.hz, 103.7).abs() < 3.0, "{} Hz", tone.hz);
        }
    }

    #[test]
    fn a_tone_is_found_under_noise_as_loud_as_itself() {
        let mut signal = white(50, 48_000, 1.0, 0.245);
        for (s, tone) in signal.iter_mut().zip(sine(48_000, 880.0, 0.2, 1.0)) {
            *s += tone;
        }
        for tone in &tone_track(&signal, 48_000, &Band::new(500.0, 1_500.0), 0.05)[2..18] {
            assert!(cents(tone.hz, 880.0).abs() < 2.0, "{} Hz", tone.hz);
        }
    }

    #[test]
    fn the_strongest_of_two_tones_in_the_band_is_the_one_followed() {
        let mut signal = sine(48_000, 600.0, 0.05, 1.0);
        for (s, tone) in signal.iter_mut().zip(sine(48_000, 700.0, 0.2, 1.0)) {
            *s += tone;
        }
        for tone in &tone_track(&signal, 48_000, &Band::new(500.0, 900.0), 0.05)[2..18] {
            assert!((tone.hz - 700.0).abs() < 0.5);
        }
        // A band beside both sees the foot of the nearer one, and says by its level that it is no tone.
        for tone in &tone_track(&signal, 48_000, &Band::new(800.0, 900.0), 0.05)[2..18] {
            assert!(tone.level_db < -60.0, "{} dB at {} Hz", tone.level_db, tone.hz);
        }
        assert!(tone_track(&signal, 48_000, &Band::new(30_000.0, 40_000.0), 0.05).is_empty());
        assert!(tone_track(&[], 48_000, &Band::new(500.0, 900.0), 0.05).is_empty());
    }

    /// A tone of `hz` on a source that passes at `speed` m/s, `aside` metres
    /// from the listener at its nearest, at 2 s: 4 s of it.
    fn passing(hz: f64, speed: f64, aside: f64) -> Vec<f32> {
        let mut phase = 0.0f64;
        (0..4 * 48_000)
            .map(|n| {
                let along = speed * (n as f64 / 48_000.0 - 2.0);
                // The speed towards the listener, of the speed of sound (340 m/s).
                let closing = -speed * along / (along * along + aside * aside).sqrt() / 340.0;
                phase += TAU * hz / (1.0 - closing) / 48_000.0;
                (0.2 * phase.sin()) as f32
            })
            .collect()
    }

    #[test]
    fn a_tone_that_passes_reads_the_shift_its_speed_makes() {
        // 36 km/h, 3 m aside: a second or more from the passage, the pitch
        // is within a few cents of what it is from afar.
        let (speed, aside) = (10.0f64, 3.0f64);
        let track = tone_track(&passing(1_000.0, speed, aside), 48_000, &Band::new(900.0, 1_100.0), 0.05);
        let shift = tone_shift(&track, 2.0, 1.0).unwrap();
        // The medians are those of the frames 1.5 s from the passage.
        let along = speed * 1.5;
        let closing = speed * along / (along * along + aside * aside).sqrt() / 340.0;
        let expected = cents((1.0 - closing) / (1.0 + closing), 1.0);
        assert!((expected + 99.9).abs() < 0.2, "{expected} cents expected");
        assert!((shift.cents - expected).abs() < 2.0, "{} cents against {expected}", shift.cents);
        assert!((shift.ratio - 2f64.powf(expected / 1_200.0)).abs() < 2e-3);
        assert!(shift.before_hz > 1_025.0 && shift.after_hz < 975.0, "{shift:?}");
        assert!(shift.spread_cents < 5.0, "spread {}", shift.spread_cents);
        // From afar it tends to (c − v)/(c + v).
        assert!((cents(330.0 / 350.0, 1.0) + 101.9).abs() < 0.1);
    }

    #[test]
    fn a_steady_tone_shifts_by_nothing_and_noise_says_it_is_no_tone() {
        let track = tone_track(&sine(48_000, 500.0, 0.2, 2.0), 48_000, &Band::new(400.0, 600.0), 0.05);
        let shift = tone_shift(&track, 1.0, 0.2).unwrap();
        assert!(shift.cents.abs() < 0.5 && shift.spread_cents < 0.5, "{shift:?}");
        let track = tone_track(&white(51, 48_000, 2.0, 0.5), 48_000, &Band::new(400.0, 600.0), 0.05);
        assert!(tone_shift(&track, 1.0, 0.2).unwrap().spread_cents > 50.0);
        // Nothing before the instant: nothing to compare.
        assert_eq!(tone_shift(&track, 0.0, 0.2), None);
        assert_eq!(tone_shift(&[], 1.0, 0.2), None);
    }

    #[test]
    fn frames_where_the_tone_is_absent_do_not_count() {
        // A tone, a silence, the same tone a little lower.
        let mut signal = sine(48_000, 1_000.0, 0.2, 1.0);
        signal.extend(vec![0.0f32; 48_000]);
        signal.extend(sine(48_000, 980.0, 0.2, 1.0));
        let track = tone_track(&signal, 48_000, &Band::new(900.0, 1_100.0), 0.05);
        let shift = tone_shift(&track, 1.5, 0.0).unwrap();
        assert!((shift.cents - cents(980.0, 1_000.0)).abs() < 1.0, "{shift:?}");
    }
}
