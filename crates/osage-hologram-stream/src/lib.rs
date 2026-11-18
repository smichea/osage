// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Holograms as streams (D-134): how a hologram's views travel from the
//! server that keeps them to the client that draws them.
//!
//! A hologram is a field of views: one picture per viewing direction, on a
//! grid of azimuths and elevations, in series — the colour as seen
//! ([`Series::View`]), its depth ([`Series::Depth`]), and for a hologram lit
//! where it is drawn its colour without light and its normals. The first
//! experiment (research/09) found that such a field compresses best as a
//! video along the viewing angle: each view a small turn from the last, the
//! codec's motion compensation finds the parallax, ×5–10 smaller than
//! pictures one by one at 2.5° steps.
//!
//! A client needs only the views around the direction it sees a hologram
//! from, and those it will soon. So the grid is cut into blocks of a few
//! rows and columns ([`Tiling`]), each block of each series one short AV1
//! video (an IVF file, [`ivf`]) that starts with a whole picture and can be
//! decoded alone ([`decode`]); its views go back and forth along the rows,
//! each a step of angle from the one before. A server sends the blocks the
//! viewer's reach needs ([`views_within`], [`Tiling::blocks_of`]); the
//! packer makes them from a baked hologram ([`encode`], `hologram-pack`).
//!
//! Depth is a number, not a colour, and it need not be exact: a centimetre
//! off moves the drawn point by a fraction of a millimetre between two
//! views. Its outline must be: where the object is travels on its own
//! ([`Series::Mask`], 0 or 255, nearly lossless and almost free), and the
//! depth as 12-bit grey, lossy, its ground padded like a colour's
//! ([`depth_to_grey`]).

pub mod decode;
pub mod encode;
pub mod http;
pub mod ivf;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The series of pictures a hologram has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Series {
    /// The colour as photographed, light and all.
    View,
    /// The colour without light, for a hologram lit where it is drawn.
    Albedo,
    /// The normals, for a hologram lit where it is drawn.
    Normal,
    /// How far each point stands towards the camera.
    Depth,
    /// Where the object is (taken from the baked depths).
    Mask,
}

impl Series {
    pub const ALL: [Series; 5] = [Series::View, Series::Albedo, Series::Normal, Series::Depth, Series::Mask];

    pub fn name(self) -> &'static str {
        match self {
            Series::View => "view",
            Series::Albedo => "albedo",
            Series::Normal => "normal",
            Series::Depth => "depth",
            Series::Mask => "mask",
        }
    }

    /// The baked picture of a view of this series (as `hologram-bake` names
    /// it); a mask is read from the depth's.
    pub fn picture(self, row: u16, column: u16) -> String {
        let name = if self == Series::Mask { Series::Depth.name() } else { self.name() };
        format!("{name}-r{row:02}-c{column:02}.png")
    }
}

/// A block of the grid: its place among the blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Block {
    pub row: u16,
    pub column: u16,
}

impl Block {
    /// The file of this block of a series.
    pub fn file(self, series: Series) -> String {
        format!("{}-b{:02}-{:02}.ivf", series.name(), self.row, self.column)
    }
}

/// How the grid is cut into blocks: rows and columns of views per block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tiling {
    pub rows: u16,
    pub columns: u16,
}

impl Default for Tiling {
    /// Three rows of four: twelve views a block, a whole picture and eleven
    /// small turns from it.
    fn default() -> Self {
        Self { rows: 3, columns: 4 }
    }
}

impl Tiling {
    pub fn block_of(&self, row: u16, column: u16) -> Block {
        Block { row: row / self.rows, column: column / self.columns }
    }

    /// Every block of a grid of `rows` by `columns` views.
    pub fn blocks(&self, rows: u16, columns: u16) -> impl Iterator<Item = Block> + use<> {
        let (down, across) = (rows.div_ceil(self.rows), columns.div_ceil(self.columns));
        (0..down).flat_map(move |row| (0..across).map(move |column| Block { row, column }))
    }

    /// The views of `block` in the order of its video: along its first row,
    /// back along the next, and so on; the grid's edge may cut it short.
    pub fn views(&self, block: Block, rows: u16, columns: u16) -> Vec<(u16, u16)> {
        let (r0, c0) = (block.row * self.rows, block.column * self.columns);
        let (r1, c1) = ((r0 + self.rows).min(rows), (c0 + self.columns).min(columns));
        let mut views = Vec::new();
        for (i, row) in (r0..r1).enumerate() {
            let along: Vec<u16> = if i % 2 == 0 { (c0..c1).collect() } else { (c0..c1).rev().collect() };
            views.extend(along.into_iter().map(|column| (row, column)));
        }
        views
    }

    /// The blocks that hold any of `views`.
    pub fn blocks_of(&self, views: impl IntoIterator<Item = (u16, u16)>) -> Vec<Block> {
        let mut blocks: Vec<Block> = views.into_iter().map(|(r, c)| self.block_of(r, c)).collect();
        blocks.sort();
        blocks.dedup();
        blocks
    }
}

/// The directions of a hologram's views: azimuths and elevations, degrees.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    pub azimuths: Vec<f32>,
    pub elevations: Vec<f32>,
}

impl Grid {
    /// From first, last and step of azimuth, then of elevation (degrees),
    /// as `hologram.json` keeps them.
    pub fn from_steps([a0, a1, a2, e0, e1, e2]: [f32; 6]) -> Self {
        let range = |from: f32, to: f32, step: f32| {
            let n = ((to - from) / step).round() as usize;
            (0..=n).map(|i| from + i as f32 * step).collect::<Vec<_>>()
        };
        Self { azimuths: range(a0, a1, a2), elevations: range(e0, e1, e2) }
    }

    /// The unit direction from the object towards the camera, as
    /// `osage-hologram` takes it: azimuth 0 and elevation 0 look at the
    /// object's front (+Z) square on.
    pub fn direction(azimuth: f32, elevation: f32) -> [f32; 3] {
        let (a, e) = (azimuth.to_radians(), elevation.to_radians());
        [a.sin() * e.cos(), e.sin(), a.cos() * e.cos()]
    }

    fn step(values: &[f32]) -> f32 {
        if values.len() > 1 { (values[1] - values[0]).abs() } else { 360.0 }
    }
}

/// The views of `grid` worth having to see the object from directions
/// within `half_angle` radians of `axis` (in the object's frame, from the
/// object towards the viewer): those within it, and those a step out of it
/// that a direction within it is drawn between. Every view for a half angle
/// of π or more.
pub fn views_within(grid: &Grid, axis: [f32; 3], half_angle: f32) -> Vec<(u16, u16)> {
    let step = Grid::step(&grid.azimuths).max(Grid::step(&grid.elevations)).to_radians();
    let reach = half_angle + step * 1.5;
    let length = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt().max(1e-9);
    let axis = axis.map(|v| v / length);
    let mut views = Vec::new();
    for (row, &elevation) in grid.elevations.iter().enumerate() {
        for (column, &azimuth) in grid.azimuths.iter().enumerate() {
            let d = Grid::direction(azimuth, elevation);
            let cos = (d[0] * axis[0] + d[1] * axis[1] + d[2] * axis[2]).clamp(-1.0, 1.0);
            if reach >= std::f32::consts::PI || cos.acos() <= reach {
                views.push((row as u16, column as u16));
            }
        }
    }
    views
}

/// What a packed hologram holds, written beside its blocks as
/// [`Packed::FILE`]: how it was framed (as `hologram.json`), its grid, how
/// it is cut, its series, and the size of each block's file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Packed {
    /// The photographed sphere's centre and radius, the views' height to
    /// width, where the hologram stands (translation, quaternion).
    pub centre: [f32; 3],
    pub radius: f32,
    pub aspect: f32,
    pub at: [f32; 7],
    /// First, last and step of azimuth, then of elevation, degrees.
    pub grid: [f32; 6],
    pub tiling: Tiling,
    /// Each view's width and height, pixels.
    pub size: [u32; 2],
    pub series: Vec<Series>,
    /// Bytes of each block file, by file name.
    pub bytes: BTreeMap<String, u64>,
}

impl Packed {
    pub const FILE: &str = "stream.json";

    pub fn grid(&self) -> Grid {
        Grid::from_steps(self.grid)
    }

    /// Whether it is lit where it is drawn (it has albedo and normals).
    pub fn lit(&self) -> bool {
        self.series.contains(&Series::Albedo) && self.series.contains(&Series::Normal)
    }
}

/// The 16-bit depth of a baked depth texel (red, green) as 12-bit grey.
pub fn depth_to_grey(texel: [u8; 4]) -> u16 {
    ((f32::from(u16::from_be_bytes([texel[0], texel[1]])) / 65535.0 * 4095.0).round() as u16).min(4095)
}

/// 12-bit grey back as a baked depth texel, where `there` says the object is.
pub fn grey_to_depth(grey: u16, there: bool) -> [u8; 4] {
    if !there {
        return [0, 0, 0, 255];
    }
    let [high, low] = ((f32::from(grey.min(4095)) / 4095.0 * 65535.0).round() as u16).to_be_bytes();
    [high, low, 255, 255]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_goes_back_and_forth_and_the_blocks_cover_the_grid() {
        let tiling = Tiling { rows: 3, columns: 4 };
        let views = tiling.views(Block { row: 0, column: 0 }, 9, 15);
        assert_eq!(&views[..5], &[(0, 0), (0, 1), (0, 2), (0, 3), (1, 3)]);
        for pair in views.windows(2) {
            let ((r0, c0), (r1, c1)) = (pair[0], pair[1]);
            assert_eq!(r0.abs_diff(r1) + c0.abs_diff(c1), 1, "{pair:?}");
        }
        let mut seen = std::collections::BTreeSet::new();
        for block in tiling.blocks(9, 15) {
            for view in tiling.views(block, 9, 15) {
                assert_eq!(tiling.block_of(view.0, view.1), block);
                assert!(seen.insert(view));
            }
        }
        assert_eq!(seen.len(), 9 * 15);
    }

    #[test]
    fn depth_survives_grey() {
        for depth in [0u16, 1, 300, 32_768, 65_000, 65_535] {
            let [high, low] = depth.to_be_bytes();
            let back = grey_to_depth(depth_to_grey([high, low, 255, 255]), true);
            assert_eq!(back[2], 255);
            assert!(u16::from_be_bytes([back[0], back[1]]).abs_diff(depth) <= 9, "{depth}");
        }
        assert_eq!(grey_to_depth(2000, false)[2], 0, "nothing stays nothing");
    }

    #[test]
    fn the_views_towards_the_viewer_are_the_ones_sent() {
        let grid = Grid::from_steps([-70.0, 70.0, 10.0, -10.0, 50.0, 7.5]);
        let front = views_within(&grid, [0.0, 0.0, 1.0], 0.05);
        assert!(front.contains(&(1, 7)), "{front:?}");
        assert!(front.len() < 20, "{}", front.len());
        assert!(front.iter().all(|&(_, c)| (5..=9).contains(&c)));
        assert_eq!(views_within(&grid, [1.0, 0.0, 0.0], 4.0).len(), grid.azimuths.len() * grid.elevations.len());
    }
}
