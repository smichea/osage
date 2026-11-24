// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Continuous first contact of an upright, translating capsule with fixed triangle meshes.
//! The spatial tree is an index into the supplied surfaces. Their order and ownership are preserved.

use super::StaticMesh;
use anyhow::{Result, ensure};
use glam::DVec3;
use rapier3d::parry::{bounding_volume::Aabb, partitioning::Bvh};
use serde::Serialize;

/// Contact classification includes a one-nanometre geometric tolerance in the query frame.
const DIST_EPS: f64 = 1e-9;
const TIME_EPS: f64 = 1e-9;

#[derive(Clone, Copy)]
pub struct Flight {
    pub center: DVec3,
    pub velocity: DVec3,
    pub half_segment: f64,
    pub radius: f64,
    pub gravity: f64,
    pub terminal_speed: f64,
    pub duration: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct FlightHit {
    pub seconds: f64,
    pub surface: usize,
    pub triangle: usize,
    pub point: [f64; 3],
    pub normal: [f64; 3],
    pub center: [f64; 3],
}

struct Face {
    surface: usize,
    triangle: usize,
    vertices: [DVec3; 3],
}

pub struct BallisticScene {
    faces: Vec<Face>,
    bvh: Bvh,
}

#[derive(Clone, Copy)]
struct Arc {
    start: DVec3,
    velocity: DVec3,
    gravity: f64,
}

impl Arc {
    fn at(self, t: f64) -> DVec3 {
        self.start + self.velocity * t - DVec3::Y * (0.5 * self.gravity * t * t)
    }

    fn bounds(self, dt: f64, half: f64, radius: f64) -> Aabb {
        let end = self.at(dt);
        let mut high = self.start.max(end);
        if self.gravity > 0. {
            high.y = high.y.max(self.at((self.velocity.y / self.gravity).clamp(0., dt)).y);
        }
        let extent = DVec3::splat(radius + DIST_EPS) + DVec3::Y * half;
        Aabb::new(
            (self.start.min(end) - extent).as_vec3().to_array().map(f32::next_down).into(),
            (high + extent).as_vec3().to_array().map(f32::next_up).into(),
        )
    }

    // For two disjoint convex sets the closest pair defines a separating plane.
    // Translation preserves the capsule's support value relative to its centre.
    fn min_shift(self, n: DVec3, mid: f64, lo: f64, hi: f64) -> f64 {
        let a = -self.gravity * n.y;
        let v = self.velocity.dot(n) + a * mid;
        let displacement = |t: f64| t * (v + 0.5 * a * t);
        let (l, r) = (lo - mid, hi - mid);
        let mut min = displacement(l).min(displacement(r));
        if a > 0. {
            min = min.min(displacement((-v / a).clamp(l, r)));
        }
        min
    }
}

impl BallisticScene {
    pub fn new(meshes: &[StaticMesh]) -> Result<Self> {
        let mut faces = Vec::new();
        let mut bounds = Vec::new();
        for (surface, mesh) in meshes.iter().enumerate() {
            for (triangle, ids) in mesh.triangles.iter().enumerate() {
                ensure!(ids.iter().all(|&i| (i as usize) < mesh.vertices.len()), "bad triangle in {}", mesh.id);
                let vs = ids.map(|i| DVec3::from_array(mesh.vertices[i as usize].map(f64::from)));
                ensure!(vs.iter().all(|p| p.is_finite()), "non-finite triangle in {}", mesh.id);
                let [a, b, c] = vs;
                bounds.push(Aabb::new(a.min(b).min(c).as_vec3(), a.max(b).max(c).as_vec3()));
                faces.push(Face { surface, triangle, vertices: vs });
            }
        }
        ensure!(u32::try_from(faces.len()).is_ok(), "too many indexed faces");
        Ok(Self { faces, bvh: Bvh::from_leaves(Default::default(), &bounds) })
    }

    pub fn first_impact(&self, flight: Flight) -> Result<Option<FlightHit>> {
        let Flight { center, mut velocity, gravity, terminal_speed, duration, radius, half_segment } = flight;
        ensure!(
            center.is_finite()
                && velocity.is_finite()
                && [gravity, duration, radius, half_segment, terminal_speed].iter().all(|x| x.is_finite())
                && gravity >= 0.
                && duration >= 0.
                && radius > 0.
                && half_segment >= 0.
                && terminal_speed > 0.,
            "invalid capsule flight"
        );
        velocity.y = velocity.y.max(-terminal_speed);
        let dt = if gravity > 0. { duration.min((velocity.y + terminal_speed) / gravity) } else { duration };
        let arc = Arc { start: center, velocity, gravity };
        ensure!(arc.at(dt).is_finite() && arc.at(duration).is_finite(), "flight outside numerical range");
        if let Some(hit) = self.interval(arc, dt, half_segment, radius) {
            return Ok(Some(hit));
        }
        if dt < duration {
            velocity.y = -terminal_speed;
            let steady = Arc { start: arc.at(dt), velocity, gravity: 0. };
            if let Some(mut hit) = self.interval(steady, duration - dt, half_segment, radius) {
                hit.seconds += dt;
                return Ok(Some(hit));
            }
        }
        Ok(None)
    }

    fn interval(&self, arc: Arc, dt: f64, half: f64, radius: f64) -> Option<FlightHit> {
        let mut best: Option<FlightHit> = None;
        let bounds = arc.bounds(dt, half, radius);
        for i in self.bvh.intersect_aabb(&bounds) {
            let f = &self.faces[i as usize];
            let cap = best.map_or(dt, |h| (h.seconds + TIME_EPS).min(dt));
            if let Some((seconds, pair)) = f.first(arc, cap, half, radius) {
                let earlier = best.is_none_or(|h| {
                    seconds < h.seconds - TIME_EPS
                        || ((seconds - h.seconds).abs() <= TIME_EPS
                            && (f.surface, f.triangle) < (h.surface, h.triangle))
                });
                if earlier {
                    let face_normal =
                        (f.vertices[1] - f.vertices[0]).cross(f.vertices[2] - f.vertices[0]).normalize_or_zero();
                    let towards_start = if face_normal.dot(arc.velocity) > 0. { -face_normal } else { face_normal };
                    let normal = (pair.0 - pair.1)
                        .try_normalize()
                        .or_else(|| towards_start.try_normalize())
                        .or_else(|| (-arc.velocity).try_normalize())
                        .unwrap_or(DVec3::Y);
                    best = Some(FlightHit {
                        seconds,
                        surface: f.surface,
                        triangle: f.triangle,
                        point: pair.1.to_array(),
                        normal: normal.to_array(),
                        center: arc.at(seconds).to_array(),
                    });
                }
            }
        }
        best
    }
}

type Pair = (DVec3, DVec3);

impl Face {
    // Closest points on the capsule spine and the closed triangle, including its edges.
    fn closest(&self, center: DVec3, half: f64) -> Pair {
        let [a, b, c] = self.vertices;
        let mut best = (center, a);
        let mut best_d2 = f64::INFINITY;
        let mut submit = |spine: DVec3, triangle: DVec3| {
            let d2 = spine.distance_squared(triangle);
            if d2 < best_d2 {
                best = (spine, triangle);
                best_d2 = d2;
            }
        };
        let low = center - DVec3::Y * half;
        let high = center + DVec3::Y * half;
        let edges = [(a, b), (b, c), (c, a)];
        let n = (b - a).cross(c - a);
        if let Some(unit) = n.try_normalize() {
            let contains = |p| edges.iter().all(|&(v, w)| (w - v).cross(p - v).dot(n) >= 0.);
            // Signed height along the spine is linear. Its closest point to the face
            // is an endpoint or the intersection of the spine with the face plane.
            let height = (center - a).dot(unit);
            let offset = if unit.y != 0. { (-height / unit.y).clamp(-half, half) } else { 0. };
            let spine = center + DVec3::Y * offset;
            let projected = spine - unit * (spine - a).dot(unit);
            if contains(projected) {
                submit(spine, projected);
            }
        }
        for (v, w) in edges {
            let d = w - v;
            let horizontal = DVec3::new(d.x, 0., d.z);
            let along = |offset: DVec3, axis: DVec3| {
                let len = axis.length_squared();
                if len > 0. { (offset.dot(axis) / len).clamp(0., 1.) } else { 0. }
            };
            // In the open spine interval only horizontal distance matters. Outside
            // it the nearest point is an endpoint. These are the three quadratic minima.
            for t in [0., 1., along(low - v, d), along(high - v, d), along(center - v, horizontal)] {
                let on_edge = v + t * d;
                let spine = DVec3::new(center.x, on_edge.y.clamp(low.y, high.y), center.z);
                submit(spine, on_edge);
            }
        }
        best
    }

    fn first(&self, arc: Arc, cap: f64, half: f64, radius: f64) -> Option<(f64, Pair)> {
        let start = self.closest(arc.start, half);
        if start.0.distance(start.1) <= radius + DIST_EPS {
            return Some((0., start));
        }
        let mut intervals = vec![(0., cap)];
        while let Some((lo, hi)) = intervals.pop() {
            let mid = lo + 0.5 * (hi - lo);
            let pair = self.closest(arc.at(mid), half);
            let delta = pair.0 - pair.1;
            let dist = delta.length();
            if dist > radius + DIST_EPS && dist - radius + arc.min_shift(delta / dist, mid, lo, hi) > DIST_EPS {
                continue;
            }
            if (hi - lo <= TIME_EPS && dist <= radius + DIST_EPS) || mid == lo || mid == hi {
                if dist <= radius + DIST_EPS {
                    return Some((mid, pair));
                }
                continue;
            }
            intervals.push((mid, hi));
            intervals.push((lo, mid));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(points: &[[f32; 3]], indices: &[[u32; 3]]) -> BallisticScene {
        BallisticScene::new(&[StaticMesh {
            material: None,
            id: "test".into(),
            vertices: points.to_vec(),
            triangles: indices.to_vec(),
        }])
        .unwrap()
    }

    fn drop(center: [f64; 3], velocity: [f64; 3]) -> Flight {
        Flight {
            center: center.into(),
            velocity: velocity.into(),
            radius: 0.25,
            half_segment: 0.5,
            gravity: 10.,
            terminal_speed: 50.,
            duration: 4.,
        }
    }

    fn floor() -> BallisticScene {
        scene(&[[-1., 0., -1.], [-1., 0., 1.], [1., 0., 1.], [1., 0., -1.]], &[[0, 1, 2], [0, 2, 3]])
    }

    #[test]
    fn free_fall_and_terminal_transition_use_continuous_time() {
        let world = floor();
        for (height, vel, expected) in [(5.75, 0., 1.), (5.75, -49., 0.101), (1.05, -50., 0.006)] {
            let h = world.first_impact(drop([0., height, 0.], [0., vel, 0.])).unwrap().unwrap();
            assert!((h.seconds - expected).abs() < 2e-8, "{h:?}");
            assert_eq!((h.surface, h.triangle), (0, 0));
            assert!(DVec3::from(h.point).length() < 1e-8);
            assert_eq!(h.normal, [0., 1., 0.]);
        }
    }

    #[test]
    fn detects_rounded_edge_corner_tangent_and_clear_miss() {
        let world = floor();
        for (x, z, radial) in [(1.15, 0., 0.15), (1.09, 1.12, 0.15), (1.25, 0., 0.25)] {
            let h = world.first_impact(Flight { gravity: 0., ..drop([x, 2.5, z], [0., -40., 0.]) }).unwrap().unwrap();
            let t = (2. - f64::sqrt(0.25 * 0.25 - radial * radial)) / 40.;
            // A grazing contact converts one nanometre of spatial tolerance into sub-microsecond time error.
            assert!((h.seconds - t).abs() < 7e-7, "{h:?}");
            assert!((h.point[0] - 1.).abs() < 1e-8);
        }
        assert!(world.first_impact(drop([1.25001, 3., 0.], [0., -50., 0.])).unwrap().is_none());
    }

    #[test]
    fn ceiling_on_rising_arc_and_initial_spine_intersection() {
        let world = floor();
        let h = world.first_impact(drop([0., -1.75, 0.], [0., 10., 0.])).unwrap().unwrap();
        assert!((h.seconds - (1. - 0.8f64.sqrt())).abs() < 1e-8);
        assert_eq!(h.normal, [0., -1., 0.]);
        assert_eq!(world.first_impact(drop([0., 0.1, 0.], [0., 0., 0.])).unwrap().unwrap().seconds, 0.);
    }

    #[test]
    fn near_vertical_side_slope_and_degenerate_segment() {
        let wall = scene(&[[1., -2., -2.], [1., 2., -2.], [1., 0., 2.]], &[[0, 1, 2]]);
        let h = wall.first_impact(drop([0., 0.5, 0.], [4., 0., 0.])).unwrap().unwrap();
        assert!((h.seconds - 0.1875).abs() < 2e-8);
        assert_eq!(h.normal, [-1., 0., 0.]);
        let incline = scene(&[[-3., -3., -3.], [3., 3., -3.], [0., 0., 3.]], &[[0, 1, 2]]);
        let h = incline.first_impact(Flight { gravity: 0., ..drop([0., 3., 0.], [0., -1., 0.]) }).unwrap().unwrap();
        assert!((h.seconds - (2.5 - 0.25 * 2f64.sqrt())).abs() < 1e-8);
        let segment = scene(&[[-1., 0., 0.], [1., 0., 0.]], &[[0, 0, 1]]);
        let h = segment.first_impact(drop([0., 5.75, 0.], [0., 0., 0.])).unwrap().unwrap();
        assert!((h.seconds - 1.).abs() < 1e-8);
    }

    #[test]
    fn empty_scene_and_invalid_route() {
        let empty = BallisticScene::new(&[]).unwrap();
        assert!(empty.first_impact(drop([0.; 3], [0.; 3])).unwrap().is_none());
        assert!(empty.first_impact(Flight { radius: f64::NAN, ..drop([0.; 3], [0.; 3]) }).is_err());
    }
}
