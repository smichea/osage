// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Who drives each kart, one physics step per tick: a human's controls in
//! order, or the world's autopilot for bots and AI agents.

use std::collections::BTreeMap;

use bevy::prelude::*;
use libm::{atan2f, cosf, fabsf, sinf};
use osage_kart_physics::{Controls, Kart, MAX_SPEED, bump, step, wrap};
use osage_world_module_api::IntentReceived;
use prost::Message as _;

use crate::{
    Body, Circuit, Course, Ids,
    proto::{self, Phase},
    race::{Progress, Race},
};

/// With more steps than this waiting, a kart catches up two steps a tick.
const BACKLOG: usize = 6;
/// Steps a kart may have in hand: one more each tick, one per step applied.
/// Catching up uses what waiting saved; a client never drives faster than
/// the world's 60 steps a second, however many it sends.
const CREDIT: u32 = 8;
/// Ticks a kart waits for a late step before coasting: its driver is gone.
const PATIENCE: u32 = 8;

#[derive(Component)]
pub enum Pilot {
    Human(Steps),
    Agent(proto::Autopilot),
    Bot(proto::Autopilot),
}

/// A human's controls, applied in sequence order and only those: the world
/// never makes up a step, so that the driver's client predicts exactly
/// (D-031). A late step makes the kart wait a tick rather than guess.
#[derive(Default)]
pub struct Steps {
    waiting: BTreeMap<u64, Controls>,
    applied: u64,
    credit: u32,
    idle: u32,
}

impl Pilot {
    pub fn human() -> Self {
        Self::Human(Steps::default())
    }

    /// An agent's kart waits until the agent says how to drive.
    pub fn agent() -> Self {
        Self::Agent(proto::Autopilot::default())
    }

    pub fn bot(pace: f32, line: f32) -> Self {
        Self::Bot(proto::Autopilot { pace, line, drift: true })
    }

    /// The last human step applied: what the driver's client predicts from.
    pub fn applied(&self) -> u64 {
        match self {
            Self::Human(steps) => steps.applied,
            _ => 0,
        }
    }
}

pub fn intents(ids: Res<Ids>, mut intents: MessageReader<IntentReceived>, mut pilots: Query<&mut Pilot>) {
    for intent in intents.read() {
        let Ok(mut pilot) = pilots.get_mut(intent.entity) else { continue };
        match &mut *pilot {
            Pilot::Human(steps) if intent.type_id == ids.controls => {
                let Ok(controls) = proto::Controls::decode(intent.value.as_slice()) else { continue };
                for step in controls.steps.into_iter().filter(|s| s.sequence > steps.applied) {
                    let controls = Controls { throttle: step.throttle, steer: step.steer, drift: step.drift };
                    steps.waiting.insert(step.sequence, controls.sanitized());
                }
                while steps.waiting.len() > 4 * BACKLOG {
                    steps.waiting.pop_first();
                }
            }
            Pilot::Agent(autopilot) if intent.type_id == ids.autopilot => {
                let Ok(wanted) = proto::Autopilot::decode(intent.value.as_slice()) else { continue };
                let clean = |v: f32, low| if v.is_finite() { v.clamp(low, 1.0) } else { 0.0 };
                *autopilot = proto::Autopilot {
                    pace: clean(wanted.pace, 0.0),
                    line: clean(wanted.line, -1.0),
                    drift: wanted.drift,
                };
            }
            _ => {}
        }
    }
}

pub fn drive(race: Res<Race>, course: Res<Course>, mut karts: Query<(&mut Body, &mut Pilot, &Progress)>) {
    let track = &course.0.track;
    for (mut body, mut pilot, progress) in &mut karts {
        let mut controls = match &mut *pilot {
            Pilot::Human(steps) => next_steps(steps),
            Pilot::Agent(wanted) | Pilot::Bot(wanted) => vec![autopilot(&body.0, wanted, &course.0)],
        };
        // Past the line, the autopilot brings the kart round gently.
        if progress.finish_ms.is_some() {
            controls = vec![autopilot(&body.0, &proto::Autopilot { pace: 0.5, line: 0.0, drift: false }, &course.0)];
        }
        for mut controls in controls {
            // The lights hold the karts; after the race, they coast.
            if race.phase != Phase::Racing {
                controls.throttle = 0.0;
            }
            step(&mut body.0, controls, track);
        }
    }
}

/// The steps a human's kart takes this tick: none while it waits for a late
/// one, two when catching up.
fn next_steps(steps: &mut Steps) -> Vec<Controls> {
    steps.credit = (steps.credit + 1).min(CREDIT);
    let burst = if steps.waiting.len() > BACKLOG { 2 } else { 1 };
    let mut taken = Vec::with_capacity(burst);
    while taken.len() < burst && steps.credit > 0 {
        let Some((sequence, controls)) = steps.waiting.pop_first() else { break };
        steps.applied = sequence;
        steps.credit -= 1;
        taken.push(controls);
    }
    if taken.is_empty() {
        steps.idle += 1;
        if steps.idle > PATIENCE {
            taken.push(Controls::default());
        }
    } else {
        steps.idle = 0;
    }
    taken
}

/// The world's autopilot, which drives bots and AI agents' karts: it aims at
/// a point ahead on its line, holds its pace, slows for sharp bends and
/// drifts through them if allowed. Same bits on every platform (`libm`), so
/// that a local simulation — a benchmark, a replay — is reproducible.
pub fn autopilot(kart: &Kart, wanted: &proto::Autopilot, circuit: &Circuit) -> Controls {
    let track = &circuit.track;
    let spot = track.locate(kart.x, kart.z);
    let (centre, road) = track.at(spot.along + 6.0 + kart.speed.max(0.0) * 0.55);
    let offset = wanted.line.clamp(-1.0, 1.0) * (track.half_width - 1.5);
    let target = [centre[0] - cosf(road) * offset, centre[1] + sinf(road) * offset];
    let error = wrap(atan2f(target[0] - kart.x, target[1] - kart.z) - kart.heading);
    let bend = fabsf(error);
    let pace = wanted.pace.clamp(0.0, 1.0) * if bend > 0.5 { 0.75 } else { 1.0 };
    let throttle = if pace <= 0.0 {
        if kart.speed > 0.5 { -1.0 } else { 0.0 }
    } else if kart.speed < pace * MAX_SPEED {
        1.0
    } else {
        0.0
    };
    Controls {
        throttle,
        steer: (-error * 2.2).clamp(-1.0, 1.0),
        drift: wanted.drift && bend > 0.18 && kart.speed > 14.0,
    }
}

/// Karts bump: overlapping karts are pushed apart and lose a little speed.
pub fn collide(mut karts: Query<&mut Body>) {
    let mut pairs = karts.iter_combinations_mut();
    while let Some([mut a, mut b]) = pairs.fetch_next() {
        bump(&mut a.0, &mut b.0);
    }
}
