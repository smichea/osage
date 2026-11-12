// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Who drives each kart, one physics step per tick: a human's controls in
//! order, the world's autopilot for bots and AI agents, or a run kept from
//! an earlier time trial, driven again (D-062).

use std::collections::BTreeMap;

use bevy::prelude::*;
use osage_kart_ghost::{Recorder, Replay};
use osage_kart_items::Kind;
use osage_kart_physics::{Controls, Kart, bump, launch, slipstream, step};
use osage_kart_racecraft::{Hazard, Mind, Temper, drive_among, field};
use osage_world_module_api::IntentReceived;
use prost::Message as _;

use crate::{
    Body, Circuit, Course, Ids,
    paddock::Items,
    proto::{self, Phase},
    race::{Calls, Progress, Race},
    roster::Choices,
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
    /// A run driven again, tick for tick ([osage-kart-ghost](osage_kart_ghost)).
    Ghost(Replay),
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
            Self::Agent(_) | Self::Ghost(_) => Some(false),
            Self::Bot(_) => None,
        }
    }
}

pub fn intents(
    ids: Res<Ids>,
    race: Res<Race>,
    mut intents: MessageReader<IntentReceived>,
    mut calls: ResMut<Calls>,
    mut choices: ResMut<Choices>,
    mut cups: ResMut<crate::cup::Cups>,
    mut pilots: Query<(&mut Pilot, Option<&osage_world_module_api::Owner>, &mut Body, &mut Progress)>,
) {
    for intent in intents.read() {
        let Ok((mut pilot, owner, mut body, mut progress)) = pilots.get_mut(intent.entity) else { continue };
        // A driver and a kart, chosen: kept for the races that follow, and
        // on the grid at once (D-055).
        if intent.type_id == ids.choice
            && let Some(owner) = owner
        {
            let Ok(choice) = proto::Choice::decode(intent.value.as_slice()) else { continue };
            let pair = choices.take(owner.0, &choice);
            if !choice.event.is_empty() {
                cups.choose(owner.0, &choice.event);
            }
            if matches!(race.phase, Phase::Ready | Phase::Countdown | Phase::Waiting) {
                body.0.build = osage_kart_roster::build(pair.0, pair.1);
                progress.set_pair(pair);
            }
            continue;
        }
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

pub fn drive(
    time: Res<Time>,
    race: Res<Race>,
    course: Res<Course>,
    cups: Res<crate::cup::Cups>,
    paddock: Option<Res<Items>>,
    mut karts: Query<(&mut Body, &mut Pilot, &Progress, &mut Launch)>,
) {
    let track = &course.0.track;
    let (racing, trial) = (race.phase == Phase::Racing, cups.trial());
    let left = race.countdown_left(time.elapsed());
    // Where every kart is, for the bots to see the field: the query's order,
    // the same in both loops.
    let karts_now: Vec<Kart> = karts.iter().map(|(body, ..)| body.0).collect();
    let field = field(track, &karts_now);
    // What lies on the road for the bots to steer round (a drone is in the air).
    let hazards: Vec<Hazard> = paddock
        .iter()
        .flat_map(|p| p.0.things())
        .filter(|t| t.kind != Kind::Drone)
        .map(|t| Hazard { spot: track.locate(t.x, t.z), radius: t.kind.radius() })
        .collect();
    for (me, (mut body, mut pilot, progress, mut start)) in karts.iter_mut().enumerate() {
        // A ghost drives its run again from the lights, then is brought
        // round like any kart past the line; it waits and coasts as they do.
        if let Pilot::Ghost(replay) = &mut *pilot {
            if racing && !start.judged {
                start.judged = true;
                body.0 = replay.start();
            }
            let round = proto::Autopilot { pace: 0.5, ..default() };
            match racing.then(|| replay.next_tick()) {
                Some(Some(tick)) => tick.steps().iter().for_each(|controls| step(&mut body.0, *controls, track)),
                Some(None) => {
                    let controls = autopilot(&body.0, &round, &course.0);
                    step(&mut body.0, controls, track);
                }
                None => step(&mut body.0, Controls::default(), track),
            }
            continue;
        }
        let mut controls = match &mut *pilot {
            Pilot::Human(steps) => next_steps(steps),
            Pilot::Agent(wanted) => vec![autopilot(&body.0, wanted, &course.0)],
            Pilot::Bot(bot) => {
                let (temper, mind) = &mut **bot;
                vec![drive_among(&body.0, me, &field, &hazards, temper, mind, track)]
            }
            Pilot::Ghost(_) => Vec::new(),
        };
        // The start: a human's throttle on the grid is timed against the lights.
        let human = matches!(*pilot, Pilot::Human(_));
        if let Some(left) = left {
            if human && let Some(last) = controls.last() {
                start.pressed = (last.throttle > 0.5).then(|| start.pressed.unwrap_or(left));
            }
        } else if race.phase == Phase::Racing && !start.judged {
            start.judged = true;
            let pressed = match *pilot {
                Pilot::Human(_) => start.pressed,
                Pilot::Bot(_) => bot_start(me, race.races()),
                // An agent's kart waits for the agent: it presses nothing.
                Pilot::Agent(_) | Pilot::Ghost(_) => None,
            };
            launch(&mut body.0, pressed);
            // A time trial is recorded from here, to be driven again (D-062).
            if trial && !matches!(*pilot, Pilot::Bot(_)) {
                start.run = Some(Recorder::new(body.0));
            }
        }
        // Past the line, the autopilot brings the kart round gently.
        if progress.finish_ms.is_some() {
            controls = vec![autopilot(&body.0, &proto::Autopilot { pace: 0.5, line: 0.0, ..default() }, &course.0)];
        } else if racing && let Some(run) = &mut start.run {
            run.tick(&controls);
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

/// How a kart starts (D-050): when, before the lights went, its throttle
/// was pressed and held since; judged once, as the race starts.
#[derive(Component, Default)]
pub struct Launch {
    pressed: Option<f32>,
    judged: bool,
    /// The run so far, in a time trial (D-062).
    pub run: Option<Recorder>,
}

/// The start of the world's own drivers, which press no throttle: a good
/// start for some, a stall now and then, by seat and race, the same
/// everywhere.
fn bot_start(seat: usize, races: u32) -> Option<f32> {
    match (seat as u32 * 7 + races * 3) % 10 {
        0..=3 => Some(0.2),
        4 => Some(2.0),
        _ => None,
    }
}

/// The world's autopilot, for AI agents' karts: a pace, a line and drifts
/// as the agent asks; it brakes for bends and knows nothing of the others
/// ([`osage_kart_racecraft::follow`]). Same bits on every platform, so that
/// a local simulation — a benchmark, a replay — is reproducible.
pub fn autopilot(kart: &Kart, wanted: &proto::Autopilot, circuit: &Circuit) -> Controls {
    osage_kart_racecraft::follow(kart, wanted.pace, wanted.line, wanted.drift, &circuit.track)
}

/// Karts collide (D-045), and those in another's wake charge a slipstream
/// (D-050) — but in a time trial, where each drives alone (D-062).
pub fn collide(cups: Res<crate::cup::Cups>, mut karts: Query<&mut Body>) {
    if cups.trial() {
        return;
    }
    let mut pairs = karts.iter_combinations_mut();
    while let Some([mut a, mut b]) = pairs.fetch_next() {
        bump(&mut a.0, &mut b.0);
    }
    let mut all: Vec<Kart> = karts.iter().map(|body| body.0).collect();
    slipstream(&mut all);
    for (mut body, kart) in karts.iter_mut().zip(all) {
        body.0 = kart;
    }
}
