// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{
    Grid, Ids, Striding, core,
    passers::Patrol,
    proto,
    walk::{DIAMETER_SQ, EYES, Walk},
};
use bevy::prelude::*;
use osage_world_module_api::{Discrete, EntityIds, Joined, Left, Owner, SessionKind, builtin, osage_proto::v0::Role};
use std::collections::VecDeque;

#[derive(Default)]
pub(super) struct Entrance {
    waiting: VecDeque<Joined>,
    // Built only if a session's original spiral and the start itself are occupied.
    ground: Option<Vec<Vec3>>,
}

impl Entrance {
    fn place(&mut self, walk: &Walk, session: u64, occupied: &[Vec3]) -> Option<Vec3> {
        let free = |at: Vec3| {
            occupied.iter().all(|p| (p.y - at.y).abs() >= EYES || p.xz().distance_squared(at.xz()) >= DIAMETER_SQ)
        };
        let supported = |xz: Vec2| {
            if !walk.holds(xz) {
                return None;
            }
            Some(Vec3::new(xz.x, walk.ground(xz)?, xz.y))
        };
        for k in 0..16 {
            let angle = (session as f64 + f64::from(k)) * 2.399_963;
            let offset = Vec2::new(angle.cos() as f32, angle.sin() as f32) * (0.8 + 0.2 * k as f32);
            if let Some(at) = supported(walk.start.xz() + offset).filter(|&at| free(at)) {
                return Some(at);
            }
        }
        if let Some(start) = supported(walk.start.xz()).filter(|&at| free(at)) {
            return Some(start);
        }
        let ground = self.ground.get_or_insert_with(|| {
            let mut cells: Vec<_> =
                walk.places_near(walk.start.xz(), 3.8).filter_map(|at| supported(at.xz())).collect();
            cells.sort_by(|a, b| a.distance_squared(walk.start).total_cmp(&b.distance_squared(walk.start)));
            cells
        });
        ground.iter().copied().find(|&at| free(at))
    }
}

type People = Or<(With<Striding>, With<Patrol>)>;

pub(super) fn admit(
    mut commands: Commands,
    mut entity_ids: ResMut<EntityIds>,
    ids: Res<Ids>,
    grid: Res<Grid>,
    (mut joined, mut left): (MessageReader<Joined>, MessageReader<Left>),
    people: Query<&Transform, People>,
    mut entrance: Local<Entrance>,
) {
    entrance.waiting.extend(joined.read().filter(|j| j.role == Role::Player || j.kind == SessionKind::Agent).cloned());
    for departure in left.read() {
        entrance.waiting.retain(|j| j.session != departure.session);
    }
    if entrance.waiting.is_empty() {
        return;
    }
    let mut occupied: Vec<_> = people.iter().map(|t| t.translation).collect();
    while let Some(j) = entrance.waiting.front() {
        let session = j.session;
        let Some(at) = entrance.place(&grid.0, session, &occupied) else { break };
        let joined = entrance.waiting.pop_front().unwrap();
        let discrete = Discrete::default()
            .with(ids.content_ref, &builtin(core::BUILTIN_AVATAR))
            .with(ids.controller, &joined.controller)
            .with(ids.walker, &proto::Walker { look: session, passer_by: false });
        commands.spawn((
            entity_ids.next_id(),
            Transform::from_translation(at),
            discrete,
            Owner(session),
            Striding::default(),
        ));
        occupied.push(at);
    }
}
