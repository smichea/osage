// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Holographic textures (issue #7): an object textured not by one image
//! but by how it looks from every viewing angle — a field of views —
//! compressed as a video is, with the step from one view to the next in
//! viewing angle where a video steps in time.
//!
//! This crate holds, for now, the first experiment's parts: the grid of
//! viewing directions an object is photographed from ([`Grid`]), and the
//! order the views are put in to make a "video" ([`Grid::path`]): along a
//! row of azimuths, back along the next, so that each view follows one a
//! step of angle away from it, as each frame of a film follows one a step
//! of time away.

use std::f32::consts::PI;

/// The viewing directions an object is photographed from: azimuths and
/// elevations in degrees, the object's front facing +Z.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    pub azimuths: Vec<f32>,
    pub elevations: Vec<f32>,
}

impl Grid {
    /// A grid from `from` to `to` degrees by `step`, in azimuth and in
    /// elevation.
    pub fn new(azimuth: (f32, f32, f32), elevation: (f32, f32, f32)) -> Self {
        let range = |(from, to, step): (f32, f32, f32)| {
            let n = ((to - from) / step).round() as usize;
            (0..=n).map(|i| from + i as f32 * step).collect::<Vec<_>>()
        };
        Self { azimuths: range(azimuth), elevations: range(elevation) }
    }

    /// A building's front: from 70° to either side, from 10° below the
    /// horizontal to 50° above, by 10° and 7.5°.
    pub fn facade() -> Self {
        Self::new((-70.0, 70.0, 10.0), (-10.0, 50.0, 7.5))
    }

    pub fn len(&self) -> usize {
        self.azimuths.len() * self.elevations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The unit direction from the object towards the camera, for an
    /// azimuth and an elevation in degrees: azimuth 0 and elevation 0 look
    /// at the object's front (+Z) square on.
    pub fn direction(azimuth: f32, elevation: f32) -> [f32; 3] {
        let (a, e) = (azimuth * PI / 180.0, elevation * PI / 180.0);
        [a.sin() * e.cos(), e.sin(), a.cos() * e.cos()]
    }

    /// Every view as (row, column) of (elevation, azimuth), in the order of
    /// the "video": along each row of azimuths, back along the next.
    pub fn path(&self) -> Vec<(usize, usize)> {
        let columns = self.azimuths.len();
        (0..self.elevations.len())
            .flat_map(|row| {
                let along: Vec<usize> =
                    if row % 2 == 0 { (0..columns).collect() } else { (0..columns).rev().collect() };
                along.into_iter().map(move |column| (row, column))
            })
            .collect()
    }

    /// A view's file name: its row and column, so that any order can be
    /// made from the files.
    pub fn name(row: usize, column: usize) -> String {
        format!("view-r{row:02}-c{column:02}.png")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_facade_grid_has_its_views() {
        let grid = Grid::facade();
        assert_eq!(grid.azimuths.len(), 15);
        assert_eq!(grid.elevations.len(), 9);
        assert_eq!(grid.len(), 135);
        assert_eq!(*grid.azimuths.first().unwrap(), -70.0);
        assert_eq!(*grid.elevations.last().unwrap(), 50.0);
    }

    #[test]
    fn the_path_visits_every_view_once_each_a_step_from_the_last() {
        let grid = Grid::facade();
        let path = grid.path();
        assert_eq!(path.len(), grid.len());
        let mut seen = std::collections::HashSet::new();
        for pair in path.windows(2) {
            let ((r0, c0), (r1, c1)) = (pair[0], pair[1]);
            assert_eq!(r0.abs_diff(r1) + c0.abs_diff(c1), 1, "{pair:?}");
        }
        for view in path {
            assert!(seen.insert(view));
        }
    }

    #[test]
    fn directions_are_unit_and_look_at_the_front() {
        let [x, y, z] = Grid::direction(0.0, 0.0);
        assert!((x, y, z) == (0.0, 0.0, 1.0));
        let [x, y, z] = Grid::direction(30.0, 20.0);
        assert!(((x * x + y * y + z * z) - 1.0).abs() < 1e-5);
        assert!(x > 0.0 && y > 0.0 && z > 0.0);
    }
}
