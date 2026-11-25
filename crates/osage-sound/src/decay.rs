// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! How long the space round a point rings, found from its faces: rays sent
//! from the point in every direction bounce from face to face, each losing
//! at every face what its material does not send back and in the air what
//! the air takes, until they leave for the open sky. The energy still in
//! flight, summed over the rays, falls with time as the space's sound does;
//! how fast it falls from 5 dB down to 25 dB down, in each band, is the
//! decay the space's own answer is given.
//!
//! A face that scatters sends a ray off in a direction drawn as a rough
//! wall sends sound (Lambert's law), one that does not mirrors it. What a
//! face lets through leaves the space on the other side; a face that hides
//! without sending anything back takes all of it.
//!
//! In a closed room whose faces scatter, this is the statistical decay of
//! C. F. Eyring ("Reverberation time in 'dead' rooms", JASA 1, 1930), air
//! included; in a street, the share of rays that escape upward at each
//! bounce sets it. Sources: H. Kuttruff, *Room Acoustics* (5th ed., 2009),
//! ch. 4 and 9 (ray tracing).

use crate::{
    faces::Faces,
    space::{Vec3, vec3},
    ways::SOUND,
};

/// The three bands a decay is given in, Hz.
pub const CENTRES: [f64; 3] = [250.0, 1_000.0, 4_000.0];

/// The decays found are kept between these, s, as a scene's are.
pub const SHORTEST: f64 = 0.05;
pub const LONGEST: f64 = 10.0;

/// The energy in flight is kept in bins of this many seconds, for as long
/// as the longest decay.
const BIN: f64 = 0.005;
const BINS: usize = (LONGEST / BIN) as usize;

/// A ray that meets nothing within this many metres has left for the sky.
const OPEN: f64 = 300.0;

/// A ray is followed until it has lost this much of its energy in every
/// band (35 dB: the fit stops at 25 dB down), or met this many faces.
const SPENT: f64 = 3.2e-4;
const BOUNCES: usize = 400;

/// The decay of the space round a point, found and kept up to date: rays
/// are added a few at a time, and what older rays found is forgotten
/// little by little, so that it follows a listener who moves.
pub struct Decay {
    /// For each bin, the energy in flight in each band, summed over the
    /// rays, older rays counting less.
    flight: Vec<[f64; 3]>,
    /// The energy the rays started with, summed as the flight is.
    sent: f64,
    /// What the air takes in each band, of the energy, a metre.
    air: [f64; 3],
    state: u64,
}

impl Decay {
    /// `air_db`: what the air takes in each of [`CENTRES`], dB a metre;
    /// `seed`: what the rays' directions are drawn from.
    pub fn new(air_db: [f64; 3], seed: u64) -> Decay {
        Decay {
            flight: vec![[0.0; 3]; BINS],
            sent: 0.0,
            air: air_db.map(|db| 10f64.powf(-db / 10.0)),
            state: seed ^ 0x9e37_79b9_7f4a_7c15,
        }
    }

    /// A number drawn evenly in [0, 1).
    fn draw(&mut self) -> f64 {
        // SplitMix64 (G. Steele, D. Lea and C. Flood, 2014).
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }

    /// A direction drawn evenly over the sphere.
    fn anywhere(&mut self) -> Vec3 {
        let z = 2.0 * self.draw() - 1.0;
        let (s, c) = (std::f64::consts::TAU * self.draw()).sin_cos();
        let r = (1.0 - z * z).max(0.0).sqrt();
        vec3(r * c, r * s, z)
    }

    /// A direction drawn as a rough face sends sound: more of it along
    /// `normal` than askew, by the cosine (Lambert).
    fn rough(&mut self, normal: Vec3) -> Vec3 {
        let (a, b) = (self.draw(), self.draw());
        let (s, c) = (std::f64::consts::TAU * b).sin_cos();
        let r = a.sqrt();
        let side = if normal.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
        let u = normal.cross(side).unit_or(Vec3::Z);
        let v = normal.cross(u);
        (u * (r * c) + v * (r * s) + normal * (1.0 - a).sqrt()).unit_or(normal)
    }

    /// Whether any ray was sent.
    pub fn started(&self) -> bool {
        self.sent > 0.0
    }

    /// Forgets `share` of what the rays so far found.
    pub fn forget(&mut self, share: f64) {
        let keep = 1.0 - share;
        for bin in &mut self.flight {
            for band in bin.iter_mut() {
                *band *= keep;
            }
        }
        self.sent *= keep;
    }

    /// Sends `count` rays from `from` among `faces`.
    pub fn trace(&mut self, from: Vec3, faces: &Faces, count: usize) {
        let mut near: Vec<usize> = Vec::new();
        for _ in 0..count {
            self.sent += 1.0;
            let (mut at, mut toward) = (from, self.anywhere());
            let (mut energy, mut time) = ([1.0f64; 3], 0.0f64);
            for _ in 0..BOUNCES {
                // The nearest face the ray meets: of two as near, the first.
                let end = at + toward * OPEN;
                near.clear();
                faces.along(at, end, 0.0, |k| near.push(k));
                near.sort_unstable();
                near.dedup();
                let mut met: Option<(f64, usize, Vec3)> = None;
                for &k in &near {
                    let face = &faces[k];
                    if !(face.reflects || face.occludes) {
                        continue;
                    }
                    if let Some(p) = face.crossed(at, end) {
                        let distance = at.distance(p);
                        if met.is_none_or(|(best, _, _)| distance < best) {
                            met = Some((distance, k, p));
                        }
                    }
                }
                // Gone for the open sky: nothing more of it is in the space.
                let Some((distance, k, p)) = met else { break };
                // In flight until it meets the face, losing what the air takes.
                let arrive = time + distance / SOUND;
                self.fly(time, arrive, energy, distance);
                for (band, share) in energy.iter_mut().enumerate() {
                    *share *= self.air[band].powf(distance);
                }
                time = arrive;
                if time >= LONGEST {
                    break;
                }
                let face = &faces[k];
                if !face.reflects {
                    break;
                }
                // What the face sends back, mirrored or scattered.
                let material = face.material;
                for (band, share) in energy.iter_mut().enumerate() {
                    *share *= (1.0 - material.absorption[band] - material.transmission[band]).max(0.0);
                }
                if energy.iter().all(|share| *share < SPENT) {
                    break;
                }
                let facing = if toward.dot(face.normal) > 0.0 { -face.normal } else { face.normal };
                toward = if self.draw() < material.scattering {
                    self.rough(facing)
                } else {
                    toward - face.normal * (2.0 * toward.dot(face.normal))
                };
                at = p;
            }
        }
    }

    /// A ray in flight from `t0` to `t1`, s, with `energy` when it set off
    /// and losing what the air takes over `metres`: added to the bins it
    /// spans, each for the part of it the ray spends there.
    fn fly(&mut self, t0: f64, t1: f64, energy: [f64; 3], metres: f64) {
        let last = ((t1 / BIN) as usize).min(BINS - 1);
        let mut bin = (t0 / BIN) as usize;
        while bin <= last && bin < BINS {
            let (from, to) = ((bin as f64 * BIN).max(t0), ((bin + 1) as f64 * BIN).min(t1));
            if to > from {
                // At the middle of its time in the bin.
                let gone = ((from + to) / 2.0 - t0) / (t1 - t0).max(1e-12) * metres;
                let part = (to - from) / BIN;
                for (band, share) in energy.iter().enumerate() {
                    self.flight[bin][band] += share * self.air[band].powf(gone) * part;
                }
            }
            bin += 1;
        }
    }

    /// The decay in each of [`CENTRES`], s, from what the rays found: three
    /// times how long the energy in flight takes from 5 dB to 25 dB below
    /// what it was at first. `None` before any ray was sent. A space the
    /// rays leave at once rings for the shortest decay; one they never
    /// leave and lose nothing in, for the longest.
    pub fn seconds(&self) -> Option<[f64; 3]> {
        if self.sent <= 0.0 {
            return None;
        }
        Some(std::array::from_fn(|band| {
            let first = self.flight[0][band];
            if first <= 0.0 {
                return SHORTEST;
            }
            let down = |bin: usize| 10.0 * (self.flight[bin][band].max(1e-300) / first).log10();
            // Where it crosses a level, between two bins.
            let cross = |level: f64| {
                (1..BINS).find(|&bin| down(bin) <= level).map(|bin| {
                    let (a, b) = (down(bin - 1), down(bin));
                    let share = if a > b { ((a - level) / (a - b)).clamp(0.0, 1.0) } else { 0.0 };
                    (bin as f64 - 1.0 + share) * BIN
                })
            };
            match (cross(-5.0), cross(-25.0)) {
                (Some(t5), Some(t25)) => (3.0 * (t25 - t5)).clamp(SHORTEST, LONGEST),
                (Some(t5), None) => {
                    // It had not fallen so far when the bins end: as fast
                    // as it was falling at their end.
                    let end = (BINS - 1) as f64 * BIN;
                    let fallen = -down(BINS - 1) - 5.0;
                    if fallen > 0.0 { (60.0 * (end - t5) / fallen).clamp(SHORTEST, LONGEST) } else { LONGEST }
                }
                _ => LONGEST,
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{scene::Material, ways::Face};

    /// A box of six faces, `size` m, round the origin, all of `material`.
    fn room(size: [f64; 3], material: Material, ceiling: bool) -> Faces {
        let [x, y, z] = size.map(|s| s / 2.0);
        let corners = |a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]| [a, b, c, d].map(|p| vec3(p[0], p[1], p[2]));
        let mut quads = vec![
            corners([-x, -y, -z], [-x, y, -z], [-x, y, z], [-x, -y, z]),
            corners([x, -y, -z], [x, y, -z], [x, y, z], [x, -y, z]),
            corners([-x, -y, -z], [x, -y, -z], [x, -y, z], [-x, -y, z]),
            corners([-x, -y, -z], [x, -y, -z], [x, y, -z], [-x, y, -z]),
            corners([-x, -y, z], [x, -y, z], [x, y, z], [-x, y, z]),
        ];
        if ceiling {
            quads.push(corners([-x, y, -z], [x, y, -z], [x, y, z], [-x, y, z]));
        }
        let faces: Vec<Face> = quads.into_iter().map(|q| Face::new(q, material, true, true)).collect();
        let moving = vec![false; faces.len()];
        Faces::new(faces, &moving)
    }

    /// The decay of a box of `size` m whose faces take `absorption`, with
    /// what the air takes, `air_db` dB a metre: Eyring's, 0.161·V / (−S·ln(1 − α) + 4·m·V), with
    /// Kuttruff's correction for how much the lengths between two faces vary in a box (their relative
    /// variance γ² ≈ 0.4): −S·ln(1 − α)·(1 + γ²/2·ln(1 − α)) for −S·ln(1 − α) (Room Acoustics, 5.4).
    fn eyring(size: [f64; 3], absorption: f64, air_db: f64) -> f64 {
        let [a, b, c] = size;
        let (volume, surface) = (a * b * c, 2.0 * (a * b + b * c + c * a));
        let m = air_db / (10.0 * std::f64::consts::LOG10_E);
        // 0.161 is 24·ln 10 / c at 343 m/s.
        let k = 24.0 * std::f64::consts::LN_10 / SOUND;
        let lost = (1.0 - absorption).ln();
        k * volume / (-surface * lost * (1.0 + 0.2 * lost) + 4.0 * m * volume)
    }

    #[test]
    fn a_closed_room_that_scatters_rings_as_eyring_says() {
        let size = [12.0, 5.0, 8.0];
        let rough = |absorption: [f64; 3]| Material { absorption, transmission: [0.0; 3], scattering: 1.0 };
        let air = [0.001, 0.005, 0.03];
        let mut decay = Decay::new(air, 7);
        decay.trace(vec3(1.0, 0.3, -0.5), &room(size, rough([0.08, 0.2, 0.4]), true), 4_000);
        let found = decay.seconds().unwrap();
        for (band, absorption) in [0.08, 0.2, 0.4].into_iter().enumerate() {
            let expected = eyring(size, absorption, air[band]);
            assert!((found[band] / expected - 1.0).abs() < 0.05, "band {band}: {} s for {expected} s", found[band]);
        }
    }

    #[test]
    fn without_its_ceiling_a_room_rings_shorter_and_without_faces_the_least() {
        let hard = Material { absorption: [0.1; 3], transmission: [0.0; 3], scattering: 0.5 };
        let size = [10.0, 6.0, 10.0];
        let found = |faces: &Faces| {
            let mut decay = Decay::new([0.0; 3], 3);
            decay.trace(vec3(0.0, -1.0, 0.0), faces, 2_000);
            decay.seconds().unwrap()[1]
        };
        let (closed, open) = (found(&room(size, hard, true)), found(&room(size, hard, false)));
        assert!(open < 0.5 * closed, "{open} s open, {closed} s closed");
        assert_eq!(found(&Faces::new(Vec::new(), &[])), SHORTEST);
        assert!(Decay::new([0.0; 3], 1).seconds().is_none());
    }

    #[test]
    fn what_is_forgotten_counts_less() {
        let hard = Material { absorption: [0.1; 3], transmission: [0.0; 3], scattering: 1.0 };
        let (big, small) = (room([20.0, 10.0, 20.0], hard, true), room([4.0, 3.0, 4.0], hard, true));
        let mut decay = Decay::new([0.0; 3], 5);
        decay.trace(Vec3::ZERO, &big, 2_000);
        let long = decay.seconds().unwrap()[1];
        // The listener steps into a small room: as rays from there come and the old are forgotten, the
        // decay follows.
        for _ in 0..1_500 {
            decay.forget(1.0 / 256.0);
            decay.trace(Vec3::ZERO, &small, 8);
        }
        let short = decay.seconds().unwrap()[1];
        let mut alone = Decay::new([0.0; 3], 5);
        alone.trace(Vec3::ZERO, &small, 2_000);
        let small_only = alone.seconds().unwrap()[1];
        assert!(short < 0.5 * long && (short / small_only - 1.0).abs() < 0.15, "{long} then {short}, {small_only}");
    }
}
