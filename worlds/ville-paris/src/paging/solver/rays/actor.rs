// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{Intersection, Segment, World};
use bevy::math::{DQuat, DVec3};
use osage_objects::CollisionMaterial;
use serde::Serialize;

#[derive(Serialize)]
pub(super) struct ActorContact {
    part: usize,
    normal: [f64; 3],
}

#[derive(Serialize)]
pub(super) struct ActorSpan<'a> {
    pub part: usize,
    parent: Option<usize>,
    name: &'a str,
    pub distance_range_m: [f64; 2],
    pub points_m: [[f64; 3]; 2],
    length_m: f64,
    surface_only: bool,
}

// Interval of an infinite line inside a ball. Projecting perpendicular to the
// direction avoids subtracting two large squared lengths on a long segment.
fn ball(p: DVec3, d: DVec3, radius: f64) -> Option<[f64; 2]> {
    let speed = d.length();
    if speed == 0. {
        return (p.length() <= radius).then_some([f64::NEG_INFINITY, f64::INFINITY]);
    }
    let unit = d / speed;
    let offset = p.cross(unit).length();
    if offset > radius {
        return None;
    }
    let near = -p.dot(unit);
    let reach = ((radius - offset) * (radius + offset)).sqrt();
    Some([(near - reach) / speed, (near + reach) / speed])
}

// A Y capsule is the union of the finite cylinder and its two end balls.
// Convexity makes the union of the three line intervals a single interval.
fn capsule(p: DVec3, d: DVec3, half: f64, radius: f64) -> Option<[f64; 2]> {
    if half == 0. {
        return ball(p, d, radius);
    }
    let radial = |v: DVec3| DVec3::new(v.x, 0., v.z);
    let cylinder = ball(radial(p), radial(d), radius).and_then(|[lo, hi]| {
        if d.y == 0. {
            return (p.y.abs() <= half).then_some([lo, hi]);
        }
        let a = (-half - p.y) / d.y;
        let b = (half - p.y) / d.y;
        let clipped = [lo.max(a.min(b)), hi.min(a.max(b))];
        (clipped[0] <= clipped[1]).then_some(clipped)
    });
    [cylinder, ball(p - DVec3::Y * half, d, radius), ball(p + DVec3::Y * half, d, radius)]
        .into_iter()
        .flatten()
        .reduce(|a, b| [a[0].min(b[0]), a[1].max(b[1])])
}

impl World {
    pub(super) fn probe_actor<'a>(
        &'a self,
        index: usize,
        (from, to): Segment,
        hits: &mut Vec<Intersection<'a>>,
        spans: &mut Vec<ActorSpan<'a>>,
    ) {
        let part = &self.parts[index];
        let pose = self.sim.part_collider(index).position();
        let center = DVec3::from(pose.translation.to_array().map(f64::from));
        let rotation = DQuat::from_array(pose.rotation.to_array().map(f64::from)).normalize();
        let start = DVec3::from(from.map(f64::from));
        let end = DVec3::from(to.map(f64::from));
        let delta = end - start;
        let length = delta.length();
        // Measure roots from the point on the line nearest the body. On a very
        // long probe, both surface points can round to the same distance from its start.
        let unit = delta / length;
        let origin = if start.distance_squared(center) <= end.distance_squared(center) { start } else { end };
        let offset = unit.cross((origin - center).cross(unit));
        let local = rotation.conjugate() * offset;
        let direction = rotation.conjugate() * unit;
        let first = (start - center).dot(unit);
        let last = (end - center).dot(unit);
        let half = f64::from(part.half_segment);
        let radius = f64::from(part.radius);
        let Some([lo, hi]) = capsule(local, direction, half, radius) else { return };
        let interval = [lo.max(first), hi.min(last)];
        if interval[0] > interval[1] {
            return;
        }
        let side = half > 0. && direction.x == 0. && direction.z == 0. && local.x.hypot(local.z) == radius;
        let points = interval.map(|t| {
            if t == first {
                start
            } else if t == last {
                end
            } else {
                center + offset + t * unit
            }
        });
        spans.push(ActorSpan {
            part: index,
            parent: part.parent,
            name: &part.name,
            distance_range_m: interval.map(|t| t - first),
            points_m: points.map(|p| p.to_array()),
            length_m: interval[1] - interval[0],
            surface_only: side || interval[0] == interval[1],
        });
        for (i, t) in interval.into_iter().enumerate() {
            if (i == 1 && t == interval[0]) || (!side && t != lo && t != hi) {
                continue;
            }
            let p = local + t * direction;
            let normal = rotation * (p - DVec3::Y * p.y.clamp(-half, half)).normalize();
            hits.push(Intersection {
                id: &part.name,
                source_node: None,
                source_triangle: None,
                local_triangle: None,
                box_face: None,
                actor: Some(ActorContact { part: index, normal: normal.to_array() }),
                distance_m: t - first,
                point_m: points[i].to_array(),
                tile: None,
                geometry: if half == 0. { "actor_sphere" } else { "actor_capsule" },
                material: None,
                coefficients: part
                    .material
                    .unwrap_or(CollisionMaterial { friction: crate::physics::CAPSULE.friction, restitution: 0. }),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capsule_intervals_cover_tangencies_axial_spans_and_near_misses() {
        assert_eq!(capsule(DVec3::new(1., -4., 0.), DVec3::Y, 2., 1.), Some([2., 6.]));
        assert_eq!(capsule(DVec3::new(1., -4., 0.), DVec3::Y, 0., 1.), Some([4., 4.]));
        assert_eq!(capsule(DVec3::new(f64::from(1_f32.next_up()), -4., 0.), DVec3::Y, 2., 1.), None);
        assert_eq!(capsule(DVec3::ZERO, DVec3::Y, 2., 1.), Some([-3., 3.]));
        assert_eq!(capsule(DVec3::new(-1e6, 2., 0.), DVec3::X, 2., 1.), Some([999999., 1000001.]));
    }
}
