// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The kart world in a region, tick by tick in simulated time, seen through
//! the client SDK as a client would see it.

use osage_kart_physics::{Controls, Kart, Track, step};
use osage_sdk::{
    World, intent,
    v0::{Role, ServerMessage},
};
use osage_server::world::{Harness, SessionKind};
use osage_world_kart::{KartWorld, TICK_HZ, proto, types};
use tokio::sync::mpsc::UnboundedReceiver;

const SECOND: usize = TICK_HZ as usize;

struct Client {
    session: u64,
    messages: UnboundedReceiver<ServerMessage>,
    world: World,
}

impl Client {
    fn join(harness: &mut Harness, kind: SessionKind, role: Role, name: &str) -> Self {
        let (session, messages) = harness.join(kind, role, name);
        Self { session, messages, world: World::new(&harness.welcome()) }
    }

    fn catch_up(&mut self) {
        while let Ok(message) = self.messages.try_recv() {
            self.world.apply(&message);
        }
    }

    fn race(&mut self) -> proto::Race {
        self.catch_up();
        let board = self.world.entities().find(|&e| self.world.get::<proto::Race>(e, types::RACE).is_some());
        board.and_then(|e| self.world.get(e, types::RACE)).expect("the race board")
    }

    /// (entity, racer, state) of every kart.
    fn karts(&mut self) -> Vec<(u64, proto::Racer, proto::KartState)> {
        self.catch_up();
        let karts: Vec<u64> =
            self.world.entities().filter(|&e| self.world.get::<proto::Racer>(e, types::RACER).is_some()).collect();
        karts
            .into_iter()
            .map(|e| (e, self.world.get(e, types::RACER).unwrap(), self.world.get(e, types::STATE).unwrap()))
            .collect()
    }

    fn kart_of(&mut self, name: &str) -> (u64, proto::Racer, proto::KartState) {
        self.karts().into_iter().find(|(_, racer, _)| racer.name == name).expect("a kart")
    }

    fn track(&mut self) -> Track {
        self.catch_up();
        let entity = self.world.entities().find(|&e| self.world.get::<proto::Track>(e, types::TRACK).is_some());
        let track: proto::Track = self.world.get(entity.expect("the track"), types::TRACK).unwrap();
        // Crests and all, as a client builds it.
        osage_world_kart::Circuit::from_proto(&track).track
    }
}

fn phase(race: &proto::Race) -> proto::Phase {
    race.phase()
}

fn run_until(harness: &mut Harness, client: &mut Client, seconds: usize, done: impl Fn(&mut Client) -> bool) {
    for _ in 0..seconds {
        harness.run(SECOND);
        if done(client) {
            return;
        }
    }
    panic!("not within {seconds} s");
}

#[test]
fn bots_race_and_laps_count_through_every_sector() {
    let mut harness = Harness::new(KartWorld::unattended(4, 1), TICK_HZ);
    let mut alice = Client::join(&mut harness, SessionKind::Human, Role::Player, "alice");
    harness.run(2);
    assert_eq!(phase(&alice.race()), proto::Phase::Countdown);
    let karts = alice.karts();
    assert_eq!(karts.len(), 4);
    assert_eq!(karts.iter().filter(|(_, r, _)| r.bot).count(), 3, "bots fill the grid");

    run_until(&mut harness, &mut alice, 5, |c| phase(&c.race()) == proto::Phase::Racing);
    run_until(&mut harness, &mut alice, 120, |c| c.karts().iter().any(|(_, r, _)| r.finish_ms > 0));

    let (_, me, _) = alice.kart_of("alice");
    assert_eq!(me.lap, 0, "alice sent no controls and never crossed the line");
    assert_eq!(me.position, 4, "last");
    let winner = alice.karts().into_iter().find(|(_, r, _)| r.position == 1).unwrap();
    assert!(winner.1.bot && winner.1.finish_ms > 20_000 && winner.1.lap == 2, "{:?}", winner.1);
    assert_eq!(alice.race().order.first(), Some(&winner.0));

    // The others get 30 s, then the results, then the next race.
    run_until(&mut harness, &mut alice, 35, |c| phase(&c.race()) == proto::Phase::Finished);
    run_until(&mut harness, &mut alice, 10, |c| phase(&c.race()) == proto::Phase::Countdown);
    assert!(alice.karts().iter().all(|(_, r, _)| r.lap == 0 && r.finish_ms == 0), "a fresh grid");
}

/// The heart of client-side prediction (D-031): a client that applies its own
/// steps with the shared physics, on the track the world described, computes
/// exactly the state the server sends back.
#[test]
fn a_client_predicts_its_kart_exactly() {
    let mut harness = Harness::new(KartWorld::unattended(1, 3), TICK_HZ);
    let mut alice = Client::join(&mut harness, SessionKind::Human, Role::Player, "alice");
    run_until(&mut harness, &mut alice, 5, |c| phase(&c.race()) == proto::Phase::Racing);
    let track = alice.track();
    let (entity, _, start) = alice.kart_of("alice");
    let controls_type = alice.world.types().id(types::CONTROLS).unwrap();

    let mut predicted = Kart { x: start.x, z: start.z, heading: start.heading, ..Kart::default() };
    let (mut fastest, mut drifted): (f32, bool) = (0.0, false);
    for sequence in 1..=600u64 {
        let (steer, drift) = match sequence {
            1..=200 => (0.0, false),
            201..=250 => (0.3, true), // a drift…
            _ => (0.0, false),        // …then straight on, into the wall: walls are predicted too
        };
        let step_controls = proto::Step { sequence, throttle: 1.0, steer, drift, ..Default::default() };
        let message = proto::Controls { steps: vec![step_controls] };
        harness.send(alice.session, intent(sequence, entity, controls_type, &message));
        harness.run(1);
        step(&mut predicted, Controls { throttle: 1.0, steer, drift }, &track);
        fastest = fastest.max(predicted.speed);
        drifted |= predicted.drift != 0.0;
    }

    let (_, _, server) = alice.kart_of("alice");
    assert_eq!(server.applied, 600);
    let server = osage_world_kart::kart_of(&server);
    assert_eq!(server, predicted, "bit for bit");
    assert!(fastest > 21.0 && drifted, "the kart reached full speed and drifted: {fastest}");
}

#[test]
fn an_agent_drives_through_the_autopilot() {
    let mut harness = Harness::new(KartWorld::unattended(1, 3), TICK_HZ);
    let mut agent = Client::join(&mut harness, SessionKind::Agent, Role::Agent, "agent-1.alice");
    run_until(&mut harness, &mut agent, 5, |c| phase(&c.race()) == proto::Phase::Racing);
    harness.run(2 * SECOND);
    let (entity, _, waiting) = agent.kart_of("agent-1.alice");
    assert_eq!(waiting.speed, 0.0, "an agent's kart waits for its first instruction");

    let autopilot_type = agent.world.types().id(types::AUTOPILOT).unwrap();
    let pace = proto::Autopilot { pace: 0.8, line: 0.0, drift: false, use_items: false };
    harness.send(agent.session, intent(1, entity, autopilot_type, &pace));
    harness.run(20 * SECOND);
    let (_, racer, driving) = agent.kart_of("agent-1.alice");
    assert!(driving.speed > 10.0, "{driving:?}");
    assert_eq!(racer.lap, 1, "it crossed the line and drives its first lap");
}

#[test]
fn a_late_driver_takes_a_bots_place_and_an_empty_world_stops() {
    let mut harness = Harness::new(KartWorld::unattended(3, 1), TICK_HZ);
    let mut alice = Client::join(&mut harness, SessionKind::Human, Role::Player, "alice");
    harness.run(SECOND);
    let bob = Client::join(&mut harness, SessionKind::Human, Role::Player, "bob");
    harness.run(2);
    let names: Vec<String> = alice.karts().into_iter().filter(|(_, r, _)| !r.bot).map(|(_, r, _)| r.name).collect();
    assert_eq!(alice.karts().len(), 3);
    assert!(names.contains(&"alice".to_owned()) && names.contains(&"bob".to_owned()), "{names:?}");

    run_until(&mut harness, &mut alice, 5, |c| phase(&c.race()) == proto::Phase::Racing);
    harness.leave(bob.session);
    harness.leave(alice.session);
    let observer = harness.join(SessionKind::Human, Role::Observer, "watcher");
    let mut observer = Client { session: observer.0, messages: observer.1, world: World::new(&harness.welcome()) };
    harness.run(SECOND);
    assert_eq!(phase(&observer.race()), proto::Phase::Finished, "no driver left");
    harness.run(10 * SECOND);
    assert_eq!(phase(&observer.race()), proto::Phase::Waiting);
    assert!(observer.karts().is_empty());
}

/// Paddock objects (kart.paddock/1): bots break crates, hold what they get
/// — the world says so in their Racer — and use it on the track.
#[test]
fn bots_pick_items_from_the_crates_and_use_them() {
    let mut harness = Harness::new(KartWorld::unattended(6, 3), TICK_HZ);
    let mut alice = Client::join(&mut harness, SessionKind::Human, Role::Player, "alice");
    run_until(&mut harness, &mut alice, 5, |c| phase(&c.race()) == proto::Phase::Racing);
    let paddock = |c: &mut Client| -> proto::Paddock {
        c.catch_up();
        let board = c.world.entities().find(|&e| c.world.get::<proto::Paddock>(e, types::PADDOCK).is_some());
        board.and_then(|e| c.world.get(e, types::PADDOCK)).expect("the paddock")
    };
    assert_eq!(paddock(&mut alice).crates.len(), 15);
    let (mut broken, mut held, mut used) = (false, false, false);
    for _ in 0..90 * 4 {
        harness.run(SECOND / 4);
        let now = paddock(&mut alice);
        broken |= now.crates.iter().any(|c| !c.standing);
        used |= !now.things.is_empty();
        held |= alice.karts().iter().any(|(_, r, _)| r.held() != proto::Item::None);
        if broken && held && used {
            return;
        }
    }
    panic!("broken {broken}, held {held}, used {used}");
}

/// Bots use every kind of paddock object (D-052): over a race or two they
/// hold many kinds, keep some behind their kart as a shield, and leave or
/// send things on the track — drones among them.
#[test]
fn bots_hold_items_behind_and_use_many_kinds() {
    let mut harness = Harness::new(KartWorld::unattended(8, 3), TICK_HZ);
    let mut alice = Client::join(&mut harness, SessionKind::Human, Role::Player, "alice");
    run_until(&mut harness, &mut alice, 5, |c| phase(&c.race()) == proto::Phase::Racing);
    let mut held = std::collections::BTreeSet::new();
    let mut things = std::collections::BTreeSet::new();
    let mut trailing = false;
    let mut flew = false;
    for _ in 0..400 * 30 {
        harness.run(SECOND / 30);
        alice.catch_up();
        for (_, racer, _) in alice.karts() {
            held.insert(racer.held);
            trailing |= racer.trailing;
        }
        let board = alice.world.entities().find(|&e| alice.world.get::<proto::Paddock>(e, types::PADDOCK).is_some());
        if let Some(paddock) = board.and_then(|e| alice.world.get::<proto::Paddock>(e, types::PADDOCK)) {
            for thing in &paddock.things {
                things.insert(thing.kind);
                flew |= thing.kind() == proto::Item::Drone && thing.height > 2.0;
            }
        }
        if held.len() >= 9 && trailing && things.len() >= 4 && flew {
            break;
        }
    }
    held.remove(&(proto::Item::None as i32));
    assert!(held.len() >= 8, "kinds held: {held:?}");
    assert!(trailing, "no bot held its item behind");
    assert!(things.len() >= 3, "kinds on the track: {things:?}");
    assert!(flew, "no drone flew: {things:?}");
}

/// Sends one step carrying `call` for `client`'s kart.
fn call(harness: &mut Harness, client: &mut Client, name: &str, sequence: u64, call: proto::RaceCall) {
    let (entity, _, _) = client.kart_of(name);
    let controls_type = client.world.types().id(types::CONTROLS).unwrap();
    let step = proto::Step { sequence, call: call.into(), ..Default::default() };
    harness.send(client.session, intent(sequence, entity, controls_type, &proto::Controls { steps: vec![step] }));
    harness.run(2);
}

/// The host (the first human driver) starts each race; the others wait on
/// the grid with them, and whoever races alone can end the race.
#[test]
fn the_host_starts_the_race_and_a_driver_alone_can_end_it() {
    let mut harness = Harness::new(KartWorld { grid: 3, laps: 3, host_starts: true, records: None }, TICK_HZ);
    let mut alice = Client::join(&mut harness, SessionKind::Human, Role::Player, "alice");
    harness.run(SECOND);
    let mut bob = Client::join(&mut harness, SessionKind::Human, Role::Player, "bob");
    harness.run(5 * SECOND);
    let race = alice.race();
    assert_eq!(phase(&race), proto::Phase::Ready, "waiting for the host");
    assert_eq!(race.host, alice.kart_of("alice").0, "alice came first");

    call(&mut harness, &mut bob, "bob", 1, proto::RaceCall::Start);
    assert_eq!(phase(&alice.race()), proto::Phase::Ready, "bob is not the host");
    call(&mut harness, &mut alice, "alice", 1, proto::RaceCall::Start);
    assert_eq!(phase(&alice.race()), proto::Phase::Countdown);
    run_until(&mut harness, &mut alice, 5, |c| phase(&c.race()) == proto::Phase::Racing);

    call(&mut harness, &mut alice, "alice", 2, proto::RaceCall::Stop);
    assert_eq!(phase(&alice.race()), proto::Phase::Racing, "bob races too");
    harness.leave(bob.session);
    harness.run(2);
    call(&mut harness, &mut alice, "alice", 3, proto::RaceCall::Stop);
    assert_eq!(phase(&alice.race()), proto::Phase::Finished, "alone, alice ends it");
    run_until(&mut harness, &mut alice, 10, |c| phase(&c.race()) == proto::Phase::Ready);
}

/// The start is judged against the lights (D-050): a driver who holds the
/// throttle through the countdown stalls, one who presses it as the lights
/// go gets a boost.
#[test]
fn a_start_in_time_boosts_and_one_too_early_stalls() {
    // Ticks before the end of the countdown at which the throttle goes down.
    let start = |pressed_before: usize| {
        let mut harness = Harness::new(KartWorld::unattended(1, 3), TICK_HZ);
        let mut alice = Client::join(&mut harness, SessionKind::Human, Role::Player, "alice");
        harness.run(2);
        assert_eq!(phase(&alice.race()), proto::Phase::Countdown);
        let (entity, _, _) = alice.kart_of("alice");
        let controls_type = alice.world.types().id(types::CONTROLS).unwrap();
        let countdown = 3 * SECOND;
        for sequence in 1..=(countdown + 30) as u64 {
            let throttle = if sequence as usize + pressed_before >= countdown { 1.0 } else { 0.0 };
            let step = proto::Step { sequence, throttle, ..Default::default() };
            harness
                .send(alice.session, intent(sequence, entity, controls_type, &proto::Controls { steps: vec![step] }));
            harness.run(1);
            if phase(&alice.race()) == proto::Phase::Racing {
                harness.run(2);
                break;
            }
        }
        alice.kart_of("alice").2
    };
    let in_time = start(SECOND / 6);
    assert!(in_time.boost > 0.5 && in_time.stall == 0.0, "{in_time:?}");
    let early = start(3 * SECOND);
    assert!(early.stall > 0.0 && early.boost == 0.0, "{early:?}");
    let late = start(0);
    assert!(late.stall == 0.0, "{late:?}");
}

/// The circuit's crest lifts karts taken over it flat out into the air, and
/// the bold among the bots do a trick for a boost on landing.
#[test]
fn bots_fly_off_the_crests_and_trick() {
    let mut harness = Harness::new(KartWorld::unattended(6, 3), TICK_HZ);
    let mut alice = Client::join(&mut harness, SessionKind::Human, Role::Player, "alice");
    run_until(&mut harness, &mut alice, 5, |c| phase(&c.race()) == proto::Phase::Racing);
    let track = alice.track();
    assert_eq!(track.crests().len(), 1, "the circuit has its crest");
    let (mut flew, mut tricked) = (false, false);
    for _ in 0..60 * SECOND {
        harness.run(1);
        for (_, _, kart) in alice.karts() {
            flew |= kart.height > track.lift(track.locate(kart.x, kart.z).along).0 + 0.5;
            tricked |= kart.trick > 0.0;
        }
        if flew && tricked {
            return;
        }
    }
    panic!("flew {flew}, tricked {tricked}");
}

/// The feel of the driving is tuned against the bots (D-050): their race
/// time over two laps stays in a band, so that a change to the physics that
/// makes the circuit much faster or slower is seen.
#[test]
fn bots_lap_the_circuit_in_their_time_band() {
    let mut harness = Harness::new(KartWorld::unattended(6, 2), TICK_HZ);
    let mut alice = Client::join(&mut harness, SessionKind::Human, Role::Player, "alice");
    run_until(&mut harness, &mut alice, 5, |c| phase(&c.race()) == proto::Phase::Racing);
    run_until(&mut harness, &mut alice, 120, |c| c.karts().iter().filter(|(_, r, _)| r.finish_ms > 0).count() >= 3);
    let mut times: Vec<u64> =
        alice.karts().iter().filter(|(_, r, _)| r.finish_ms > 0).map(|(_, r, _)| r.finish_ms).collect();
    times.sort();
    let winner = times[0] as f32 / 1000.0;
    assert!((BAND.0..BAND.1).contains(&winner), "winner in {winner} s");
}

/// The winning bot's time over two laps, seconds: 60.1 s when the band was
/// set (2026-11-12, with jumps, three-level mini-turbos and slipstreams),
/// give or take a tenth.
const BAND: (f32, f32) = (54.0, 66.0);

/// Every flight off the crest in a race of bots, watched step by step: karts
/// take off at speed, the fast ones fly about half a second, all land on the
/// road pointing down it and race on without losing their speed (D-051).
#[test]
fn every_kart_flies_off_the_crests_and_lands_on_the_road() {
    #[derive(Debug, Default)]
    struct Flight {
        takeoff: f32,
        peak: f32,
        steps: u32,
        landing: Option<(f32, f32, f32)>,
        after: Option<f32>,
    }
    let mut harness = Harness::new(KartWorld::unattended(6, 2), TICK_HZ);
    let mut alice = Client::join(&mut harness, SessionKind::Human, Role::Player, "alice");
    run_until(&mut harness, &mut alice, 5, |c| phase(&c.race()) == proto::Phase::Racing);
    let track = alice.track();
    let mut flights: Vec<(u64, Flight)> = Vec::new();
    let mut flying: std::collections::HashMap<u64, usize> = Default::default();
    let mut since: std::collections::HashMap<u64, u32> = Default::default();
    for _ in 0..70 * SECOND {
        harness.run(1);
        for (entity, racer, kart) in alice.karts() {
            if racer.finish_ms > 0 {
                continue;
            }
            let spot = track.locate(kart.x, kart.z);
            let airborne = kart.height > track.lift(spot.along).0 + 1e-3;
            match (airborne, flying.get(&entity).copied()) {
                (true, None) => {
                    flying.insert(entity, flights.len());
                    flights.push((
                        entity,
                        Flight { takeoff: kart.speed, peak: kart.height, steps: 1, ..Default::default() },
                    ));
                }
                (true, Some(i)) => {
                    let flight = &mut flights[i].1;
                    flight.peak = flight.peak.max(kart.height);
                    flight.steps += 1;
                }
                (false, Some(i)) => {
                    let (_, road) = track.at(spot.along);
                    let off = osage_kart_physics::wrap(kart.heading - road).abs();
                    flights[i].1.landing = Some((spot.lateral, off, kart.speed));
                    flying.remove(&entity);
                    since.insert(entity, 0);
                }
                (false, None) => {}
            }
            // Half a second after landing: still racing.
            if let Some(n) = since.get_mut(&entity) {
                *n += 1;
                if *n == SECOND as u32 / 2 {
                    if let Some((_, flight)) = flights.iter_mut().rev().find(|(e, _)| *e == entity) {
                        flight.after = Some(kart.speed);
                    }
                    since.remove(&entity);
                }
            }
        }
    }
    let jumps = flights.iter().filter(|(_, f)| f.steps as f32 / TICK_HZ as f32 > 0.3).count();
    assert!(jumps >= 6, "six karts, one crest, two laps: {jumps} jumps in {flights:?}");
    for (entity, f) in &flights {
        let (lateral, off, speed) = f.landing.unwrap_or_else(|| panic!("kart {entity} never landed: {f:?}"));
        assert!(f.takeoff > 12.0, "kart {entity} took off slowly: {f:?}");
        assert!(f.steps as f32 / TICK_HZ as f32 <= 0.8, "kart {entity} flew {} steps", f.steps);
        assert!(f.peak < 3.2, "kart {entity} flew too high: {f:?}");
        assert!(
            lateral.abs() < track.half_width + osage_kart_physics::KERB,
            "kart {entity} landed off the road: {f:?}"
        );
        assert!(off < 0.5, "kart {entity} landed askew: {f:?}");
        // Only a boost that runs out in the air brings it back to top speed.
        let kept = f.takeoff.min(osage_kart_physics::MAX_SPEED);
        assert!(speed > kept - 2.0, "kart {entity} lost speed in the air: {f:?}");
        if let Some(after) = f.after {
            assert!(after > 0.7 * speed, "kart {entity} lost its speed after landing: {f:?}");
        }
    }
}

/// Records (kart.records/1, D-053): an agent's lap and race enter the
/// boards, fresh; the world saves them, a world started again on the same
/// file shows them, and a faster lap by the same driver replaces its own.
#[test]
fn records_are_kept_beaten_and_survive_a_restart() {
    let dir = std::env::temp_dir().join(format!("osage-kart-records-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("records.txt");
    let _ = std::fs::remove_file(&file);
    let world = || KartWorld { records: Some(file.clone()), ..KartWorld::unattended(1, 1) };
    let records = |c: &mut Client| -> Option<proto::Records> {
        c.catch_up();
        let board = c.world.entities().find(|&e| c.world.get::<proto::Race>(e, types::RACE).is_some())?;
        c.world.get(board, types::RECORDS)
    };
    let race_at = |pace: f32| {
        let mut harness = Harness::new(world(), TICK_HZ);
        let mut agent = Client::join(&mut harness, SessionKind::Agent, Role::Agent, "agent-1.alice");
        run_until(&mut harness, &mut agent, 5, |c| phase(&c.race()) == proto::Phase::Racing);
        let before = records(&mut agent).expect("published from the start");
        let (entity, ..) = agent.kart_of("agent-1.alice");
        let autopilot_type = agent.world.types().id(types::AUTOPILOT).unwrap();
        let wanted = proto::Autopilot { pace, line: 0.0, drift: false, use_items: false };
        harness.send(agent.session, intent(1, entity, autopilot_type, &wanted));
        run_until(&mut harness, &mut agent, 120, |c| c.kart_of("agent-1.alice").1.finish_ms > 0);
        harness.run(2);
        (before, records(&mut agent).unwrap(), agent.kart_of("agent-1.alice").1.finish_ms)
    };

    let (before, first, finish) = race_at(0.7);
    assert!(before.laps.is_empty() && before.races.is_empty(), "a new world has none: {before:?}");
    assert_eq!((first.track.as_str(), first.race_laps), ("paddock", 1));
    assert_eq!(first.laps.len(), 1, "{first:?}");
    let (lap, race) = (&first.laps[0], &first.races[0]);
    assert!(lap.name == "agent-1.alice" && lap.fresh && race.fresh && lap.at > 1_700_000_000, "{first:?}");
    assert_eq!(u64::from(race.ms), finish);
    assert!(lap.ms > 10_000 && lap.ms < race.ms, "the lap is from line to line, the race from the lights");
    assert!(std::fs::read_to_string(&file).unwrap().starts_with("kart-records/1\n"));

    let (kept, second, _) = race_at(0.95);
    assert_eq!(kept.laps.len(), 1, "the restarted world shows the saved records");
    assert_eq!(kept.laps[0].ms, lap.ms);
    assert!(!kept.laps[0].fresh, "not set in this race");
    assert_eq!(second.laps.len(), 1, "one entry per driver: {second:?}");
    assert!(second.laps[0].ms < lap.ms && second.laps[0].fresh, "beaten: {second:?}");
    std::fs::remove_dir_all(&dir).unwrap();
}
