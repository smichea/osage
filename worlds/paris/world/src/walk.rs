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

mod route;

use bevy::math::{Vec2, Vec3, Vec3Swizzles};
use serde::Deserialize;
use std::ops::Range;

/// Walking and running speeds, m/s.
pub const WALK: f32 = 1.4;
pub const RUN: f32 = 4.0;
/// How far from a walker's centre the ground must hold it, m.
pub const RADIUS: f32 = 0.2;
pub(super) const DIAMETER_SQ: f32 = (2. * RADIUS) * (2. * RADIUS);
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
    #[serde(default)]
    patrols: Option<Vec<[[f32; 2]; 2]>>,
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
    /// One bit per centre cell whose entire 3×3 neighbourhood supports a disc.
    clear_cells: Vec<u64>,
    /// Where walkers arrive: on the ground under the scene's camera.
    pub start: Vec3,
    pub(super) patrols: Option<Vec<[[f32; 2]; 2]>>,
}

impl Walk {
    /// From `walk.json` and `walk.bin`.
    pub fn parse(header: &str, heights: &[u8]) -> Result<Self, String> {
        let h: Header = serde_json::from_str(header).map_err(|e| format!("walk.json: {e}"))?;
        if heights.len() != h.columns * h.rows * 4 {
            return Err(format!("walk.bin: {} bytes for {} x {} cells", heights.len(), h.columns, h.rows));
        }
        let heights = heights.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect::<Vec<_>>();
        let mut walk = Self::new(h.cell, h.columns, h.rows, Vec2::from(h.origin), heights, Vec3::from(h.start));
        walk.patrols = h.patrols;
        Ok(walk)
    }

    pub fn new(cell: f32, columns: usize, rows: usize, origin: Vec2, heights: Vec<f32>, start: Vec3) -> Self {
        assert_eq!(heights.len(), columns * rows, "a height a cell");
        let mut clear_cells = vec![0; heights.len().div_ceil(64)];
        if cell >= RADIUS {
            for r in 1..rows.saturating_sub(1) {
                for c in 1..columns.saturating_sub(1) {
                    let id = r * columns + c;
                    if !heights[id].is_finite() {
                        continue;
                    }
                    let mut low = heights[id];
                    let mut high = low;
                    for row in r - 1..=r + 1 {
                        for &h in &heights[row * columns + c - 1..=row * columns + c + 1] {
                            if !h.is_finite() {
                                high = f32::INFINITY;
                            }
                            low = low.min(h);
                            high = high.max(h);
                        }
                    }
                    if high - low <= STEP {
                        clear_cells[id / 64] |= 1 << (id % 64);
                    }
                }
            }
        }
        Self { cell, columns, rows, origin, heights, start, clear_cells, patrols: None }
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

    pub(super) fn same_cell(&self, a: Vec2, b: Vec2) -> bool {
        self.index(a) == self.index(b)
    }

    fn clear_cell(&self, (c, r): (usize, usize)) -> bool {
        let id = r * self.columns + c;
        self.clear_cells[id / 64] & (1 << (id % 64)) != 0
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
        self.places_in(0..self.columns, 0..self.rows)
    }

    /// Cell centres inside a horizontal disc. Visit only the rows and columns it touches.
    pub(super) fn places_near(&self, centre: Vec2, radius: f32) -> impl Iterator<Item = Vec3> + '_ {
        let lo = ((centre - radius - self.origin) / self.cell).floor();
        let hi = ((centre + radius - self.origin) / self.cell).floor();
        let span = |a: f32, b: f32, count: usize| (a as usize).min(count)..((b + 1.) as usize).min(count);
        self.places_in(span(lo.x, hi.x, self.columns), span(lo.y, hi.y, self.rows))
            .filter(move |p| p.xz().distance_squared(centre) <= radius * radius)
    }

    fn places_in(&self, columns: Range<usize>, rows: Range<usize>) -> impl Iterator<Item = Vec3> + '_ {
        rows.flat_map(move |r| {
            columns.clone().filter_map(move |c| {
                let h = self.at(c, r)?;
                let p = self.origin + (Vec2::new(c as f32, r as f32) + 0.5) * self.cell;
                Some(Vec3::new(p.x, h, p.y))
            })
        })
    }

    /// Whether a walker can stand at `p`: the ground holds it all round.
    pub fn holds(&self, p: Vec2) -> bool {
        p.is_finite()
            && self.index(p).is_some_and(|cell| {
                self.clear_cell(cell) || self.at(cell.0, cell.1).is_some_and(|h| self.supports_sweep(p, p, h))
            })
    }

    /// A walker's next place, `dt` seconds on: as far as its stride takes it,
    /// sliding along what stops it; where it is when nothing lets it go.
    pub fn step(&self, at: Vec3, stride: &Stride, dt: f32) -> Vec3 {
        let ahead = Vec2::new(-stride.heading.sin(), -stride.heading.cos());
        let right = Vec2::new(-ahead.y, ahead.x);
        let wish = (ahead * stride.forward + right * stride.right).clamp_length_max(1.0);
        let speed = if stride.run { RUN } else { WALK };
        self.slide(at, wish * speed * dt)
    }

    pub(super) fn slide(&self, at: Vec3, movement: Vec2) -> Vec3 {
        let from = Vec2::new(at.x, at.z);
        let here = self.ground(from).unwrap_or(at.y);
        for to in [from + movement, from + Vec2::new(movement.x, 0.0), from + Vec2::new(0.0, movement.y)] {
            if to != from
                && self.clear_segment(from, to)
                && let Some(h) = self.ground(to).filter(|h| (h - here).abs() <= STEP)
            {
                return Vec3::new(to.x, h, to.y);
            }
        }
        Vec3::new(from.x, here, from.y)
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
    fn neighbourhood_bits_preserve_the_full_swept_footprint_test() {
        let cached = street();
        assert!(cached.clear_cells.iter().any(|word| *word != 0));
        let mut uncached = cached.clone();
        uncached.clear_cells.fill(0);
        let mut positions = 0;
        for cell in cached.places() {
            let start = cell.xz() + Vec2::new(0.04, -0.11);
            assert_eq!(cached.holds(start), uncached.holds(start));
            for delta in
                [Vec2::new(0.04, 0.03), Vec2::new(-0.13, 0.05), Vec2::new(0.24, -0.28), Vec2::new(-0.67, -0.53)]
            {
                assert_eq!(cached.clear_segment(start, start + delta), uncached.clear_segment(start, start + delta));
            }
            positions += 1;
        }
        assert!(positions > 1000);
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
