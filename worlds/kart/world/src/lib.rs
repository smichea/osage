// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The osage kart world: a circuit, races of a few laps, karts for humans,
//! AI agents and bots. A world module (D-030): the engine replicates, the
//! rules are here. Humans' karts take their controls step by step and their
//! clients predict them with the same physics (D-031); agents drive through
//! the world's autopilot; bots fill the grid.

mod battle;
mod circuit;
mod cup;
mod driving;
mod paddock;
mod race;
mod records;
mod roster;
mod trial;
mod weather;

use bevy::prelude::*;
use osage_kart_physics::Kart;
use osage_world_module_api::{Discrete, EntityIds, RegionSet, Schema, TypeSpec, Volatile, WorldModule};

pub use circuit::Circuit;
pub use cup::events;
pub use driving::autopilot;
pub use weather::Weather;

/// Generated from `proto/kart/v1/kart.proto`.
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/kart.v1.rs"));
}

/// Encoded descriptor set of the kart messages.
/// How a kart is built, from its `kart.racer/1` (D-055): the standard kart
/// when the racer says nothing.
pub fn build_of(racer: &proto::Racer) -> osage_kart_physics::Build {
    racer.build.as_ref().map_or(osage_kart_physics::Build::STANDARD, |b| osage_kart_physics::Build {
        top: b.top,
        accel: b.accel,
        handling: b.handling,
        grip: b.grip,
        weight: b.weight,
    })
}

/// A kart's physics state from its `kart.state/1`: what a predicting
/// client restarts from, with the standard build — set its own from its
/// racer ([`build_of`]).
pub fn kart_of(state: &proto::KartState) -> osage_kart_physics::Kart {
    osage_kart_physics::Kart {
        x: state.x,
        z: state.z,
        heading: state.heading,
        speed: state.speed,
        slide: state.slide,
        drift: state.drift,
        charge: state.charge,
        boost: state.boost,
        spin: state.spin,
        rocket: state.rocket,
        height: state.height,
        climb: state.climb,
        trick: state.trick,
        stall: state.stall,
        draft: state.draft,
        held: state.held,
        slick: state.slick,
        build: osage_kart_physics::Build::STANDARD,
    }
}

pub const DESCRIPTORS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/kart_descriptors.bin"));

/// Names of the kart world's component types.
pub mod types {
    pub const CONTROLS: &str = "kart.controls/1";
    pub const AUTOPILOT: &str = "kart.autopilot/1";
    pub const STATE: &str = "kart.state/1";
    pub const RACER: &str = "kart.racer/1";
    pub const TRACK: &str = "kart.track/1";
    pub const RACE: &str = "kart.race/1";
    pub const PADDOCK: &str = "kart.paddock/1";
    pub const RECORDS: &str = "kart.records/1";
    pub const CHOICE: &str = "kart.choice/1";
    pub const ROSTER: &str = "kart.roster/1";
}

/// The kart world; runs at 60 ticks per second, one physics step per tick.
pub struct KartWorld {
    /// Karts per race; bots fill the places drivers leave free.
    pub grid: usize,
    pub laps: u32,
    /// The host (the first human driver) starts each race, the karts
    /// waiting on the grid until then; otherwise races follow one another.
    pub host_starts: bool,
    /// The file the world keeps its records in (D-053); none, in memory.
    pub records: Option<std::path::PathBuf>,
    /// The folder the world keeps its time trials' ghosts in (D-062);
    /// none, in memory.
    pub ghosts: Option<std::path::PathBuf>,
    /// The track it races on (D-056), when it races no cup.
    pub track: osage_kart_tracks::TrackDef,
    /// What it races (D-057): a cup (`cup:ID`), a track (`track:ID`) or a
    /// time trial on one (`trial:ID`, D-062); none, `track` race after race.
    pub event: Option<String>,
    /// What the weather follows (D-068): each track's chance of rain, or
    /// always dry, or always wet.
    pub weather: Weather,
    /// The port where the host shares their music, announced to the
    /// players; none when nothing is shared.
    pub music_port: Option<u16>,
    /// The port of the voice relay beside the world, announced to the
    /// players; none when there is no relay.
    pub voice_port: Option<u16>,
}

/// The ports beside the world, as announced (0: none): the host's shared
/// music, the voice relay.
#[derive(Resource, Clone, Copy, Default)]
pub struct Beside {
    pub music: u16,
    pub voice: u16,
}

impl Default for KartWorld {
    fn default() -> Self {
        Self {
            grid: 6,
            laps: 3,
            host_starts: true,
            records: None,
            ghosts: None,
            weather: Weather::Track,
            track: paddock(),
            event: None,
            music_port: None,
            voice_port: None,
        }
    }
}

impl KartWorld {
    /// Races that start by themselves: for tests and unattended worlds.
    pub fn unattended(grid: usize, laps: u32) -> Self {
        Self {
            grid,
            laps,
            host_starts: false,
            records: None,
            ghosts: None,
            weather: Weather::Track,
            track: paddock(),
            event: None,
            music_port: None,
            voice_port: None,
        }
    }
}

fn paddock() -> osage_kart_tracks::TrackDef {
    osage_kart_tracks::by_id("paddock").expect("the paddock is built in")
}

pub const TICK_HZ: u32 = 60;

impl Plugin for KartWorld {
    fn build(&self, app: &mut App) {
        let (cups, track) = match &self.event {
            Some(event) => cup::Cups::new(event),
            None => (cup::Cups::single(&self.track), self.track.clone()),
        };
        app.insert_resource(Rules { grid: self.grid, laps: self.laps, host_starts: self.host_starts })
            .insert_resource(cups)
            .insert_resource(Beside { music: self.music_port.unwrap_or(0), voice: self.voice_port.unwrap_or(0) })
            .init_resource::<race::Calls>()
            .insert_resource(self.weather)
            .init_resource::<weather::Forecast>()
            .insert_resource(Course(Circuit::of(&track)))
            .init_resource::<race::Race>()
            .init_resource::<race::Drivers>()
            .init_resource::<records::Times>()
            .init_resource::<roster::Choices>()
            .insert_resource(records::Book::open(self.records.clone()))
            .insert_resource(trial::Ghosts(osage_kart_ghost::Shelf::open(self.ghosts.clone())))
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    race::sessions,
                    driving::intents,
                    race::flow,
                    weather::forecast,
                    driving::drive,
                    driving::collide,
                    paddock::items,
                    race::progress,
                    trial::keep,
                    records::keep,
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
            spec(
                types::RACER,
                "Racer",
                false,
                "Who drives a kart: name, lap, position, finish time; ghost: a run kept from an earlier time \
                 trial, driven again (name is who drove it); in a battle, lives left and whether it is out.",
            ),
            spec(
                types::TRACK,
                "Track",
                false,
                "The circuit: which track (id, name, about, sky, scenery), centre line, road half width, start line, \
                 sectors, crests.",
            ),
            spec(
                types::RACE,
                "Race",
                false,
                "The race: phase (waiting, ready, countdown, racing, finished), when it ends, laps, order, host, \
                 rain (a wet road grips and brakes less); \
                 the cup being raced with its standings, the event chosen next, votes and the events there are \
                 (cup:ID, track:ID, trial:ID — a time trial: alone against the clock and the ghosts of the \
                 best runs, see Racer.ghost — or battle:ARENA: three lives each, every hit takes one, the last \
                 kart in wins, see Racer.lives and Racer.out). \
                 The host starts each race with Step.call and chooses the event with Choice.event; a driver \
                 alone can end a race.",
            ),
            spec(
                types::PADDOCK,
                "Paddock",
                true,
                "Crates across the road (break one for an item: see Racer.held, use it with Step.use_item or \
                 Autopilot.use_items) and what karts left on the track: rolling tyres, cones, oil.",
            ),
            spec(
                types::CHOICE,
                "Choice",
                false,
                "Intent for your own kart: the driver and kart body to race with (ids from kart.roster/1). \
                 On the grid it changes your kart at once; during a race, from the next one.",
            ),
            spec(
                types::ROSTER,
                "Roster",
                false,
                "The drivers (light, medium, heavy) and kart bodies there are, each with the build it gives: \
                 top speed, acceleration, handling, grip and weight as shares of the standard kart's.",
            ),
            spec(
                types::RECORDS,
                "Records",
                false,
                "The world's records on this track: best laps, best races and best time trials, one per \
                 driver name, fastest first, with when they were set; fresh ones were set in the latest race.",
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
    host_starts: bool,
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
    paddock: u32,
    records: u32,
    choice: u32,
    track: u32,
}

/// The entity carrying `kart.track/1`.
#[derive(Component)]
struct TrackBoard;

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
        paddock: id(types::PADDOCK),
        records: id(types::RECORDS),
        choice: id(types::CHOICE),
        track: id(types::TRACK),
    };
    commands.insert_resource(kart);
    commands.spawn((
        ids.next_id(),
        Transform::default(),
        Discrete::default().with(kart.track, &course.0.to_proto()),
        TrackBoard,
    ));
    let board = proto::Race { phase: proto::Phase::Waiting.into(), laps: rules.laps, ..default() };
    commands.spawn((
        ids.next_id(),
        Transform::default(),
        Discrete::default().with(kart.race, &board).with(id(types::ROSTER), &roster::to_proto()),
        Volatile::default(),
        RaceBoard,
    ));
}
