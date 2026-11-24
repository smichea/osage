// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

use super::VisualLod;
use glam::{DVec3, Mat4, Vec3};

/// World-space bounds include every offered mesh. Error values enclose the
/// bidirectional geometric deviation from the primary mesh in metres.
pub struct VisualDetail {
    pub center: DVec3,
    pub half_extents: DVec3,
    pub levels: Vec<VisualLevel>,
}

pub struct VisualLevel {
    pub level: VisualLod,
    pub mesh: usize,
    pub triangles: usize,
    pub error_m: f64,
}

#[derive(Clone, Copy)]
pub struct DetailView {
    pub eye: DVec3,
    pub forward: DVec3,
    pub focal_pixels: f64,
    pub near: f64,
}

impl DetailView {
    pub fn new(eye: DVec3, direction: DVec3, fov_y: f64, height_px: u32, near: f64) -> Option<Self> {
        if !eye.is_finite()
            || !(0.0..std::f64::consts::PI).contains(&fov_y)
            || fov_y == 0.0
            || height_px == 0
            || !near.is_finite()
            || near <= 0.0
        {
            return None;
        }
        Some(Self {
            eye,
            forward: direction.try_normalize()?,
            focal_pixels: f64::from(height_px) / (2. * (0.5 * fov_y).tan()),
            near,
        })
    }
}

/// Bounds the longest transformed unit vector, including nonuniform scale and shear.
pub fn maximum_stretch(transform: Mat4) -> f64 {
    let axes = [transform.x_axis, transform.y_axis, transform.z_axis].map(|v| v.truncate().as_dvec3());
    axes.iter().map(|a| axes.iter().map(|b| a.dot(*b).abs()).sum::<f64>()).fold(0., f64::max).sqrt()
}

impl VisualDetail {
    pub fn world_bounds(local_min: Vec3, local_max: Vec3, matrix: Mat4) -> (DVec3, DVec3) {
        let transform = matrix.as_dmat4();
        let half = (local_max.as_dvec3() - local_min.as_dvec3()) * 0.5;
        let center = transform.transform_point3(local_min.as_dvec3() + half);
        let extents = transform.x_axis.truncate().abs() * half.x
            + transform.y_axis.truncate().abs() * half.y
            + transform.z_axis.truncate().abs() * half.z;
        (center, extents)
    }

    /// Perspective projection has a larger derivative near a screen edge.
    /// Bound both the depth and off-axis displacement over the whole part.
    pub fn pixels_per_metre(&self, view: DetailView) -> f64 {
        let delta = self.center - view.eye;
        let depth = delta.dot(view.forward);
        let depth_extent = self.half_extents.dot(view.forward.abs());
        if depth + depth_extent < view.near {
            return 0.;
        }
        let near = depth - depth_extent;
        if near <= view.near {
            return f64::INFINITY;
        }
        let lateral = (delta - view.forward * depth).length() + self.half_extents.length();
        view.focal_pixels * (1. + (lateral / near).powi(2)).sqrt() / near
    }

    /// Draw the cheapest eligible resident mesh. Retain an existing coarse
    /// choice up to 2 px, and require at most 1 px to switch to a cheaper one.
    pub fn choose(&self, current: usize, view: DetailView) -> usize {
        let scale = self.pixels_per_metre(view);
        let error = |l: &VisualLevel| if l.error_m == 0. { 0. } else { l.error_m * scale };
        let limit = if error(&self.levels[current]) <= 2. { 1. } else { 2. };
        let mut best = if error(&self.levels[current]) <= 2. { current } else { 0 };
        for (i, l) in self.levels.iter().enumerate() {
            if l.triangles < self.levels[best].triangles && error(l) <= limit {
                best = i;
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn object() -> VisualDetail {
        VisualDetail {
            center: DVec3::ZERO,
            half_extents: DVec3::ZERO,
            levels: vec![
                VisualLevel { level: VisualLod::High, mesh: 0, triangles: 120, error_m: 0. },
                VisualLevel { level: VisualLod::Low, mesh: 1, triangles: 8, error_m: 0.1 },
            ],
        }
    }
    fn camera(distance: f64, height: u32) -> DetailView {
        DetailView::new(DVec3::Z * distance, -DVec3::Z, std::f64::consts::FRAC_PI_2, height, 0.1).unwrap()
    }
    #[test]
    fn cut_depends_on_pixels_history_and_near_plane() {
        let mut model = object();
        assert_eq!(model.choose(0, camera(40., 1000)), 0);
        assert_eq!(model.choose(1, camera(40., 1000)), 1);
        assert_eq!(model.choose(0, camera(60., 1000)), 1);
        assert_eq!(model.choose(1, camera(20., 1000)), 0);
        assert_eq!(model.choose(0, camera(60., 2000)), 0);
        model.half_extents = DVec3::splat(2.);
        assert_eq!(model.choose(1, camera(2., 1000)), 0);
    }
    #[test]
    fn projected_offset_on_a_sheared_piece_stays_inside_the_bound() {
        let mut m = Mat4::from_scale(Vec3::new(2., 3., 4.));
        m.y_axis.x = 1.5;
        let (center, half_extents) = VisualDetail::world_bounds(Vec3::splat(-1.), Vec3::splat(1.), m);
        let model = VisualDetail { center, half_extents, levels: vec![] };
        let eye = DVec3::new(10., 2., 20.);
        let view = DetailView::new(eye, -DVec3::Z, 1., 720, 0.1).unwrap();
        let screen = |p: DVec3| (p - eye).truncate() * view.focal_pixels / (eye.z - p.z);
        for p in [DVec3::ONE, DVec3::NEG_ONE, DVec3::Y] {
            for q in [DVec3::X, DVec3::Y, DVec3::Z, -DVec3::Y] {
                let a = m.as_dmat4().transform_point3(p);
                let b = m.as_dmat4().transform_point3(p * 0.9 + q * 0.1);
                let local_error = p.distance(p * 0.9 + q * 0.1);
                assert!(
                    screen(a).distance(screen(b))
                        <= local_error * maximum_stretch(m) * model.pixels_per_metre(view) + 1e-10
                );
            }
        }
    }
}
