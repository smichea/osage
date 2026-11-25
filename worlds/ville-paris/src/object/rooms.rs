// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Authored convex acoustic rooms and the finite apertures through their shared walls.
mod path;
use anyhow::{Context, Result, ensure};
use bevy::math::{DMat4, DVec3, Mat4, Vec3};
use osage_objects::{AcousticPortal, AcousticRoom, ObjectProfile};
use path::{Opening, Path};
use rapier3d::parry::{bounding_volume::Aabb, partitioning::Bvh};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

struct Room {
    id: String,
    local: DMat4,
    bounds: [DVec3; 2],
    doors: Vec<usize>,
    component: usize,
}

impl Room {
    fn contains(&self, p: DVec3) -> bool {
        let q = self.local.transform_point3(p);
        let [lo, hi] = self.bounds;
        q.cmpge(lo).all() && q.cmple(hi).all()
    }

    fn has_wall(&self, points: &[DVec3; 4]) -> bool {
        let p = points.map(|v| self.local.transform_point3(v));
        let [lo, hi] = self.bounds;
        let eps = (hi - lo).abs() * 1e-6 + (lo.abs() + hi.abs() + DVec3::ONE) * (64. * f64::EPSILON);
        p.iter().all(|v| v.cmpge(lo - eps).all() && v.cmple(hi + eps).all())
            && (0..3).any(|axis| {
                [lo[axis], hi[axis]].iter().any(|plane| p.iter().all(|q| (q[axis] - plane).abs() <= eps[axis]))
            })
    }
}

struct Door {
    id: String,
    sides: [usize; 2],
    opening: Opening,
}

impl Door {
    fn other(&self, room: usize) -> usize {
        if self.sides[0] == room { self.sides[1] } else { self.sides[0] }
    }
}

pub(crate) struct Graph {
    rooms: Vec<Room>,
    doors: Vec<Door>,
    bounds: Bvh,
    cyclic: Vec<bool>,
}

#[derive(Default, Serialize)]
pub(crate) struct Route {
    pub status: &'static str,
    pub search_complete: bool,
    pub distance_m: Option<f64>,
    pub lower_bound_m: Option<f64>,
    pub rooms: Vec<String>,
    pub portals: Vec<String>,
    pub points_m: Vec<[f64; 3]>,
    pub room_bounds_tested: usize,
    pub room_candidates_tested: usize,
    pub rooms_visited: usize,
    pub routes_evaluated: usize,
}

struct Candidate {
    doors: Vec<usize>,
    geometry: Path,
}

pub(crate) struct Placed<'a> {
    pub id: &'a str,
    pub world: Mat4,
    pub room: Option<&'a AcousticRoom>,
    pub portal: Option<&'a AcousticPortal>,
}

impl Graph {
    pub(crate) fn from_profile(profile: &ObjectProfile) -> Result<Option<Self>> {
        let placed = profile
            .nodes
            .iter()
            .filter(|n| n.geometry.acoustic_room.is_some() || n.geometry.acoustic_portal.is_some())
            .map(|n| Placed {
                id: &n.geometry.id,
                world: n.world_from_local,
                room: n.geometry.acoustic_room.as_ref(),
                portal: n.geometry.acoustic_portal.as_ref(),
            })
            .collect::<Vec<_>>();
        Self::from_placed(&placed)
    }

    pub(crate) fn from_placed(nodes: &[Placed<'_>]) -> Result<Option<Self>> {
        if nodes.is_empty() {
            return Ok(None);
        }
        ensure!(
            nodes.iter().all(|n| n.world.is_finite()
                && n.world.determinant().is_finite()
                && n.world.determinant() != 0.
                && n.world.row(3) == bevy::math::Vec4::W),
            "invalid indoor placement transform"
        );
        let mut graph = Self { rooms: Vec::new(), doors: Vec::new(), bounds: Bvh::default(), cyclic: Vec::new() };
        let mut bounds = Vec::new();
        let mut names = HashMap::new();
        for n in nodes {
            let Some(spec) = n.room else { continue };
            let [lo, hi] = spec.bounds_m.map(|p| Vec3::from_array(p).as_dvec3());
            ensure!(lo.is_finite() && hi.is_finite() && lo.cmplt(hi).all(), "invalid acoustic room bounds on {}", n.id);
            let world = n.world.as_dmat4();
            let local = world.inverse();
            ensure!(local.is_finite(), "singular acoustic room on {}", n.id);
            let mut min = DVec3::splat(f64::INFINITY);
            let mut max = -min;
            for x in [lo.x, hi.x] {
                for y in [lo.y, hi.y] {
                    for z in [lo.z, hi.z] {
                        let p = world.transform_point3(DVec3::new(x, y, z));
                        min = min.min(p);
                        max = max.max(p);
                    }
                }
            }
            let margin = (min.abs().max(max.abs()) + DVec3::ONE) * (4. * f64::from(f32::EPSILON));
            let (min, max) = ((min - margin).as_vec3(), (max + margin).as_vec3());
            ensure!(min.is_finite() && max.is_finite(), "unrepresentable acoustic room bounds");
            bounds.push(Aabb::new(min.to_array().into(), max.to_array().into()));
            names.insert(n.id, graph.rooms.len());
            graph.rooms.push(Room {
                id: n.id.to_owned(),
                local,
                bounds: [lo, hi],
                doors: Vec::new(),
                component: usize::MAX,
            });
        }
        for n in nodes {
            let Some(spec) = n.portal else { continue };
            let sides = [
                *names.get(spec.rooms[0].as_str()).context("unknown first room of acoustic portal")?,
                *names.get(spec.rooms[1].as_str()).context("unknown second room of acoustic portal")?,
            ];
            ensure!(
                sides[0] != sides[1] && (0.0..=1.).contains(&spec.closure_fraction),
                "invalid acoustic portal {}",
                n.id
            );
            let world = n.world.as_dmat4();
            let corners = spec.corners_m.map(|p| world.transform_point3(Vec3::from_array(p).as_dvec3()));
            let full = Opening::new(corners).with_context(|| format!("invalid portal {}", n.id))?;
            ensure!(
                sides.iter().all(|&s| graph.rooms[s].has_wall(&corners)),
                "portal {} must lie on a wall of both rooms",
                n.id
            );
            let room_center = |s: usize| {
                let r = &graph.rooms[s];
                r.local.inverse().transform_point3((r.bounds[0] + r.bounds[1]) * 0.5)
            };
            let normal = (corners[1] - corners[0]).cross(corners[3] - corners[0]);
            ensure!(
                (room_center(sides[0]) - corners[0]).dot(normal).signum()
                    != (room_center(sides[1]) - corners[0]).dot(normal).signum(),
                "portal {} joins rooms on the same side",
                n.id
            );
            let closed = spec.closure_fraction == 1.;
            let opening = if closed || spec.closure_fraction == 0. {
                full
            } else {
                let scale = (1. - f64::from(spec.closure_fraction)).sqrt();
                Opening::new(corners.map(|p| full.center + (p - full.center) * scale))?
            };
            if !closed {
                for s in sides {
                    graph.rooms[s].doors.push(graph.doors.len());
                }
            }
            graph.doors.push(Door { id: n.id.to_owned(), sides, opening });
        }
        for room in &mut graph.rooms {
            room.doors.sort_by(|&a, &b| graph.doors[a].id.cmp(&graph.doors[b].id));
        }
        for start in 0..graph.rooms.len() {
            if graph.rooms[start].component != usize::MAX {
                continue;
            }
            let component = graph.cyclic.len();
            let mut pending = vec![start];
            let (mut nodes, mut degree) = (0, 0);
            graph.rooms[start].component = component;
            while let Some(i) = pending.pop() {
                nodes += 1;
                degree += graph.rooms[i].doors.len();
                for k in 0..graph.rooms[i].doors.len() {
                    let next = graph.doors[graph.rooms[i].doors[k]].other(i);
                    if graph.rooms[next].component == usize::MAX {
                        graph.rooms[next].component = component;
                        pending.push(next);
                    }
                }
            }
            graph.cyclic.push(degree / 2 >= nodes);
        }
        graph.bounds = Bvh::from_leaves(Default::default(), &bounds);
        Ok(Some(graph))
    }

    fn locate(&self, p: DVec3, stats: &mut Route) -> Result<Option<usize>, ()> {
        let mut found = None;
        let mut ambiguous = false;
        use rapier3d::parry::partitioning::TraversalAction;
        self.bounds.traverse(|node| {
            stats.room_bounds_tested += 1;
            let b = node.aabb();
            if !(0..3).all(|i| p[i] >= f64::from(b.mins[i]) && p[i] <= f64::from(b.maxs[i])) {
                return TraversalAction::Prune;
            }
            if let Some(leaf) = node.leaf_data() {
                stats.room_candidates_tested += 1;
                let i = leaf as usize;
                if self.rooms[i].contains(p) && found.replace(i).is_some() {
                    ambiguous = true;
                    return TraversalAction::EarlyExit;
                }
            }
            TraversalAction::Continue
        });
        if ambiguous { Err(()) } else { Ok(found) }
    }

    fn evaluate(&self, a: DVec3, b: DVec3, path: &[usize]) -> Candidate {
        let openings = path.iter().map(|&d| &self.doors[d].opening).collect::<Vec<_>>();
        Candidate { doors: path.to_vec(), geometry: path::shortest(a, b, &openings) }
    }

    pub(crate) fn query(&self, from: [f32; 3], to: [f32; 3]) -> Route {
        let mut result = Route { search_complete: true, ..Default::default() };
        let mut a = Vec3::from_array(from).as_dvec3();
        let mut b = Vec3::from_array(to).as_dvec3();
        let (Ok(first), Ok(last)) = (self.locate(a, &mut result), self.locate(b, &mut result)) else {
            result.status = "ambiguous";
            return result;
        };
        let (Some(mut start), Some(mut goal)) = (first, last) else {
            result.status = "outside";
            return result;
        };
        if self.rooms[start].component != self.rooms[goal].component {
            result.status = "disconnected";
            return result;
        }
        // Solve in the same direction when the two endpoints are interchanged.
        let reverse = self.rooms[start].id > self.rooms[goal].id;
        if reverse {
            std::mem::swap(&mut start, &mut goal);
            std::mem::swap(&mut a, &mut b);
        }
        let mut parents = HashMap::from([(start, (start, usize::MAX))]);
        let mut queue = VecDeque::from([start]);
        while let Some(i) = queue.pop_front() {
            if i == goal {
                break;
            }
            for &d in &self.rooms[i].doors {
                let next = self.doors[d].other(i);
                if let std::collections::hash_map::Entry::Vacant(entry) = parents.entry(next) {
                    entry.insert((i, d));
                    queue.push_back(next);
                }
            }
        }
        result.rooms_visited = parents.len();
        let mut path = Vec::new();
        let mut here = goal;
        while here != start {
            let (parent, door) = parents[&here];
            path.push(door);
            here = parent;
        }
        path.reverse();
        let mut best = self.evaluate(a, b, &path);
        let mut lower = best.geometry.lower;
        result.routes_evaluated = 1;
        if start != goal && self.cyclic[self.rooms[start].component] {
            let mut used = HashSet::from([start]);
            let mut branch: Vec<usize> = Vec::new();
            let mut stack = vec![(start, 0_usize, 0.)];
            let mut expansions = 0;
            while let Some((i, cursor, prefix)) = stack.last_mut() {
                let Some(&d) = self.rooms[*i].doors.get(*cursor) else {
                    used.remove(i);
                    stack.pop();
                    branch.pop();
                    continue;
                };
                *cursor += 1;
                let next = self.doors[d].other(*i);
                if used.contains(&next) {
                    continue;
                }
                let opening = &self.doors[d].opening;
                let separation = if let Some(&p) = branch.last() {
                    let other = &self.doors[p].opening;
                    (opening.center.distance(other.center) - opening.radius - other.radius).max(0.)
                } else {
                    (a.distance(opening.center) - opening.radius).max(0.)
                };
                let prefix = *prefix + separation;
                let bound = prefix + (opening.center.distance(b) - opening.radius).max(0.);
                if bound > best.geometry.length + 1e-7 {
                    continue;
                }
                expansions += 1;
                if expansions > 8192 {
                    result.search_complete = false;
                    lower = a.distance(b);
                    break;
                }
                branch.push(d);
                if next == goal {
                    if branch != path {
                        let option = self.evaluate(a, b, &branch);
                        result.routes_evaluated += 1;
                        lower = lower.min(option.geometry.lower);
                        if option.geometry.length < best.geometry.length {
                            best = option
                        }
                    }
                    branch.pop();
                } else {
                    used.insert(next);
                    stack.push((next, 0, prefix));
                }
            }
        }
        let mut room = start;
        result.rooms.push(self.rooms[room].id.clone());
        for &d in &best.doors {
            let door = &self.doors[d];
            result.portals.push(door.id.clone());
            room = door.other(room);
            result.rooms.push(self.rooms[room].id.clone());
        }
        result.points_m.push(a.to_array());
        result.points_m.extend(best.geometry.crossings.iter().map(|p| p.to_array()));
        result.points_m.push(b.to_array());
        if reverse {
            result.rooms.reverse();
            result.portals.reverse();
            result.points_m.reverse();
        }
        result.status = "connected";
        result.distance_m = Some(best.geometry.length);
        result.lower_bound_m = Some(lower);
        result
    }
}
