// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Server-owned walkers following street patrols. Each accepted velocity is visible
//! to the next decision in the same fixed tick.
mod neighbours;

use super::{
    Grid, Ids, Striding, core, proto,
    walk::{DIAMETER_SQ, WALK, Walk},
};
use bevy::prelude::*;
use neighbours::Neighbours;
use osage_world_module_api::{Discrete, EntityIds, Replicated, builtin};

#[derive(Component)]
pub(super) struct Patrol {
    points: Vec<Vec3>,
    ends: [Vec2; 2],
    next: usize,
    forward: bool,
    velocity: Vec2,
    still_seconds: f32,
    yield_remaining: f32,
}

impl Patrol {
    fn new(points: Vec<Vec3>, ends: [Vec2; 2]) -> Self {
        Self { points, ends, next: 1, forward: true, velocity: Vec2::ZERO, still_seconds: 0., yield_remaining: 0. }
    }

    fn record_step(&mut self, velocity: Vec2, dt: f32) {
        self.velocity = velocity;
        if self.yield_remaining > 0. {
            self.yield_remaining = (self.yield_remaining - dt).max(0.);
        } else if velocity.length_squared() < 0.01 {
            self.still_seconds += dt;
            if self.still_seconds >= 0.75 {
                self.yield_remaining = 0.6;
                self.still_seconds = 0.;
            }
        } else {
            self.still_seconds = 0.;
        }
    }

    fn destination(&mut self, walk: &Walk, at: Vec3) -> Vec3 {
        let last = self.points.len() - 1;
        let distance = at.xz().distance_squared(self.points[self.next].xz());
        if self.next == 0 || self.next == last {
            if at.xz().distance_squared(self.ends[usize::from(self.next == last)]) < ARRIVAL * ARRIVAL {
                self.forward = self.next == 0;
                self.next = if self.forward { 1 } else { last - 1 };
            }
        } else {
            let following = if self.forward { self.next + 1 } else { self.next - 1 };
            // Local avoidance can carry a passer past a corner. Let it enter
            // the next leg as soon as its footprint can turn there safely.
            if distance <= 1. && walk.clear_segment(at.xz(), self.points[following].xz()) {
                self.next = following;
            }
        }
        self.points[self.next]
    }
}

/// Choose the nearest valid crossing to the street entrance. The grid may have
/// disconnected floors: check every segment by walking it with the movement rules.
fn crossing(walk: &Walk) -> Option<[[Vec3; 2]; 2]> {
    let mut cells: Vec<_> =
        walk.places_near(walk.start.xz(), 12.).filter(|p| p.distance_squared(walk.start) < 12. * 12.).collect();
    let preferred = walk.start + Vec3::new(2.5, 0., -3.5);
    cells.sort_by(|a, b| a.distance_squared(preferred).total_cmp(&b.distance_squared(preferred)));
    for centre in cells {
        let end = |axis: Vec2| {
            let mut at = centre;
            for _ in 0..30 {
                let delta = axis * 0.1;
                let next = walk.slide(at, delta);
                if (next.xz() - at.xz() - delta).length_squared() > 1e-8 {
                    return None;
                }
                at = next;
            }
            Some(at)
        };
        if walk.holds(centre.xz())
            && let [Some(w), Some(e), Some(n), Some(s)] = [Vec2::NEG_X, Vec2::X, Vec2::NEG_Y, Vec2::Y].map(end)
        {
            return Some([[w, e], [n, s]]);
        }
    }
    None
}

const ARRIVAL: f32 = 0.05;

/// If a corner clips the body at the requested endpoint, find a standing
/// position on that same cell within the existing patrol arrival tolerance.
fn standing_end(walk: &Walk, requested: Vec2) -> Option<Vec2> {
    if walk.holds(requested) {
        return Some(requested);
    }
    walk.height(requested)?;
    for radius in [0.01, 0.02, 0.03, 0.04, ARRIVAL - 0.001] {
        for direction in 0..16 {
            let p = requested + radius * Vec2::from_angle(std::f32::consts::TAU * direction as f32 / 16.);
            if walk.same_cell(requested, p) && walk.holds(p) {
                return Some(p);
            }
        }
    }
    None
}

pub(super) fn spawn(mut commands: Commands, grid: Res<Grid>, ids: Res<Ids>, mut entity_ids: ResMut<EntityIds>) {
    let walk = &grid.0;
    let endpoints = match &walk.patrols {
        Some(routes) => routes.iter().map(|ends| ends.map(Vec2::from)).collect(),
        None => crossing(walk).into_iter().flatten().map(|ends| ends.map(|p| p.xz())).collect::<Vec<_>>(),
    };
    let mut starts: Vec<Vec3> = Vec::new();
    for [from, to] in endpoints {
        let route = standing_end(walk, from).zip(standing_end(walk, to)).and_then(|(a, b)| walk.route(a, b));
        let Some(points) = route.filter(|points| points.len() >= 2) else {
            tracing::warn!(?from, ?to, "no walkable patrol route");
            continue;
        };
        let start = points[0];
        if starts
            .iter()
            .any(|p| (p.y - start.y).abs() < super::walk::EYES && p.xz().distance_squared(start.xz()) < DIAMETER_SQ)
        {
            tracing::warn!(?from, "patrol starts too close to another passer");
            continue;
        }
        starts.push(start);
        let id = entity_ids.next_id();
        let dir = points[1] - start;
        let discrete = Discrete::default()
            .with(ids.content_ref, &builtin(core::BUILTIN_AVATAR))
            .with(ids.walker, &proto::Walker { look: id.0, passer_by: true });
        commands.spawn((
            id,
            discrete,
            Transform::from_translation(start).with_rotation(Quat::from_rotation_y((-dir.x).atan2(-dir.z))),
            Patrol::new(points, [from, to]),
        ));
    }
}

#[derive(Clone, Copy)]
struct Motion {
    at: Vec3,
    velocity: Vec2,
    /// A later walker may still choose to stand for this tick.
    undecided: bool,
}

fn separation_sq(delta: Vec2, relative_velocity: Vec2, horizon: f32) -> f32 {
    let speed_sq = relative_velocity.length_squared();
    let time = if speed_sq > 0. { (-delta.dot(relative_velocity) / speed_sq).clamp(0., horizon) } else { 0. };
    (delta + relative_velocity * time).length_squared()
}

const HORIZON: f32 = 1.;
const COMFORT: f32 = 0.65;

struct Clearance {
    /// Earlier decisions are final. Later walkers can still stand at their starting position.
    reserved: f32,
    /// Includes the possibility that a later walker keeps its old velocity.
    tick: f32,
    forecast: f32,
    close_approaches: usize,
}

/// Minimum squared centre distances for this candidate.
fn clearance<'a>(at: Vec3, v: Vec2, others: impl Iterator<Item = &'a Motion>, dt: f32) -> Clearance {
    let mut reserved = DIAMETER_SQ;
    let mut tick = DIAMETER_SQ;
    let mut forecast = f32::INFINITY;
    let mut close_approaches = 0;
    for other in others {
        if (other.at.y - at.y).abs() >= super::walk::EYES {
            continue;
        }
        let delta = at.xz() - other.at.xz();
        let relative = v - other.velocity;
        let moving = separation_sq(delta, relative, dt);
        let committed = if other.undecided { separation_sq(delta, v, dt) } else { moving };
        reserved = reserved.min(committed);
        tick = tick.min(moving);
        let approach = separation_sq(delta, relative, HORIZON);
        close_approaches += usize::from(approach < COMFORT * COMFORT);
        forecast = forecast.min(approach);
    }
    Clearance { reserved, tick: tick.min(reserved), forecast, close_approaches }
}

fn choose(
    grid: &Walk,
    at: Vec3,
    goal: Vec3,
    previous: Vec2,
    measure: impl Fn(Vec2) -> Clearance,
    dt: f32,
    yielding: bool,
) -> Vec3 {
    let wish = ((goal.xz() - at.xz()) / dt).clamp_length_max(WALK);
    let straight = grid.slide(at, wish * dt);
    let direct = measure((straight.xz() - at.xz()) / dt);
    if direct.tick >= DIAMETER_SQ && direct.forecast >= DIAMETER_SQ {
        return straight;
    }
    // When several streams converge, favour the same side of each walker's
    // own heading. Its actual step still has to clear the reserved space.
    let preferred = if yielding {
        Vec2::from_angle(-std::f32::consts::FRAC_PI_2).rotate(wish)
    } else if direct.close_approaches >= 2 {
        Vec2::from_angle(-std::f32::consts::FRAC_PI_4).rotate(wish)
    } else {
        wish
    };
    let mut best = at;
    let mut best_cost = f32::INFINITY;
    let mut best_gap = (-1., -1.);
    let headings = [0_f32, -15., 15., -30., 30., -45., 45., -65., 65., -90., 90., -135., 135., 180.];
    let candidates = if yielding { &headings[..] } else { &headings[..11] };
    for speed in [1., 0.5, 0.] {
        for &angle in candidates {
            let movement = Vec2::from_angle(angle.to_radians()).rotate(wish) * (speed * dt);
            let next = grid.slide(at, movement);
            let v = (next.xz() - at.xz()) / dt;
            let safety = measure(v);
            let gap = (safety.reserved, safety.tick);
            let comfort = (COMFORT - safety.forecast.sqrt()).max(0.);
            let cost = v.distance_squared(preferred) + 0.15 * v.distance_squared(previous) + 24. * comfort * comfort;
            if gap > best_gap || (gap == best_gap && cost < best_cost) {
                best = next;
                best_gap = gap;
                best_cost = cost;
            }
            if speed == 0. {
                break;
            }
        }
    }
    best
}

type Walkers<'a> = (Entity, &'a Replicated, &'a mut Transform, &'a mut Patrol);

pub(super) fn advance(
    grid: Res<Grid>,
    time: Res<Time>,
    mut people: Query<Walkers>,
    players: Query<(&Transform, &Striding), Without<Patrol>>,
) {
    let dt = time.delta_secs();
    if dt <= 0. || people.is_empty() {
        return;
    }
    let mut neighbours: Vec<_> = players
        .iter()
        .map(|(p, s)| Motion { at: p.translation - s.velocity * dt, velocity: s.velocity.xz(), undecided: false })
        .collect();
    let mut order: Vec<_> = people
        .iter()
        .map(|(e, id, p, route)| (id.0, e, Motion { at: p.translation, velocity: route.velocity, undecided: true }))
        .collect();
    order.sort_unstable_by_key(|r| r.0);
    let first = neighbours.len();
    neighbours.extend(order.iter().map(|r| r.2));
    let mut search = Neighbours::new(&neighbours);
    for (i, (_, entity, _)) in order.into_iter().enumerate() {
        let (_, _, mut transform, mut patrol) = people.get_mut(entity).unwrap();
        let from = transform.translation;
        let destination = patrol.destination(&grid.0, from);
        let index = first + i;
        let measure = |v| search.measure(&neighbours, index, v, dt);
        let to = choose(&grid.0, from, destination, patrol.velocity, measure, dt, patrol.yield_remaining > 0.);
        let velocity = (to.xz() - from.xz()) / dt;
        search.accept(velocity);
        neighbours[index] = Motion { at: from, velocity, undecided: false };
        transform.translation = to;
        if velocity.length_squared() > 1e-6 {
            transform.rotation = Quat::from_rotation_y((-velocity.x).atan2(-velocity.y));
        }
        patrol.record_step(velocity, dt);
    }
}

#[cfg(test)]
mod tests;
