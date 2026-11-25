// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use bevy::prelude::Vec3;

/// Greatest vertical distance (terrain Y minus sole Y) on the overlapping X/Z projections.
/// The terrain triangle must have a nonzero horizontal area. Both windings are accepted.
pub fn triangle_support_lift(sole: [Vec3; 3], terrain: [Vec3; 3]) -> Option<f32> {
    let [a, b, c] = terrain;
    let cross = |u: Vec3, v: Vec3| u.x * v.z - u.z * v.x;
    let area = cross(b - a, c - a);
    if area == 0. || !area.is_finite() {
        return None;
    }
    // Height above each sole vertex is affine across the sole face. Clip this value
    // together with X/Z so the largest lift also includes an edge crossing or a
    // terrain corner wholly inside the sole.
    let mut polygon = [Vec3::ZERO; 8];
    for (out, p) in polygon.iter_mut().zip(sole) {
        let t = cross(p - a, c - a) / area;
        let u = cross(b - a, p - a) / area;
        *out = Vec3::new(p.x, a.y + t * (b.y - a.y) + u * (c.y - a.y) - p.y, p.z);
    }
    let mut count = 3;
    for (start, end) in [(a, b), (b, c), (c, a)] {
        let input = polygon;
        let mut prev = input[count - 1];
        let side = |p| cross(end - start, p - start) * area.signum();
        let mut prev_side = side(prev);
        let mut written = 0;
        for &p in &input[..count] {
            let d = side(p);
            if (d > 0. && prev_side < 0.) || (d < 0. && prev_side > 0.) {
                polygon[written] = prev.lerp(p, prev_side / (prev_side - d));
                written += 1;
            }
            if d >= 0. {
                polygon[written] = p;
                written += 1;
            }
            prev = p;
            prev_side = d;
        }
        if written == 0 {
            return None;
        }
        count = written;
    }
    polygon[..count].iter().map(|p| p.y).reduce(f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_sloped_patch_inside_the_sole_has_its_own_high_point() {
        let sole = [Vec3::ZERO, Vec3::X * 3., Vec3::Z * 3.];
        let patch = [Vec3::new(0.1, 0.03, 0.2), Vec3::new(0.5, 0.08, 0.2), Vec3::new(0.2, 0.05, 0.6)];
        for terrain in [patch, [patch[2], patch[1], patch[0]]] {
            assert!((triangle_support_lift(sole, terrain).unwrap() - 0.08).abs() < 1e-6);
        }
        assert!(triangle_support_lift(sole.map(|v| v + Vec3::X * 5.), patch).is_none());
        assert!(triangle_support_lift(sole, [Vec3::ZERO; 3]).is_none());
    }
}
