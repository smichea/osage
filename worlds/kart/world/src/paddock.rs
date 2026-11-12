// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Paddock objects in the race ([osage-kart-items](osage_kart_items)):
//! crates to break, items to use, things on the track. The world holds them
//! — clients only see them — and its own drivers use what they pick up.

use bevy::prelude::*;
use osage_kart_items::{Item, Kind, Paddock, wants};
use osage_kart_physics::{Kart, Track};

use crate::{
    Body,
    driving::Pilot,
    proto::{self, Phase},
    race::{Progress, Race},
};

/// The race's paddock; a new one for every race.
#[derive(Resource)]
pub struct Items(pub Paddock);

/// A kart's place on the grid, which is its index in the [`Items`].
#[derive(Component, Clone, Copy)]
pub struct Seat(pub usize);

/// Karts break crates, use what they hold, meet what others left.
pub fn items(
    race: Res<Race>,
    course: Res<crate::Course>,
    paddock: Option<ResMut<Items>>,
    mut karts: Query<(&Seat, &mut Body, &Pilot, &Progress)>,
) {
    let Some(mut paddock) = paddock else { return };
    if race.phase != Phase::Racing {
        return;
    }
    let track = &course.0.track;
    let seats = karts.iter().map(|(seat, ..)| seat.0 + 1).max().unwrap_or(0);
    // Seats left empty stay far away.
    let mut bodies = vec![Kart { x: 1e6, z: 1e6, ..Kart::default() }; seats];
    let mut places = vec![seats; seats];
    for (seat, body, _, progress) in &karts {
        bodies[seat.0] = body.0;
        places[seat.0] = progress.position.saturating_sub(1) as usize;
    }
    let spots: Vec<_> = bodies.iter().map(|k| track.locate(k.x, k.z)).collect();
    let mut uses = vec![false; seats];
    for (seat, body, pilot, _) in &karts {
        let me = seat.0;
        uses[me] = match pilot.uses_items() {
            Some(held) => held,
            None => paddock.0.held(me).is_some_and(|item| wants(item, me, &spots, &body.0, track)),
        };
    }
    paddock.0.step(&mut bodies, &uses, &places, track);
    for (seat, mut body, _, _) in &mut karts {
        if body.0 != bodies[seat.0] {
            body.0 = bodies[seat.0];
        }
    }
}

/// A new paddock for a race of `seats` karts.
pub fn items_for(track: &Track, seats: usize, race: u32) -> Items {
    Items(Paddock::new(track, seats, race.wrapping_mul(0x9e37_79b9) ^ 0x6b61_7274))
}

pub fn item(item: Option<Item>) -> proto::Item {
    match item {
        None => proto::Item::None,
        Some(Item::Nitro) => proto::Item::Nitro,
        Some(Item::Tyre) => proto::Item::Tyre,
        Some(Item::Cone) => proto::Item::Cone,
        Some(Item::Oil) => proto::Item::Oil,
        Some(Item::Rocket) => proto::Item::Rocket,
        Some(Item::Smoke) => proto::Item::Smoke,
    }
}

/// What clients see of the paddock.
pub fn to_proto(paddock: &Paddock) -> proto::Paddock {
    let kind = |kind: Kind| match kind {
        Kind::Tyre => proto::Item::Tyre,
        Kind::Cone => proto::Item::Cone,
        Kind::Oil => proto::Item::Oil,
    };
    proto::Paddock {
        crates: paddock.crates().iter().map(|c| proto::Crate { x: c.x, z: c.z, standing: c.stands() }).collect(),
        things: paddock
            .things()
            .iter()
            .map(|t| proto::Thing { kind: kind(t.kind).into(), x: t.x, z: t.z, vx: t.vx, vz: t.vz, life: t.life })
            .collect(),
    }
}
