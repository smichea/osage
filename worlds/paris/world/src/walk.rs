// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Where a walker can stand, and how it moves (`tools/bistro/export.py`'s
//! walk grid): cells of a quarter metre over the street, each with the
//! height of its ground, or none where nobody stands — under a table, on a
//! bollard, inside a building. The server moves walkers with it, and each
//! client predicts its own walker with the same steps.
//!
//! Frame: the glTF's — x east, y up, z south; a walker's heading turns
//! about y, 0 facing -z.

use bevy::math::{Vec2, Vec3};
use serde::Deserialize;

/// Walking and running speeds, m/s.
pub const WALK: f32 = 1.4;
pub const RUN: f32 = 4.0;
/// How far from a walker's centre the ground must hold it, m.
pub const RADIUS: f32 = 0.2;
/// The highest step a walker takes up or down at once, m.
pub const STEP: f32 = 0.4;
/// Where a walker's eyes are above the ground, m.
pub const EYES: f32 = 1.65;

/// A step's wish, as the `paris.stroll/1` intent carries it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stride {
    /// Ahead (+1) or back (-1).
    pub forward: f32,
    /// To the right (+1) or the left (-1).
    pub right: f32,
    /// Where the walker faces, radians about y; 0 faces -z.
    pub heading: f32,
    pub run: bool,
}

#[derive(Deserialize)]
struct Header {
    cell: f32,
    columns: usize,
    rows: usize,
    origin: [f32; 2],
    start: [f32; 3],
}

/// The walk grid.
#[derive(Clone, Debug)]
pub struct Walk {
    cell: f32,
    columns: usize,
    rows: usize,
    /// The corner of the first cell: x and z.
    origin: Vec2,
    /// Row by row from the smallest z; NaN where nobody stands.
    heights: Vec<f32>,
    /// Where walkers arrive: on the ground under the scene's camera.
    pub start: Vec3,
}

impl Walk {
    /// From `walk.json` and `walk.bin`.
    pub fn parse(header: &str, heights: &[u8]) -> Result<Self, String> {
        let h: Header = serde_json::from_str(header).map_err(|e| format!("walk.json: {e}"))?;
        if heights.len() != h.columns * h.rows * 4 {
            return Err(format!("walk.bin: {} bytes for {} x {} cells", heights.len(), h.columns, h.rows));
        }
        let heights = heights.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect::<Vec<_>>();
        Ok(Self::new(h.cell, h.columns, h.rows, Vec2::from(h.origin), heights, Vec3::from(h.start)))
    }

    pub fn new(cell: f32, columns: usize, rows: usize, origin: Vec2, heights: Vec<f32>, start: Vec3) -> Self {
        assert_eq!(heights.len(), columns * rows, "a height a cell");
        Self { cell, columns, rows, origin, heights, start }
    }

    /// The cell `p` (x, z) is in, if on the grid.
    fn index(&self, p: Vec2) -> Option<(usize, usize)> {
        let g = (p - self.origin) / self.cell;
        if g.x < 0.0 || g.y < 0.0 {
            return None;
        }
        let (c, r) = (g.x as usize, g.y as usize);
        (c < self.columns && r < self.rows).then_some((c, r))
    }

    fn at(&self, c: usize, r: usize) -> Option<f32> {
        Some(self.heights[r * self.columns + c]).filter(|h| !h.is_nan())
    }

    /// The ground of the cell `p` (x, z) is in; none where nobody stands.
    pub fn height(&self, p: Vec2) -> Option<f32> {
        let (c, r) = self.index(p)?;
        self.at(c, r)
    }

    /// The ground under `p`, smooth from cell to cell: between the centres
    /// of the cells around it that hold a walker, nearer ones weighing more.
    pub fn ground(&self, p: Vec2) -> Option<f32> {
        let own = self.height(p)?;
        let g = (p - self.origin) / self.cell - 0.5;
        let (c0, r0) = (g.x.floor() as isize, g.y.floor() as isize);
        let (tx, ty) = (g.x - c0 as f32, g.y - r0 as f32);
        let (mut sum, mut weight) = (0.0, 0.0);
        for (dc, dr, w) in
            [(0, 0, (1.0 - tx) * (1.0 - ty)), (1, 0, tx * (1.0 - ty)), (0, 1, (1.0 - tx) * ty), (1, 1, tx * ty)]
        {
            let (c, r) = (c0 + dc, r0 + dr);
            if c < 0 || r < 0 || c as usize >= self.columns || r as usize >= self.rows {
                continue;
            }
            if let Some(h) = self.at(c as usize, r as usize).filter(|h| (h - own).abs() <= STEP) {
                sum += w * h;
                weight += w;
            }
        }
        Some(if weight > 0.0 { sum / weight } else { own })
    }

    /// The centre of every cell a walker can stand in, on its ground.
    pub fn places(&self) -> impl Iterator<Item = Vec3> + '_ {
        (0..self.rows).flat_map(move |r| {
            (0..self.columns).filter_map(move |c| {
                let h = self.at(c, r)?;
                let p = self.origin + (Vec2::new(c as f32, r as f32) + 0.5) * self.cell;
                Some(Vec3::new(p.x, h, p.y))
            })
        })
    }

    /// Whether a walker can stand at `p`: the ground holds it all round.
    pub fn holds(&self, p: Vec2) -> bool {
        let Some(own) = self.height(p) else { return false };
        [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y]
            .iter()
            .all(|&d| self.height(p + d * RADIUS).is_some_and(|h| (h - own).abs() <= STEP))
    }

    /// A walker's next place, `dt` seconds on: as far as its stride takes it,
    /// sliding along what stops it; where it is when nothing lets it go.
    pub fn step(&self, at: Vec3, stride: &Stride, dt: f32) -> Vec3 {
        let ahead = Vec2::new(-stride.heading.sin(), -stride.heading.cos());
        let right = Vec2::new(-ahead.y, ahead.x);
        let wish = (ahead * stride.forward + right * stride.right).clamp_length_max(1.0);
        let speed = if stride.run { RUN } else { WALK };
        let from = Vec2::new(at.x, at.z);
        let here = self.ground(from).unwrap_or(at.y);
        let movement = wish * speed * dt;
        let fits = |p: Vec2| self.holds(p) && self.ground(p).is_some_and(|h| (h - here).abs() <= STEP);
        let to = [from + movement, from + Vec2::new(movement.x, 0.0), from + Vec2::new(0.0, movement.y)]
            .into_iter()
            .find(|&p| p != from && fits(p))
            .unwrap_or(from);
        Vec3::new(to.x, self.ground(to).unwrap_or(here), to.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A street 4 m wide along x, a kerb of 0.15 m on its north side, a wall
    /// beyond, a bollard in the middle.
    fn street() -> Walk {
        let (columns, rows) = (80, 40);
        let mut heights = vec![f32::NAN; columns * rows];
        for r in 4..30 {
            for c in 0..columns {
                let z = r as f32 * 0.25;
                heights[r * columns + c] = if z < 2.0 { 0.15 } else { 0.0 };
            }
        }
        // The bollard.
        for r in 18..20 {
            for c in 40..42 {
                heights[r * columns + c] = f32::NAN;
            }
        }
        Walk::new(0.25, columns, rows, Vec2::ZERO, heights, Vec3::new(5.0, 0.0, 5.0))
    }

    #[test]
    fn a_walker_goes_where_it_faces_at_its_pace() {
        let walk = street();
        let east = Stride { forward: 1.0, heading: -std::f32::consts::FRAC_PI_2, ..Stride::default() };
        let at = walk.step(Vec3::new(2.0, 0.0, 5.0), &east, 1.0);
        assert!((at.x - (2.0 + WALK)).abs() < 1e-4 && (at.z - 5.0).abs() < 1e-4, "{at}");
        let running = walk.step(Vec3::new(2.0, 0.0, 5.0), &Stride { run: true, ..east }, 0.5);
        assert!((running.x - (2.0 + RUN * 0.5)).abs() < 1e-4);
    }

    #[test]
    fn walls_and_bollards_stop_it_and_it_slides_along_them() {
        let walk = street();
        // North into the wall (z < 1): it stays on the pavement.
        let north = Stride { forward: 1.0, heading: 0.0, ..Stride::default() };
        let mut at = Vec3::new(5.0, 0.15, 1.6);
        for _ in 0..50 {
            at = walk.step(at, &north, 0.05);
        }
        assert!(at.z >= 1.0 + RADIUS - 1e-3, "{at}");
        // North-east against the wall: it slides east.
        let along = Stride { forward: 1.0, right: 1.0, heading: 0.0, ..Stride::default() };
        let slid = walk.step(at, &along, 0.2);
        assert!(slid.x > at.x, "{slid} from {at}");
        // Into the bollard (x 10..10.5, z 4.5..5).
        let east = Stride { forward: 1.0, heading: -std::f32::consts::FRAC_PI_2, ..Stride::default() };
        let mut at = Vec3::new(8.0, 0.0, 4.75);
        for _ in 0..100 {
            at = walk.step(at, &east, 0.05);
        }
        assert!(at.x < 10.0 - RADIUS + 0.26, "through the bollard: {at}");
    }

    #[test]
    fn the_ground_is_smooth_and_the_kerb_is_a_step() {
        let walk = street();
        let up = walk.ground(Vec2::new(5.0, 1.9)).unwrap();
        let down = walk.ground(Vec2::new(5.0, 2.1)).unwrap();
        assert!(up > down && up <= 0.15 + 1e-4 && down >= -1e-4, "{up} {down}");
        assert!(walk.ground(Vec2::new(5.0, 0.5)).is_none(), "nobody stands in the wall");
    }

    #[test]
    fn a_grid_round_trips_through_its_files() {
        let walk = street();
        let bytes: Vec<u8> = walk.heights.iter().flat_map(|h| h.to_le_bytes()).collect();
        let header = r#"{"cell": 0.25, "columns": 80, "rows": 40, "origin": [0, 0], "start": [5, 0, 5]}"#;
        let back = Walk::parse(header, &bytes).unwrap();
        assert_eq!(back.height(Vec2::new(5.0, 5.0)), Some(0.0));
        assert!(Walk::parse(header, &bytes[4..]).is_err());
    }
}
