// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Who drives each kart, one physics step per tick: a human's controls in
//! order, or the world's autopilot for bots and AI agents.

use std::collections::BTreeMap;

use bevy::prelude::*;
use osage_kart_physics::{Controls, Kart, bump, step};
use osage_kart_racecraft::{Mind, Temper, field};
use osage_world_module_api::IntentReceived;
use prost::Message as _;

use crate::{
    Body, Circuit, Course, Ids,
    proto::{self, Phase},
    race::{Calls, Progress, Race},
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
    /// The world's own drivers race with a temperament
    /// ([osage-kart-racecraft](osage_kart_racecraft)).
    Bot(Box<(Temper, Mind)>),
}

/// A human's controls, applied in sequence order and only those: the world
/// never makes up a step, so that the driver's client predicts exactly
/// (D-031). A late step makes the kart wait a tick rather than guess.
#[derive(Default)]
pub struct Steps {
    waiting: BTreeMap<u64, (Controls, bool)>,
    applied: u64,
    /// The use button, as of the last step applied.
    using: bool,
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

    pub fn bot(temper: Temper, seed: u32) -> Self {
        Self::Bot(Box::new((temper, Mind::new(seed))))
    }

    /// The last human step applied: what the driver's client predicts from.
    pub fn applied(&self) -> u64 {
        match self {
            Self::Human(steps) => steps.applied,
            _ => 0,
        }
    }

    /// Whether the use button is down; `None` when the world's rule for
    /// its own drivers decides ([`osage_kart_items::wants`]).
    pub fn uses_items(&self) -> Option<bool> {
        match self {
            Self::Human(steps) => Some(steps.using),
            Self::Agent(autopilot) if autopilot.use_items => None,
            Self::Agent(_) => Some(false),
            Self::Bot(_) => None,
        }
    }
}

pub fn intents(
    ids: Res<Ids>,
    mut intents: MessageReader<IntentReceived>,
    mut calls: ResMut<Calls>,
    mut pilots: Query<(&mut Pilot, Option<&osage_world_module_api::Owner>)>,
) {
    for intent in intents.read() {
        let Ok((mut pilot, owner)) = pilots.get_mut(intent.entity) else { continue };
        match &mut *pilot {
            Pilot::Human(steps) if intent.type_id == ids.controls => {
                let Ok(controls) = proto::Controls::decode(intent.value.as_slice()) else { continue };
                for step in &controls.steps {
                    if let (Some(owner), Ok(call)) = (owner, proto::RaceCall::try_from(step.call))
                        && call != proto::RaceCall::None
                    {
                        calls.0.push((owner.0, call));
                    }
                }
                for step in controls.steps.into_iter().filter(|s| s.sequence > steps.applied) {
                    let controls = Controls { throttle: step.throttle, steer: step.steer, drift: step.drift };
                    steps.waiting.insert(step.sequence, (controls.sanitized(), step.use_item));
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
                    use_items: wanted.use_items,
                };
            }
            _ => {}
        }
    }
}

pub fn drive(race: Res<Race>, course: Res<Course>, mut karts: Query<(&mut Body, &mut Pilot, &Progress)>) {
    let track = &course.0.track;
    // Where every kart is, for the bots to see the field: the query's order,
    // the same in both loops.
    let karts_now: Vec<Kart> = karts.iter().map(|(body, ..)| body.0).collect();
    let field = field(track, &karts_now);
    for (me, (mut body, mut pilot, progress)) in karts.iter_mut().enumerate() {
        let mut controls = match &mut *pilot {
            Pilot::Human(steps) => next_steps(steps),
            Pilot::Agent(wanted) => vec![autopilot(&body.0, wanted, &course.0)],
            Pilot::Bot(bot) => {
                let (temper, mind) = &mut **bot;
                vec![osage_kart_racecraft::drive(&body.0, me, &field, temper, mind, track)]
            }
        };
        // Past the line, the autopilot brings the kart round gently.
        if progress.finish_ms.is_some() {
            controls = vec![autopilot(&body.0, &proto::Autopilot { pace: 0.5, line: 0.0, ..default() }, &course.0)];
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
        let Some((sequence, (controls, using))) = steps.waiting.pop_first() else { break };
        steps.applied = sequence;
        steps.using = using;
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

/// The world's autopilot, for AI agents' karts: a pace, a line and drifts
/// as the agent asks; it brakes for bends and knows nothing of the others
/// ([`osage_kart_racecraft::follow`]). Same bits on every platform, so that
/// a local simulation — a benchmark, a replay — is reproducible.
pub fn autopilot(kart: &Kart, wanted: &proto::Autopilot, circuit: &Circuit) -> Controls {
    osage_kart_racecraft::follow(kart, wanted.pace, wanted.line, wanted.drift, &circuit.track)
}

/// Karts collide (D-045).
pub fn collide(mut karts: Query<&mut Body>) {
    let mut pairs = karts.iter_combinations_mut();
    while let Some([mut a, mut b]) = pairs.fetch_next() {
        bump(&mut a.0, &mut b.0);
    }
}
