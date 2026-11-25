// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::{Result, ensure};
use bevy::math::DVec3;

#[derive(Clone)]
pub(super) struct Opening {
    pub corners: [DVec3; 4],
    normal: DVec3,
    pub center: DVec3,
    pub radius: f64,
}

impl Opening {
    pub(super) fn new(mut corners: [DVec3; 4]) -> Result<Self> {
        let [a, b, c, d] = corners;
        let center = (a + b + c + d) * 0.25;
        let radius = corners.iter().map(|p| p.distance(center)).fold(0., f64::max);
        let normal = (b - a).cross(d - a).normalize_or_zero();
        ensure!(
            corners.iter().all(|p| p.is_finite()) && normal != DVec3::ZERO && radius.is_finite(),
            "degenerate aperture"
        );
        ensure!((c - a).dot(normal).abs() <= radius * 1e-6, "nonplanar aperture");
        corners[2] -= (c - a).dot(normal) * normal;
        let center = corners.iter().copied().sum::<DVec3>() * 0.25;
        let radius = corners.iter().map(|p| p.distance(center)).fold(0., f64::max);
        for i in 0..4 {
            let edge = corners[(i + 1) % 4] - corners[i];
            let next = corners[(i + 2) % 4] - corners[(i + 1) % 4];
            ensure!(
                edge.cross(next).dot(normal) > edge.length() * next.length() * 1e-12,
                "aperture must be a convex ordered quadrilateral"
            );
        }
        Ok(Self { corners, normal, center, radius })
    }

    fn best_crossing(&self, a: DVec3, b: DVec3) -> DVec3 {
        // Replace both points by their projections and positive heights. Unfolding
        // the second point across the plane gives the optimum on the infinite plane.
        let h = (a - self.corners[0]).dot(self.normal);
        let k = (b - self.corners[0]).dot(self.normal);
        let u = a - h * self.normal;
        let v = b - k * self.normal;
        let total = h.abs() + k.abs();
        let t = if total > 0. { h.abs() / total } else { 0.5 };
        let p = u.lerp(v, t);
        if (0..4)
            .all(|i| (self.corners[(i + 1) % 4] - self.corners[i]).cross(p - self.corners[i]).dot(self.normal) >= 0.)
        {
            return p;
        }
        // The constrained minimum then lies on an edge. Minimize exactly
        // along each finite segment, including its endpoints.
        let mut best = self.corners[0];
        let mut cost = a.distance(best) + b.distance(best);
        for i in 0..4 {
            let start = self.corners[i];
            let delta = self.corners[(i + 1) % 4] - start;
            let length = delta.length();
            let axis = delta / length;
            let project = |p: DVec3| {
                let d = p - start;
                let x = d.dot(axis);
                (x, (d - x * axis).length())
            };
            let (x, r) = project(a);
            let (y, s) = project(b);
            let at = if r + s > 0. { x + (y - x) * (r / (r + s)) } else { (x + y) * 0.5 };
            let point = start + axis * at.clamp(0., length);
            let value = a.distance(point) + b.distance(point);
            if value < cost {
                best = point;
                cost = value;
            }
        }
        best
    }
}

pub(super) struct Path {
    pub crossings: Vec<DVec3>,
    pub length: f64,
    pub lower: f64,
}

/// Minimize the length of the polyline through an ordered list of convex openings.
/// The tangent plane of the convex length on the product of polygons gives
/// a separate lower bound even when two consecutive crossing points coincide.
pub(super) fn shortest(start: DVec3, end: DVec3, openings: &[&Opening]) -> Path {
    let mut points = Vec::with_capacity(openings.len() + 2);
    points.push(start);
    points.extend(openings.iter().map(|o| o.center));
    points.push(end);
    let n = openings.len();
    let mut length = start.distance(end);
    let mut lower = length;
    for _ in 0..128 {
        for i in (0..n).chain((0..n).rev()) {
            points[i + 1] = openings[i].best_crossing(points[i], points[i + 2]);
        }
        length = points.windows(2).map(|p| p[0].distance(p[1])).sum();
        let mut gap = 0.;
        for (i, o) in openings.iter().enumerate() {
            let p = points[i + 1];
            let gradient = (p - points[i]).normalize_or_zero() + (p - points[i + 2]).normalize_or_zero();
            gap += o.corners.iter().map(|v| gradient.dot(p - *v)).fold(0., f64::max);
        }
        let roundoff = (n + 1) as f64 * 64. * f64::EPSILON * (length + start.abs().max(end.abs()).max_element() + 1.);
        lower = lower.max(length - gap - roundoff).min(length);
        if length - lower <= 1e-7 + length * 1e-11 {
            break;
        }
    }
    points.pop();
    points.remove(0);
    Path { crossings: points, length, lower }
}
