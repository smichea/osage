// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! What two things sound like when they touch: the kind "impact".
//!
//! A blow ([`Hit`](crate::Hit)) is a short push — the softer of the two
//! things sets how long it lasts, and a longer push is a duller sound — that
//! sets ringing what was struck: a few modes, each a decaying sine, whose
//! frequencies say what shape it is and whose decay says what it is made of
//! (steel rings for a second, asphalt and rubber for a few hundredths). On
//! top, for rough or soft things, a breath of noise: the scuff of a sole, the
//! slap of a tyre.
//!
//! - **the push:** a raised cosine of 0.3 ms (hard on hard) to 6 ms (a sole,
//!   a tyre), as strong as the blow;
//! - **the modes:** two-multiply resonators, up to six, at the ratios of a
//!   slab (a thud: 1, 1.6, 2.3), a plate (1, 2.3, 3.9, 5.2, 7.1, 8.6) or a bar
//!   (1, 2.76, 5.4, 8.9), each lasting `ring` seconds at the lowest and
//!   shorter in proportion higher up (a material's own damping: the decay
//!   rate grows with the frequency), a little different at each blow, as a
//!   thing struck elsewhere rings differently;
//! - **the scuff:** noise through a band at 1 to 4 kHz, 20 to 50 ms;
//! - **the grain:** for what is brittle — stone under a hammer — a crackle
//!   of small breaks for a few hundredths of a second after the blow.
//!
//! Parameters: `tone` (Hz, the lowest mode), `ring` (s, how long it lasts),
//! `shape` (0 a slab, 1 a plate, 2 a bar), `hardness` (0…1, of the softer of
//! the two), `scuff` (0…1), `grain` (0…1). Up to eight blows sound at once.
//! Peak about 0.4 for a blow of strength 1 (0.34 to 0.43 over seeds).
//!
//! Both things sound: a hammer on an anvil is the anvil ringing and the
//! hammer's head ticking, a clog on a paving stone is the stone's thud and
//! the clog's hollow clop. Each is a voice of its own, made from its own
//! [`Matter`] and from how hard the other is ([`Matter::touched_by`]): the
//! same hammer ticks less on limestone than on steel, because the stone
//! gives way and the push lasts longer.
//!
//! # Assumptions
//!
//! Mode ratios are those of textbook free plates and bars, not of any object
//! of the bridge; the push is a raised cosine rather than Hertz's contact
//! force; a material's damping is one number (decay rate proportional to
//! frequency), after the observation that the ear tells materials by how the
//! decay changes with pitch. Sources: K. van den Doel and D. Pai, "The sounds
//! of physical shapes" (Presence 7(4), 1998); R. Klatzky, D. Pai and E.
//! Krotkov, "Perception of material from contact sounds" (Presence 9(4),
//! 2000).

use std::f32::consts::{PI, TAU};

use crate::{
    Drive, Params, Voice,
    dsp::{Band, DT, Noise, soft},
};

pub const ABOUT: &str = "Two things touching: a short push that sets ringing what was struck, and a scuff of noise";
pub const PARAMS: &[(&str, f32)] =
    &[("tone", 160.0), ("ring", 0.06), ("shape", 0.0), ("hardness", 0.4), ("scuff", 0.4), ("grain", 0.0)];

/// How a thing sounds when touched: the sound side of what it is made of
/// and of its shape, beside the friction and the bounce its collision
/// material has.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matter {
    /// Its lowest mode, Hz: its size and stiffness. 0 for what does not
    /// ring at all (a sole, a tyre: it only shapes the push).
    pub tone: f32,
    /// How long that mode lasts, s: its damping. Steel a second, stone and
    /// wood a few hundredths.
    pub ring: f32,
    /// 0 a slab (a thud), 1 a plate, 2 a bar.
    pub shape: f32,
    /// 0…1: of two things touching, the softer sets how long the push lasts.
    pub hardness: f32,
    /// 0…1: how much it rubs (rough, or soft and dragging).
    pub scuff: f32,
    /// 0…1: how much it crumbles under a blow.
    pub grain: f32,
}

impl Matter {
    /// An anvil: a block of steel that rings high and long.
    pub const ANVIL: Matter = Matter { tone: 1_250.0, ring: 1.6, shape: 2.0, hardness: 1.0, scuff: 0.0, grain: 0.0 };
    /// The head of a hammer: small, hard, held by its handle.
    pub const HAMMER: Matter = Matter { tone: 2_900.0, ring: 0.1, shape: 2.0, hardness: 1.0, scuff: 0.0, grain: 0.0 };
    /// A block of limestone: hard, but it gives way and crumbles.
    pub const LIMESTONE: Matter =
        Matter { tone: 540.0, ring: 0.035, shape: 0.0, hardness: 0.7, scuff: 0.15, grain: 0.6 };
    /// A granite paving stone in its bed of sand.
    pub const PAVING: Matter = Matter { tone: 300.0, ring: 0.03, shape: 0.0, hardness: 0.85, scuff: 0.25, grain: 0.05 };
    /// A leather sole: it does not ring, it lengthens the push and drags.
    pub const LEATHER: Matter = Matter { tone: 0.0, ring: 0.0, shape: 0.0, hardness: 0.3, scuff: 0.5, grain: 0.0 };
    /// A wooden clog: a hollow body that clops.
    pub const CLOG: Matter = Matter { tone: 520.0, ring: 0.14, shape: 1.0, hardness: 0.6, scuff: 0.05, grain: 0.0 };

    /// Whether it makes a sound of its own when touched.
    pub fn rings(&self) -> bool {
        self.tone > 0.0 && self.ring > 0.0
    }

    /// What the voice of this thing is given when `other` touches it: its
    /// own modes, the push of the softer of the two, the rub of the rougher.
    pub fn touched_by(&self, other: &Matter) -> Params {
        Params::default()
            .with("tone", self.tone)
            .with("ring", self.ring)
            .with("shape", self.shape)
            .with("hardness", self.hardness.min(other.hardness))
            .with("scuff", self.scuff.max(other.scuff))
            .with("grain", self.grain)
    }
}

const SLAB: &[f32] = &[1.0, 1.6, 2.3];
const PLATE: &[f32] = &[1.0, 2.3, 3.9, 5.2, 7.1, 8.6];
const BAR: &[f32] = &[1.0, 2.76, 5.4, 8.9];
/// Blows sounding at once.
const BLOWS: usize = 8;
const MODES: usize = 6;

/// A mode ringing: `y[n] = c1·y[n−1] − c2·y[n−2]`, fed by the push.
#[derive(Clone, Copy, Debug, Default)]
struct Mode {
    y1: f32,
    y2: f32,
    c1: f32,
    c2: f32,
    gain: f32,
}

/// One blow under way.
#[derive(Clone, Debug, Default)]
struct Blow {
    modes: [Mode; MODES],
    count: usize,
    /// Samples since it began, and how many the push lasts.
    age: u32,
    push: u32,
    strength: f32,
    /// The scuff: how much is left, and how much stays after a sample.
    scuff: f32,
    fade: f32,
    /// The grain: how much is left of it.
    grain: f32,
    /// Samples until it begins (a blow part-way through a block).
    wait: u32,
    /// Samples until it is taken as over.
    left: u32,
}

/// Two things touching.
pub struct Impact {
    noise: Noise,
    blows: Vec<Blow>,
    /// What does not move the air: a mode struck from rest swings more to
    /// one side than to the other, and a sound has no such lasting push.
    still: f32,
    grit: Band,
    crack: Band,
    tone: f32,
    ring: f32,
    ratios: &'static [f32],
    hardness: f32,
    scuff: f32,
    grain: f32,
}

/// How long a scuff takes to fall by a factor of e, s: 7 ms for steel, 14
/// for a sole — a rub lasts no longer than the push and a little after. (At
/// 12 + 30 ms per unit of softness, a hiss outlasted the ring of what was
/// struck and sounded like a sheet.)
fn scuff_seconds(hardness: f32) -> f32 {
    0.006 + 0.012 * (1.0 - hardness)
}

impl Impact {
    pub fn new(seed: u32, params: &Params) -> Self {
        let get = |name: &str| params.get(name, PARAMS.iter().find(|(n, _)| *n == name).map_or(0.0, |(_, d)| *d));
        let ratios = match get("shape").round() as i32 {
            1 => PLATE,
            2 => BAR,
            _ => SLAB,
        };
        let hardness = get("hardness").clamp(0.0, 1.0);
        Self {
            noise: Noise::new(seed),
            blows: Vec::with_capacity(BLOWS),
            still: 0.0,
            // Brighter grit for harder things.
            grit: Band::at(1_200.0 + 2_600.0 * hardness, 0.9),
            crack: Band::at(3_800.0, 1.3),
            tone: get("tone").clamp(20.0, 4_000.0),
            // None: what does not ring only shapes the push and rubs.
            ring: get("ring").clamp(0.0, 4.0),
            ratios,
            hardness,
            scuff: get("scuff").clamp(0.0, 1.0),
            grain: get("grain").clamp(0.0, 1.0),
        }
    }

    fn strike(&mut self, wait: usize, strength: f32) {
        // Not a number is no blow, nor is nothing.
        if strength.is_nan() || strength <= 0.0 {
            return;
        }
        let strength = strength.min(1.5);
        // The softer thing gives way for longer: 6 ms for a sole, 0.3 ms for
        // steel on steel. A harder blow is a little shorter.
        let seconds = 0.006 * (0.05f32).powf(self.hardness) / (0.7 + 0.3 * strength);
        let push = ((seconds / DT) as u32).max(3);
        let mut blow = Blow {
            count: if self.ring >= 0.005 { self.ratios.len().min(MODES) } else { 0 },
            push,
            strength,
            scuff: self.scuff * strength * 0.5,
            grain: self.grain * strength,
            fade: (-DT / scuff_seconds(self.hardness)).exp(),
            wait: wait as u32,
            ..Blow::default()
        };
        // The push itself says how much each mode takes: one slower than the
        // push takes all of it, one faster little (a raised cosine's spectrum
        // falls past the inverse of its length).
        let mut longest = 0.0f32;
        let mut shares = 0.0f32;
        for (i, ratio) in self.ratios.iter().take(MODES).enumerate() {
            // Struck elsewhere, it rings a little differently.
            let frequency = (self.tone * ratio * (1.0 + 0.012 * self.noise.next())).min(0.45 / DT);
            // A material's damping: the decay rate grows with the frequency.
            let seconds = (self.ring.max(0.005) * self.tone / frequency).max(0.004);
            // −60 dB after `seconds`.
            let r = (-6.91 * DT / seconds).exp();
            let share = (0.55 + 0.45 * self.noise.unit()) / (1.0 + i as f32 * 0.55);
            let turn = TAU * frequency * DT;
            // A resonator struck once swings to 1/sin(turn): `share` is then
            // the mode's own swing.
            blow.modes[i] = Mode { c1: 2.0 * r * turn.cos(), c2: r * r, gain: share * turn.sin(), ..Mode::default() };
            shares += share;
            longest = longest.max(seconds);
        }
        // All modes swinging together reach 0.6 for a blow of strength 1.
        for mode in &mut blow.modes[..blow.count] {
            mode.gain *= 0.6 / shares.max(1e-6);
        }
        // It lasts until its modes, its scuff and its crackle have all died
        // by 60 dB: a sole's scuff outlasts what rings.
        let scuffing = if blow.scuff > 0.0 { 8.0 * scuff_seconds(self.hardness) } else { 0.0 };
        let crackling = if blow.grain > 0.0 { 6.91 * DT / (1.0 - 0.999_17) } else { 0.0 };
        let lasts = (longest.max(0.06) * 1.15).max(scuffing).max(crackling);
        blow.left = wait as u32 + push + (lasts / DT) as u32;
        if self.blows.len() == BLOWS {
            // The one nearest its end makes room.
            let oldest = (0..BLOWS).min_by_key(|&i| self.blows[i].left).unwrap_or(0);
            self.blows.swap_remove(oldest);
        }
        self.blows.push(blow);
    }
}

impl Voice for Impact {
    fn render(&mut self, drive: &Drive, out: &mut [f32]) {
        if let Some(hit) = drive.hit {
            self.strike(hit.at.min(out.len().saturating_sub(1)), hit.strength);
        }
        if self.blows.is_empty() && self.still == 0.0 {
            // Nothing sounding, nothing to make.
            out.fill(0.0);
            return;
        }
        for sample in out.iter_mut() {
            let mut sum = 0.0f32;
            let mut grit = 0.0f32;
            let mut grain = 0.0f32;
            for blow in &mut self.blows {
                if blow.wait > 0 {
                    blow.wait -= 1;
                    blow.left = blow.left.saturating_sub(1);
                    continue;
                }
                // The push: a raised cosine, its area the blow's strength.
                let force = if blow.age < blow.push {
                    blow.strength * (1.0 - (TAU * (blow.age as f32 + 0.5) / blow.push as f32).cos()) / blow.push as f32
                } else {
                    0.0
                };
                for mode in &mut blow.modes[..blow.count] {
                    let y = mode.c1 * mode.y1 - mode.c2 * mode.y2 + force;
                    mode.y2 = mode.y1;
                    mode.y1 = y;
                    sum += y * mode.gain;
                }
                // The scuff swells with the push and dies after it.
                if blow.age < blow.push {
                    grit += blow.scuff * (PI * blow.age as f32 / blow.push as f32).sin();
                } else {
                    blow.scuff *= blow.fade;
                    grit += blow.scuff;
                    // What breaks, breaks just after the push: 25 ms or so.
                    grain += blow.grain;
                    blow.grain *= 0.999_17;
                }
                blow.age += 1;
                blow.left = blow.left.saturating_sub(1);
            }
            // Not run on silence: left to itself the band's state would sink
            // into numbers so small that a processor handles them a hundred
            // times slower.
            let hiss = if grit > 1e-5 { self.grit.level(self.noise.next()) * grit } else { 0.0 };
            // Small breaks, a thousand or so a second, each a click.
            let crackle = if grain > 1e-4 {
                let click = if self.noise.unit() < 0.022 { self.noise.next() * 6.0 } else { 0.0 };
                self.crack.level(click) * grain
            } else {
                0.0
            };
            // Less its slow part: what stays under 30 Hz or so.
            self.still += (sum - self.still) * 0.004;
            *sample = soft(sum - self.still + hiss + crackle);
        }
        self.blows.retain(|blow| blow.left > 0);
        if self.blows.is_empty() && self.still.abs() < 1e-6 {
            // Nothing sounding: left at nothing, not at a number sinking towards it.
            self.still = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Hit, RATE};

    /// A blow of `strength` then two seconds, with the kind's parameters set
    /// by `pairs`.
    fn struck(pairs: &[(&str, f32)], strength: f32, seed: u32) -> Vec<f32> {
        let mut params = Params::default();
        for (name, value) in pairs {
            params.set(name, *value);
        }
        let mut voice = Impact::new(seed, &params);
        let mut out = vec![0.0; RATE as usize * 2];
        for (i, block) in out.chunks_mut(128).enumerate() {
            let hit = (i == 2).then_some(Hit { at: 40, strength });
            voice.render(&Drive { hit, ..Drive::default() }, block);
        }
        out
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
    }

    /// How long after the blow the sound stays within 40 dB of its peak, s.
    fn lasts(samples: &[f32]) -> f32 {
        let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let last = samples.iter().rposition(|s| s.abs() > peak * 0.01).unwrap_or(0);
        (last as f32 - 296.0) / RATE as f32
    }

    const STEEL: &[(&str, f32)] = &[("tone", 210.0), ("ring", 0.9), ("shape", 1.0), ("hardness", 0.9), ("scuff", 0.05)];
    const ASPHALT: &[(&str, f32)] =
        &[("tone", 130.0), ("ring", 0.05), ("shape", 0.0), ("hardness", 0.3), ("scuff", 0.6)];

    #[test]
    fn a_soft_sole_s_scuff_dies_out_before_the_blow_is_let_go() {
        // Leather on paving: nothing rings, the scuff is all. Its last sound, before the blow is dropped, is 60 dB
        // under its peak: no cut.
        let out = struck(&[("tone", 0.0), ("ring", 0.0), ("hardness", 0.3), ("scuff", 0.5)], 1.0, 2);
        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let end = out.iter().rposition(|s| *s != 0.0).unwrap();
        let tail = out[end.saturating_sub(48)..=end].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(tail < peak * 1e-3, "{} dB at its end, {} ms", 20.0 * (tail / peak).log10(), end as f32 / 48.0);
    }

    #[test]
    fn a_blow_that_is_not_a_number_is_no_blow() {
        // Not a number, then two good blows: they are heard, and every sample is a number.
        let mut voice = Impact::new(3, &Params::default());
        let mut out = vec![0.0; RATE as usize];
        for (i, block) in out.chunks_mut(128).enumerate() {
            let hit = match i {
                0 => Some(Hit { at: 0, strength: f32::NAN }),
                100 | 200 => Some(Hit { at: 3, strength: 1.0 }),
                _ => None,
            };
            voice.render(&Drive { hit, ..Drive::default() }, block);
        }
        assert!(out.iter().all(|s| s.is_finite()));
        assert!(out[..100 * 128].iter().all(|s| *s == 0.0) && rms(&out[100 * 128..]) > 0.01);
    }

    #[test]
    fn nothing_sounds_until_something_is_struck_and_then_it_dies() {
        let out = struck(STEEL, 1.0, 3);
        // Two blocks and forty samples of silence before the blow.
        assert!(out[..2 * 128 + 40].iter().all(|s| *s == 0.0));
        assert!(out[2 * 128 + 40..2 * 128 + 400].iter().any(|s| s.abs() > 0.05));
        assert!(out.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        assert_eq!(rms(&out[RATE as usize * 3 / 2..]), 0.0, "it never ends");
        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((0.25..=0.9).contains(&peak), "peak {peak}");
    }

    #[test]
    fn steel_rings_and_asphalt_thuds() {
        let (steel, asphalt) = (struck(STEEL, 1.0, 3), struck(ASPHALT, 1.0, 3));
        assert!(lasts(&steel) > 0.3, "steel lasts {}", lasts(&steel));
        assert!(lasts(&asphalt) < 0.12, "asphalt lasts {}", lasts(&asphalt));
        // The softer of the two things sets the push: a long push leaves the
        // high modes alone, so the same plate struck with something soft
        // changes sign less often.
        let plate = |hardness: f32| {
            struck(&[("tone", 200.0), ("ring", 0.5), ("shape", 1.0), ("hardness", hardness), ("scuff", 0.0)], 1.0, 3)
        };
        // How fast it changes for how loud it is, over its first 20 ms.
        let bright = |s: &[f32]| {
            let s = &s[296..296 + 960];
            s.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum::<f32>() / s.iter().map(|x| x * x).sum::<f32>()
        };
        assert!(bright(&plate(1.0)) > 3.0 * bright(&plate(0.0)), "{} {}", bright(&plate(1.0)), bright(&plate(0.0)));
        let peak = asphalt.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((0.2..=0.9).contains(&peak), "peak {peak}");
    }

    #[test]
    fn a_stronger_blow_is_louder_and_none_is_silent() {
        let (soft_blow, hard_blow) = (struck(ASPHALT, 0.3, 5), struck(ASPHALT, 1.0, 5));
        assert!(rms(&hard_blow) > 2.0 * rms(&soft_blow));
        assert_eq!(rms(&struck(ASPHALT, 0.0, 5)), 0.0);
    }

    #[test]
    fn the_same_twice_and_another_with_another_seed() {
        assert!(struck(STEEL, 0.8, 9) == struck(STEEL, 0.8, 9));
        assert!(struck(STEEL, 0.8, 9) != struck(STEEL, 0.8, 10));
    }

    /// One thing struck by another: what each of the two sounds like, added.
    fn contact(struck: &Matter, striker: &Matter) -> Vec<f32> {
        let mut voices = vec![Impact::new(3, &struck.touched_by(striker))];
        if striker.rings() {
            voices.push(Impact::new(4, &striker.touched_by(struck)));
        }
        let mut out = vec![0.0; RATE as usize * 3];
        let mut one = [0.0f32; 128];
        for (i, block) in out.chunks_mut(128).enumerate() {
            for voice in &mut voices {
                let hit = (i == 2).then_some(Hit { at: 40, strength: 1.0 });
                voice.render(&Drive { hit, ..Drive::default() }, &mut one[..block.len()]);
                block.iter_mut().zip(&one).for_each(|(sum, s)| *sum += s);
            }
        }
        out
    }

    /// Where a sound's weight is, Hz: the root of the mean square frequency.
    fn weight(samples: &[f32]) -> f32 {
        let s = &samples[296..296 + 4_800];
        let moved = s.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum::<f32>();
        (moved / s.iter().map(|x| x * x).sum::<f32>()).sqrt() * RATE as f32 / TAU
    }

    #[test]
    fn a_contact_tells_what_both_things_are_made_of() {
        let anvil = contact(&Matter::ANVIL, &Matter::HAMMER);
        let limestone = contact(&Matter::LIMESTONE, &Matter::HAMMER);
        let leather = contact(&Matter::PAVING, &Matter::LEATHER);
        let clog = contact(&Matter::PAVING, &Matter::CLOG);
        // A hammer on an anvil rings for the best part of a second; on limestone it is over in a tenth or so.
        assert!(lasts(&anvil) > 0.7, "the anvil lasts {}", lasts(&anvil));
        assert!(lasts(&limestone) < 0.2, "the limestone lasts {}", lasts(&limestone));
        // Leather on a paving stone is a thud and a rub; a clog on the same stone has a note of its own,
        // the clog's: far more of its sound is at 520 Hz.
        let note = |sound: &[f32]| {
            let s = &sound[296..296 + 4_800];
            let (mut re, mut im) = (0.0f32, 0.0f32);
            for (i, x) in s.iter().enumerate() {
                let turn = TAU * 520.0 * i as f32 * DT;
                re += x * turn.cos();
                im += x * turn.sin();
            }
            (re * re + im * im) / s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32
        };
        assert!(note(&clog) > 4.0 * note(&leather), "the clog's note {} and leather's {}", note(&clog), note(&leather));
        // The stone crumbles: its sound is noise as well, higher than its own thud.
        let whole = Matter { grain: 0.0, ..Matter::LIMESTONE };
        assert!(weight(&limestone) > 1.2 * weight(&contact(&whole, &Matter::HAMMER)));
        // The same hammer ticks less on stone, which gives way, than on steel.
        let tick = |on: &Matter| {
            let mut voice = Impact::new(4, &Matter::HAMMER.touched_by(on));
            let mut out = vec![0.0; 4_800];
            for (i, block) in out.chunks_mut(128).enumerate() {
                let hit = (i == 0).then_some(Hit { at: 0, strength: 1.0 });
                voice.render(&Drive { hit, ..Drive::default() }, block);
            }
            rms(&out)
        };
        assert!(
            tick(&Matter::ANVIL) > 1.5 * tick(&Matter::LIMESTONE),
            "{} {}",
            tick(&Matter::ANVIL),
            tick(&Matter::LIMESTONE)
        );
        for sound in [&anvil, &limestone, &leather, &clog] {
            assert!(sound.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
            assert!(sound.iter().fold(0.0f32, |m, s| m.max(s.abs())) > 0.05);
        }
        // What they measure, for the record.
        for (name, sound) in [
            ("hammer on anvil", &anvil),
            ("hammer on limestone", &limestone),
            ("leather on paving", &leather),
            ("clog on paving", &clog),
        ] {
            println!("{name:<20} lasts {:.3} s, weight at {:.0} Hz", lasts(sound), weight(sound));
        }
    }

    #[test]
    fn many_blows_at_once_stay_within_bounds() {
        let mut voice = Impact::new(1, &Params::default().with("ring", 1.5).with("shape", 1.0).with("hardness", 1.0));
        let mut out = vec![0.0; RATE as usize];
        for (i, block) in out.chunks_mut(128).enumerate() {
            let hit = (i % 3 == 0).then_some(Hit { at: 127, strength: 1.5 });
            voice.render(&Drive { hit, ..Drive::default() }, block);
        }
        assert!(out.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        assert!(voice.blows.len() <= BLOWS);
    }
}
