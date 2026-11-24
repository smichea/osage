// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Far sound as a field: what arrives at the centre of a cube from beyond a
//! distance, kept by direction in a few channels (ambisonics) — made once
//! for the cube, whoever listens in it — and turned to each listener's head
//! and decoded to two ears or two loudspeakers.
//!
//! - **The channels** are real spherical harmonics in the ACN order with
//!   SN3D normalisation (the AmbiX convention): 4 at the first order (W, Y,
//!   Z, X), 9 at the second. A sound from a direction is written into each
//!   channel times that channel's harmonic in that direction. The field's
//!   axes are the world's, not a head's: ahead is −Z, left is −X, up is +Y.
//! - **The boundary** between near and far is a distance from the cube's
//!   centre, the same for every listener in the cube: an emitter beyond
//!   it is far, nearer it is near, and across its last 15 % it is both, at
//!   equal power, so that nothing steps as it crosses ([`far_share`]).
//! - **The decoder** samples the field at virtual loudspeakers evenly
//!   spread round the head — an octahedron at the first order, an
//!   icosahedron at the second (spherical designs of degree 3 and 5) — with
//!   the weights that keep the energy's direction sharpest (max-rE), and
//!   hears each loudspeaker through the engine's own ears ([`crate::ears`]).
//!   Turning the head turns where the loudspeakers sample the field.
//!
//! Sources: M. Gerzon, "Periphony: with-height sound reproduction" (JAES
//! 21(1), 1973); F. Zotter and M. Frank, *Ambisonics* (Springer, 2019, open
//! access), chapters 2 to 4 for the harmonics, the max-rE weights and
//! sampling decoders.

use crate::{
    scene::Head,
    space::{Vec3, vec3},
};

/// The most channels a field has: the second order's.
pub const MOST: usize = 9;

/// How many channels a field of `order` has.
pub fn channels(order: usize) -> usize {
    (order + 1) * (order + 1)
}

/// The field's own axes for a direction given in the world: ahead (−Z), left
/// (−X), up (+Y).
fn axes(world: Vec3) -> (f64, f64, f64) {
    (-world.z, -world.x, world.y)
}

/// The harmonics of a direction (in the world, need not be a unit), ACN
/// order, SN3D, up to `order`; the rest are 0.
pub fn harmonics(direction: Vec3, order: usize) -> [f64; MOST] {
    let unit = direction.unit_or(vec3(0.0, 0.0, -1.0));
    let (x, y, z) = axes(unit);
    let mut h = [0.0; MOST];
    h[0] = 1.0;
    if order >= 1 {
        (h[1], h[2], h[3]) = (y, z, x);
    }
    if order >= 2 {
        let r3 = 3f64.sqrt();
        h[4] = r3 * x * y;
        h[5] = r3 * y * z;
        h[6] = 0.5 * (3.0 * z * z - 1.0);
        h[7] = r3 * x * z;
        h[8] = 0.5 * r3 * (x * x - y * y);
    }
    h
}

/// The order of each channel, ACN.
fn degree(channel: usize) -> usize {
    match channel {
        0 => 0,
        1..=3 => 1,
        _ => 2,
    }
}

/// How much of an emitter `distance` from the cube's centre is far, 0…1,
/// for a boundary `beyond` metres away: none nearer than 85 % of it, all
/// beyond it, a smooth step between. The near part is heard by
/// `√(1 − share)`, the far part by `√share`: the two are not the same
/// sound, so they cross at equal power.
pub fn far_share(distance: f64, beyond: f64) -> f64 {
    let x = ((distance - 0.85 * beyond) / (0.15 * beyond)).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Where far sound is gathered and how finely.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Far {
    /// The cube's centre, in the world, m: where the field is made.
    pub centre: Vec3,
    /// The boundary, m from the centre: beyond it an emitter is far.
    pub beyond: f64,
    /// 1 or 2.
    pub order: usize,
}

/// The virtual loudspeakers the field is heard through, in the head's frame:
/// across (right), up, ahead.
fn layout(order: usize) -> Vec<[f64; 3]> {
    if order <= 1 {
        // The octahedron: a design of degree 3.
        vec![[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]]
    } else {
        // The icosahedron: a design of degree 5.
        let g = (1.0 + 5f64.sqrt()) / 2.0;
        let n = (1.0 + g * g).sqrt();
        let mut points = Vec::new();
        for a in [-1.0, 1.0] {
            for b in [-g, g] {
                points.push([0.0, a / n, b / n]);
                points.push([a / n, b / n, 0.0]);
                points.push([b / n, 0.0, a / n]);
            }
        }
        points
    }
}

/// The max-rE weight of each order, for a field of `order` heard in three
/// dimensions: Legendre polynomials at cos(137.9° / (order + 1.51)).
fn weights(order: usize) -> [f64; 3] {
    let x = (137.9f64.to_radians() / (order as f64 + 1.51)).cos();
    [1.0, x, 0.5 * (3.0 * x * x - 1.0)]
}

/// A field heard by one head: the gains that take it to each virtual
/// loudspeaker, for the way the head is turned.
#[derive(Clone, Debug)]
pub struct Decoder {
    pub order: usize,
    /// Each loudspeaker's direction in the head's frame.
    pub speakers: Vec<[f64; 3]>,
    /// For each loudspeaker, the gain of each channel above
    /// [`CROSSOVER`] (max-rE, keeping the energy) and below it (the basic
    /// decoder, keeping the amplitude).
    gains: Vec<[f32; MOST]>,
    low: Vec<[f32; MOST]>,
}

/// Where the decoder's two bands part, Hz. Below, the loudspeakers' feeds
/// reach each ear in step and add as amplitudes: the basic decoder, whose
/// feeds sum to the sound itself, keeps the level and the time between the
/// ears. Above, they add as powers: the max-rE decoder, brought to keep the
/// energy, keeps the level and sharpens the direction. (Zotter and Frank,
/// §4.5: decoders of two bands.)
pub const CROSSOVER: f64 = 700.0;

impl Decoder {
    pub fn new(order: usize) -> Decoder {
        let speakers = layout(order);
        let gains = vec![[0.0; MOST]; speakers.len()];
        let low = gains.clone();
        Decoder { order: order.clamp(1, 2), speakers, gains, low }
    }

    /// The gains for a head turned as `head` is: a sampling decoder, each
    /// channel weighed by (2n + 1) (SN3D to N3D) over the number of
    /// loudspeakers. Above the crossover each is also weighed by its order's
    /// max-rE weight and the whole brought to keep the energy of a sound
    /// from any direction; below, the feeds of a sound sum to the sound.
    pub fn turn(&mut self, head: &Head) {
        let w = weights(self.order);
        let count = self.speakers.len() as f64;
        let n = channels(self.order);
        // The energy the loudspeakers of a design are fed of a sound from
        // anywhere, unscaled: Σ (2n + 1)·wₙ² over their number.
        let energy: f64 = (0..n).map(|c| w[degree(c)].powi(2)).sum::<f64>() / count;
        let scale = 1.0 / energy.sqrt();
        for (s, speaker) in self.speakers.iter().enumerate() {
            let world = head.right * speaker[0] + head.up * speaker[1] + head.forward * speaker[2];
            let h = harmonics(world, self.order);
            for c in 0..MOST {
                let basic = if c < n { (2 * degree(c) + 1) as f64 * h[c] / count } else { 0.0 };
                self.low[s][c] = basic as f32;
                self.gains[s][c] = (scale * w[degree(c)] * basic) as f32;
            }
        }
    }

    /// The gains of each loudspeaker above the crossover, as last turned.
    pub fn gains(&self) -> &[[f32; MOST]] {
        &self.gains
    }

    /// The gains of each loudspeaker below the crossover.
    pub fn low_gains(&self) -> &[[f32; MOST]] {
        &self.low
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head() -> Head {
        Head { at: Vec3::ZERO, forward: vec3(0.0, 0.0, -1.0), right: Vec3::X, up: Vec3::Y }
    }

    #[test]
    fn the_harmonics_are_ambix_s() {
        // Ahead: W and X; left: W and Y; up: W, Z and the zonal second order at 1.
        let ahead = harmonics(vec3(0.0, 0.0, -3.0), 2);
        assert_eq!((ahead[0], ahead[1], ahead[2], ahead[3]), (1.0, 0.0, 0.0, 1.0));
        let left = harmonics(vec3(-1.0, 0.0, 0.0), 1);
        assert_eq!((left[1], left[3]), (1.0, 0.0));
        let up = harmonics(vec3(0.0, 2.0, 0.0), 2);
        assert!((up[2] - 1.0).abs() < 1e-12 && (up[6] - 1.0).abs() < 1e-12);
        // SN3D: the mean square of each harmonic over the sphere is 1 / (2n + 1).
        let layout = layout(2);
        for c in 0..9 {
            let mean = layout.iter().map(|p| harmonics(vec3(p[0], p[1], p[2]), 2)[c].powi(2)).sum::<f64>() / 12.0;
            assert!((mean - 1.0 / (2 * degree(c) + 1) as f64).abs() < 1e-9, "channel {c}: {mean}");
        }
    }

    #[test]
    fn a_sound_is_heard_loudest_from_the_loudspeaker_it_comes_from_and_keeps_its_energy() {
        for order in [1, 2] {
            let mut decoder = Decoder::new(order);
            decoder.turn(&head());
            for (s, speaker) in decoder.speakers.clone().iter().enumerate() {
                // A sound from where this loudspeaker is, in the world as the head is turned.
                let from = vec3(speaker[0], speaker[1], -speaker[2]);
                let h = harmonics(from, order);
                let fed: Vec<f64> = decoder
                    .gains()
                    .iter()
                    .map(|g| (0..channels(order)).map(|c| f64::from(g[c]) * h[c]).sum())
                    .collect();
                let loudest = fed.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
                assert_eq!(loudest, s, "order {order}");
                let energy: f64 = fed.iter().map(|g| g * g).sum();
                assert!((energy - 1.0).abs() < 0.25, "order {order}: energy {energy}");
            }
        }
    }

    #[test]
    fn below_the_crossover_the_feeds_of_a_sound_sum_to_it() {
        for order in [1, 2] {
            let mut decoder = Decoder::new(order);
            decoder.turn(&head().turned(0.7));
            for from in [vec3(1.0, 0.2, -3.0), vec3(-2.0, -1.0, 0.5), vec3(0.0, 4.0, 0.0)] {
                let h = harmonics(from, order);
                let sum: f64 = decoder
                    .low_gains()
                    .iter()
                    .map(|g| (0..channels(order)).map(|c| f64::from(g[c]) * h[c]).sum::<f64>())
                    .sum();
                assert!((sum - 1.0).abs() < 1e-6, "order {order}: {sum}");
            }
        }
    }

    #[test]
    fn turning_the_head_turns_where_the_field_is_heard() {
        // A sound from the world's left is heard from the loudspeaker ahead of a head turned a quarter to
        // its left, and from the one on the left of a head that is not turned.
        let h = harmonics(vec3(-1.0, 0.0, 0.0), 1);
        let loudest = |turn: f64| {
            let mut decoder = Decoder::new(1);
            decoder.turn(&head().turned(turn));
            let fed = |s: usize| (0..4).map(|c| f64::from(decoder.gains()[s][c]) * h[c]).sum::<f64>();
            let s = (0..decoder.speakers.len()).max_by(|a, b| fed(*a).total_cmp(&fed(*b))).unwrap();
            decoder.speakers[s]
        };
        assert_eq!(loudest(0.0), [-1.0, 0.0, 0.0]);
        assert_eq!(loudest(std::f64::consts::FRAC_PI_2), [0.0, 0.0, 1.0]);
    }

    #[test]
    fn the_boundary_is_crossed_at_equal_power() {
        for d in [0.0, 80.0, 90.0, 95.0, 100.0, 200.0] {
            let far = far_share(d, 100.0);
            assert!((far + (1.0 - far) - 1.0).abs() < 1e-12);
        }
        assert_eq!(far_share(84.0, 100.0), 0.0);
        assert_eq!(far_share(100.0, 100.0), 1.0);
    }
}
