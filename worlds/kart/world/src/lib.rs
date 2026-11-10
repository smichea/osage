// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The osage kart world: a circuit, races of a few laps, karts for humans,
//! AI agents and bots. A world module (D-030): the engine replicates, the
//! rules are here. Humans' karts take their controls step by step and their
//! clients predict them with the same physics (D-031); agents drive through
//! the world's autopilot; bots fill the grid.

mod circuit;
mod driving;
mod race;

use bevy::prelude::*;
use osage_kart_physics::Kart;
use osage_world_module_api::{Discrete, EntityIds, RegionSet, Schema, TypeSpec, WorldModule};

pub use circuit::Circuit;
pub use driving::autopilot;

/// Generated from `proto/kart/v1/kart.proto`.
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/kart.v1.rs"));
}

/// Encoded descriptor set of the kart messages.
pub const DESCRIPTORS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/kart_descriptors.bin"));

/// Names of the kart world's component types.
pub mod types {
    pub const CONTROLS: &str = "kart.controls/1";
    pub const AUTOPILOT: &str = "kart.autopilot/1";
    pub const STATE: &str = "kart.state/1";
    pub const RACER: &str = "kart.racer/1";
    pub const TRACK: &str = "kart.track/1";
    pub const RACE: &str = "kart.race/1";
}

/// The kart world; runs at 60 ticks per second, one physics step per tick.
pub struct KartWorld {
    /// Karts per race; bots fill the places drivers leave free.
    pub grid: usize,
    pub laps: u32,
}

impl Default for KartWorld {
    fn default() -> Self {
        Self { grid: 6, laps: 3 }
    }
}

pub const TICK_HZ: u32 = 60;

impl Plugin for KartWorld {
    fn build(&self, app: &mut App) {
        app.insert_resource(Rules { grid: self.grid, laps: self.laps })
            .insert_resource(Course(Circuit::new()))
            .init_resource::<race::Race>()
            .init_resource::<race::Drivers>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    race::sessions,
                    driving::intents,
                    race::flow,
                    driving::drive,
                    driving::collide,
                    race::progress,
                    race::publish,
                )
                    .chain()
                    .in_set(RegionSet::World),
            );
    }
}

impl WorldModule for KartWorld {
    fn component_types(&self) -> Vec<TypeSpec> {
        let spec = |name: &str, message: &str, volatile, description: &str| TypeSpec {
            name: name.to_owned(),
            message_type: format!("kart.v1.{message}"),
            volatile,
            description: description.to_owned(),
        };
        vec![
            spec(
                types::CONTROLS,
                "Controls",
                false,
                "Intent for a human's kart: the latest physics steps (1/60 s each) of throttle, steering and drift.",
            ),
            spec(
                types::AUTOPILOT,
                "Autopilot",
                false,
                "Intent for an AI agent's kart: pace 0..1, line -1 (left edge)..1 (right edge), drift through bends. \
                 Your kart waits on the grid until you send one; resend it to change how you drive.",
            ),
            spec(types::STATE, "KartState", true, "A kart's physics state, every step."),
            spec(types::RACER, "Racer", false, "Who drives a kart: name, lap, position, finish time."),
            spec(types::TRACK, "Track", false, "The circuit: centre line, road half width, start line, sectors."),
            spec(
                types::RACE,
                "Race",
                false,
                "The race: phase (waiting, countdown, racing, finished), when it ends, laps, order.",
            ),
        ]
    }

    fn descriptors(&self) -> Vec<u8> {
        DESCRIPTORS.to_vec()
    }
}

#[derive(Resource, Clone, Copy)]
struct Rules {
    grid: usize,
    laps: u32,
}

#[derive(Resource)]
struct Course(Circuit);

/// Compact ids of the types this world uses.
#[derive(Resource, Clone, Copy)]
struct Ids {
    content_ref: u32,
    controller: u32,
    controls: u32,
    autopilot: u32,
    state: u32,
    racer: u32,
    race: u32,
}

/// The entity carrying `kart.race/1`.
#[derive(Component)]
struct RaceBoard;

/// A kart's physics.
#[derive(Component)]
struct Body(Kart);

fn setup(
    mut commands: Commands,
    schema: Res<Schema>,
    mut ids: ResMut<EntityIds>,
    course: Res<Course>,
    rules: Res<Rules>,
) {
    let id = |name| schema.id(name).unwrap_or_else(|| panic!("{name} is announced"));
    let kart = Ids {
        content_ref: id(osage_world_module_api::osage_proto::types::CONTENT_REF),
        controller: id(osage_world_module_api::osage_proto::types::CONTROLLER),
        controls: id(types::CONTROLS),
        autopilot: id(types::AUTOPILOT),
        state: id(types::STATE),
        racer: id(types::RACER),
        race: id(types::RACE),
    };
    commands.insert_resource(kart);
    commands.spawn((
        ids.next_id(),
        Transform::default(),
        Discrete::default().with(id(types::TRACK), &course.0.to_proto()),
    ));
    let board = proto::Race { phase: proto::Phase::Waiting.into(), laps: rules.laps, ..default() };
    commands.spawn((ids.next_id(), Transform::default(), Discrete::default().with(kart.race, &board), RaceBoard));
}
