// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The circuit: a closed Catmull-Rom spline through hand-placed points — a
//! long start straight, a sweeping right-hander, an S, a chicane and a
//! hairpin; a ramp halfway down the start straight and a smaller one on the
//! long sweep before the line (D-050).

use libm::{cosf, sinf};
use osage_kart_physics::{Kart, Ramp, Track};

use crate::proto;

/// Control points, metres; the cars run in this order.
const CONTROL: &[[f32; 2]] = &[
    [0.0, -90.0],
    [0.0, -20.0],
    [0.0, 45.0],
    [-10.0, 80.0],
    [-38.0, 100.0],
    [-75.0, 95.0],
    [-95.0, 65.0],
    [-88.0, 30.0],
    [-62.0, 12.0],
    [-58.0, -14.0],
    [-82.0, -34.0],
    [-112.0, -42.0],
    [-132.0, -68.0],
    [-124.0, -104.0],
    [-92.0, -124.0],
    [-52.0, -128.0],
    [-18.0, -118.0],
];
const SAMPLES_PER_SPAN: usize = 10;
pub const HALF_WIDTH: f32 = 7.0;
pub const VERGE: f32 = 4.0;
pub const SECTORS: u32 = 8;

pub struct Circuit {
    pub track: Track,
    /// Distance along the centre line of the start/finish line.
    pub start: f32,
}

impl Default for Circuit {
    fn default() -> Self {
        Self::new()
    }
}

impl Circuit {
    pub fn new() -> Self {
        let n = CONTROL.len();
        let mut points = Vec::with_capacity(n * SAMPLES_PER_SPAN);
        for i in 0..n {
            let [p0, p1, p2, p3] = [CONTROL[(i + n - 1) % n], CONTROL[i], CONTROL[(i + 1) % n], CONTROL[(i + 2) % n]];
            for s in 0..SAMPLES_PER_SPAN {
                let t = s as f32 / SAMPLES_PER_SPAN as f32;
                points.push([catmull_rom(p0[0], p1[0], p2[0], p3[0], t), catmull_rom(p0[1], p1[1], p2[1], p3[1], t)]);
            }
        }
        let track = Track::new(points, HALF_WIDTH, VERGE);
        // The start line sits at the second control point, on the straight.
        let start = track.locate(CONTROL[1][0], CONTROL[1][1]).along;
        let sweep = track.locate(-72.0, -127.0).along;
        let ramps = vec![
            Ramp { along: start + 42.0, length: 6.0, height: 1.0 },
            Ramp { along: sweep, length: 5.0, height: 0.7 },
        ];
        Self { track: track.with_ramps(ramps), start }
    }

    /// The circuit a world announced in its `kart.track/1`.
    pub fn from_proto(track: &proto::Track) -> Self {
        let points = track.points.as_chunks::<2>().0.to_vec();
        let ramps = track.ramps.iter().map(|r| Ramp { along: r.along, length: r.length, height: r.height }).collect();
        Self { track: Track::new(points, track.half_width, track.verge).with_ramps(ramps), start: track.start }
    }

    /// Slot `n` of the starting grid: two columns, rows every 7 m behind the line.
    pub fn grid_slot(&self, n: usize) -> Kart {
        let along = self.start - 8.0 - 7.0 * (n / 2) as f32;
        let (centre, heading) = self.track.at(along);
        let side = if n.is_multiple_of(2) { -3.0 } else { 3.0 };
        // Right of the road is (-cos h, sin h).
        Kart { x: centre[0] - cosf(heading) * side, z: centre[1] + sinf(heading) * side, heading, ..Kart::default() }
    }

    /// Distance from the start line, `0..length`.
    pub fn from_start(&self, along: f32) -> f32 {
        (along - self.start).rem_euclid(self.track.length())
    }

    pub fn sector_length(&self) -> f32 {
        self.track.length() / SECTORS as f32
    }

    pub fn to_proto(&self) -> proto::Track {
        proto::Track {
            points: self.track.points().iter().flatten().copied().collect(),
            half_width: self.track.half_width,
            verge: self.track.verge,
            start: self.start,
            sectors: SECTORS,
            ramps: self
                .track
                .ramps()
                .iter()
                .map(|r| proto::Ramp { along: r.along, length: r.length, height: r.height })
                .collect(),
        }
    }
}

fn catmull_rom(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let (t2, t3) = (t * t, t * t * t);
    0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3)
}
