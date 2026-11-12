// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Drivers and karts (D-055, [`osage_kart_roster`]): what each human or
//! agent chose (`kart.choice/1`), kept by session for the races that
//! follow; the roster published on the race board (`kart.roster/1`); the
//! builds the physics steps karts with.

use std::collections::BTreeMap;

use osage_kart_physics::Build;
use osage_kart_roster::{BODIES, Body, DRIVERS, Driver, body, driver};

use bevy::prelude::*;

use crate::proto;

/// A driver in a body.
pub type Pair = (&'static Driver, &'static Body);

/// Paints there are (D-064): the eight team colours, then four special ones.
pub const PAINTS: u32 = 12;

/// What each session chose, kept while the world runs: the pair, and the
/// paint when it is not the driver's own colour.
#[derive(Resource, Default)]
pub struct Choices {
    pairs: BTreeMap<u64, Pair>,
    paints: BTreeMap<u64, u32>,
}

impl Choices {
    /// The pair `session` races with: its choice, or the default.
    pub fn of(&self, session: u64) -> Pair {
        self.pairs.get(&session).copied().unwrap_or((&DRIVERS[0], &BODIES[0]))
    }

    /// The colour `session`'s kart is painted with in `pair` (D-064).
    pub fn colour(&self, session: u64, pair: Pair) -> u32 {
        self.paints.get(&session).copied().unwrap_or(pair.0.colour)
    }

    /// Takes `choice` for `session` on top of what it had; ids it does not
    /// know leave that half as it was, a paint there is not leaves the
    /// paint. The pair now chosen.
    pub fn take(&mut self, session: u64, choice: &proto::Choice) -> Pair {
        let (had_driver, had_body) = self.of(session);
        let pair = (driver(&choice.driver).unwrap_or(had_driver), body(&choice.kart).unwrap_or(had_body));
        self.pairs.insert(session, pair);
        match choice.paint {
            Some(0) => {
                self.paints.remove(&session);
            }
            Some(paint) if paint <= PAINTS => {
                self.paints.insert(session, paint - 1);
            }
            _ => {}
        }
        pair
    }
}

/// The pair the world's bot `index` (named as a roster driver) races with.
pub fn bot(name: &str, index: usize) -> Pair {
    let driver = driver(&name.to_lowercase()).unwrap_or(&DRIVERS[index % DRIVERS.len()]);
    (driver, &BODIES[index % BODIES.len()])
}

pub fn build_proto(b: Build) -> proto::Build {
    proto::Build { top: b.top, accel: b.accel, handling: b.handling, grip: b.grip, weight: b.weight }
}

/// Everyone there is to drive and drive in.
pub fn to_proto() -> proto::Roster {
    proto::Roster {
        drivers: DRIVERS
            .iter()
            .map(|d| proto::RosterDriver {
                id: d.id.to_owned(),
                name: d.name.to_owned(),
                class: d.class.name().to_owned(),
                color: d.colour,
                about: d.about.to_owned(),
                build: Some(build_proto(d.class.build())),
            })
            .collect(),
        karts: BODIES
            .iter()
            .map(|b| proto::RosterKart {
                id: b.id.to_owned(),
                name: b.name.to_owned(),
                about: b.about.to_owned(),
                build: Some(build_proto(b.build)),
            })
            .collect(),
    }
}
