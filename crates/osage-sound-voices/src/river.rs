// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! A broad, slow river against stone — a pier, a quay wall, the tip of an
//! island: the kind "water". Three layers from one seed, after K. van den
//! Doel, "Physically based models for liquid sounds" (ACM Transactions on
//! Applied Perception 2(4), 2005):
//!
//! - **the bed:** the mass of bubbles too many to tell apart, as noise kept
//!   between 300 Hz and 3 kHz and tilted down (6 dB an octave less from
//!   450 Hz, 12 from 1.1 kHz), so that it is a murmur and not a hiss. It
//!   swells slowly and irregularly, by 3.5 dB between a quiet moment and a
//!   full one (noise under 0.5 Hz);
//! - **the bubbles:** each a decaying sine, which is what says water. A
//!   bubble of radius r rings at f = 3/r Hz (Minnaert) and dies at the rate
//!   d = 0.043 f + 0.0014 f^1.5 a second; its swing is r^1.5 times a depth
//!   factor rnd^10 (most are too deep to be heard, a few stand out); the
//!   shallow ones rise in pitch as they come up, f (1 + 0.1 d t): the chirp.
//!   Radii from 0.3 mm to 6 mm (10 kHz down to 500 Hz), small ones more
//!   often (as many as 1/r for each step of the radius in proportion). They
//!   come in gusts: more when the bed swells, many more just after a lap.
//!   Each is a two-multiply resonator in a pool of 64, the quietest replaced
//!   when it is full; those that could not be heard are not made;
//! - **the laps:** now and then — every 4 to 16 seconds on an ordinary day,
//!   twice as often in a wake, never on still water — water strikes the
//!   stone and falls back: a slap (noise in a band at 220 to 420 Hz, 60 to
//!   120 ms to swell and three times that to die), three to six large
//!   bubbles of about one size (5 to 30 mm, 100 to 600 Hz, their pitch
//!   rising a little), then a brighter wash (noise at 0.8 to 3.5 kHz) as the
//!   water runs off.
//!
//! The sum goes through a limiter that leaves alone everything under 0.7,
//! which the water on an ordinary day seldom reaches.
//!
//! [`Drive::load`](crate::Drive) is how agitated the water is: 0.5 the river
//! on an ordinary day, where the level is [`NOMINAL_RMS`] (over half a
//! minute: ten seconds may be 2 dB off, as it swells and laps); 0 still
//! water (8 dB quieter, fewer and smaller bubbles, to 3 mm); 1 a boat's wake
//! arriving (5 dB louder, more and larger bubbles, to 12 mm, louder laps).
//! [`Drive::detail`](crate::Drive) scales the number of bubbles made; the
//! bed takes over the power they held, so the level stays. At 0 the bed and
//! the laps are left. Speed, acceleration and blows are not heard.
//!
//! Parameters:
//! - `bubbles` (800): bubbles formed a second on an ordinary day, the deep
//!   ones counted: about a third are made;
//! - `bed` (0.45): the share of the sound's power the bed holds, 0.05…1; the
//!   laps and the bubbles share the rest, as far as the largest bubble can
//!   swing;
//! - `laps` (0.1): laps a second on an ordinary day: at 0.1, one every 4 to
//!   16 seconds; as many again in a wake (load 1), none on still water;
//! - `size` (1): every radius times this, 0.5…2: under 1 a trickle, higher in
//!   pitch; over 1 a heavy wash.
//!
//! Several of these placed along the water with different seeds make one
//! wide source: two seeds share nothing.
//!
//! # Assumptions
//!
//! What the paper gives: the bubble (frequency, decay, rising pitch, swing
//! as r^1.5), the depth factor rnd^10, radii spread as r^−γ with γ under 3
//! for a stream, a rise of 0.1 for drops and 0.5 to 1 for large bubbles.
//! What no source backs:
//!
//! - the three layers and their shares: no measurement of the Seine at a
//!   pier was found, nor a published recipe for a river. The sound was
//!   judged by its spectrogram and its levels, not by ear;
//! - the largest bubble of the bank, 6 mm, where the paper's range goes to
//!   50 mm: with swings as r^1.5, the largest bubbles there are hide all the
//!   others, so the bank stops where the laps' bubbles begin;
//! - γ = 1, and which bubbles rise (those whose depth factor is over 0.5;
//!   the paper suggests 0.9 for sparse sounds);
//! - the bed's band and tilt, the swell's depth and pace;
//! - everything about a lap: its shape, its length, its bands, how many
//!   bubbles and that they are of a size, that small bubbles follow it;
//! - what `load` does: the levels (−8 dB, +5 dB), the count of bubbles (0.4
//!   to 1.6 times) and their largest radius (half to twice);
//! - bubbles of either sign (so that no offset builds up), and not making
//!   those under −70 dB;
//! - the pitch of a bubble rises in steps, every 32 samples, and its swing
//!   then falls a little faster than the paper's (as the inverse square root
//!   of the pitch): the price of two multiplies a sample.

use std::f32::consts::{E, LN_2, PI, TAU};

use crate::{
    Drive, Params, Voice,
    dsp::{Band, DT, Low, NOMINAL_RMS, Noise, RATE, seed_for, soft},
};

pub const ABOUT: &str =
    "A broad river against stone: a murmur that swells, bubbles, a lap now and then as the water stirs";
pub const PARAMS: &[(&str, f32)] = &[("bubbles", 800.0), ("bed", 0.45), ("laps", 0.1), ("size", 1.0)];

/// Samples between two updates of what changes slowly: the swell, the laps,
/// the pitch of rising bubbles, the drive.
const TICK: u32 = 32;
const TICK_S: f32 = TICK as f32 * DT;
/// Bubbles ringing at once; a multiple of [`LANES`].
const SLOTS: usize = 64;
/// Bubbles summed side by side, for the processor to do them at once.
const LANES: usize = 4;

/// A bubble of radius r rings at 3/r Hz, r in metres (Minnaert, as the paper
/// rounds it).
const MINNAERT: f32 = 3.0;
/// No bubble rings higher: partials stay under 0.45 of the rate.
const HIGHEST: f32 = 0.45 * RATE as f32;
/// The most a bubble's turn gains in a tick, radians.
const STEP: f32 = 0.3;
/// Radii of the bank, metres, for a `size` of 1 on an ordinary day.
const SMALLEST: f32 = 0.3e-3;
const LARGEST: f32 = 6.0e-3;
/// γ: as many bubbles as r^−γ for each step of the radius in proportion.
const SPREAD: f32 = 1.0;
/// The mean of the depth factor squared, rnd^10 squared: 1/21; less what
/// rising takes from the shallowest fifth of it, 1.2 %.
const DEPTH_POWER: f32 = 0.988 / 21.0;
/// ξ: how fast a shallow bubble rises in pitch, and how shallow it must be:
/// 0.9, as the paper suggests for sparse sounds (at 0.5 three quarters of
/// the bank's power rose, and chirped).
const RISE: f32 = 0.1;
const RISES_OVER: f32 = 0.9;
/// The largest of a lap's bubbles, metres, its γ, and the ξ of them all.
const LAP_SMALLEST: f32 = 5.0e-3;
const LAP_LARGEST: f32 = 30.0e-3;
const LAP_SPREAD: f32 = 1.0;
const LAP_RISE: (f32, f32) = (0.1, 0.3);
/// A lap's bubbles: their radius as a part of the largest of that lap, and
/// what they swing of what their radius allows.
const GULP_PART: (f32, f32) = (0.5, 1.0);
const GULP_SWING: (f32, f32) = (0.5, 1.0);
/// A lap: how strong, seconds to swell, where the slap's band is, Hz.
const LAP_STRENGTH: (f32, f32) = (0.55, 1.0);
const LAP_SWELL: (f32, f32) = (0.06, 0.12);
const SLAP_BAND: (f32, f32) = (220.0, 420.0);
const SLAP_Q: f32 = 1.2;
/// The largest bubble of a lap, the slap and the wash at a lap's height,
/// against a swing of 1 for the largest bubble of the bank.
const GULP: f32 = 0.3;
const SLAP: f32 = 0.25;
const WASH: f32 = 0.075;
/// The share of the bank's bubbles that follow the laps rather than the
/// swell.
const AFTER_LAPS: f32 = 0.2;
/// The swell: one standard deviation in nepers (1.75 dB), and the band of
/// the noise it follows, Hz: slow, but ten seconds hold several swells.
const SWELL: f32 = 0.2;
const SWELL_BAND: (f32, f32) = (0.15, 0.6);
/// A bubble under this is not made, and one that fell under that is over.
const UNHEARD: f32 = 3.0e-4;
const GONE: f32 = 1.0e-4;
/// A lap's noises are not made under this.
const SILENT: f32 = 1.0e-4;
/// The largest bubble's swing, at most: the rest of the power goes to the bed
/// whatever `bed` says. At 0.85 the largest bubbles took the sum over the
/// limiter's knee twenty times a minute; at 0.5, once or not at all.
const HEADROOM: f32 = 0.5;
/// The limiter leaves alone everything under this.
const KNEE: f32 = 0.7;
/// Seconds for a change of load or detail to be followed.
const EASE: f32 = 0.03;
/// The power of the bed's noise and of the wash's before their gains, for a
/// `size` of 1: their bands, so their powers, are as wide as 1/size.
const BED_POWER: f32 = 0.00259;
const WASH_POWER: f32 = 0.0239;

/// How fast a bubble ringing at `frequency` dies, 1/s: what it radiates and
/// what heat takes (the paper's equation 5).
fn decay(frequency: f32) -> f32 {
    0.043 * frequency + 0.0014 * frequency * frequency.sqrt()
}

/// How much louder agitated water is: −8 dB still, 1 on an ordinary day,
/// +5 dB in a wake.
fn agitation(load: f32) -> f32 {
    0.4 + load * (1.02 + 0.36 * load)
}

/// How often the water laps for a load, against an ordinary day: not at all
/// on still water, twice as often in a wake.
fn lapping(load: f32) -> f32 {
    (2.0 * load).clamp(0.0, 2.0)
}

/// How many more bubbles agitated water makes.
fn crowd(load: f32) -> f32 {
    0.4 + 1.2 * load
}

/// The mean, over bubbles drawn as [`radius`] draws them up to `top` metres,
/// of the power one bubble a second gives when the largest swings 1:
/// (r/top)³ / 4d.
fn mean_power(top: f32, span: f32, spread: f32) -> f32 {
    const STEPS: usize = 64;
    let (mut sum, mut weights) = (0.0f32, 0.0f32);
    for i in 0..STEPS {
        let x = (i as f32 + 0.5) / STEPS as f32 * span;
        let weight = (-spread * x).exp();
        let part = (x - span).exp();
        sum += weight * part * part * part / (4.0 * decay((MINNAERT / (top * part)).min(HIGHEST)));
        weights += weight;
    }
    sum / weights
}

/// The same for one of a lap's bubbles when the largest of its lap swings 1:
/// part³ / 4d over the laps' sizes and the bubbles' parts of them.
fn gulp_power(size: f32) -> f32 {
    const STEPS: usize = 32;
    let span = (LAP_LARGEST / LAP_SMALLEST).ln();
    let (mut sum, mut weights) = (0.0f32, 0.0f32);
    for i in 0..STEPS {
        let x = (i as f32 + 0.5) / STEPS as f32 * span;
        let weight = (-LAP_SPREAD * x).exp();
        for j in 0..8 {
            let part = GULP_PART.0 + (GULP_PART.1 - GULP_PART.0) * (j as f32 + 0.5) / 8.0;
            let frequency = MINNAERT / (LAP_SMALLEST * size * x.exp() * part);
            sum += weight * part * part * part / (4.0 * decay(frequency));
            weights += weight;
        }
    }
    sum / weights
}

/// A radius drawn between a smallest and e^span times it, small ones more
/// often: its logarithm over the smallest. `edge` is 1 − e^(−spread·span).
fn radius(unit: f32, spread: f32, edge: f32) -> f32 {
    -(1.0 - unit * edge).ln() / spread
}

/// Everything under [`KNEE`] as it is, the rest bent to stay within ±1.
fn limit(x: f32) -> f32 {
    let over = x.abs() - KNEE;
    if over <= 0.0 { x } else { (KNEE + (1.0 - KNEE) * soft(over / (1.0 - KNEE))).copysign(x) }
}

/// A gain that reaches what it aims at in a straight line over a tick.
#[derive(Clone, Copy, Debug, Default)]
struct Gain {
    now: f32,
    step: f32,
}

impl Gain {
    fn aim(&mut self, target: f32) {
        self.step = (target - self.now) / TICK as f32;
    }

    fn next(&mut self) -> f32 {
        self.now += self.step;
        self.now
    }
}

/// The bubbles ringing: each `y[n] = c1·y[n−1] − c2·y[n−2]`, a decaying sine
/// started from rest. Those alive are kept first, one array for each number,
/// so that several are done at once.
#[derive(Clone, Debug)]
struct Pool {
    live: usize,
    y1: [f32; SLOTS],
    y2: [f32; SLOTS],
    c1: [f32; SLOTS],
    c2: [f32; SLOTS],
    /// Where the pitch is, as the cosine and sine of the turn a sample, and
    /// the turn it gains a tick.
    cos: [f32; SLOTS],
    sin: [f32; SLOTS],
    gain_cos: [f32; SLOTS],
    gain_sin: [f32; SLOTS],
    /// Twice what is left after a sample.
    keep: [f32; SLOTS],
    /// How large it swings by now, and what is left of that after a tick.
    swing: [f32; SLOTS],
    fade: [f32; SLOTS],
}

impl Pool {
    fn new() -> Self {
        let zero = [0.0; SLOTS];
        Self {
            live: 0,
            y1: zero,
            y2: zero,
            c1: zero,
            c2: zero,
            cos: zero,
            sin: zero,
            gain_cos: zero,
            gain_sin: zero,
            keep: zero,
            swing: zero,
            fade: zero,
        }
    }

    /// A bubble of `radius` metres begins, swinging `swing` at first (either
    /// sign), its pitch rising `rise` times as fast as it dies.
    fn add(&mut self, radius: f32, swing: f32, rise: f32) {
        let slot = if self.live < SLOTS {
            self.live += 1;
            self.live - 1
        } else {
            // The quietest makes room, if it is quieter than the newcomer:
            // what it still held is lost, a step no larger than itself.
            let mut quietest = 0;
            for i in 1..SLOTS {
                if self.swing[i] < self.swing[quietest] {
                    quietest = i;
                }
            }
            if self.swing[quietest] >= swing.abs() {
                return;
            }
            quietest
        };
        let frequency = (MINNAERT / radius).min(HIGHEST);
        let dies = decay(frequency);
        let turn = TAU * frequency * DT;
        let (sin, cos) = turn.sin_cos();
        let left = (-dies * DT).exp();
        // f (1 + ξ d t): the turn gains the same angle every tick; no more
        // than [`STEP`], which only the shortest-lived bubbles would.
        let gain = (turn * rise * dies * TICK_S).min(STEP);
        // What is left after a tick of 32 samples.
        let mut fade = left * left;
        for _ in 0..4 {
            fade *= fade;
        }
        // At rest one sample before a·sin(0) = 0: the next sample is
        // a·left·sin(turn).
        self.y1[slot] = 0.0;
        self.y2[slot] = -swing * sin / left;
        self.c1[slot] = 2.0 * left * cos;
        self.c2[slot] = left * left;
        self.cos[slot] = cos;
        self.sin[slot] = sin;
        self.gain_cos[slot] = 1.0 - gain * gain * 0.5;
        self.gain_sin[slot] = gain - gain * gain * gain / 6.0;
        self.keep[slot] = 2.0 * left;
        self.swing[slot] = swing.abs();
        self.fade[slot] = fade;
    }

    /// A tick has passed: rising bubbles are tuned higher, those that have
    /// died out leave.
    fn tick(&mut self) {
        // The cosine of the turn a step short of 0.45 of the rate: a bubble
        // there rises no more.
        const TOP: f32 = -0.817;
        for i in 0..self.live {
            self.swing[i] *= self.fade[i];
            if self.cos[i] > TOP {
                let cos = self.cos[i] * self.gain_cos[i] - self.sin[i] * self.gain_sin[i];
                self.sin[i] = self.sin[i] * self.gain_cos[i] + self.cos[i] * self.gain_sin[i];
                self.cos[i] = cos;
                self.c1[i] = self.keep[i] * cos;
            }
        }
        for i in (0..self.live).rev() {
            if self.swing[i] < GONE {
                // The last one alive takes its place.
                self.live -= 1;
                let last = self.live;
                for array in [
                    &mut self.y1,
                    &mut self.y2,
                    &mut self.c1,
                    &mut self.c2,
                    &mut self.cos,
                    &mut self.sin,
                    &mut self.gain_cos,
                    &mut self.gain_sin,
                    &mut self.keep,
                    &mut self.swing,
                    &mut self.fade,
                ] {
                    array[i] = array[last];
                }
                // An empty slot holds zeros: it is summed with the others.
                self.y1[last] = 0.0;
                self.y2[last] = 0.0;
            }
        }
    }

    /// The next sample of the first `count` bubbles together; `count` is a
    /// multiple of [`LANES`].
    #[inline(always)]
    fn next(&mut self, count: usize) -> f32 {
        let mut sum = [0.0f32; LANES];
        let (y1, _) = self.y1[..count].as_chunks_mut::<LANES>();
        let (y2, _) = self.y2[..count].as_chunks_mut::<LANES>();
        let (c1, _) = self.c1[..count].as_chunks::<LANES>();
        let (c2, _) = self.c2[..count].as_chunks::<LANES>();
        for (((y1, y2), c1), c2) in y1.iter_mut().zip(y2).zip(c1).zip(c2) {
            for lane in 0..LANES {
                let y = c1[lane] * y1[lane] - c2[lane] * y2[lane];
                y2[lane] = y1[lane];
                y1[lane] = y;
                sum[lane] += y;
            }
        }
        (sum[0] + sum[2]) + (sum[1] + sum[3])
    }
}

/// A bubble of a lap, waiting for its moment.
#[derive(Clone, Copy, Debug, Default)]
struct Gulp {
    /// Ticks until it begins.
    wait: u32,
    radius: f32,
    swing: f32,
    rise: f32,
}

/// Water against a pier, a quay or the tip of an island.
pub struct Water {
    // What the parameters come to.
    /// Bubbles formed a second on an ordinary day, less those too deep to be
    /// heard at any radius.
    rate: f32,
    /// The depth factor's draw starts here: under it, no bubble is heard.
    deep: f32,
    /// Laps a second.
    laps: f32,
    size: f32,
    /// The swing of the largest bubble of the bank on an ordinary day.
    peak: f32,
    bed_gain: f32,
    /// The power of the bank's bubbles for a load of 0, 0.5 and 1, over the
    /// power of the bed's noise: what the bed gives back when they are not
    /// made.
    bank_power: [f32; 3],
    /// The mean of the laps' wash, by which the bubbles that follow the laps
    /// are counted; 0 without laps.
    lap_mean: f32,
    swell_ease: (f32, f32),
    swell_norm: f32,
    drive_ease: f32,

    // A noise for each layer, and for what happens when in the bank and in
    // the laps: less detail leaves the laps where they were.
    hiss: Noise,
    spray: Noise,
    slow: Noise,
    bank: Noise,
    chance: Noise,

    // The bed.
    floor: [Low; 2],
    dip: Low,
    tilt: Low,
    ceiling: Low,
    bed: Gain,
    swell: [f32; 3],

    // The laps.
    slap: Band,
    slap_top: Low,
    slap_gain: f32,
    slap_now: Gain,
    wash_floor: Low,
    wash_top: Low,
    wash_ceiling: Low,
    wash_gain: f32,
    wash_now: Gain,
    /// Whether a lap's noises are to be made this tick.
    lapping: bool,
    /// A lap's shape: a blow, and the blow smoothed once and twice.
    lap: [f32; 3],
    lap_keep: f32,
    /// Ticks until the next lap.
    lap_in: u32,
    gulps: [Gulp; 8],
    gulps_waiting: usize,

    // The bubbles.
    pool: Pool,
    /// What follows the load, and the load it was worked out for: how much
    /// louder, the bank's largest radius, its logarithm over the smallest,
    /// 1 − e^(−γ·span), the swing of a bubble at that radius and no depth,
    /// the bank's power over the bed's noise.
    tuned_for: f32,
    louder: f32,
    top: f32,
    span: f32,
    edge: f32,
    bank_peak: f32,
    bank_now: f32,
    /// Bubbles still to wait for before the next one, counted at the tick's
    /// start, and how many come in a sample.
    gap: f32,
    pace: f32,

    // Where the drive is, and where it is going.
    load: f32,
    detail: f32,
    load_to: f32,
    detail_to: f32,
    started: bool,
    /// Samples left in this tick, and until the next bubble if it is in this
    /// tick.
    tick_left: u32,
    bubble_in: u32,

    #[cfg(test)]
    tally: Tally,
}

/// What a test counts.
#[cfg(test)]
#[derive(Clone, Debug, Default)]
struct Tally {
    /// Bubbles of the bank made, and the largest of their radii.
    bubbles: u32,
    largest: f32,
    /// The tick each lap began at.
    laps: Vec<u32>,
    ticks: u32,
    /// The most bubbles ringing at once.
    fullest: usize,
}

impl Water {
    pub fn new(seed: u32, params: &Params) -> Self {
        let get = |name: &str| params.get(name, PARAMS.iter().find(|(n, _)| *n == name).map_or(0.0, |(_, d)| *d));
        let finite = |value: f32, otherwise: f32| if value.is_finite() { value } else { otherwise };
        let bubbles = finite(get("bubbles"), 0.0).clamp(0.0, 20_000.0);
        let share = finite(get("bed"), 1.0).clamp(0.05, 1.0);
        let laps = finite(get("laps"), 0.0).clamp(0.0, 4.0);
        let size = finite(get("size"), 1.0).clamp(0.5, 2.0);

        // The power each layer has when the largest bubble swings 1, on an
        // ordinary day.
        let mean = |range: (f32, f32)| (range.0 + range.1) / 2.0;
        let mean_square = |range: (f32, f32)| (range.0 * range.0 + range.0 * range.1 + range.1 * range.1) / 3.0;
        let bank = |load: f32| {
            let top = LARGEST * size * (2.0 * load - 1.0).exp2();
            let span = (top / (SMALLEST * size)).ln();
            bubbles * crowd(load) * agitation(load) * DEPTH_POWER * mean_power(top, span, SPREAD)
        };
        // A lap's shapes are (t/τ)·e^(1−t/τ) and (t/τ)²·e^(2−t/τ)/2, squared
        // and summed over time: e²τ/4 and 3e⁴τ/64. A rising bubble swings
        // less long (see the assumptions): 0.77 of its power at ξ = 0.75.
        let strength = mean_square(LAP_STRENGTH) * mean(LAP_SWELL);
        let gulps = GULP * GULP * 4.5 * mean_square(GULP_SWING) * mean_square(LAP_STRENGTH) * 0.77;
        let (slap, wash) = (SLAP * SLAP * strength * E * E / 4.0, WASH * WASH * strength * 3.0 * E.powi(4) / 64.0);
        // The slap and the wash are made of one noise and meet around 700 Hz:
        // together they hold more than their sum, by a quarter of their
        // geometric mean (measured).
        let lap = laps * (slap + wash + 0.25 * (slap * wash).sqrt() + gulps * gulp_power(size));
        let events = bank(0.5) + lap;
        let nominal = NOMINAL_RMS * NOMINAL_RMS;
        let peak = if events > 0.0 { ((1.0 - share) * nominal / events).sqrt().min(HEADROOM) } else { 0.0 };
        let bed_power = nominal - peak * peak * events;
        // The swell's gain squared is e^(2·SWELL²) on average.
        let bed_gain = (bed_power * size / (BED_POWER * (2.0 * SWELL * SWELL).exp())).sqrt();

        // The swell: noise of variance 1/3, a sample a tick, under the top of
        // its band twice and over its bottom once. What is left of its power
        // is the mean of the three filters' gains squared over all pitches,
        // summed here on steps of equal ratio.
        let swell_ease = (1.0 - (-TAU * SWELL_BAND.0 * TICK_S).exp(), 1.0 - (-TAU * SWELL_BAND.1 * TICK_S).exp());
        let (mut kept, steps, lowest) = (0.0f32, 600, 1.0e-6f32);
        let ratio = (PI / lowest).powf(1.0 / steps as f32);
        for i in 0..steps {
            let turn = lowest * ratio.powf(i as f32 + 0.5);
            let chord = 4.0 * (turn / 2.0).sin().powi(2);
            let under = swell_ease.1 * swell_ease.1 / (swell_ease.1 * swell_ease.1 + (1.0 - swell_ease.1) * chord);
            let q = 1.0 - swell_ease.0;
            let over = q * q * chord / (swell_ease.0 * swell_ease.0 + q * chord);
            kept += under * under * over * turn * (ratio.sqrt() - 1.0 / ratio.sqrt());
        }
        let swell_norm = (3.0 * PI / kept).sqrt();

        // Each noise is named: adding one leaves the others as they were.
        let noise = |part: &str| Noise::new(seed_for(u64::from(seed), part));
        // No bubble deeper than this could be heard, whatever its radius.
        let loudest = peak * agitation(1.0).sqrt();
        let deep = if loudest > UNHEARD { (UNHEARD / loudest).powf(0.1) } else { 1.0 };
        let mut water = Self {
            rate: bubbles * (1.0 - deep),
            deep,
            laps,
            size,
            peak,
            bed_gain,
            bank_power: [0.0, 0.5, 1.0].map(|load| peak * peak * bank(load) * size / BED_POWER),
            lap_mean: laps * mean(LAP_STRENGTH) * mean(LAP_SWELL) * E * E / 2.0,
            swell_ease,
            swell_norm,
            drive_ease: 1.0 - (-TICK_S / EASE).exp(),
            hiss: noise("water/bed"),
            spray: noise("water/laps"),
            slow: noise("water/swell"),
            bank: noise("water/bubbles"),
            chance: noise("water/laps/when"),
            floor: [Low::at(220.0 / size), Low::at(220.0 / size)],
            dip: Low::at(450.0 / size),
            tilt: Low::at(1_100.0 / size),
            ceiling: Low::at(3_000.0 / size),
            bed: Gain::default(),
            swell: [0.0; 3],
            slap: Band::at(300.0 / size, SLAP_Q),
            slap_top: Low::at(600.0 / size),
            slap_gain: 0.0,
            slap_now: Gain::default(),
            wash_floor: Low::at(800.0 / size),
            wash_top: Low::at(3_500.0 / size),
            wash_ceiling: Low::at(3_500.0 / size),
            wash_gain: peak * WASH * (size / WASH_POWER).sqrt(),
            wash_now: Gain::default(),
            lapping: false,
            lap: [0.0; 3],
            lap_keep: 0.0,
            lap_in: u32::MAX,
            gulps: [Gulp::default(); 8],
            gulps_waiting: 0,
            pool: Pool::new(),
            tuned_for: f32::NAN,
            louder: 1.0,
            top: LARGEST * size,
            span: 0.0,
            edge: 0.0,
            bank_peak: 0.0,
            bank_now: 0.0,
            gap: 0.0,
            pace: 0.0,
            load: 0.5,
            detail: 1.0,
            load_to: 0.5,
            detail_to: 1.0,
            started: false,
            tick_left: 0,
            bubble_in: u32::MAX,
            #[cfg(test)]
            tally: Tally::default(),
        };
        // The swell is under way when the sound begins, and so is the wait
        // for the first lap and the first bubble.
        for _ in 0..(5.0 / TICK_S) as u32 {
            water.sway();
        }
        let lapping = laps * lapping(water.load);
        if lapping > 0.0 {
            water.lap_in = (water.chance.between(0.1, 1.6) / (lapping * TICK_S)) as u32 + 1;
        }
        water.gap = -(1.0 - water.bank.unit()).ln();
        water
    }

    /// The swell moves on by a tick: about one standard deviation either way.
    fn sway(&mut self) -> f32 {
        self.swell[0] += (self.slow.next() - self.swell[0]) * self.swell_ease.1;
        self.swell[1] += (self.swell[0] - self.swell[1]) * self.swell_ease.1;
        self.swell[2] += (self.swell[1] - self.swell[2]) * self.swell_ease.0;
        (self.swell[1] - self.swell[2]) * self.swell_norm
    }

    /// What follows the load is worked out again, when the load has moved.
    fn tune(&mut self) {
        if (self.load - self.tuned_for).abs() < 1e-5 {
            return;
        }
        self.tuned_for = self.load;
        self.louder = agitation(self.load);
        // More and larger bubbles in agitated water: half to twice the radius.
        let reach = 2.0 * self.load - 1.0;
        self.top = LARGEST * self.size * reach.exp2();
        self.span = (LARGEST / SMALLEST).ln() + reach * LN_2;
        self.edge = 1.0 - (-SPREAD * self.span).exp();
        self.bank_peak = self.peak * self.louder.sqrt();
        let at = self.load * 2.0;
        self.bank_now = if at < 1.0 {
            self.bank_power[0] + (self.bank_power[1] - self.bank_power[0]) * at
        } else {
            self.bank_power[1] + (self.bank_power[2] - self.bank_power[1]) * (at - 1.0)
        };
    }

    /// A lap begins: a blow to its shape, a band for its slap, its bubbles
    /// set to come as it swells and falls.
    fn lap_begins(&mut self) {
        let strength = self.chance.between(LAP_STRENGTH.0, LAP_STRENGTH.1);
        let swell = self.chance.between(LAP_SWELL.0, LAP_SWELL.1);
        self.lap_keep = (-TICK_S / swell).exp();
        self.lap[0] += strength;
        let band = self.chance.between(SLAP_BAND.0, SLAP_BAND.1) / self.size;
        self.slap.tune(band, SLAP_Q);
        self.slap_top.tune(2.0 * band);
        // White noise of variance 1/3 keeps π·f/(3·Q·rate) of its power
        // through the band, and 0.6 of that through a one-pole at twice the
        // band: the slap is as loud wherever its band is.
        self.slap_gain = self.peak * SLAP * (3.0 * SLAP_Q * RATE as f32 / (0.6 * PI * band)).sqrt();
        // The hollow the water falls into sets the size of its bubbles.
        let span = (LAP_LARGEST / LAP_SMALLEST).ln();
        let edge = 1.0 - (-LAP_SPREAD * span).exp();
        let largest = LAP_SMALLEST * self.size * radius(self.chance.unit(), LAP_SPREAD, edge).exp();
        let count = 3 + (self.chance.unit() * 4.0) as usize;
        for _ in 0..count {
            let part = self.chance.between(GULP_PART.0, GULP_PART.1);
            let swing =
                self.bank_peak * GULP * strength * self.chance.between(GULP_SWING.0, GULP_SWING.1) * part * part.sqrt();
            let gulp = Gulp {
                wait: (self.chance.between(0.3, 3.0) * swell / TICK_S) as u32 + 1,
                radius: largest * part,
                swing: if self.chance.next() < 0.0 { -swing } else { swing },
                rise: self.chance.between(LAP_RISE.0, LAP_RISE.1),
            };
            if swing >= UNHEARD && self.gulps_waiting < self.gulps.len() {
                self.gulps[self.gulps_waiting] = gulp;
                self.gulps_waiting += 1;
            }
        }
        // The mean of two draws: between 0.4 and 1.6 of the mean wait, more
        // often near it.
        let wait = (self.chance.between(0.4, 1.6) + self.chance.between(0.4, 1.6)) / 2.0;
        let rate = self.laps * lapping(self.load);
        self.lap_in = if rate > 0.0 { (wait / (rate * TICK_S)) as u32 + 1 } else { u32::MAX };
        #[cfg(test)]
        self.tally.laps.push(self.tally.ticks);
    }

    /// A bubble of the bank begins, if it can be heard.
    fn bubble(&mut self) {
        let rnd = self.deep + (1.0 - self.deep) * self.bank.unit();
        let square = rnd * rnd;
        let fourth = square * square;
        let depth = fourth * fourth * square;
        // The square root of the radius over the largest.
        let root = (0.5 * (radius(self.bank.unit(), SPREAD, self.edge) - self.span)).exp();
        let swing = self.bank_peak * depth * root * root * root;
        let sign = self.bank.next();
        if swing < UNHEARD {
            return;
        }
        let rise = if depth > RISES_OVER { RISE } else { 0.0 };
        self.pool.add(self.top * root * root, if sign < 0.0 { -swing } else { swing }, rise);
        #[cfg(test)]
        {
            self.tally.bubbles += 1;
            self.tally.largest = self.tally.largest.max(self.top * root * root);
        }
    }

    /// What changes slowly moves on by a tick.
    fn control(&mut self) {
        let ease = if self.started { self.drive_ease } else { 1.0 };
        self.load += (self.load_to - self.load) * ease;
        self.detail += (self.detail_to - self.detail) * ease;
        self.tune();

        let swell = (SWELL * self.sway().clamp(-2.5, 2.5)).exp();

        if self.lap_in != u32::MAX {
            self.lap_in -= 1;
            if self.lap_in == 0 {
                self.lap_begins();
            }
        } else if self.laps * lapping(self.load) > 0.0 {
            // The water stirs again: the next lap is waited for.
            let rate = self.laps * lapping(self.load);
            self.lap_in = (self.chance.between(0.4, 1.6) / (rate * TICK_S)) as u32 + 1;
        }
        let ease_lap = 1.0 - self.lap_keep;
        self.lap[0] *= self.lap_keep;
        self.lap[1] += (self.lap[0] - self.lap[1]) * ease_lap;
        self.lap[2] += (self.lap[1] - self.lap[2]) * ease_lap;
        if self.lap[2] < 1e-18 && self.lap[1] < 1e-18 {
            // Long after a lap, before the numbers get too small to be fast.
            self.lap = [0.0; 3];
        }
        // The slap peaks at 1 a swell's time after the blow, the wash after
        // twice that.
        let slap = self.lap[1] * E;
        let wash = self.lap[2] * E * E / 2.0;
        let mut i = 0;
        while i < self.gulps_waiting {
            self.gulps[i].wait -= 1;
            if self.gulps[i].wait == 0 {
                let gulp = self.gulps[i];
                self.pool.add(gulp.radius, gulp.swing, gulp.rise);
                self.gulps_waiting -= 1;
                self.gulps[i] = self.gulps[self.gulps_waiting];
            } else {
                i += 1;
            }
        }

        self.pool.tick();

        // The bank's bubbles come with the swell, and after the laps.
        let after = if self.lap_mean > 0.0 { AFTER_LAPS } else { 0.0 };
        let mut gust = (1.0 - after) * swell * swell / (2.0 * SWELL * SWELL).exp();
        if after > 0.0 {
            gust += after * wash / self.lap_mean;
        }
        self.pace = self.rate * crowd(self.load) * self.detail * gust * DT;
        self.bubble_in = self.wait(TICK);

        // What the bubbles not made would have held, the bed holds.
        let bed = self.bed_gain * self.louder * swell;
        let bed = (bed * bed + (1.0 - self.detail) * self.bank_now).sqrt();
        let slap = self.slap_gain * self.louder * slap;
        let wash = self.wash_gain * self.louder * wash;
        if !self.started {
            (self.bed.now, self.slap_now.now, self.wash_now.now) = (bed, slap, wash);
            self.started = true;
        }
        self.bed.aim(bed);
        // A lap's noises are made while they can be heard, and no longer.
        self.lapping = slap.max(wash).max(self.slap_now.now).max(self.wash_now.now) > SILENT;
        if self.lapping {
            self.slap_now.aim(slap);
            self.wash_now.aim(wash);
        } else {
            (self.slap_now, self.wash_now) = (Gain::default(), Gain::default());
        }
        #[cfg(test)]
        {
            self.tally.ticks += 1;
            self.tally.fullest = self.tally.fullest.max(self.pool.live);
        }
    }

    /// Samples until the next bubble if it comes within the `left` samples of
    /// this tick; if not, the wait is that much shorter.
    fn wait(&mut self, left: u32) -> u32 {
        if self.gap < self.pace * left as f32 {
            ((self.gap / self.pace).ceil() as u32).clamp(1, left)
        } else {
            self.gap -= self.pace * left as f32;
            u32::MAX
        }
    }

    /// The next samples, between two things happening.
    fn run(&mut self, out: &mut [f32]) {
        for sample in out.iter_mut() {
            let white = self.hiss.next();
            let bed = self.floor[0].above(white);
            let bed = self.floor[1].above(bed);
            let bed = self.ceiling.pass(self.tilt.pass(self.dip.pass(bed)));
            *sample = bed * self.bed.next();
        }
        if self.lapping {
            for sample in out.iter_mut() {
                let white = self.spray.next();
                let slap = self.slap_top.pass(self.slap.level(white));
                let wash = self.wash_ceiling.pass(self.wash_top.pass(self.wash_floor.above(white)));
                *sample += slap * self.slap_now.next() - wash * self.wash_now.next();
            }
        }
        let count = self.pool.live.next_multiple_of(LANES);
        if count > 0 {
            for sample in out.iter_mut() {
                *sample = limit(*sample + self.pool.next(count));
            }
        } else {
            for sample in out.iter_mut() {
                *sample = limit(*sample);
            }
        }
    }
}

impl Voice for Water {
    fn render(&mut self, drive: &Drive, out: &mut [f32]) {
        if drive.load.is_finite() {
            self.load_to = drive.load.clamp(0.0, 1.0);
        }
        if drive.detail.is_finite() {
            self.detail_to = drive.detail.clamp(0.0, 1.0);
        }
        // Ticks and bubbles fall where they fall, whatever the length of
        // `out`: the same samples come out however they are asked for.
        let mut at = 0;
        while at < out.len() {
            if self.tick_left == 0 {
                self.control();
                self.tick_left = TICK;
            }
            let n = (out.len() - at).min(self.tick_left as usize).min(self.bubble_in as usize);
            self.run(&mut out[at..at + n]);
            at += n;
            self.tick_left -= n as u32;
            if self.bubble_in != u32::MAX {
                self.bubble_in -= n as u32;
                if self.bubble_in == 0 {
                    self.bubble();
                    self.gap = -(1.0 - self.bank.unit()).ln();
                    self.bubble_in = if self.tick_left > 0 { self.wait(self.tick_left) } else { u32::MAX };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::Biquad;

    const SECOND: usize = RATE as usize;

    fn water(seed: u32, pairs: &[(&str, f32)]) -> Water {
        let mut params = Params::default();
        for (name, value) in pairs {
            params.set(name, *value);
        }
        Water::new(seed, &params)
    }

    /// The river on an ordinary day, all its bubbles made.
    fn ordinary() -> Drive {
        Drive { load: 0.5, ..Drive::default() }
    }

    /// The next `seconds` of `water`, asked for in blocks of 128.
    fn sound(water: &mut Water, drive: Drive, seconds: usize) -> Vec<f32> {
        let mut out = vec![0.0; SECOND * seconds];
        for block in out.chunks_mut(128) {
            water.render(&drive, block);
        }
        out
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
    }

    fn decibels(ratio: f32) -> f32 {
        20.0 * ratio.max(1e-9).log10()
    }

    /// The level of `samples`, decibels over the nominal level.
    fn level(samples: &[f32]) -> f32 {
        decibels(rms(samples) / NOMINAL_RMS)
    }

    /// The level in the octave around `centre`, dB.
    fn octave(samples: &[f32], centre: f32) -> f32 {
        let mut band = Biquad::default();
        band.band_pass(centre, 1.414);
        decibels(rms(&samples.iter().map(|s| band.pass(*s)).collect::<Vec<_>>()))
    }

    fn largest_step(samples: &[f32]) -> f32 {
        samples.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()))
    }

    /// How many times `samples` go up through zero.
    fn crossings(samples: &[f32]) -> usize {
        samples.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count()
    }

    /// The power of `samples` at `frequency`, frame by frame (Goertzel).
    fn power_at(samples: &[f32], frequency: f32) -> f32 {
        let turn = 2.0 * (TAU * frequency * DT).cos();
        samples
            .chunks(4_800)
            .map(|frame| {
                let (mut s1, mut s2) = (0.0f32, 0.0f32);
                for x in frame {
                    (s1, s2) = (x + turn * s1 - s2, s1);
                }
                s1 * s1 + s2 * s2 - turn * s1 * s2
            })
            .sum()
    }

    #[test]
    fn on_an_ordinary_day_it_sounds_at_the_nominal_level() {
        for seed in 1..=6 {
            let out = sound(&mut water(seed, &[]), ordinary(), 30);
            assert!(level(&out).abs() < 1.0, "seed {seed}: {} dB", level(&out));
            // It swells and laps: ten seconds are not half a minute.
            for part in out.chunks(10 * SECOND) {
                assert!(level(part).abs() < 2.0, "seed {seed}: {} dB over ten seconds", level(part));
            }
        }
    }

    #[test]
    fn the_parameters_change_the_water_and_not_its_level() {
        for pairs in [
            &[("bed", 1.0)][..],
            &[("bed", 0.2)],
            &[("bubbles", 0.0)],
            &[("bubbles", 3_000.0)],
            &[("laps", 0.0)],
            &[("laps", 1.0)],
            &[("size", 0.5)],
            &[("size", 2.0)],
            &[("bubbles", 0.0), ("laps", 0.0), ("bed", 0.1)],
        ] {
            let out = sound(&mut water(4, pairs), ordinary(), 30);
            assert!(level(&out).abs() < 1.5, "{pairs:?}: {} dB", level(&out));
        }
    }

    #[test]
    fn every_sample_is_finite_and_within_bounds_whatever_is_asked() {
        let wild = [("bubbles", 20_000.0), ("bed", 0.0), ("laps", 4.0), ("size", 0.5)];
        let broken = [("bubbles", f32::NAN), ("bed", f32::INFINITY), ("laps", -3.0), ("size", f32::NAN)];
        for pairs in [&[][..], &wild, &broken, &[("size", 2.0), ("bubbles", 5_000.0)]] {
            let mut water = water(9, pairs);
            for load in [0.5, 0.0, 1.0, 7.0, -2.0, f32::NAN, f32::INFINITY] {
                for detail in [1.0, 0.0, f32::NAN, 3.0] {
                    let out = sound(&mut water, Drive { load, detail, speed: 1e9, ..Drive::default() }, 1);
                    assert!(out.iter().all(|s| s.is_finite() && s.abs() <= 1.0), "{pairs:?} {load} {detail}");
                    assert!(rms(&out) > 0.003, "{pairs:?} {load} {detail}: silent");
                    assert!(water.pool.live <= SLOTS);
                }
            }
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_river_and_two_seeds_share_nothing() {
        let (a, b) = (sound(&mut water(6, &[]), ordinary(), 10), sound(&mut water(6, &[]), ordinary(), 10));
        assert!(a == b, "not the same twice");
        // Placed side by side they must not sound as one source: no likeness
        // at any delay up to a millisecond.
        let c = sound(&mut water(7, &[]), ordinary(), 10);
        let n = a.len() - 48;
        for lag in 0..=48 {
            for (x, y) in [(&a, &c), (&c, &a)] {
                let dot = x[..n].iter().zip(&y[lag..]).map(|(p, q)| p * q).sum::<f32>();
                let alike = dot / (rms(&x[..n]) * rms(&y[lag..lag + n]) * n as f32);
                assert!(alike.abs() < 0.1, "lag {lag}: {alike}");
            }
        }
        // Not the same bubbles, not the same laps.
        assert!((level(&a[..SECOND]) - level(&c[..SECOND])).abs() > 0.01);
    }

    #[test]
    fn there_is_no_offset() {
        for load in [0.0, 0.5, 1.0] {
            let out = sound(&mut water(2, &[]), Drive { load, ..Drive::default() }, 20);
            let mean = out.iter().sum::<f32>() / out.len() as f32;
            assert!(mean.abs() < 0.01 * rms(&out), "load {load}: {mean}");
        }
    }

    #[test]
    fn the_length_of_the_blocks_changes_nothing() {
        let whole = sound(&mut water(5, &[]), ordinary(), 3);
        for length in [1, 7, 32, 100, 4_096] {
            let mut water = water(5, &[]);
            let mut out = vec![0.0; whole.len()];
            for block in out.chunks_mut(length) {
                water.render(&ordinary(), block);
            }
            assert!(out == whole, "blocks of {length}");
        }
    }

    #[test]
    fn a_bubble_rings_at_three_over_its_radius_and_dies_as_the_paper_says() {
        // 3 mm: 1 kHz, dying at 43 + 44.3 a second.
        let mut pool = Pool::new();
        pool.add(3.0e-3, 0.5, 0.0);
        let out: Vec<f32> = (0..SECOND / 10).map(|_| pool.next(LANES)).collect();
        // Twenty turns in twenty milliseconds, the last one just ending.
        assert_eq!(crossings(&out[..SECOND / 50 + 10]), 20);
        let peak = |from: usize| out[from..from + 48].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((peak(0) - 0.49).abs() < 0.01, "{}", peak(0));
        let later = (RATE as f32 / decay(1_000.0)) as usize;
        assert!((peak(later) / peak(0) - (-1.0f32).exp()).abs() < 0.02, "{}", peak(later) / peak(0));
        // It starts from rest and holds no offset of its own sign for long.
        assert!(out[0].abs() < 0.07 && out[0] > 0.0);
    }

    #[test]
    fn a_shallow_bubble_rises_in_pitch_and_never_over_the_top() {
        let ring = |radius: f32, rise: f32| {
            let mut pool = Pool::new();
            pool.add(radius, 0.5, rise);
            let mut out = Vec::new();
            for _ in 0..150 {
                pool.swing[0] = 0.5; // kept alive, to hear where its pitch goes
                pool.tick();
                out.extend((0..TICK).map(|_| pool.next(LANES)));
            }
            (out, pool)
        };
        // 3 mm rising at 0.1: 1 kHz (1 + 8.73 t), so 1.22 kHz after 25 ms.
        let (flat, _) = ring(3.0e-3, 0.0);
        let (rising, _) = ring(3.0e-3, RISE);
        let (early, late) = (0..SECOND / 100, SECOND / 50..SECOND * 3 / 100);
        assert!(
            (9..=10).contains(&crossings(&flat[early.clone()])) && (9..=10).contains(&crossings(&flat[late.clone()]))
        );
        assert!((10..=11).contains(&crossings(&rising[early])), "{}", crossings(&rising[..SECOND / 100]));
        assert!((12..=13).contains(&crossings(&rising[late])), "{}", crossings(&rising[SECOND / 50..]));
        // A small bubble rising as fast as a gulp, 15 kHz at first: its turn
        // goes up and stops under 0.45 of the rate.
        let (_, pool) = ring(0.2e-3, 1.0);
        assert!(pool.cos[0] >= (TAU * 0.45).cos() && pool.cos[0] < (TAU * 0.35).cos(), "{}", pool.cos[0]);
        assert!((pool.cos[0].powi(2) + pool.sin[0].powi(2) - 1.0).abs() < 0.01);
    }

    #[test]
    fn the_quietest_bubble_makes_room_when_the_pool_is_full() {
        let mut pool = Pool::new();
        for i in 0..SLOTS {
            pool.add(2.0e-3, 0.01 * (i + 2) as f32, 0.0);
        }
        assert_eq!(pool.live, SLOTS);
        pool.add(1.0e-3, 0.001, 0.0);
        assert!(pool.swing.iter().all(|s| *s >= 0.02), "a quieter one took a place");
        pool.add(1.0e-3, -0.9, 0.0);
        assert_eq!(pool.live, SLOTS);
        assert!(pool.swing.iter().all(|s| *s > 0.02), "the quietest stayed");
        assert_eq!(pool.swing.iter().filter(|s| **s == 0.9).count(), 1);
        // Once they have died out the pool is empty and silent.
        for _ in 0..3_000 {
            pool.tick();
        }
        assert_eq!(pool.live, 0);
        assert_eq!(pool.next(SLOTS), 0.0);
    }

    #[test]
    fn agitated_water_is_louder_with_more_and_larger_bubbles() {
        let heard = |load: f32| {
            let mut water = water(3, &[]);
            let out = sound(&mut water, Drive { load, ..Drive::default() }, 30);
            (level(&out), water.tally.bubbles as f32 / 30.0, water.tally.largest)
        };
        let (still, ordinary, wake) = (heard(0.0), heard(0.5), heard(1.0));
        // −8 dB and +5 dB, give or take what thirty seconds hold.
        assert!((still.0 + 8.0).abs() < 1.5 && ordinary.0.abs() < 1.0 && (wake.0 - 5.0).abs() < 1.5);
        assert!(
            still.1 * 1.5 < ordinary.1 && ordinary.1 * 1.3 < wake.1,
            "{} {} {} a second",
            still.1,
            ordinary.1,
            wake.1
        );
        // Up to 3 mm, 6 mm and 12 mm.
        for (heard, top) in [(still, 3.0e-3), (ordinary, 6.0e-3), (wake, 12.0e-3)] {
            assert!(heard.2 > 0.8 * top && heard.2 <= 1.001 * top, "{} m for {top}", heard.2);
        }
    }

    #[test]
    fn less_detail_makes_fewer_bubbles_and_keeps_level_and_colour() {
        for load in [0.5, 1.0] {
            let heard = |detail: f32| {
                let mut water = water(8, &[]);
                let out = sound(&mut water, Drive { load, detail, ..Drive::default() }, 30);
                (out, water.tally.bubbles as f32)
            };
            let (all, half, none) = (heard(1.0), heard(0.5), heard(0.0));
            assert!((half.1 / all.1 - 0.5).abs() < 0.08, "{} of {}", half.1, all.1);
            assert_eq!(none.1, 0.0);
            for less in [&half.0, &none.0] {
                assert!((level(less) - level(&all.0)).abs() < 2.0, "load {load}: {} dB", level(less) - level(&all.0));
                for centre in [125.0, 250.0, 500.0, 1_000.0, 2_000.0, 4_000.0, 8_000.0] {
                    let moved = octave(less, centre) - octave(&all.0, centre);
                    assert!(moved.abs() < 2.0, "load {load}, {centre} Hz: {moved} dB");
                }
            }
        }
    }

    #[test]
    fn a_change_of_load_or_detail_makes_no_step() {
        // The bed alone, where any step would show, then the whole river.
        for pairs in [&[("bed", 1.0)][..], &[]] {
            let mut water = water(11, pairs);
            let mut out = sound(&mut water, ordinary(), 1);
            for (load, detail) in [(1.0, 1.0), (0.0, 1.0), (0.5, 0.0), (1.0, 1.0), (0.5, 1.0), (0.0, 0.0)] {
                let at = out.len();
                out.extend(sound(&mut water, Drive { load, detail, ..Drive::default() }, 1));
                // The two milliseconds after the change, against the half
                // second before it.
                let (before, after) = (largest_step(&out[at - SECOND / 2..at]), largest_step(&out[at - 1..at + 96]));
                assert!(after <= before, "{pairs:?} to {load} {detail}: {after} after {before}");
            }
        }
    }

    #[test]
    fn a_lap_comes_every_four_to_sixteen_seconds_and_follows_the_water() {
        let mut water = water(12, &[]);
        let out = sound(&mut water, ordinary(), 240);
        let laps = &water.tally.laps;
        assert!((15..=30).contains(&laps.len()), "{} laps in four minutes", laps.len());
        let gaps: Vec<f32> = laps.windows(2).map(|w| (w[1] - w[0]) as f32 * TICK_S).collect();
        assert!(gaps.iter().all(|gap| (3.99..=16.01).contains(gap)), "{gaps:?}");
        // Irregular: short waits and long ones.
        assert!(gaps.iter().any(|gap| *gap < 7.0) && gaps.iter().any(|gap| *gap > 13.0));
        // A lap is heard: the third of a second after it begins is louder
        // than the half second before.
        let heard = laps.iter().map(|tick| *tick as usize * TICK as usize).filter(|at| {
            *at > SECOND
                && at + SECOND < out.len()
                && rms(&out[at + 2_400..at + 16_800]) > rms(&out[at - SECOND / 2..*at])
        });
        assert!(heard.count() * 10 >= laps.len() * 7, "laps are not heard");
        // Still water does not lap; a wake laps twice as often.
        let mut still = water_at(12, 0.0);
        sound(&mut still, Drive { load: 0.0, ..Drive::default() }, 120);
        assert!(still.tally.laps.is_empty(), "{} laps on still water", still.tally.laps.len());
        let mut wake = water_at(12, 1.0);
        sound(&mut wake, Drive { load: 1.0, ..Drive::default() }, 240);
        let ratio = wake.tally.laps.len() as f32 / laps.len() as f32;
        assert!((1.5..=2.6).contains(&ratio), "{} laps in a wake for {}", wake.tally.laps.len(), laps.len());
    }

    /// Water settled at `load` before its first sample.
    fn water_at(seed: u32, load: f32) -> Water {
        let mut water = water(seed, &[]);
        water.load = load;
        water.load_to = load;
        let laps = water.laps * lapping(load);
        water.lap_in =
            if laps > 0.0 { (water.chance.between(0.1, 1.6) / (laps * TICK_S)) as u32 + 1 } else { u32::MAX };
        water
    }

    #[test]
    fn on_an_ordinary_day_the_limiter_is_seldom_at_work() {
        // Samples over the knee, counted as events (those within 10 ms of each other are one): a few a minute at
        // most, over six seeds.
        for seed in 1..=6 {
            let out = sound(&mut water(seed, &[]), ordinary(), 60);
            let over: Vec<usize> = (0..out.len()).filter(|&i| out[i].abs() > KNEE).collect();
            let events = over.windows(2).filter(|w| w[1] - w[0] > SECOND / 100).count() + usize::from(!over.is_empty());
            assert!(events <= 5, "seed {seed}: over the knee {events} times in a minute");
        }
    }

    #[test]
    fn the_bed_swells_slowly_by_a_few_decibels() {
        // The bed alone: its level over quarters of a second, sorted.
        let out = sound(&mut water(13, &[("bed", 1.0)]), ordinary(), 120);
        let mut levels: Vec<f32> = out.chunks(SECOND / 4).map(level).collect();
        // Slowly: a quarter of a second is much like the next one.
        let jumps = levels.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f32>() / (levels.len() - 1) as f32;
        levels.sort_by(f32::total_cmp);
        let spread = levels[levels.len() * 9 / 10] - levels[levels.len() / 10];
        assert!((3.0..=6.0).contains(&spread), "{spread} dB between a quiet moment and a full one");
        assert!(jumps < 1.2, "{jumps} dB from one quarter second to the next");
    }

    #[test]
    fn the_ticks_are_not_heard() {
        // Nothing stands out at the rate of the ticks, 1.5 kHz, nor at twice
        // that, over what is on either side.
        let out = sound(&mut water(14, &[]), ordinary(), 60);
        for tone in [RATE as f32 / TICK as f32, 2.0 * RATE as f32 / TICK as f32] {
            let beside = [-170.0, -90.0, 90.0, 170.0].map(|off| power_at(&out, tone + off)).iter().sum::<f32>() / 4.0;
            let there = power_at(&out, tone);
            assert!(there < 2.0 * beside, "{tone} Hz: {} dB over its sides", 10.0 * (there / beside).log10());
        }
    }

    #[test]
    fn it_is_a_murmur_with_little_at_the_top() {
        let out = sound(&mut water(15, &[]), ordinary(), 20);
        let (low, mid, high, top) =
            (octave(&out, 500.0), octave(&out, 2_000.0), octave(&out, 8_000.0), octave(&out, 16_000.0));
        assert!(low > mid + 3.0 && mid > high + 6.0 && high > top + 6.0, "{low} {mid} {high} {top}");
        // And the limiter is hardly ever at work.
        let over = out.iter().filter(|s| s.abs() > KNEE).count();
        assert!(over * 1_000 < out.len(), "{over} samples over the knee");
    }

    #[test]
    #[ignore = "a measurement: cargo test --offline measure -- --ignored --nocapture"]
    fn measure() {
        for (detail, load) in [(1.0, 0.5), (0.5, 0.5), (0.0, 0.5), (1.0, 1.0), (1.0, 0.0)] {
            let drive = Drive { load, detail, ..Drive::default() };
            // Other work on the machine only ever adds time: the least each
            // quarter of a second took over many runs.
            let mut least = vec![u128::MAX; 40];
            for _ in 0..12 {
                let mut water = water(3, &[]);
                let mut out = vec![0.0f32; SECOND / 4];
                for least in &mut least {
                    let start = std::time::Instant::now();
                    for block in out.chunks_mut(128) {
                        water.render(&drive, block);
                    }
                    *least = (*least).min(start.elapsed().as_nanos());
                }
            }
            let ns = least.iter().sum::<u128>() as f64 / (least.len() * SECOND / 4) as f64;
            println!("detail {detail} load {load}: {ns:.1} ns a sample");
        }
        for pairs in [&[("bubbles", 0.0), ("laps", 0.0)][..], &[]] {
            let mut best = u128::MAX;
            for _ in 0..50 {
                let mut water = water(3, pairs);
                water.detail_to = 0.0;
                let start = std::time::Instant::now();
                for _ in 0..3_000 {
                    water.control();
                }
                best = best.min(start.elapsed().as_nanos());
            }
            println!("{pairs:?}: a tick costs {} ns", best / 3_000);
        }
        for seed in 1..9 {
            let mut water = water(seed, &[]);
            let out = sound(&mut water, ordinary(), 60);
            let over = out.iter().filter(|s| s.abs() > KNEE).count();
            println!(
                "seed {seed}: {:.2} dB, 10 s: {:?}, over the knee {over}, bubbles/s {}, laps {}, fullest {}",
                level(&out),
                out.chunks(10 * SECOND).map(|c| (level(c) * 10.0).round() / 10.0).collect::<Vec<_>>(),
                water.tally.bubbles / 60,
                water.tally.laps.len(),
                water.tally.fullest
            );
        }
        for size in [0.5f32, 1.0, 2.0] {
            let mut water = water(1, &[("size", size)]);
            println!("peak {} bank {:?}", water.peak, water.bank_power.map(|p| p * BED_POWER / size * 100.0));
            let n = SECOND * 120;
            let (mut bed, mut wash) = (0.0f64, 0.0f64);
            for _ in 0..n {
                let b = water.floor[0].above(water.hiss.next());
                let b = water.floor[1].above(b);
                let b = water.ceiling.pass(water.tilt.pass(water.dip.pass(b)));
                let w = water.wash_ceiling.pass(water.wash_top.pass(water.wash_floor.above(water.spray.next())));
                bed += f64::from(b * b);
                wash += f64::from(w * w);
            }
            let (bed, wash) = (bed / n as f64 * f64::from(size), wash / n as f64 * f64::from(size));
            println!("size {size}: bed·size {bed} wash·size {wash}");
        }
    }
}
