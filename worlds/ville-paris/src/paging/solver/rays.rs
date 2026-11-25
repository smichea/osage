// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Finite segment probes on one snapshot of installed terrain and moving supports at their current solver pose.
mod actor;

use super::{Owner, World};
use crate::paging::{
    channels::Segment,
    support::{Gap, Guard},
    tiles::Index,
};
use anyhow::{Context, Result};
use bevy::math::{DQuat, DVec3};
use osage_objects::{CollisionMaterial, TriangleRay};
use rapier3d::{
    parry::partitioning::{Bvh, TraversalAction},
    prelude::{Aabb, ColliderHandle, Vector},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

enum Target<'a> {
    Installed(&'a Owner),
    Box(&'a Owner),
    Actor(usize),
    Missing(Gap),
}

#[derive(Serialize)]
struct Intersection<'a> {
    id: &'a str,
    source_node: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_triangle: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_triangle: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    box_face: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor: Option<actor::ActorContact>,
    distance_m: f64,
    point_m: [f64; 3],
    tile: Option<[i32; 2]>,
    geometry: &'static str,
    material: Option<&'a str>,
    coefficients: CollisionMaterial,
}

#[derive(Serialize)]
struct FaceOverlap<'a> {
    id: &'a str,
    geometry: &'static str,
    box_face: &'static str,
    from_m: [f64; 3],
    to_m: [f64; 3],
    distance_range_m: [f64; 2],
}

// Clip the segment against the other two face axes. A single grazing point
// is already represented by the transverse face it meets.
fn coplanar_interval(a: DVec3, d: DVec3, half: [f64; 3], axis: usize) -> Option<[f64; 2]> {
    let (mut lo, mut hi) = (0_f64, 1_f64);
    for i in (0..3).filter(|&i| i != axis) {
        if d[i] == 0. {
            if a[i].abs() > half[i] {
                return None;
            }
        } else {
            let u = (-half[i] - a[i]) / d[i];
            let v = (half[i] - a[i]) / d[i];
            lo = lo.max(u.min(v));
            hi = hi.min(u.max(v));
        }
    }
    (lo < hi).then_some([lo, hi])
}

#[derive(Default, Serialize)]
struct Stats {
    resident_meshes: usize,
    resident_boxes: usize,
    boxes_tested: usize,
    collider_and_coverage_bounds_tested: usize,
    triangle_bounds_tested: usize,
    triangles_tested: usize,
}

impl World {
    fn mesh_hits<'a>(
        &self,
        owner: &'a Owner,
        material: Option<&'a str>,
        ray: TriangleRay,
        stats: &mut Stats,
        hits: &mut Vec<Intersection<'a>>,
    ) {
        // Terrain vertices were placed into world space before insertion
        // with an identity native collider pose, including mirrored nodes.
        let mesh = self.sim.collider(owner.handle).shape().as_trimesh().expect("static terrain mesh");
        mesh.bvh().traverse(|child| {
            stats.triangle_bounds_tested += 1;
            let b = child.aabb();
            if !ray.intersects_bounds(b.mins.to_array(), b.maxs.to_array()) {
                return TraversalAction::Prune;
            }
            if let Some(local) = child.leaf_data() {
                stats.triangles_tested += 1;
                let t = mesh.triangle(local);
                if let Some(distance_m) = ray.hit([t.a.to_array(), t.b.to_array(), t.c.to_array()]) {
                    hits.push(Intersection {
                        id: &owner.name,
                        source_node: owner.node,
                        source_triangle: owner.faces.source_triangle(local),
                        local_triangle: Some(local),
                        box_face: None,
                        actor: None,
                        distance_m,
                        point_m: ray.point(distance_m),
                        tile: owner.tile,
                        geometry: owner.geometry,
                        material,
                        coefficients: owner.material.unwrap_or(CollisionMaterial {
                            friction: crate::physics::CAPSULE.friction,
                            restitution: 0.,
                        }),
                    });
                }
            }
            TraversalAction::Continue
        });
    }

    fn box_hits<'a>(
        &self,
        owner: &'a Owner,
        segment: Segment,
        hits: &mut Vec<Intersection<'a>>,
        overlaps: &mut Vec<FaceOverlap<'a>>,
    ) {
        let collider = self.sim.collider(owner.handle);
        let half = collider.shape().as_cuboid().expect("moving support cuboid").half_extents.to_array().map(f64::from);
        let pose = collider.position();
        let center = DVec3::from(pose.translation.to_array().map(f64::from));
        let local = DQuat::from_array(pose.rotation.to_array().map(f64::from)).normalize().conjugate();
        let start = DVec3::from(segment.0.map(f64::from));
        let end = DVec3::from(segment.1.map(f64::from));
        let a = local * (start - center);
        let b = local * (end - center);
        let direction = b - a;
        let length = start.distance(end);
        // A primitive has six physical faces. Preserve both labels at a transverse edge
        // and report travel within a face as an interval.
        for (axis, names) in [["-x", "+x"], ["-y", "+y"], ["-z", "+z"]].into_iter().enumerate() {
            for (side, label) in [(-half[axis], names[0]), (half[axis], names[1])] {
                if direction[axis] == 0. {
                    if a[axis] == side
                        && let Some([lo, hi]) = coplanar_interval(a, direction, half, axis)
                    {
                        overlaps.push(FaceOverlap {
                            id: &owner.name,
                            geometry: owner.geometry,
                            box_face: label,
                            from_m: start.lerp(end, lo).to_array(),
                            to_m: start.lerp(end, hi).to_array(),
                            distance_range_m: [length * lo, length * hi],
                        });
                    }
                    continue;
                }
                let t = (side - a[axis]) / direction[axis];
                if !(0. ..=1.).contains(&t) {
                    continue;
                }
                let point = a + direction * t;
                if (0..3).any(|i| i != axis && point[i].abs() > half[i]) {
                    continue;
                }
                hits.push(Intersection {
                    id: &owner.name,
                    source_node: owner.node,
                    source_triangle: None,
                    local_triangle: None,
                    box_face: Some(label),
                    actor: None,
                    distance_m: length * t,
                    point_m: start.lerp(end, t).to_array(),
                    tile: owner.tile,
                    geometry: owner.geometry,
                    material: None,
                    coefficients: owner.material.expect("moving support material"),
                });
            }
        }
    }

    pub(in crate::paging) fn probe_segments(
        &self,
        index: &Index,
        guard: &Guard,
        full: &BTreeMap<usize, Vec<ColliderHandle>>,
        segments: &[Segment],
        include_actor: bool,
    ) -> Result<Vec<Value>> {
        let rays = segments
            .iter()
            .map(|&(from, to)| TriangleRay::new(from, to).context("invalid physical segment"))
            .collect::<Result<Vec<_>>>()?;
        let material_labels: HashMap<_, _> =
            index.object_profile.iter().flat_map(|p| p.collision_materials()).collect();
        let mut bounds = Vec::new();
        let mut targets = Vec::new();
        let mut installed: Vec<_> = self.owners.values().collect();
        installed.sort_unstable_by_key(|o| (o.tile, o.node, &o.name, o.handle.into_raw_parts()));
        let resident_meshes = installed.iter().filter(|o| o.tile.is_some()).count();
        let resident_boxes = installed.len() - resident_meshes;
        for owner in installed {
            let collider = self.sim.collider(owner.handle);
            bounds.push(collider.compute_aabb());
            targets.push(if owner.tile.is_some() { Target::Installed(owner) } else { Target::Box(owner) });
        }
        if include_actor {
            for index in 0..self.parts.len() {
                bounds.push(self.sim.part_collider(index).compute_aabb());
                targets.push(Target::Actor(index));
            }
        }
        for gap in guard.ray_gaps(index, full) {
            bounds.push(Aabb::new(Vector::from_array(gap.bounds[0]), Vector::from_array(gap.bounds[1])));
            targets.push(Target::Missing(gap));
        }
        u32::try_from(bounds.len()).context("too many collision probe bounds")?;
        let tree = Bvh::from_leaves(Default::default(), &bounds);
        let mut results = Vec::with_capacity(rays.len());
        for (ray, &(from, to)) in rays.iter().zip(segments) {
            let mut hits = Vec::new();
            let mut gaps = Vec::new();
            let mut overlaps = Vec::new();
            let mut spans = Vec::new();
            let mut actor_parts_tested = 0;
            let mut stats = Stats { resident_meshes, resident_boxes, ..Default::default() };
            tree.traverse(|node| {
                stats.collider_and_coverage_bounds_tested += 1;
                let b = node.aabb();
                if !ray.intersects_bounds(b.mins.to_array(), b.maxs.to_array()) {
                    return TraversalAction::Prune;
                }
                if let Some(i) = node.leaf_data() {
                    match &targets[i as usize] {
                        Target::Actor(index) => {
                            actor_parts_tested += 1;
                            self.probe_actor(*index, (from, to), &mut hits, &mut spans);
                        }
                        Target::Missing(gap) => {
                            if gap.may_intersect(*ray, (from, to)) {
                                gaps.push(gap);
                            }
                        }
                        Target::Box(owner) => {
                            stats.boxes_tested += 1;
                            self.box_hits(owner, (from, to), &mut hits, &mut overlaps);
                        }
                        Target::Installed(owner) => {
                            let material =
                                owner.node.and_then(|n| material_labels.get(&(n, owner.name.as_str())).copied());
                            self.mesh_hits(owner, material, *ray, &mut stats, &mut hits);
                        }
                    }
                }
                TraversalAction::Continue
            });
            // A far origin can round two distinct surface distances to the same f64.
            // Their world points remain ordered along the largest direction component.
            let delta = DVec3::from(to.map(f64::from)) - DVec3::from(from.map(f64::from));
            let axis = delta.abs().max_position();
            let along = |a: [f64; 3], b: [f64; 3]| {
                if delta[axis] > 0. { a[axis].total_cmp(&b[axis]) } else { b[axis].total_cmp(&a[axis]) }
            };
            hits.sort_by(|a, b| {
                a.distance_m.total_cmp(&b.distance_m).then_with(|| along(a.point_m, b.point_m)).then_with(|| {
                    (a.source_node, a.id, a.source_triangle, a.tile, a.local_triangle, a.box_face).cmp(&(
                        b.source_node,
                        b.id,
                        b.source_triangle,
                        b.tile,
                        b.local_triangle,
                        b.box_face,
                    ))
                })
            });
            gaps.sort_by_key(|g| (g.tile, g.slot, g.bounds.map(|p| p.map(f32::to_bits))));
            let unknown = hits.iter().filter(|h| h.local_triangle.is_some() && h.source_triangle.is_none()).count();
            overlaps.sort_by(|a, b| {
                a.distance_range_m[0]
                    .total_cmp(&b.distance_range_m[0])
                    .then((a.id, a.box_face).cmp(&(b.id, b.box_face)))
            });
            let gaps: Vec<_> = gaps.into_iter().map(|g| g.report(index)).collect();
            let mut result = json!({"from":from,"to":to,"intersections":hits,"box_face_overlaps":overlaps,"coverage":{"complete":gaps.is_empty(),
                "gaps":gaps},"unknown_source_triangle_hits":unknown,"stats":stats});
            if include_actor {
                spans.sort_by(|a, b| {
                    a.distance_range_m[0]
                        .total_cmp(&b.distance_range_m[0])
                        .then_with(|| along(a.points_m[0], b.points_m[0]))
                        .then(a.part.cmp(&b.part))
                });
                result["actor_spans"] = json!(spans);
                result["stats"]["actor_parts_tested"] = json!(actor_parts_tested);
            }
            results.push(result);
        }
        Ok(results)
    }
}
