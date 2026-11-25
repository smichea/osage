// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Server-owned walkers on two crossing routes. Each accepted velocity is visible
//! to the next decision in the same fixed tick.
use super::{
    Grid, Ids, Striding, core, proto,
    walk::{DIAMETER_SQ, WALK, Walk},
};
use bevy::prelude::*;
use osage_world_module_api::{Discrete, EntityIds, Replicated, builtin};

#[derive(Component)]
pub(super) struct Patrol {
    ends: [Vec3; 2],
    goal: usize,
    velocity: Vec2,
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

pub(super) fn spawn(mut commands: Commands, grid: Res<Grid>, ids: Res<Ids>, mut entity_ids: ResMut<EntityIds>) {
    let Some(routes) = crossing(&grid.0) else { return };
    for ends in routes {
        let id = entity_ids.next_id();
        let dir = ends[1] - ends[0];
        let discrete = Discrete::default()
            .with(ids.content_ref, &builtin(core::BUILTIN_AVATAR))
            .with(ids.walker, &proto::Walker { look: id.0, passer_by: true });
        commands.spawn((
            id,
            discrete,
            Transform::from_translation(ends[0]).with_rotation(Quat::from_rotation_y((-dir.x).atan2(-dir.z))),
            Patrol { ends, goal: 1, velocity: Vec2::ZERO },
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

/// Measure the closest approach with simultaneous travel. The first distance
/// protects the current tick, the second anticipates contact one second ahead.
fn clearance<'a>(at: Vec3, v: Vec2, others: impl Iterator<Item = &'a Motion>, dt: f32) -> (f32, f32) {
    let mut tick = DIAMETER_SQ;
    let mut forecast = f32::INFINITY;
    for other in others {
        if (other.at.y - at.y).abs() >= super::walk::EYES {
            continue;
        }
        let delta = at.xz() - other.at.xz();
        let relative = v - other.velocity;
        tick = tick.min(separation_sq(delta, relative, dt));
        if other.undecided {
            tick = tick.min(separation_sq(delta, v, dt));
        }
        forecast = forecast.min(separation_sq(delta, relative, HORIZON));
    }
    (tick, forecast)
}

fn choose<'a>(
    grid: &Walk,
    at: Vec3,
    goal: Vec3,
    previous: Vec2,
    others: impl Iterator<Item = &'a Motion> + Clone,
    dt: f32,
) -> Vec3 {
    let wish = ((goal.xz() - at.xz()) / dt).clamp_length_max(WALK);
    let straight = grid.slide(at, wish * dt);
    let (gap, forecast) = clearance(at, (straight.xz() - at.xz()) / dt, others.clone(), dt);
    if gap >= DIAMETER_SQ && forecast >= DIAMETER_SQ {
        return straight;
    }
    let mut best = at;
    let mut best_cost = f32::INFINITY;
    let mut best_gap = -1.;
    for speed in [1., 0.5, 0.] {
        for angle in [0_f32, -15., 15., -30., 30., -45., 45., -65., 65., -90., 90.] {
            let movement = Vec2::from_angle(angle.to_radians()).rotate(wish) * (speed * dt);
            let next = grid.slide(at, movement);
            let v = (next.xz() - at.xz()) / dt;
            let (gap, forecast) = clearance(at, v, others.clone(), dt);
            let comfort = (0.65 - forecast.sqrt()).max(0.);
            let cost = v.distance_squared(wish) + 0.15 * v.distance_squared(previous) + 24. * comfort * comfort;
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
    for (i, (_, entity, _)) in order.into_iter().enumerate() {
        let (_, _, mut transform, mut patrol) = people.get_mut(entity).unwrap();
        let from = transform.translation;
        if from.xz().distance_squared(patrol.ends[patrol.goal].xz()) < 0.05 * 0.05 {
            patrol.goal = 1 - patrol.goal;
        }
        let index = first + i;
        let others = neighbours[..index].iter().chain(&neighbours[index + 1..]);
        let to = choose(&grid.0, from, patrol.ends[patrol.goal], patrol.velocity, others, dt);
        let velocity = (to.xz() - from.xz()) / dt;
        neighbours[index] = Motion { at: from, velocity, undecided: false };
        transform.translation = to;
        if velocity.length_squared() > 1e-6 {
            transform.rotation = Quat::from_rotation_y((-velocity.x).atan2(-velocity.y));
        }
        patrol.velocity = velocity;
    }
}

#[cfg(test)]
mod tests;
