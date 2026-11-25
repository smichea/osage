// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The strongest visible path around one or two screen edges for an obscured contact.
use super::{
    arrival,
    transmission::{DirectPath, Geometry},
};
use crate::{object::AcousticEdge, replay::Route};
use bevy::math::DVec3;
use osage_objects::BAND_HZ;
use osage_paris_soundscape::{
    scene::{Head, Scene},
    space::{Vec3, vec3},
    ways::SOUND,
};
use osage_paris_voices::RATE;
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Serialize)]
pub(super) struct Bend {
    surface: String,
    owner_path: Vec<String>,
    point_m: [f64; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    exterior_wedge: Option<crate::object::ExteriorWedge>,
}

#[derive(Serialize)]
pub(super) struct EdgePath {
    pub arrival_frame: u64,
    pub surface: String,
    pub owner_path: Vec<String>,
    pub point_m: [f64; 3],
    pub listener_at_arrival_m: [f64; 3],
    pub bends: Vec<Bend>,
    pub legs_m: Vec<f64>,
    pub distance_m: f64,
    pub reference_distance_m: f64,
    pub single_edges_tested: usize,
    pub edge_pairs_tested: usize,
    pub excess_m: f64,
    pub edge_loss_db: [f64; 3],
    pub leg_transmission_energy: Vec<[f64; 3]>,
    pub received_energy: [f64; 3],
    pub ear_delay_frames: [f64; 2],
    #[serde(skip)]
    pub head: Head,
}

fn candidates(scene: Geometry, bridge: &Scene, direct: &DirectPath, seconds: f64) -> Vec<AcousticEdge> {
    if let Some(obj) = &scene.object {
        let mut edges = obj.sound_edges(scene.colliders, &direct.crossings);
        edges.extend(obj.solid_edges(scene.colliders, &direct.crossings));
        return edges;
    }
    let mut edges = Vec::new();
    if scene.bundled_bridge {
        for (i, s) in bridge.surfaces.iter().enumerate().filter(|(_, s)| s.occludes) {
            let owner = bridge.tree.path(s.node);
            if !direct.crossings.iter().any(|c| c.id == s.id && c.owner_path == owner) {
                continue;
            }
            let quad = bridge.corners(i, seconds);
            for j in 0..4 {
                edges.push(AcousticEdge {
                    wedge: None,
                    a: quad[j].array(),
                    b: quad[(j + 1) % 4].array(),
                    id: s.id.clone(),
                    owner_path: owner.iter().map(|p| (*p).to_owned()).collect(),
                });
            }
        }
    }
    edges
}

pub(super) struct Search<'a> {
    pub scene: Geometry<'a>,
    pub bridge: &'a Scene,
    pub route: &'a Route,
    pub hz: u32,
    pub emitted: u64,
    pub source: Vec3,
}

fn same_segment(a: &AcousticEdge, b: &AcousticEdge) -> bool {
    (a.a == b.a && a.b == b.b) || (a.a == b.b && a.b == b.a)
}

/// Thin rigid screen insertion loss as an energy ratio, including the CNOSSOS
/// inter-edge separation correction when the path turns at two rigid screens.
pub(super) fn screen_loss_ratios(excess: f64, inter_edge_m: Option<f64>) -> [f64; 3] {
    BAND_HZ.map(|f| {
        let wave = SOUND / f64::from(f);
        let correction = if let Some(separation) = inter_edge_m.filter(|v| *v > 0.3) {
            let k = (5. * wave / separation).powi(2);
            (1. + k) / (1. / 3. + k)
        } else {
            1.
        };
        3. + 40. * excess * correction / wave
    })
}

fn outside(edge: &AcousticEdge, bend: DVec3, adjacent: DVec3) -> bool {
    edge.wedge.as_ref().is_none_or(|wedge| {
        let direction = adjacent - bend;
        wedge.normals.iter().any(|&n| direction.dot(DVec3::from(n)) >= -wedge.tolerance_m)
    })
}

fn key(edge: &AcousticEdge) -> (&str, [u64; 3], [u64; 3]) {
    let (a, b) = (edge.a.map(f64::to_bits), edge.b.map(f64::to_bits));
    (edge.id.as_str(), a.min(b), a.max(b))
}

struct Choice {
    score: f64,
    path: Option<EdgePath>,
}

impl Search<'_> {
    pub fn best(&self, direct_frame: u64, direct: &DirectPath) -> Option<EdgePath> {
        self.observe(direct_frame, direct, |_, _, _| {})
    }

    pub(super) fn observe(
        &self,
        direct_frame: u64,
        direct: &DirectPath,
        mut observe: impl FnMut(Vec3, Vec3, &DirectPath),
    ) -> Option<EdgePath> {
        if direct.transmission_energy == [1.; 3] {
            return None;
        }
        let mut best = Choice { score: 0., path: None };
        let mut pairs = Vec::new();
        let mut singles = 0;
        for edge in candidates(self.scene, self.bridge, direct, direct_frame as f64 / f64::from(RATE)) {
            singles += 1;
            let Some(arr) = self.arrival(&edge, None) else { continue };
            let legs = self.trace_legs(&arr, &edge, None, &mut observe);
            // A second obstacle can be outside the direct ray but intersect a bent leg.
            // Start with that obstruction so expansion follows the actual geometry.
            for (i, (leg, seconds)) in legs.iter().enumerate() {
                if leg.transmission_energy == [1.; 3] {
                    continue;
                }
                for other in candidates(self.scene, self.bridge, leg, *seconds) {
                    if same_segment(&edge, &other) {
                        continue;
                    }
                    pairs.push(if i == 0 { (other, edge.clone()) } else { (edge.clone(), other) });
                }
            }
            self.consider(direct, &edge, None, arr, &legs, &mut best);
        }
        let mut seen = BTreeSet::new();
        for (first, last) in &pairs {
            if !seen.insert((key(first), key(last))) {
                continue;
            }
            if let Some(arr) = self.arrival(first, Some(last)) {
                let legs = self.trace_legs(&arr, first, Some(last), &mut observe);
                self.consider(direct, first, Some(last), arr, &legs, &mut best);
            }
        }
        if let Some(path) = &mut best.path {
            path.single_edges_tested = singles;
            path.edge_pairs_tested = seen.len();
        }
        best.path
    }

    fn arrival(&self, first: &AcousticEdge, next: Option<&AcousticEdge>) -> Option<arrival::EdgeArrival> {
        let arr = arrival::via_edges(self.route, self.hz, self.emitted, DVec3::from(self.source.array()), first, next)?;
        arr.legs.iter().all(|&l| l >= 0.0001).then_some(arr)
    }

    fn trace_legs(
        &self,
        arr: &arrival::EdgeArrival,
        first: &AcousticEdge,
        next: Option<&AcousticEdge>,
        observe: &mut impl FnMut(Vec3, Vec3, &DirectPath),
    ) -> Vec<(DirectPath, f64)> {
        let mut start = self.source;
        let mut seconds = self.emitted as f64 / f64::from(RATE);
        let targets = arr.first_bend.into_iter().chain([arr.bend, DVec3::from(arr.head.at.array())]);
        let bounds = std::iter::once(first.wedge.as_ref()).chain(next.map(|edge| edge.wedge.as_ref())).chain([None]);
        let mut previous = None;
        targets
            .zip(&arr.legs)
            .zip(bounds)
            .map(|((p, length), boundary)| {
                seconds += length / SOUND;
                let end = vec3(p.x, p.y, p.z);
                let ray = DirectPath::trace_leg(self.scene, self.bridge, start, end, seconds, [previous, boundary]);
                observe(start, end, &ray);
                start = end;
                previous = boundary;
                (ray, seconds)
            })
            .collect()
    }

    fn consider(
        &self,
        direct: &DirectPath,
        first: &AcousticEdge,
        next: Option<&AcousticEdge>,
        arr: arrival::EdgeArrival,
        legs: &[(DirectPath, f64)],
        best: &mut Choice,
    ) {
        let source = DVec3::from(self.source.array());
        let eye = DVec3::from(arr.head.at.array());
        let entrance = arr.first_bend.unwrap_or(arr.bend);
        let last = next.unwrap_or(first);
        if !outside(first, entrance, source) || !outside(last, arr.bend, eye) {
            return;
        }
        if let Some(p) = arr.first_bend
            && (!outside(first, p, arr.bend) || !outside(last, arr.bend, p))
        {
            return;
        }
        let distance = arr.legs.iter().sum::<f64>();
        let straight = self.source.distance(arr.head.at);
        let excess = (distance - straight).max(0.);
        let ratios = screen_loss_ratios(excess, next.map(|_| arr.legs[1]));
        let energy: [f64; 3] = std::array::from_fn(|i| {
            legs.iter().fold((1. - direct.transmission_energy[i]) / ratios[i], |gain, (leg, _)| {
                gain * leg.transmission_energy[i]
            })
        });
        let score = energy.iter().sum::<f64>() / straight.max(1.).powi(2);
        if score == 0. || score < best.score {
            return;
        }
        let bend_at = |edge: &AcousticEdge, p: DVec3| Bend {
            surface: edge.id.clone(),
            owner_path: edge.owner_path.clone(),
            point_m: p.to_array(),
            exterior_wedge: edge.wedge.clone(),
        };
        let bends: Vec<_> =
            arr.first_bend.into_iter().map(|p| bend_at(first, p)).chain([bend_at(last, arr.bend)]).collect();
        if score == best.score
            && let Some(prior) = &best.path
        {
            let order = distance
                .total_cmp(&prior.distance_m)
                .then_with(|| bends.len().cmp(&prior.bends.len()))
                .then_with(|| {
                    bends
                        .iter()
                        .zip(&prior.bends)
                        .map(|(a, b)| {
                            a.owner_path.cmp(&b.owner_path).then_with(|| a.surface.cmp(&b.surface)).then_with(|| {
                                (0..3)
                                    .map(|i| a.point_m[i].total_cmp(&b.point_m[i]))
                                    .find(|v| !v.is_eq())
                                    .unwrap_or(std::cmp::Ordering::Equal)
                            })
                        })
                        .find(|o| !o.is_eq())
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            if !order.is_lt() {
                return;
            }
        }
        best.score = score;
        best.path = Some(EdgePath {
            arrival_frame: arr.frame,
            surface: last.id.clone(),
            owner_path: last.owner_path.clone(),
            point_m: arr.bend.to_array(),
            bends,
            listener_at_arrival_m: arr.head.at.array(),
            legs_m: arr.legs,
            distance_m: distance,
            reference_distance_m: straight,
            single_edges_tested: 0,
            edge_pairs_tested: 0,
            excess_m: excess,
            edge_loss_db: ratios.map(|r| 10. * r.log10()),
            leg_transmission_energy: legs.iter().map(|(l, _)| l.transmission_energy).collect(),
            received_energy: energy,
            head: arr.head,
            ear_delay_frames: [0.; 2],
        });
    }
}
