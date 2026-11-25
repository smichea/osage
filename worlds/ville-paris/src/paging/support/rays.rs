// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{Bounds, Guard, Index};
use crate::paging::channels::Segment;
use bevy::math::DVec3;
use osage_objects::TriangleRay;
use rapier3d::prelude::ColliderHandle;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(in crate::paging) struct Gap {
    pub(in crate::paging) tile: usize,
    pub(in crate::paging) slot: Option<usize>,
    pub(in crate::paging) bounds: Bounds,
    triangle: Option<[[f32; 3]; 3]>,
    faces: usize,
    reason: &'static str,
}

impl Guard {
    /// Only an active certified collider contributes resident geometry. The catalogue
    /// can still exclude regions containing no missing face without reading the payload.
    pub(in crate::paging) fn ray_gaps(&self, index: &Index, full: &BTreeMap<usize, Vec<ColliderHandle>>) -> Vec<Gap> {
        let mut gaps = Vec::new();
        for (i, t) in index.tiles.iter().enumerate() {
            let Some(p) = &t.physics else {
                continue;
            };
            if full.contains_key(&i) {
                continue;
            }
            if self.active.contains_key(&i) {
                if self.mode == "legacy_synchronous" {
                    continue;
                }
                for residual in &self.residual[i] {
                    gaps.push(Gap {
                        tile: i,
                        slot: residual.shape,
                        bounds: residual.bounds,
                        triangle: self.refinements.triangle(i, residual),
                        faces: residual.faces,
                        reason: "residual",
                    });
                }
            } else {
                gaps.push(Gap {
                    tile: i,
                    slot: (p.ids.len() == 1).then_some(0),
                    bounds: p.bounds,
                    triangle: None,
                    faces: p.triangles,
                    reason: "missing_page",
                });
            }
        }
        gaps
    }
}

impl Gap {
    pub(in crate::paging) fn report(&self, index: &Index) -> Value {
        let t = &index.tiles[self.tile];
        let layer = t.physics.as_ref().expect("gap on collision page");
        json!({"tile":[t.x,t.z],"reason":self.reason,"bounds":self.bounds,"triangles":self.faces,
            "id":self.slot.and_then(|slot|layer.ids.get(slot)),
            "source_node":self.slot.and_then(|slot|layer.source_nodes.get(slot))})
    }

    pub(in crate::paging) fn may_intersect(&self, ray: TriangleRay, endpoints: Segment) -> bool {
        let Some(xyz) = self.triangle else {
            return true;
        };
        if ray.hit(xyz).is_some() {
            return true;
        }
        // The two-sided crossing primitive has no single hit for a coplanar segment.
        // A segment and a triangle are convex. Along any separating direction their
        // closed projections are disjoint, including for a coplanar segment.
        let [a, b, c] = xyz.map(|p| DVec3::from_array(p.map(f64::from)));
        let endpoints = [endpoints.0, endpoints.1].map(|p| DVec3::from_array(p.map(f64::from)) - a);
        let vertices = [DVec3::ZERO, b - a, c - a];
        let edges = [b - a, c - b, a - c];
        let delta = endpoints[1] - endpoints[0];
        let normal = edges[0].cross(c - a);
        let envelope = vertices[1].abs().max(vertices[2].abs()).max(endpoints[0].abs()).max(endpoints[1].abs());
        let separates = |axis: DVec3| {
            let t = vertices.map(|p| axis.dot(p));
            let s = endpoints.map(|p| axis.dot(p));
            let margin = 64. * f64::EPSILON * envelope.dot(axis.abs());
            t.into_iter().all(|v| v > s[0].max(s[1]) + margin) || t.into_iter().all(|v| v < s[0].min(s[1]) - margin)
        };
        !separates(normal)
            && !separates(normal.cross(delta))
            && !edges.iter().any(|&edge| separates(edge.cross(delta)) || separates(normal.cross(edge)))
    }
}
