// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Conservative separation of a part's swept AABB from a single omitted triangle.
use super::{Bounds, Residual, Shape, overlaps};
use anyhow::{Context, Result, ensure};
use bevy::math::DVec3;
use std::collections::{BTreeMap, BTreeSet};

type Key = (usize, [[u32; 3]; 2]);
type Page = BTreeMap<Key, u16>;

#[derive(Default)]
pub(super) struct Refinements {
    by_tile: BTreeMap<usize, Page>,
    /// A contradictory file revoked these certificates. Keep using the coarse bounds
    /// for forecasts and actual stepping, including during later retries.
    pub(super) invalidated: BTreeSet<usize>,
}

fn key(slot: usize, bounds: Bounds) -> Key {
    (slot, bounds.map(|p| p.map(|v| if v == 0. { 0 } else { v.to_bits() })))
}

fn corner_points(bounds: &Bounds, code: u16) -> [DVec3; 3] {
    std::array::from_fn(|v| {
        let mask = code >> (3 * v);
        DVec3::from_array(std::array::from_fn(|a| f64::from(bounds[usize::from(mask >> a & 1)][a])))
    })
}

fn corner_code(points: &[[f32; 3]; 3], bounds: &Bounds) -> Option<u16> {
    let mut masks = [0u16; 3];
    for (mask, p) in masks.iter_mut().zip(points) {
        for (axis, &v) in p.iter().enumerate() {
            if v != bounds[0][axis] {
                if v != bounds[1][axis] {
                    return None;
                }
                *mask |= 1 << axis;
            }
        }
    }
    masks.sort_unstable();
    (masks[0] < masks[1] && masks[1] < masks[2]).then_some(masks[0] | masks[1] << 3 | masks[2] << 6)
}

fn validate_code(bounds: &Bounds, code: u16) -> Result<()> {
    let points = corner_points(bounds, code);
    let array = points.map(|p| p.as_vec3().to_array());
    let [a, b, c] = points;
    ensure!(
        corner_code(&array, bounds) == Some(code)
            && a.min(b).min(c).as_vec3().to_array() == bounds[0]
            && a.max(b).max(c).as_vec3().to_array() == bounds[1]
            && (b - a).cross(c - a).length_squared() > 0.,
        "invalid residual triangle corners"
    );
    Ok(())
}

impl Refinements {
    pub(super) fn bind(
        &mut self,
        records: &[[usize; 3]],
        physical: &[usize],
        residual: &[Vec<Residual>],
    ) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        ensure!(records.windows(2).all(|r| r[0][..2] < r[1][..2]), "unordered residual triangle certificates");
        for &[page, row, raw] in records {
            let tile = *physical.get(page).context("unknown residual triangle page")?;
            let r = residual[tile].get(row).context("unknown residual triangle row")?;
            let slot = r.shape.context("residual triangle has no source shape")?;
            ensure!(r.faces == 1, "residual triangle certificate requires exactly one omitted face");
            let code = u16::try_from(raw)?;
            validate_code(&r.bounds, code)?;
            ensure!(
                self.by_tile.entry(tile).or_default().insert(key(slot, r.bounds), code).is_none(),
                "ambiguous residual triangle bounds"
            );
        }
        Ok(())
    }

    pub(super) fn triangle(&self, tile: usize, residual: &Residual) -> Option<[[f32; 3]; 3]> {
        let code = self.by_tile.get(&tile)?.get(&key(residual.shape?, residual.bounds))?;
        Some(corner_points(&residual.bounds, *code).map(|p| p.as_vec3().to_array()))
    }
    pub(super) fn known_pages(&self) -> impl Iterator<Item = usize> + '_ {
        self.by_tile.keys().chain(&self.invalidated).copied()
    }

    pub(super) fn intersects(&self, tile: usize, residual: &Residual, sweep: &Bounds) -> bool {
        if !overlaps(&residual.bounds, sweep) {
            return false;
        }
        let Some(page) = self.by_tile.get(&tile) else {
            return true;
        };
        let Some(&code) = residual.shape.and_then(|s| page.get(&key(s, residual.bounds))) else {
            return true;
        };
        triangle_may_intersect(corner_points(&residual.bounds, code), sweep)
    }

    pub(super) fn verify(&mut self, tile: usize, shapes: &[Shape]) -> Result<()> {
        let Some(certs) = self.by_tile.get(&tile) else {
            return Ok(());
        };
        let mut remaining = certs.clone();
        let mut previous_slot = None;
        for &(slot, _) in certs.keys() {
            if previous_slot == Some(slot) {
                continue;
            }
            previous_slot = Some(slot);
            let s = shapes.get(slot).context("residual triangle source shape unavailable")?;
            for face in &s.triangles {
                let p = |n: u32| s.vertices.get(n as usize).copied().context("invalid collision vertex");
                let xyz = [p(face[0])?, p(face[1])?, p(face[2])?];
                let [a, b, c] = xyz.map(|p| DVec3::from_array(p.map(f64::from)));
                let n = (b - a).cross(c - a);
                if n.y > 0.5 * n.length() {
                    continue;
                }
                let bounds = [a.min(b).min(c).as_vec3().to_array(), a.max(b).max(c).as_vec3().to_array()];
                let k = key(slot, bounds);
                if let Some(expected) = certs.get(&k)
                    && (remaining.remove(&k).is_none() || corner_code(&xyz, &bounds).as_ref() != Some(expected))
                {
                    return self.revoke(tile);
                }
            }
        }
        if !remaining.is_empty() {
            return self.revoke(tile);
        }
        Ok(())
    }

    fn revoke(&mut self, tile: usize) -> Result<()> {
        self.by_tile.remove(&tile);
        self.invalidated.insert(tile);
        anyhow::bail!("residual triangle certificate differs from full collision geometry")
    }
}

/// The initial AABB test handles the three box axes. The triangle normal and
/// edge × box-axis directions complete the separating axis test. Equality holds the page.
fn triangle_may_intersect(points: [DVec3; 3], bounds: &Bounds) -> bool {
    let [low, high] = bounds.map(|p| DVec3::from_array(p.map(f64::from)));
    let half = (high - low) * 0.5;
    let center = low + half;
    let [a, b, c] = points.map(|p| p - center);
    let envelope = a.abs().max(b.abs()).max(c.abs()).max(half);
    let separates = |axis: DVec3| {
        let radius = half.dot(axis.abs());
        // Bound roundoff of potentially cancelling projections in local f64 coordinates.
        let tolerance = 64. * f64::EPSILON * envelope.dot(axis.abs());
        let p = [axis.dot(a), axis.dot(b), axis.dot(c)];
        p.iter().all(|&x| x > radius + tolerance) || p.iter().all(|&x| x < -radius - tolerance)
    };
    let edges = [b - a, c - b, a - c];
    !separates(edges[0].cross(c - a))
        && !edges.iter().any(|edge| [DVec3::X, DVec3::Y, DVec3::Z].iter().any(|axis| separates(edge.cross(*axis))))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent convex clipping oracle. Retain the triangle polygon in each of the six AABB half-spaces.
    fn clipped(triangle: [DVec3; 3], bounds: &Bounds) -> bool {
        let mut polygon = triangle.to_vec();
        for axis in 0..3 {
            for (side, limit) in bounds.iter().enumerate() {
                if polygon.is_empty() {
                    return false;
                }
                let plane = f64::from(limit[axis]);
                let inside = |p: DVec3| if side == 0 { p[axis] >= plane } else { p[axis] <= plane };
                let mut output = Vec::new();
                let mut prev = *polygon.last().unwrap();
                for &p in &polygon {
                    if inside(prev) != inside(p) {
                        let alpha = (plane - prev[axis]) / (p[axis] - prev[axis]);
                        output.push(prev + (p - prev) * alpha);
                    }
                    if inside(p) {
                        output.push(p);
                    }
                    prev = p;
                }
                polygon = output;
            }
        }
        !polygon.is_empty()
    }

    #[test]
    fn separating_axes_agree_with_clipping_on_oblique_and_flat_triangles() {
        let mut separated = 0;
        let mut intersecting = 0;
        for vertices in [
            [[-4., 0., -3.], [5., 0., -3.], [-4., 0., 6.]],
            [[-4., 1., 2.], [5., 1., -3.], [-4., 7., -3.]],
            [[2., -5., -2.], [-4., 3., 1.], [6., 4., 8.]],
        ] {
            let points = vertices.map(DVec3::from_array);
            let lo = points[0].min(points[1]).min(points[2]);
            let hi = points[0].max(points[1]).max(points[2]);
            for x in -5..=6 {
                for y in -6..=7 {
                    for z in -4..=8 {
                        let p = DVec3::new(f64::from(x) + 0.125, f64::from(y) - 0.0625, f64::from(z) + 0.1875);
                        let h = DVec3::new(0.28125, 0.34375, 0.46875);
                        let bounds = [(p - h).as_vec3().to_array(), (p + h).as_vec3().to_array()];
                        let in_bounds = overlaps(&[lo.as_vec3().to_array(), hi.as_vec3().to_array()], &bounds);
                        let result = in_bounds && triangle_may_intersect(points, &bounds);
                        assert_eq!(result, clipped(points, &bounds), "triangle {vertices:?}, swept box {bounds:?}");
                        intersecting += usize::from(result);
                        separated += usize::from(!result && in_bounds);
                    }
                }
            }
        }
        assert!(intersecting > 50 && separated > 100);
    }

    #[test]
    fn exact_edge_vertex_and_face_tangencies_remain_blocked() {
        let tri = [[0., 0., 0.], [4., 0., 0.], [0., 0., 4.]].map(DVec3::from_array);
        for bounds in [
            [[2., 0., 2.], [2.5, 1., 2.5]],
            [[4., 0., 0.], [5., 1., 1.]],
            [[0.5, -1., 0.5], [1., 0., 1.]],
            [[2., 0., 2.], [2., 0., 2.]],
        ] {
            assert!(triangle_may_intersect(tri, &bounds));
        }
        assert!(!triangle_may_intersect(tri, &[[2.0001, 0., 2.], [2.5, 1., 2.5]]));
    }

    #[test]
    fn unknown_shapes_and_incomplete_or_unusable_certificates_keep_the_box() {
        let bounds = [[0., 0., 0.], [4., 0., 4.]];
        let region = [[3., -0.1, 3.], [3.1, 0.1, 3.1]];
        let residual = Residual { bounds, shape: Some(2), faces: 2 };
        let mut refs = Refinements::default();
        assert!(refs.intersects(0, &residual, &region));
        let lower = 1 << 3 | 4 << 6;
        validate_code(&bounds, lower).unwrap();
        for bad in [0, 7, 0x200 | lower, 1 | (1 << 3) | (4 << 6), 2 | (4 << 3) | (5 << 6)] {
            assert!(validate_code(&bounds, bad).is_err());
        }
        refs.by_tile.entry(0).or_default().insert(key(1, bounds), lower);
        assert!(refs.intersects(0, &residual, &region));
        let unknown = Residual { bounds, shape: None, faces: 1 };
        assert!(refs.intersects(0, &unknown, &region));
        let known = Residual { bounds, shape: Some(1), faces: 1 };
        assert!(!refs.intersects(0, &known, &region));
        assert!(refs.intersects(1, &known, &region));
    }
}
