// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The race: who drives, the phases (waiting, countdown, racing, results),
//! the grid, laps counted through every sector in order, and the ranking.

use std::{collections::BTreeMap, time::Duration};

use bevy::prelude::*;
use osage_kart_physics::Kart;
use osage_kart_racecraft::Temper;
use osage_world_module_api::{
    Discrete, EntityIds, Joined, Left, Owner, Replicated, SessionKind, Volatile, builtin,
    osage_proto::{components, types as core_types, v0::Role},
    server_time_us,
};

use crate::{
    Body, Course, Ids, RaceBoard, Rules,
    driving::Pilot,
    paddock::{Items, Seat, item, items_for, to_proto},
    proto::{self, Phase},
    records::{Timed, Times},
    roster::{Choices, Pair, build_proto},
};
use osage_kart_records::Event;

const COUNTDOWN: Duration = Duration::from_secs(3);
const RESULTS: Duration = Duration::from_secs(8);
/// After the first kart finishes, the others have this long.
const FINISH_WINDOW: Duration = Duration::from_secs(30);
/// A race never lasts longer.
const LONGEST_RACE: Duration = Duration::from_secs(360);
/// The world's drivers, each with a temperament: the presets, and three
/// drawn from their place.
const BOTS: &[(&str, Option<Temper>)] = &[
    ("Comet", Some(Temper::VETERAN)),
    ("Pixel", Some(Temper::ROOKIE)),
    ("Bolt", Some(Temper::HOTHEAD)),
    ("Nova", Some(Temper::STEADY)),
    ("Zippy", None),
    ("Echo", Some(Temper::CAUTIOUS)),
    ("Flash", None),
    ("Juno", None),
];

/// Connected humans and agents who drive, by session.
#[derive(Resource, Default)]
pub struct Drivers(BTreeMap<u64, components::Controller>);

/// Calls about the race heard this tick: (session, call).
#[derive(Resource, Default)]
pub struct Calls(pub Vec<(u64, proto::RaceCall)>);

/// Seconds the karts wait on the grid for the host before the world starts
/// the race itself: a host gone quiet does not hold everyone.
const LONGEST_WAIT: Duration = Duration::from_secs(300);

#[derive(Resource)]
pub struct Race {
    pub phase: Phase,
    ends: Option<Duration>,
    started: Duration,
    first_finish: Option<Duration>,
    order: Vec<u64>,
    /// Races lined up so far: each draws its items differently.
    races: u32,
}

impl Race {
    /// Seconds left until the lights go, during the countdown.
    pub fn countdown_left(&self, now: Duration) -> Option<f32> {
        (self.phase == Phase::Countdown).then(|| self.ends.map_or(0.0, |end| end.saturating_sub(now).as_secs_f32()))
    }

    /// Races lined up so far.
    pub fn races(&self) -> u32 {
        self.races
    }
}

impl Default for Race {
    fn default() -> Self {
        Self {
            phase: Phase::Waiting,
            ends: None,
            started: Duration::ZERO,
            first_finish: None,
            order: Vec::new(),
            races: 0,
        }
    }
}

/// How a kart's race goes.
#[derive(Component)]
pub struct Progress {
    name: String,
    color: u32,
    bot: bool,
    /// 0 on the grid, 1 on the first lap … laps + 1 once finished.
    lap: u32,
    sector: u32,
    /// Distance covered, counted only through sectors in order.
    distance: f32,
    pub finish_ms: Option<u64>,
    pub position: u32,
    /// When it last crossed the line, racing.
    lap_started: Option<Duration>,
    /// Who drives and in what (D-055).
    pub pair: Pair,
}

impl Progress {
    /// Distance covered in the race, counted only through sectors in order, m.
    pub fn covered(&self) -> f32 {
        self.distance
    }

    /// Another driver and kart, chosen on the grid: the colour follows.
    pub fn set_pair(&mut self, pair: Pair) {
        self.pair = pair;
        self.color = pair.0.colour;
    }
}

impl Drivers {
    /// The host: the first human driver still here (sessions count up).
    fn host(&self) -> Option<u64> {
        self.0.iter().find(|(_, c)| c.kind() == components::ControllerKind::Human).map(|(session, _)| *session)
    }

    /// Drivers who race: humans and agents.
    fn count(&self) -> usize {
        self.0.len()
    }
}

pub fn sessions(mut joined: MessageReader<Joined>, mut left: MessageReader<Left>, mut drivers: ResMut<Drivers>) {
    for joined in joined.read() {
        let human = joined.kind == SessionKind::Human && joined.role == Role::Player;
        if human || joined.kind == SessionKind::Agent {
            drivers.0.insert(joined.session, joined.controller.clone());
        }
    }
    for left in left.read() {
        drivers.0.remove(&left.session);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn flow(
    mut commands: Commands,
    time: Res<Time>,
    rules: Res<Rules>,
    course: Res<Course>,
    ids: Res<Ids>,
    drivers: Res<Drivers>,
    mut calls: ResMut<Calls>,
    mut race: ResMut<Race>,
    mut entity_ids: ResMut<EntityIds>,
    choices: Res<Choices>,
    karts: Query<(Entity, &Progress, Option<&Owner>)>,
) {
    let now = time.elapsed();
    let ended = race.ends.is_some_and(|end| now >= end);
    let calls = std::mem::take(&mut calls.0);
    let host = drivers.host().filter(|_| rules.host_starts);
    let called = |wanted: proto::RaceCall, by: Option<u64>| {
        calls.iter().any(|(session, call)| *call == wanted && by.is_some_and(|by| by == *session))
    };
    // Lined up, the karts wait for the host, unless there is none.
    let ready = |race: &mut Race| {
        if host.is_some() {
            race.phase = Phase::Ready;
            race.ends = Some(now + LONGEST_WAIT);
        } else {
            race.phase = Phase::Countdown;
            race.ends = Some(now + COUNTDOWN);
        }
    };
    match race.phase {
        Phase::Waiting if !drivers.0.is_empty() => {
            line_up(&mut commands, &rules, &course, &ids, &drivers, &choices, &mut entity_ids, &karts, &mut race.races);
            ready(&mut race);
        }
        Phase::Ready if drivers.0.is_empty() => {
            for (entity, _, _) in &karts {
                commands.entity(entity).despawn();
            }
            race.phase = Phase::Waiting;
            race.ends = None;
        }
        Phase::Ready if ended || host.is_none() || called(proto::RaceCall::Start, host) => {
            race.phase = Phase::Countdown;
            race.ends = Some(now + COUNTDOWN);
        }
        Phase::Ready => {
            // Drivers who arrive take a bot's place.
            let seated: Vec<u64> = karts.iter().filter_map(|(_, _, owner)| owner.map(|o| o.0)).collect();
            if drivers.0.keys().any(|session| !seated.contains(session)) {
                line_up(
                    &mut commands,
                    &rules,
                    &course,
                    &ids,
                    &drivers,
                    &choices,
                    &mut entity_ids,
                    &karts,
                    &mut race.races,
                );
            }
        }
        Phase::Countdown if ended => {
            race.phase = Phase::Racing;
            race.started = now;
            race.first_finish = None;
            race.ends = None;
        }
        Phase::Countdown => {
            // Drivers who arrive during the countdown take a bot's place.
            let seated: Vec<u64> = karts.iter().filter_map(|(_, _, owner)| owner.map(|o| o.0)).collect();
            if drivers.0.keys().any(|session| !seated.contains(session)) {
                line_up(
                    &mut commands,
                    &rules,
                    &course,
                    &ids,
                    &drivers,
                    &choices,
                    &mut entity_ids,
                    &karts,
                    &mut race.races,
                );
            }
        }
        Phase::Racing => {
            let humans: Vec<_> = karts.iter().filter(|(_, p, _)| !p.bot).collect();
            let all_finished = !humans.is_empty() && humans.iter().all(|(_, p, _)| p.finish_ms.is_some());
            let window_closed = race.first_finish.is_some_and(|first| now >= first + FINISH_WINDOW);
            // A driver racing alone may end the race.
            let alone = drivers.count() == 1;
            let stopped = alone && calls.iter().any(|(_, call)| *call == proto::RaceCall::Stop);
            let over = all_finished || window_closed || now >= race.started + LONGEST_RACE;
            if drivers.0.is_empty() || stopped || over {
                race.phase = Phase::Finished;
                race.ends = Some(now + RESULTS);
            }
        }
        Phase::Finished if ended => {
            if drivers.0.is_empty() {
                for (entity, _, _) in &karts {
                    commands.entity(entity).despawn();
                }
                race.phase = Phase::Waiting;
                race.ends = None;
            } else {
                line_up(
                    &mut commands,
                    &rules,
                    &course,
                    &ids,
                    &drivers,
                    &choices,
                    &mut entity_ids,
                    &karts,
                    &mut race.races,
                );
                ready(&mut race);
            }
        }
        _ => {}
    }
}

/// Clears the circuit and puts every driver, then bots, on the grid.
#[allow(clippy::too_many_arguments)]
fn line_up(
    commands: &mut Commands,
    rules: &Rules,
    course: &Course,
    ids: &Ids,
    drivers: &Drivers,
    choices: &Choices,
    entity_ids: &mut EntityIds,
    karts: &Query<(Entity, &Progress, Option<&Owner>)>,
    races: &mut u32,
) {
    for (entity, _, _) in karts {
        commands.entity(entity).despawn();
    }
    let seats = rules.grid.max(drivers.0.len());
    *races = races.wrapping_add(1);
    commands.insert_resource(items_for(&course.0.track, seats, *races));
    let bots = BOTS.iter().enumerate().map(|(i, (name, temper))| {
        let controller = components::Controller {
            kind: components::ControllerKind::System.into(),
            per_world_id: format!("bot-{}", name.to_lowercase()),
            ..default()
        };
        let pilot = Pilot::bot(temper.unwrap_or(Temper::of(i as u32)), i as u32 + 1);
        (None, name.to_string(), controller, pilot, crate::roster::bot(name, i))
    });
    let people = drivers.0.iter().map(|(session, controller)| {
        let pilot =
            if controller.kind() == components::ControllerKind::Agent { Pilot::agent() } else { Pilot::human() };
        (Some(*session), controller.per_world_id.clone(), controller.clone(), pilot, choices.of(*session))
    });
    for (slot, (owner, name, controller, pilot, pair)) in people.chain(bots).take(seats).enumerate() {
        let kart = Kart { build: osage_kart_roster::build(pair.0, pair.1), ..course.0.grid_slot(slot) };
        let bot = owner.is_none();
        let progress = Progress {
            name,
            color: pair.0.colour,
            bot,
            lap: 0,
            sector: crate::circuit::SECTORS - 1,
            distance: 0.0,
            finish_ms: None,
            position: slot as u32 + 1,
            lap_started: None,
            pair,
        };
        let discrete = Discrete::default()
            .with(ids.content_ref, &builtin(core_types::BUILTIN_AVATAR))
            .with(ids.controller, &controller)
            .with(ids.racer, &racer(&progress, None, 0));
        let volatile = Volatile::default().with(ids.state, &state(&kart, 0));
        let parts = (
            entity_ids.next_id(),
            transform(&kart),
            discrete,
            volatile,
            Body(kart),
            pilot,
            progress,
            Seat(slot),
            crate::driving::Launch::default(),
        );
        let mut spawned = commands.spawn(parts);
        if let Some(session) = owner {
            spawned.insert(Owner(session));
        }
    }
}

/// Laps through every sector in order, finishes, ranking (on the grid: the
/// grid order).
pub fn progress(
    time: Res<Time>,
    rules: Res<Rules>,
    course: Res<Course>,
    mut race: ResMut<Race>,
    mut times: ResMut<Times>,
    mut karts: Query<(&Replicated, &Body, &mut Progress)>,
) {
    let racing = race.phase == Phase::Racing;
    let now = time.elapsed();
    let (sectors, sector_length) = (crate::circuit::SECTORS, course.0.sector_length());
    for (_, body, mut progress) in &mut karts {
        let from_start = course.0.from_start(course.0.track.locate(body.0.x, body.0.z).along);
        let sector = ((from_start / sector_length) as u32).min(sectors - 1);
        if racing && sector == (progress.sector + 1) % sectors {
            progress.sector = sector;
            if sector == 0 {
                progress.lap += 1;
                let (name, bot, pair) = (progress.name.clone(), progress.bot, progress.pair);
                if let Some(started) = progress.lap_started.replace(now)
                    && progress.finish_ms.is_none()
                {
                    let ms = (now - started).as_millis() as u64;
                    times.0.push(Timed { name: name.clone(), bot, pair, event: Event::Lap, ms });
                }
                if progress.lap > rules.laps && progress.finish_ms.is_none() {
                    let ms = (now - race.started).as_millis() as u64;
                    progress.finish_ms = Some(ms);
                    race.first_finish.get_or_insert(now);
                    times.0.push(Timed { name, bot, pair, event: Event::Race { laps: rules.laps }, ms });
                }
            }
        }
        let within = (from_start - progress.sector as f32 * sector_length).clamp(0.0, sector_length);
        progress.distance = (progress.lap * sectors + progress.sector) as f32 * sector_length + within;
    }

    let mut order: Vec<(u64, Option<u64>, f32)> =
        karts.iter().map(|(id, _, p)| (id.0, p.finish_ms, p.distance)).collect();
    order.sort_by(|a, b| match (a.1, b.1) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => b.2.total_cmp(&a.2),
    });
    race.order = order.iter().map(|(id, _, _)| *id).collect();
    for (id, _, mut progress) in &mut karts {
        let position = race.order.iter().position(|o| *o == id.0).map_or(0, |p| p as u32 + 1);
        if progress.position != position {
            progress.position = position;
        }
    }
}

/// Copies the race into what clients see, changing only what changed.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn publish(
    time: Res<Time>,
    ids: Res<Ids>,
    race: Res<Race>,
    rules: Res<Rules>,
    drivers: Res<Drivers>,
    paddock: Option<Res<Items>>,
    owners: Query<(&Replicated, &Owner)>,
    mut karts: Query<
        (&Body, &Progress, &Pilot, &Seat, &mut Transform, &mut Volatile, &mut Discrete),
        Without<RaceBoard>,
    >,
    mut board: Query<(&mut Discrete, &mut Volatile), With<RaceBoard>>,
) {
    let paddock = paddock.as_deref().map(|items| &items.0);
    for (body, progress, pilot, seat, mut place, mut volatile, mut discrete) in &mut karts {
        place.set_if_neq(transform(&body.0));
        let state = prost::Message::encode_to_vec(&state(&body.0, pilot.applied()));
        if volatile.0.get(&ids.state) != Some(&state) {
            volatile.0.insert(ids.state, state);
        }
        let racer = prost::Message::encode_to_vec(&racer(progress, paddock, seat.0));
        if discrete.0.get(&ids.racer) != Some(&racer) {
            discrete.0.insert(ids.racer, racer);
        }
    }

    let now = time.elapsed();
    let at =
        |instant: Duration| server_time_us().saturating_add_signed(instant.as_micros() as i64 - now.as_micros() as i64);
    let summary = proto::Race {
        phase: race.phase.into(),
        phase_ends_us: race.ends.map_or(0, at),
        laps: rules.laps,
        started_us: if race.phase == Phase::Racing || race.phase == Phase::Finished { at(race.started) } else { 0 },
        order: race.order.clone(),
        host: drivers
            .host()
            .filter(|_| rules.host_starts)
            .and_then(|host| owners.iter().find(|(_, owner)| owner.0 == host))
            .map_or(0, |(id, _)| id.0),
    };
    if let Ok((mut board, mut volatile)) = board.single_mut() {
        if let Some(paddock) = paddock {
            let encoded = prost::Message::encode_to_vec(&to_proto(paddock));
            if volatile.0.get(&ids.paddock) != Some(&encoded) {
                volatile.0.insert(ids.paddock, encoded);
            }
        }
        // Times move with the server clock: republish only when the race itself changed.
        let current: Option<proto::Race> = board.get(ids.race);
        let same = current.is_some_and(|c| {
            c.phase == summary.phase
                && c.order == summary.order
                && c.host == summary.host
                && c.phase_ends_us.abs_diff(summary.phase_ends_us) < 50_000
                && c.started_us.abs_diff(summary.started_us) < 50_000
        });
        if !same {
            board.set(ids.race, &summary);
        }
    }
}

fn transform(kart: &Kart) -> Transform {
    Transform::from_xyz(kart.x, kart.height, kart.z).with_rotation(Quat::from_rotation_y(kart.heading))
}

fn state(kart: &Kart, applied: u64) -> proto::KartState {
    proto::KartState {
        x: kart.x,
        z: kart.z,
        heading: kart.heading,
        speed: kart.speed,
        slide: kart.slide,
        drift: kart.drift,
        charge: kart.charge,
        boost: kart.boost,
        applied,
        spin: kart.spin,
        rocket: kart.rocket,
        height: kart.height,
        climb: kart.climb,
        trick: kart.trick,
        stall: kart.stall,
        draft: kart.draft,
        held: kart.held,
    }
}

fn racer(progress: &Progress, paddock: Option<&osage_kart_items::Paddock>, seat: usize) -> proto::Racer {
    let (held, smoke) = (paddock.and_then(|p| p.held(seat)), paddock.and_then(|p| p.smoke(seat)));
    proto::Racer {
        name: progress.name.clone(),
        color: progress.color,
        bot: progress.bot,
        lap: progress.lap,
        position: progress.position,
        finish_ms: progress.finish_ms.unwrap_or(0),
        held: item(held).into(),
        smoke: smoke.unwrap_or(0),
        held_count: paddock.map_or(0, |p| u32::from(p.count(seat))),
        trailing: paddock.is_some_and(|p| p.trailing(seat)),
        // Whole hundredths: a guard that only ticks down is sent as it changes.
        guard: paddock.map_or(0.0, |p| (p.guard(seat) * 100.0).ceil() / 100.0),
        driver: progress.pair.0.id.to_owned(),
        kart: progress.pair.1.id.to_owned(),
        build: Some(build_proto(osage_kart_roster::build(progress.pair.0, progress.pair.1))),
    }
}
