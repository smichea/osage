// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! How a sound arriving from a direction reaches each ear of a head, without
//! a measured head: the structural model of C. P. Brown and R. O. Duda ("A
//! structural model for binaural sound synthesis", IEEE Trans. Speech and
//! Audio Processing 6(5), 1998).
//!
//! - **Time:** the far ear hears later, by the way round a sphere of 8.75 cm
//!   (Woodworth): up to 0.66 ms.
//! - **Shadow:** the head takes the high frequencies from the far ear and
//!   gives a little to the near one: one pole and one zero for each ear, set
//!   by the angle between the sound and that ear. Nothing below 500 Hz, 17 dB
//!   between the ears above 4 kHz for a sound from the side.
//! - **The outer ear:** five echoes within 0.4 ms whose delays shorten as the
//!   sound comes from higher: what tells up from level, a little.
//!
//! For loudspeakers, where the listener's own head does all this: an
//! equal-power pan by where the sound is across, as `osage-kart-native`
//! places its karts.
//!
//! Limits, the paper's own: fitted in front of the listener only (a sound
//! from behind is heard as its mirror in front), and what tells up from
//! down differs from one person's ears to another's.

use crate::{scene::Head, space::Vec3, ways::SOUND};

/// The radius of the sphere a head is taken for, m.
pub const HEAD_RADIUS: f64 = 0.0875;

/// The outer ear's echoes (the paper's Table I): how much of the sound each
/// sends back, and the numbers its delay is made from, in samples at
/// 44.1 kHz.
pub const PINNA: [(f64, f64, f64, f64); 5] = [
    (0.5, 1.0, 2.0, 1.0),
    (-1.0, 5.0, 4.0, 0.5),
    (0.5, 5.0, 7.0, 0.5),
    (-0.25, 5.0, 11.0, 0.5),
    (0.25, 5.0, 13.0, 0.5),
];

/// What a way is heard through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Listening {
    /// Each channel into one ear: time, shadow and outer ear are made.
    Headphones,
    /// Two loudspeakers in front: level only.
    Speakers,
}

/// How a sound from one direction reaches the two ears, left first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Heard {
    /// How much later than the nearest point of the head each ear hears, s.
    pub delay: [f64; 2],
    /// The head's shadow at each ear: the gain of the high frequencies,
    /// from 0.1 (the far side) to 2 (facing the sound).
    pub shadow: [f64; 2],
    /// For loudspeakers: the gain of each channel.
    pub pan: [f64; 2],
    /// The outer ear's echoes: how long after the sound each comes, s.
    pub pinna: [f64; 5],
}

/// How a sound reaching the head from the point `from` is heard.
pub fn hear(from: Vec3, head: &Head) -> Heard {
    hear_for(from, head, Listening::Headphones, true)
}

/// [`hear`], working out only what is listened through: the pan alone for
/// loudspeakers, and the outer ear's echoes only when they are made. What is
/// not worked out is left at nothing.
pub fn hear_for(from: Vec3, head: &Head, listening: Listening, outer_ear: bool) -> Heard {
    let towards = (from - head.at).unit_or(head.forward);
    // Across (to the right), up and ahead.
    let (x, y, z) = (towards.dot(head.right), towards.dot(head.up), towards.dot(head.forward));
    if listening == Listening::Speakers {
        let turn = (x.clamp(-1.0, 1.0) + 1.0) * std::f64::consts::FRAC_PI_4;
        return Heard { delay: [0.0; 2], shadow: [1.0; 2], pan: [turn.cos(), turn.sin()], pinna: [0.0; 5] };
    }
    let reach = HEAD_RADIUS / SOUND;
    let ear = |across: f64| {
        // The angle between the sound and this ear.
        let angle = across.clamp(-1.0, 1.0).acos();
        let delay = if angle < std::f64::consts::FRAC_PI_2 {
            reach * (1.0 - angle.cos())
        } else {
            // Round the head.
            reach * (1.0 + angle - std::f64::consts::FRAC_PI_2)
        };
        // Deepest at 150° from the ear, with a bright spot straight across.
        (delay, 1.05 + 0.95 * (angle * 1.2).cos())
    };
    let (left, right) = (ear(-x), ear(x));
    let turn = (x.clamp(-1.0, 1.0) + 1.0) * std::f64::consts::FRAC_PI_4;
    let pinna = if outer_ear {
        // The paper's angles: across, and up about the line through the ears;
        // behind is taken as its mirror in front.
        let across = x.clamp(-1.0, 1.0).asin();
        let up = y.atan2(z.abs());
        PINNA.map(|(_, a, b, d)| {
            (a * (across / 2.0).cos() * (d * (std::f64::consts::FRAC_PI_2 - up)).sin() + b) / 44_100.0
        })
    } else {
        [0.0; 5]
    };
    Heard { delay: [left.0, right.0], shadow: [left.1, right.1], pan: [turn.cos(), turn.sin()], pinna }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::vec3;

    /// A head at the origin looking along −Z, its right towards +X.
    fn head() -> Head {
        Head { at: Vec3::ZERO, forward: vec3(0.0, 0.0, -1.0), right: Vec3::X, up: Vec3::Y }
    }

    #[test]
    fn a_sound_from_the_right_reaches_the_right_ear_first_and_brighter() {
        let heard = hear(vec3(5.0, 0.0, 0.0), &head());
        assert_eq!(heard.delay[1], 0.0);
        // Round half the head: (a/c)(1 + π/2) = 656 µs.
        assert!((heard.delay[0] - 655.8e-6).abs() < 0.2e-6, "{}", heard.delay[0]);
        assert!((heard.shadow[1] - 2.0).abs() < 1e-12);
        // The far ear, straight across: the bright spot, 17 dB under the near one.
        assert!((20.0 * (heard.shadow[1] / heard.shadow[0]).log10() - 17.0).abs() < 0.1);
        assert!(heard.pan[0].abs() < 1e-12 && (heard.pan[1] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn left_and_right_are_each_other_s_mirror() {
        for from in [vec3(3.0, 1.0, -2.0), vec3(0.4, -2.0, 5.0), vec3(7.0, 0.0, 0.0)] {
            let (a, b) = (hear(from, &head()), hear(vec3(-from.x, from.y, from.z), &head()));
            assert_eq!((a.delay[0], a.delay[1]), (b.delay[1], b.delay[0]));
            assert_eq!((a.shadow[0], a.shadow[1]), (b.shadow[1], b.shadow[0]));
            assert!((a.pan[0] - b.pan[1]).abs() < 1e-12 && (a.pan[1] - b.pan[0]).abs() < 1e-12);
            assert_eq!(a.pinna, b.pinna);
        }
        // Turning the head half round does the same as moving the sound across.
        let from = vec3(4.0, 0.5, -1.0);
        let (a, b) = (hear(from, &head()), hear(from, &head().turned(std::f64::consts::PI)));
        assert!((a.delay[0] - b.delay[1]).abs() < 1e-15 && (a.shadow[1] - b.shadow[0]).abs() < 1e-12);
    }

    #[test]
    fn straight_ahead_both_ears_hear_alike_and_the_pan_keeps_the_power() {
        let heard = hear(vec3(0.0, 0.0, -9.0), &head());
        assert_eq!(heard.delay[0], heard.delay[1]);
        assert_eq!(heard.shadow[0], heard.shadow[1]);
        assert!((heard.pan[0] - heard.pan[1]).abs() < 1e-12);
        for from in [vec3(1.0, 0.0, -1.0), vec3(-3.0, 2.0, 0.5), vec3(0.0, 0.0, 4.0)] {
            let pan = hear(from, &head()).pan;
            assert!((pan[0] * pan[0] + pan[1] * pan[1] - 1.0).abs() < 1e-12);
        }
        // The time between the ears grows evenly with the angle (Woodworth): at 30°, (a/c)(θ + sin θ).
        let heard = hear(vec3(0.5, 0.0, -(0.75f64).sqrt()), &head());
        let expected = HEAD_RADIUS / SOUND * (std::f64::consts::FRAC_PI_6 + 0.5);
        assert!((heard.delay[0] - heard.delay[1] - expected).abs() < 1e-9);
    }

    #[test]
    fn the_outer_ear_s_echoes_come_sooner_from_above() {
        let level = hear(vec3(0.0, 0.0, -5.0), &head()).pinna;
        let above = hear(vec3(0.0, 4.0, -3.0), &head()).pinna;
        let overhead = hear(vec3(0.0, 5.0, -1e-9), &head()).pinna;
        for n in 0..5 {
            assert!(above[n] < level[n] && overhead[n] < above[n], "echo {n}");
            // Straight above, only the fixed part is left.
            assert!((overhead[n] * 44_100.0 - PINNA[n].2).abs() < 1e-6);
        }
        // Ahead and level: the first echo 3 samples after the sound at 44.1 kHz.
        assert!((level[0] * 44_100.0 - 3.0).abs() < 1e-9);
        // Behind is heard as its mirror in front.
        assert_eq!(hear(vec3(1.0, 2.0, 3.0), &head()).pinna, hear(vec3(1.0, 2.0, -3.0), &head()).pinna);
        // All within half a millisecond.
        assert!(level.iter().all(|t| *t > 0.0 && *t < 0.5e-3));
    }
}
