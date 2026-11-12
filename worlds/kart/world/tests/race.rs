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
        Track::new(track.points.chunks(2).map(|p| [p[0], p[1]]).collect(), track.half_width, track.verge)
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
    let mut harness = Harness::new(KartWorld { grid: 4, laps: 1 }, TICK_HZ);
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
    let mut harness = Harness::new(KartWorld { grid: 1, laps: 3 }, TICK_HZ);
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
        let step_controls = proto::Step { sequence, throttle: 1.0, steer, drift, use_item: false };
        let message = proto::Controls { steps: vec![step_controls] };
        harness.send(alice.session, intent(sequence, entity, controls_type, &message));
        harness.run(1);
        step(&mut predicted, Controls { throttle: 1.0, steer, drift }, &track);
        fastest = fastest.max(predicted.speed);
        drifted |= predicted.drift != 0.0;
    }

    let (_, _, server) = alice.kart_of("alice");
    assert_eq!(server.applied, 600);
    let server = Kart {
        x: server.x,
        z: server.z,
        heading: server.heading,
        speed: server.speed,
        slide: server.slide,
        drift: server.drift,
        charge: server.charge,
        boost: server.boost,
        spin: server.spin,
    };
    assert_eq!(server, predicted, "bit for bit");
    assert!(fastest > 21.0 && drifted, "the kart reached full speed and drifted: {fastest}");
}

#[test]
fn an_agent_drives_through_the_autopilot() {
    let mut harness = Harness::new(KartWorld { grid: 1, laps: 3 }, TICK_HZ);
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
    let mut harness = Harness::new(KartWorld { grid: 3, laps: 1 }, TICK_HZ);
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
    let mut harness = Harness::new(KartWorld { grid: 6, laps: 3 }, TICK_HZ);
    let mut alice = Client::join(&mut harness, SessionKind::Human, Role::Player, "alice");
    run_until(&mut harness, &mut alice, 5, |c| phase(&c.race()) == proto::Phase::Racing);
    let paddock = |c: &mut Client| -> proto::Paddock {
        c.catch_up();
        let board = c.world.entities().find(|&e| c.world.get::<proto::Paddock>(e, types::PADDOCK).is_some());
        board.and_then(|e| c.world.get(e, types::PADDOCK)).expect("the paddock")
    };
    assert_eq!(paddock(&mut alice).crates.len(), 12);
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
