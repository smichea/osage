// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Paddock objects: what karts collect and use during a race.
//!
//! Racks of crates stand across the road at three places of the lap. A kart
//! that drives through a crate breaks it and, if its hands are free, gets
//! one [`Item`], drawn by its place and its gap to the leader (D-052): the
//! leader gets defensive things, the back of the field and those far behind
//! strong ones. A crate comes back a few seconds later. The use button:
//!
//! - **nitro canister:** a long boost, flames and smoke from the exhausts;
//! - **rocket:** a rocket strapped behind the seat: faster than a nitro and
//!   longer; drawn mostly by the back of the field;
//! - **tyre:** held behind the kart while the button is down — a shield
//!   against a tyre from behind — and thrown ahead as it comes up, at the
//!   kart's speed and more; it rolls, slows, bounces off the barriers and
//!   spins the first kart it meets;
//! - **stack of three tyres:** the same, three times;
//! - **cone:** held behind, then dropped; a kart that hits it is slowed and
//!   knocked;
//! - **oil:** held behind, then left as a patch; a kart that drives into it
//!   spins;
//! - **smoke:** a canister that pours smoke of a colour of its own behind
//!   the kart for a while: a screen for those who follow;
//! - **drone:** flies over the track to the leader and comes down on it:
//!   the leader slows hard and spins;
//! - **bumper:** an air bag that knocks every kart within a few metres aside;
//! - **pit-crew push:** a shove that boosts the kart, and for a while
//!   nothing on the track touches it.
//!
//! The [`Paddock`] holds the crates, what each kart holds and the things on
//! the track, and steps with the physics. [`wants`] is when an AI driver
//! uses what it holds. Deterministic like the physics: no clock, no random
//! numbers but a hash of the race's seed, `libm` for every non-trivial
//! function.

use libm::{fabsf, sqrtf};
use osage_kart_physics::{BARRIER, DT, KART_RADIUS, KART_REACH, Kart, MAX_SPEED, Spot, Track, wrap};

/// What a kart can hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    Nitro,
    Tyre,
    Cone,
    Oil,
    Rocket,
    Smoke,
    Stack,
    Drone,
    Bumper,
    Push,
}

impl Item {
    pub const ALL: [Item; 10] = [
        Item::Nitro,
        Item::Tyre,
        Item::Cone,
        Item::Oil,
        Item::Rocket,
        Item::Smoke,
        Item::Stack,
        Item::Drone,
        Item::Bumper,
        Item::Push,
    ];

    /// Whether it is held behind the kart while the button is down, and
    /// used as it comes up.
    pub fn trails(self) -> bool {
        matches!(self, Item::Tyre | Item::Cone | Item::Oil | Item::Stack)
    }
}

/// A thing on the track.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Tyre,
    Cone,
    Oil,
    /// In the air, on its way to a kart.
    Drone,
}

impl Kind {
    /// Its radius on the ground, m.
    pub fn radius(self) -> f32 {
        match self {
            Kind::Tyre => TYRE_RADIUS,
            Kind::Cone => CONE_RADIUS,
            Kind::Oil => OIL_RADIUS,
            Kind::Drone => DRONE_RADIUS,
        }
    }
}

/// Where a kart stands in the race: its place (0 leads) and how far behind
/// the leader it is, metres of race.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Standing {
    pub place: usize,
    pub behind: f32,
}

/// Seconds of boost a nitro canister gives: longer than a long drift's
/// mini-turbo (1.2 s), to be felt as an item.
pub const NITRO: f32 = 2.5;
/// Seconds of rocket.
pub const ROCKET: f32 = 3.5;
/// Seconds a smoke canister pours.
pub const SMOKE: f32 = 6.0;
/// How much faster than the kart a tyre leaves it, m/s.
pub const THROW: f32 = 14.0;
/// How fast a rolling tyre slows, m/s².
pub const ROLLING: f32 = 2.0;
/// The share of its speed across a barrier a tyre keeps when it bounces.
pub const TYRE_BOUNCE: f32 = 0.6;
pub const TYRE_RADIUS: f32 = 0.35;
pub const CONE_RADIUS: f32 = 0.25;
pub const OIL_RADIUS: f32 = 1.5;
/// How long each thing stays on the track, s.
pub const TYRE_LIFE: f32 = 6.0;
pub const CONE_LIFE: f32 = 20.0;
pub const OIL_LIFE: f32 = 15.0;
/// Tyres in a stack.
pub const STACK: u8 = 3;
/// A drone: its speed, the height it flies at, its reach, how long it
/// flies before it gives up, and what it does: the share of speed kept and
/// the spin, rad/s.
pub const DRONE_SPEED: f32 = 40.0;
pub const DRONE_HEIGHT: f32 = 4.0;
pub const DRONE_RADIUS: f32 = 0.6;
pub const DRONE_LIFE: f32 = 10.0;
const DRONE_KEEP: f32 = 0.3;
const DRONE_SPIN: f32 = 6.0;
/// A bumper: how far from the kart it reaches, m, and how hard it knocks, m/s.
pub const BUMPER_REACH: f32 = 4.5;
pub const BUMPER_KNOCK: f32 = 7.0;
/// A pit-crew push: seconds of boost, and seconds nothing touches the kart.
pub const PUSH: f32 = 2.0;
pub const GUARD: f32 = 2.5;
/// How far behind the leader counts as far behind for a draw, m: as much
/// help as the last place.
pub const FAR_BEHIND: f32 = 120.0;
/// Seconds before a broken crate is back.
pub const CRATE_BACK: f32 = 1.5;
/// How close a kart's outline must come to a crate's centre to break it, m.
pub const CRATE_REACH: f32 = 0.5;
/// Where the racks stand, as shares of the lap.
pub const RACKS: [f32; 3] = [0.25, 0.55, 0.85];
/// Crates in a rack, spread across the road.
pub const RACK: usize = 5;
/// The most things on the track at once; the oldest goes first.
pub const MOST: usize = 32;
/// A tyre spares the kart that threw it for this long, s.
const SPARE: f32 = 0.5;
/// Oil and a cone spare the kart that left them for this long, s: it does
/// not slip on its own oil as it pulls away, or when it is knocked back
/// into it, but it can a lap later.
pub const OWN_SPARE: f32 = 6.0;
/// What a tyre does to the kart it hits: the share of speed kept, and the
/// spin it starts, rad/s.
const TYRE_KEEP: f32 = 0.45;
const TYRE_SPIN: f32 = 5.0;
/// A cone: the share of speed kept and the knock, m/s.
const CONE_KEEP: f32 = 0.85;
const CONE_KNOCK: f32 = 1.5;
/// Oil: the share of speed kept and the spin, rad/s.
const OIL_KEEP: f32 = 0.9;
const OIL_SPIN: f32 = 3.5;

/// A crate of a rack.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crate {
    pub x: f32,
    pub z: f32,
    /// Seconds before it is back; 0 when it stands.
    pub wait: f32,
}

impl Crate {
    pub fn stands(&self) -> bool {
        self.wait <= 0.0
    }
}

/// A thing on the track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thing {
    pub kind: Kind,
    pub x: f32,
    pub z: f32,
    /// Its velocity on the ground, m/s (a tyre, a drone).
    pub vx: f32,
    pub vz: f32,
    /// Height above the road, m (a drone).
    pub height: f32,
    /// The kart a drone flies to.
    pub target: usize,
    /// Seconds left on the track.
    pub life: f32,
    /// The kart that left it.
    pub owner: usize,
    /// Seconds it still spares its owner.
    spare: f32,
    /// The karts in it (oil), a bit each, so that it acts on entry only.
    inside: u64,
}

/// What happened during a step, for sounds and pictures.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// A kart broke a crate and got an item, or nothing with its hands full.
    Picked { kart: usize, item: Option<Item> },
    /// A kart used what it held.
    Used { kart: usize, item: Item },
    /// A thing hit a kart.
    Hit { kart: usize, kind: Kind },
    /// A kart put what it holds behind it.
    Trailing { kart: usize, item: Item },
    /// What a kart held behind it, or its push, stopped a thing.
    Blocked { kart: usize, kind: Kind },
    /// A bumper knocked a kart aside.
    Bumped { kart: usize, by: usize },
    /// A tyre bounced off a barrier.
    Bounced { x: f32, z: f32 },
}

/// The race's crates, what each kart holds, and the things on the track.
#[derive(Clone, Debug, PartialEq)]
pub struct Paddock {
    crates: Vec<Crate>,
    things: Vec<Thing>,
    held: Vec<Option<Item>>,
    /// How many of what it holds a kart has left (a stack: up to three).
    count: Vec<u8>,
    /// Whether a kart holds its item behind it.
    trailing: Vec<bool>,
    /// Seconds nothing on the track touches a kart (a push).
    guard: Vec<f32>,
    pressed: Vec<bool>,
    /// Each kart's smoke: seconds left and its colour (0xRRGGBB).
    smoke: Vec<(f32, u32)>,
    seed: u32,
    picks: u32,
}

impl Paddock {
    /// The racks of `track`, nothing held by its `karts` karts (at most 64),
    /// draws from `seed`.
    pub fn new(track: &Track, karts: usize, seed: u32) -> Self {
        assert!(karts <= 64, "at most 64 karts");
        let mut crates = Vec::with_capacity(RACKS.len() * RACK);
        for share in RACKS {
            let ([x, z], heading) = track.at(share * track.length());
            // Right of the heading.
            let right = (-libm::cosf(heading), libm::sinf(heading));
            for i in 0..RACK {
                let lateral = (i as f32 - (RACK as f32 - 1.0) / 2.0) * track.half_width * 2.0 / RACK as f32;
                crates.push(Crate { x: x + right.0 * lateral, z: z + right.1 * lateral, wait: 0.0 });
            }
        }
        Self {
            crates,
            things: Vec::new(),
            held: vec![None; karts],
            count: vec![0; karts],
            trailing: vec![false; karts],
            guard: vec![0.0; karts],
            pressed: vec![false; karts],
            smoke: vec![(0.0, 0); karts],
            seed,
            picks: 0,
        }
    }

    /// A paddock with no crates, for `karts` karts: a time trial's (D-062).
    pub fn empty(karts: usize) -> Self {
        assert!(karts <= 64, "at most 64 karts");
        Self {
            crates: Vec::new(),
            things: Vec::new(),
            held: vec![None; karts],
            count: vec![0; karts],
            trailing: vec![false; karts],
            guard: vec![0.0; karts],
            pressed: vec![false; karts],
            smoke: vec![(0.0, 0); karts],
            seed: 0,
            picks: 0,
        }
    }

    pub fn crates(&self) -> &[Crate] {
        &self.crates
    }

    pub fn things(&self) -> &[Thing] {
        &self.things
    }

    /// What kart `kart` holds.
    pub fn held(&self, kart: usize) -> Option<Item> {
        self.held.get(kart).copied().flatten()
    }

    /// The smoke kart `kart` pours: its colour (0xRRGGBB) while it lasts.
    pub fn smoke(&self, kart: usize) -> Option<u32> {
        self.smoke.get(kart).filter(|(left, _)| *left > 0.0).map(|(_, colour)| *colour)
    }

    /// How many of what it holds kart `kart` has left: three for a full
    /// stack, one for anything else, none with its hands free.
    pub fn count(&self, kart: usize) -> u8 {
        if self.held(kart).is_some() { self.count.get(kart).copied().unwrap_or(1).max(1) } else { 0 }
    }

    /// Whether kart `kart` holds its item behind it.
    pub fn trailing(&self, kart: usize) -> bool {
        self.trailing.get(kart).copied().unwrap_or(false) && self.held(kart).is_some()
    }

    /// Seconds nothing on the track touches kart `kart` for (a push).
    pub fn guard(&self, kart: usize) -> f32 {
        self.guard.get(kart).copied().unwrap_or(0.0)
    }

    /// Gives kart `kart` an item, or takes its own away.
    pub fn hold(&mut self, kart: usize, item: Option<Item>) {
        self.resize(kart + 1);
        self.held[kart] = item;
        self.count[kart] = if item == Some(Item::Stack) { STACK } else { u8::from(item.is_some()) };
        self.trailing[kart] = false;
    }

    fn resize(&mut self, karts: usize) {
        let karts = karts.max(self.held.len());
        self.held.resize(karts, None);
        self.count.resize(karts, 0);
        self.trailing.resize(karts, false);
        self.guard.resize(karts, 0.0);
        self.pressed.resize(karts, false);
        self.smoke.resize(karts, (0.0, 0));
    }

    /// The use button of an AI driver at this step: down to put a tyre,
    /// cone or oil behind its kart when another is close behind, up to use
    /// it ([`wants`]); down to use anything else.
    pub fn button(&self, me: usize, field: &[Spot], standings: &[Standing], kart: &Kart, track: &Track) -> bool {
        let Some(item) = self.held(me) else { return false };
        let wanted = wants(item, me, field, standings, kart, track);
        if !item.trails() {
            return wanted;
        }
        if self.trailing(me) { !wanted } else { wanted || followed(me, field, track) }
    }

    /// One step of [`DT`]: crates come back, karts break crates, karts use
    /// what they hold as their use button (`uses`, held or not) goes down —
    /// or, for what trails, put it behind them and use it as it comes up —
    /// things move and hit karts. `standings` is each kart's place and gap.
    pub fn step(&mut self, karts: &mut [Kart], uses: &[bool], standings: &[Standing], track: &Track) -> Vec<Event> {
        let mut events = Vec::new();
        self.resize(karts.len());
        for (left, _) in &mut self.smoke {
            *left = (*left - DT).max(0.0);
        }
        for left in &mut self.guard {
            *left = (*left - DT).max(0.0);
        }
        for c in &mut self.crates {
            c.wait = (c.wait - DT).max(0.0);
        }
        // Crates.
        for (k, kart) in karts.iter().enumerate() {
            for i in 0..self.crates.len() {
                let c = self.crates[i];
                if c.stands() && touches(kart, c.x, c.z, CRATE_REACH) {
                    self.crates[i].wait = CRATE_BACK;
                    let item = if self.held[k].is_none() {
                        let standing = standings.get(k).copied().unwrap_or_default();
                        let item = self.draw(standing, karts.len());
                        self.hold(k, Some(item));
                        Some(item)
                    } else {
                        None
                    };
                    events.push(Event::Picked { kart: k, item });
                }
            }
        }
        // Uses.
        for k in 0..karts.len() {
            let down = uses.get(k).copied().unwrap_or(false);
            let was = std::mem::replace(&mut self.pressed[k], down);
            let Some(item) = self.held[k] else { continue };
            let using = if item.trails() {
                if down && !was {
                    self.trailing[k] = true;
                    events.push(Event::Trailing { kart: k, item });
                }
                !down && was && self.trailing[k]
            } else {
                down && !was
            };
            if !using {
                continue;
            }
            self.trailing[k] = false;
            self.count[k] = self.count[k].saturating_sub(1);
            if self.count[k] == 0 {
                self.held[k] = None;
            }
            self.release(k, karts, item, standings, &mut events);
            events.push(Event::Used { kart: k, item });
        }
        // Things.
        let face = track.half_width + track.verge + BARRIER;
        let mut i = 0;
        while i < self.things.len() {
            let thing = &mut self.things[i];
            thing.life -= DT;
            thing.spare = (thing.spare - DT).max(0.0);
            if thing.kind == Kind::Tyre {
                roll(thing, face, track, &mut events);
            }
            let mut gone = thing.life <= 0.0;
            if thing.kind == Kind::Drone {
                if let Some(target) = karts.get_mut(thing.target).filter(|_| !gone) {
                    if fly(thing, target) {
                        if self.guard[thing.target] > 0.0 {
                            events.push(Event::Blocked { kart: thing.target, kind: Kind::Drone });
                        } else {
                            dropped_on(target);
                            events.push(Event::Hit { kart: thing.target, kind: Kind::Drone });
                        }
                        gone = true;
                    }
                } else {
                    gone = true;
                }
                if gone {
                    self.things.remove(i);
                } else {
                    i += 1;
                }
                continue;
            }
            for (k, kart) in karts.iter_mut().enumerate() {
                if gone {
                    break;
                }
                let bit = 1u64 << k;
                let touching = touches(kart, thing.x, thing.z, thing.kind.radius());
                let spared = k == thing.owner && thing.spare > 0.0;
                // A push: nothing touches the kart; what it drives into goes.
                if self.guard[k] > 0.0 {
                    if touching && thing.kind != Kind::Oil && !spared {
                        events.push(Event::Blocked { kart: k, kind: thing.kind });
                        gone = true;
                    }
                    continue;
                }
                if thing.kind == Kind::Oil {
                    if spared {
                        // In it when the time is up counts as in it already.
                        thing.inside = if touching { thing.inside | bit } else { thing.inside & !bit };
                        continue;
                    }
                    let entering = touching && thing.inside & bit == 0;
                    thing.inside = if touching { thing.inside | bit } else { thing.inside & !bit };
                    if entering {
                        slip(kart, k as u32 ^ self.seed);
                        events.push(Event::Hit { kart: k, kind: Kind::Oil });
                    }
                    continue;
                }
                if !touching || spared {
                    continue;
                }
                // A tyre that meets what a kart holds behind it stops there;
                // the shield goes (one tyre of a stack).
                let from_behind = {
                    let forward = (libm::sinf(kart.heading), libm::cosf(kart.heading));
                    (thing.x - kart.x) * forward.0 + (thing.z - kart.z) * forward.1 < 0.0
                };
                if thing.kind == Kind::Tyre && self.trailing[k] && self.held[k].is_some() && from_behind {
                    self.count[k] = self.count[k].saturating_sub(1);
                    if self.count[k] == 0 {
                        self.held[k] = None;
                        self.trailing[k] = false;
                    }
                    events.push(Event::Blocked { kart: k, kind: thing.kind });
                    gone = true;
                    continue;
                }
                match thing.kind {
                    Kind::Tyre => spun(kart, (thing.vx, thing.vz), (thing.x, thing.z)),
                    _ => knocked(kart, (thing.x, thing.z)),
                }
                // Spun, a kart drops what it held behind it.
                if thing.kind == Kind::Tyre && self.trailing[k] {
                    self.held[k] = None;
                    self.count[k] = 0;
                    self.trailing[k] = false;
                }
                events.push(Event::Hit { kart: k, kind: thing.kind });
                gone = true;
            }
            if gone {
                self.things.remove(i);
            } else {
                i += 1;
            }
        }
        events
    }

    /// Draws an item for a kart at `standing` among `karts`: how far back it
    /// is — by place or by its gap to the leader, whichever is more — makes
    /// a nitro, a rocket, a drone and a push likelier and a cone less likely
    /// (D-052). The leader never draws a rocket, a drone or a push.
    fn draw(&mut self, standing: Standing, karts: usize) -> Item {
        let rank = if karts > 1 { (standing.place.min(karts - 1)) as f32 / (karts - 1) as f32 } else { 0.0 };
        let far = (standing.behind / FAR_BEHIND).clamp(0.0, 1.0);
        let back = if standing.place == 0 { 0.0 } else { rank.max(far) };
        let weights = [
            1.0 + 4.0 * back,          // nitro
            3.0 - back,                // tyre
            3.0 - 2.0 * back,          // cone
            2.0 - back,                // oil
            3.0 * back * back,         // rocket
            1.5,                       // smoke
            6.0 * back * (1.0 - back), // stack: the middle of the field
            2.5 * back * back,         // drone
            1.5,                       // bumper
            3.0 * back * back * back,  // push: the very back
        ];
        let total: f32 = weights.iter().sum();
        let mut u = unit(self.seed ^ self.picks.wrapping_mul(0x9e37_79b9)) * total;
        self.picks = self.picks.wrapping_add(1);
        for (item, weight) in Item::ALL.into_iter().zip(weights) {
            if u < weight {
                return item;
            }
            u -= weight;
        }
        Item::Oil
    }

    /// What kart `k` does with `item`.
    fn release(&mut self, k: usize, karts: &mut [Kart], item: Item, standings: &[Standing], events: &mut Vec<Event>) {
        if item == Item::Bumper {
            let me = karts[k];
            for (other, kart) in karts.iter_mut().enumerate() {
                let (dx, dz) = (kart.x - me.x, kart.z - me.z);
                let d = sqrtf(dx * dx + dz * dz);
                if other == k || d > BUMPER_REACH || self.guard[other] > 0.0 {
                    continue;
                }
                let away = if d > 1e-3 { (dx / d, dz / d) } else { (1.0, 0.0) };
                // Harder the nearer; off its centre, so that it turns.
                let knock = BUMPER_KNOCK * (1.0 - 0.5 * d / BUMPER_REACH);
                kart.take(
                    (away.0 * knock, away.1 * knock),
                    (-away.0 * 0.6 - away.1 * 0.3, -away.1 * 0.6 + away.0 * 0.3),
                );
                kart.drift = 0.0;
                kart.charge = 0.0;
                events.push(Event::Bumped { kart: other, by: k });
            }
            return;
        }
        let kart = &mut karts[k];
        let forward = (libm::sinf(kart.heading), libm::cosf(kart.heading));
        let thing = |kind: Kind, along: f32, v: (f32, f32), life: f32| Thing {
            kind,
            x: kart.x + forward.0 * along,
            z: kart.z + forward.1 * along,
            vx: v.0,
            vz: v.1,
            height: 0.0,
            target: k,
            life,
            owner: k,
            spare: if kind == Kind::Tyre { SPARE } else { OWN_SPARE },
            inside: 0,
        };
        let behind = -(KART_REACH + KART_RADIUS + 0.3);
        let thing = match item {
            Item::Nitro => {
                kart.boost = kart.boost.max(NITRO);
                return;
            }
            Item::Rocket => {
                kart.rocket = kart.rocket.max(ROCKET);
                return;
            }
            Item::Smoke => {
                // A bright colour from the draw's hash: a hue, full saturation.
                let hue = unit(self.seed ^ self.picks.wrapping_mul(0x85eb_ca6b) ^ k as u32) * 6.0;
                self.smoke[k] = (SMOKE, bright(hue));
                return;
            }
            Item::Push => {
                kart.boost = kart.boost.max(PUSH);
                self.guard[k] = GUARD;
                return;
            }
            Item::Bumper => return,
            Item::Drone => {
                // To the leader; the leader's own drone goes nowhere.
                let Some(leader) = (0..standings.len()).find(|&j| standings[j].place == 0 && j != k) else {
                    return;
                };
                let mut drone = thing(Kind::Drone, 0.0, (0.0, 0.0), DRONE_LIFE);
                drone.height = 1.0;
                drone.target = leader;
                drone
            }
            Item::Tyre | Item::Stack => {
                let v = kart.velocity();
                let ahead = KART_REACH + KART_RADIUS + TYRE_RADIUS + 0.1;
                let speed = (v.0 * forward.0 + v.1 * forward.1).max(0.0) + THROW;
                thing(Kind::Tyre, ahead, (forward.0 * speed, forward.1 * speed), TYRE_LIFE)
            }
            Item::Cone => thing(Kind::Cone, behind - CONE_RADIUS, (0.0, 0.0), CONE_LIFE),
            Item::Oil => thing(Kind::Oil, behind - OIL_RADIUS, (0.0, 0.0), OIL_LIFE),
        };
        if self.things.len() >= MOST {
            self.things.remove(0);
        }
        self.things.push(thing);
    }
}

/// A rolling tyre: it slows, and bounces off the barriers.
fn roll(tyre: &mut Thing, face: f32, track: &Track, events: &mut Vec<Event>) {
    let speed = sqrtf(tyre.vx * tyre.vx + tyre.vz * tyre.vz);
    if speed > 0.0 {
        let kept = (speed - ROLLING * DT).max(0.0) / speed;
        (tyre.vx, tyre.vz) = (tyre.vx * kept, tyre.vz * kept);
    }
    tyre.x += tyre.vx * DT;
    tyre.z += tyre.vz * DT;
    let spot = track.locate(tyre.x, tyre.z);
    let depth = fabsf(spot.lateral) + TYRE_RADIUS - face;
    if depth <= 0.0 {
        return;
    }
    let (_, road) = track.at(spot.along);
    let side = if spot.lateral >= 0.0 { 1.0 } else { -1.0 };
    // Towards the road, across the barrier.
    let normal = (side * libm::cosf(road), -side * libm::sinf(road));
    tyre.x += normal.0 * depth;
    tyre.z += normal.1 * depth;
    let closing = tyre.vx * normal.0 + tyre.vz * normal.1;
    if closing < 0.0 {
        let change = -(1.0 + TYRE_BOUNCE) * closing;
        (tyre.vx, tyre.vz) = (tyre.vx + normal.0 * change, tyre.vz + normal.1 * change);
        events.push(Event::Bounced { x: tyre.x, z: tyre.z });
    }
}

/// A drone flies one step towards `target`: straight over whatever is in the
/// way, at its height until it is near, then down onto it. Whether it got there.
fn fly(drone: &mut Thing, target: &Kart) -> bool {
    let (dx, dz) = (target.x - drone.x, target.z - drone.z);
    let d = sqrtf(dx * dx + dz * dz);
    let step = DRONE_SPEED * DT;
    if d <= step + DRONE_RADIUS + KART_RADIUS && drone.height <= 1.2 {
        return true;
    }
    let ahead = step.min(d);
    (drone.vx, drone.vz) = (dx / d.max(1e-4) * DRONE_SPEED, dz / d.max(1e-4) * DRONE_SPEED);
    drone.x += dx / d.max(1e-4) * ahead;
    drone.z += dz / d.max(1e-4) * ahead;
    // Up to its height after the launch, down over the last 12 m.
    let wanted = if d < 12.0 { 0.8 + (DRONE_HEIGHT - 0.8) * d / 12.0 } else { DRONE_HEIGHT };
    let climb = 6.0 * DT;
    drone.height = if wanted > drone.height { (drone.height + climb).min(wanted) } else { wanted };
    false
}

/// A drone comes down on a kart: it loses most of its speed and spins.
fn dropped_on(kart: &mut Kart) {
    kart.speed *= DRONE_KEEP;
    kart.slide *= DRONE_KEEP;
    kart.spin += DRONE_SPIN;
    kart.drift = 0.0;
    kart.charge = 0.0;
    kart.boost = 0.0;
    kart.rocket = 0.0;
}

/// Whether another kart is close behind kart `me`, on its line: what a tyre,
/// cone or oil held behind guards against.
fn followed(me: usize, field: &[Spot], track: &Track) -> bool {
    let Some(&mine) = field.get(me) else { return false };
    let length = track.length();
    field.iter().enumerate().any(|(k, s)| {
        let along = (s.along - mine.along + length / 2.0).rem_euclid(length) - length / 2.0;
        k != me && (-15.0..-1.0).contains(&along) && fabsf(s.lateral - mine.lateral) < 3.0
    })
}

/// The point of a kart's spine closest to `(x, z)`, and whether its outline
/// comes within `reach` of it.
pub fn touches(kart: &Kart, x: f32, z: f32, reach: f32) -> bool {
    let (dx, dz) = gap(kart, x, z);
    let within = KART_RADIUS + reach;
    dx * dx + dz * dz < within * within
}

fn gap(kart: &Kart, x: f32, z: f32) -> (f32, f32) {
    let forward = (libm::sinf(kart.heading), libm::cosf(kart.heading));
    let s = ((x - kart.x) * forward.0 + (z - kart.z) * forward.1).clamp(-KART_REACH, KART_REACH);
    (x - kart.x - forward.0 * s, z - kart.z - forward.1 * s)
}

/// A tyre moving at `v` hits a kart at `at`: it loses most of its speed and
/// spins, the way the tyre struck it.
fn spun(kart: &mut Kart, v: (f32, f32), at: (f32, f32)) {
    let r = (at.0 - kart.x, at.1 - kart.z);
    // Which way the blow turns the kart (as `swept` in the physics).
    let turn = r.1 * (v.0 - kart.velocity().0) - r.0 * (v.1 - kart.velocity().1);
    kart.speed *= TYRE_KEEP;
    kart.slide *= TYRE_KEEP;
    kart.spin += if turn >= 0.0 { TYRE_SPIN } else { -TYRE_SPIN };
    kart.drift = 0.0;
    kart.charge = 0.0;
    kart.boost = 0.0;
    kart.rocket = 0.0;
}

/// A kart hits a cone at `at`: slowed, knocked away from it.
fn knocked(kart: &mut Kart, at: (f32, f32)) {
    kart.speed *= CONE_KEEP;
    let (dx, dz) = gap(kart, at.0, at.1);
    let d = sqrtf(dx * dx + dz * dz).max(1e-4);
    let r = (at.0 - kart.x, at.1 - kart.z);
    kart.take((-dx / d * CONE_KNOCK, -dz / d * CONE_KNOCK), r);
}

/// A kart drives into oil: it slows a little and spins, the way it was
/// sliding (or one picked by `seed`).
fn slip(kart: &mut Kart, seed: u32) {
    kart.speed *= OIL_KEEP;
    let way = if kart.slide != 0.0 {
        -kart.slide.signum()
    } else if hash(seed) & 1 == 0 {
        1.0
    } else {
        -1.0
    };
    kart.spin += way * OIL_SPIN;
    kart.drift = 0.0;
    kart.charge = 0.0;
}

/// Whether an AI driver uses `item` now: a nitro, rocket or push on a
/// straight, a tyre at a kart ahead on a straight, a cone or oil with a kart
/// close behind, a bumper with a kart beside, a drone when it does not lead.
/// `field` is every kart located on the track, `me` its own; `standings`
/// their places.
pub fn wants(item: Item, me: usize, field: &[Spot], standings: &[Standing], kart: &Kart, track: &Track) -> bool {
    let Some(&mine) = field.get(me) else { return false };
    let straight = |ahead: f32| {
        let (_, here) = track.at(mine.along);
        let (_, there) = track.at(mine.along + ahead);
        fabsf(wrap(there - here)) < 0.25
    };
    let length = track.length();
    let gaps = field.iter().enumerate().filter(|&(k, _)| k != me).map(|(_, s)| {
        let along = (s.along - mine.along + length / 2.0).rem_euclid(length) - length / 2.0;
        (along, fabsf(s.lateral - mine.lateral))
    });
    match item {
        Item::Nitro => kart.speed > 0.6 * MAX_SPEED && straight(30.0),
        Item::Rocket => kart.speed > 0.5 * MAX_SPEED && straight(40.0),
        Item::Tyre | Item::Stack => {
            straight(25.0) && gaps.clone().any(|(along, across)| (4.0..30.0).contains(&along) && across < 1.5)
        }
        Item::Push => kart.speed > 0.4 * MAX_SPEED && straight(30.0),
        Item::Drone => standings.get(me).is_some_and(|s| s.place > 0),
        Item::Bumper => gaps.clone().any(|(along, across)| fabsf(along) < 3.5 && across < 3.0),
        Item::Smoke => gaps.clone().any(|(along, across)| (-20.0..-3.0).contains(&along) && across < 3.0),
        Item::Cone | Item::Oil => gaps.clone().any(|(along, across)| (-12.0..-2.0).contains(&along) && across < 2.0),
    }
}

/// An integer hash with good mixing (a "lowbias32" finaliser).
/// A bright colour, 0xRRGGBB, from a hue in [0, 6).
fn bright(hue: f32) -> u32 {
    let x = 1.0 - fabsf(hue % 2.0 - 1.0);
    let (r, g, b) = match hue as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    let byte = |v: f32| ((0.25 + 0.75 * v) * 255.0) as u32;
    byte(r) << 16 | byte(g) << 8 | byte(b)
}

fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

/// A hash as a number in 0…1.
fn unit(x: u32) -> f32 {
    (hash(x) >> 8) as f32 / (1u32 << 24) as f32
}

#[cfg(test)]
mod tests {
    use osage_kart_physics::{Controls, step};

    use super::*;

    /// A 200 m × 100 m rectangle; the start heads +Z, the road's right is −X.
    fn track() -> Track {
        Track::new(vec![[0.0, 0.0], [0.0, 200.0], [-100.0, 200.0], [-100.0, 0.0]], 6.0, 3.0)
    }

    /// Standings by place alone, no gaps.
    fn at_places(places: &[usize]) -> Vec<Standing> {
        places.iter().map(|&place| Standing { place, behind: 0.0 }).collect()
    }

    fn kart(z: f32, speed: f32) -> Kart {
        Kart { z, speed, ..Kart::default() }
    }

    fn run(paddock: &mut Paddock, karts: &mut [Kart], uses: &[bool], track: &Track, steps: usize) -> Vec<Event> {
        let places: Vec<usize> = (0..karts.len()).collect();
        let standings = at_places(&places);
        let mut events = Vec::new();
        for _ in 0..steps {
            events.extend(paddock.step(karts, uses, &standings, track));
            for kart in karts.iter_mut() {
                step(kart, Controls { throttle: 1.0, ..Controls::default() }, track);
            }
        }
        events
    }

    #[test]
    fn a_crate_gives_an_item_and_comes_back() {
        let track = track();
        let mut paddock = Paddock::new(&track, 1, 7);
        assert_eq!(paddock.crates().len(), RACKS.len() * RACK);
        let c = paddock.crates()[0];
        let mut karts = [Kart { x: c.x, z: c.z - 3.0, heading: 0.0, speed: 15.0, ..Kart::default() }];
        let events = run(&mut paddock, &mut karts, &[false], &track, 30);
        let item = paddock.held(0).expect("an item");
        assert!(events.contains(&Event::Picked { kart: 0, item: Some(item) }), "{events:?}");
        assert!(!paddock.crates()[0].stands());
        // Its hands are full: a second crate gives nothing.
        let mut again = paddock.clone();
        again.crates[0].wait = 0.0;
        let mut back = [Kart { x: c.x, z: c.z, ..Kart::default() }];
        let events = again.step(&mut back, &[false], &at_places(&[0]), &track);
        assert_eq!(events, vec![Event::Picked { kart: 0, item: None }]);
        run(&mut paddock, &mut [Kart::default()], &[false], &track, (CRATE_BACK / DT) as usize + 1);
        assert!(paddock.crates()[0].stands());
    }

    #[test]
    fn the_back_of_the_field_draws_more_nitros_than_the_leader() {
        let track = track();
        let count = |place: usize| {
            let mut paddock = Paddock::new(&track, 8, 3);
            (0..2000).filter(|_| paddock.draw(Standing { place, behind: 0.0 }, 8) == Item::Nitro).count()
        };
        let (first, last) = (count(0), count(7));
        assert!(last > 2 * first, "leader {first}, last {last}");
        // The same seed draws the same items.
        let mut a = Paddock::new(&track, 8, 3);
        let mut b = Paddock::new(&track, 8, 3);
        assert!(
            (0..50).all(|i| a.draw(Standing { place: i % 8, behind: 0.0 }, 8)
                == b.draw(Standing { place: i % 8, behind: 0.0 }, 8))
        );
    }

    #[test]
    fn a_nitro_boosts_once_per_press() {
        let track = track();
        let mut paddock = Paddock::new(&track, 1, 1);
        paddock.hold(0, Some(Item::Nitro));
        let mut karts = [kart(20.0, 10.0)];
        let events = run(&mut paddock, &mut karts, &[true], &track, 1);
        assert_eq!(events, vec![Event::Used { kart: 0, item: Item::Nitro }]);
        assert!(karts[0].boost > 1.0 && paddock.held(0).is_none());
        // Holding the button down does not use the next item.
        paddock.hold(0, Some(Item::Cone));
        run(&mut paddock, &mut karts, &[true], &track, 5);
        assert_eq!(paddock.held(0), Some(Item::Cone));
    }

    #[test]
    fn a_thrown_tyre_rolls_ahead_and_spins_the_kart_it_meets_not_its_thrower() {
        let track = track();
        let mut paddock = Paddock::new(&track, 2, 1);
        paddock.hold(0, Some(Item::Tyre));
        let mut karts = [kart(20.0, MAX_SPEED), Kart { z: 40.0, x: 0.3, speed: MAX_SPEED, ..Kart::default() }];
        let mut events = paddock.step(&mut karts, &[true, false], &at_places(&[1, 0]), &track);
        paddock.step(&mut karts, &[false, false], &at_places(&[1, 0]), &track);
        let tyre = paddock.things()[0];
        assert!(tyre.kind == Kind::Tyre && tyre.vz > MAX_SPEED + THROW - 0.5 && tyre.z > karts[0].z, "{tyre:?}");
        events.extend(run(&mut paddock, &mut karts, &[false, false], &track, 180));
        assert!(events.contains(&Event::Hit { kart: 1, kind: Kind::Tyre }), "{events:?}");
        assert!(!events.contains(&Event::Hit { kart: 0, kind: Kind::Tyre }));
        assert!(paddock.things().is_empty());
    }

    #[test]
    fn a_tyre_bounces_off_the_barriers_and_stays_on_the_track() {
        let track = track();
        let mut paddock = Paddock::new(&track, 1, 1);
        paddock.hold(0, Some(Item::Tyre));
        // Thrown towards the left barrier (+X).
        let mut karts = [Kart { z: 20.0, heading: 0.7, ..Kart::default() }];
        let mut events = paddock.step(&mut karts, &[true], &at_places(&[0]), &track);
        paddock.step(&mut karts, &[false], &at_places(&[0]), &track);
        karts[0].z = -50.0; // out of the way
        for _ in 0..200 {
            events.extend(paddock.step(&mut karts, &[false], &at_places(&[0]), &track));
        }
        assert!(events.iter().any(|e| matches!(e, Event::Bounced { .. })), "{events:?}");
        let face = track.half_width + track.verge + BARRIER;
        let tyre = paddock.things()[0];
        assert!(fabsf(track.locate(tyre.x, tyre.z).lateral) <= face, "{tyre:?}");
        assert!(tyre.vx < 0.0, "it comes back off the barrier: {tyre:?}");
    }

    #[test]
    fn a_cone_slows_and_knocks_the_kart_behind() {
        let track = track();
        let mut paddock = Paddock::new(&track, 2, 1);
        paddock.hold(0, Some(Item::Cone));
        let mut karts = [kart(30.0, 0.0), kart(20.0, 0.0)];
        paddock.step(&mut karts, &[true, false], &at_places(&[0, 1]), &track);
        paddock.step(&mut karts, &[false, false], &at_places(&[0, 1]), &track);
        assert_eq!(paddock.things()[0].kind, Kind::Cone);
        karts[0].z = 100.0;
        let cone = paddock.things()[0];
        karts[1] = Kart { x: cone.x + 0.2, z: cone.z - 0.9, speed: 15.0, ..Kart::default() };
        let events = paddock.step(&mut karts, &[false, false], &at_places(&[0, 1]), &track);
        assert_eq!(events, vec![Event::Hit { kart: 1, kind: Kind::Cone }]);
        assert!(karts[1].speed < 15.0 * CONE_KEEP + 0.5 && karts[1].spin != 0.0, "{:?}", karts[1]);
    }

    #[test]
    fn oil_spins_a_kart_once_as_it_drives_in_and_spares_its_owner() {
        let track = track();
        let mut paddock = Paddock::new(&track, 2, 1);
        paddock.hold(0, Some(Item::Oil));
        let mut karts = [kart(30.0, 0.0), kart(18.0, 0.0)];
        let mut events = paddock.step(&mut karts, &[true, false], &at_places(&[0, 1]), &track);
        paddock.step(&mut karts, &[false, false], &at_places(&[0, 1]), &track);
        // Both sit still a while; the owner was in it from the start.
        for _ in 0..30 {
            events.extend(paddock.step(&mut karts, &[false, false], &at_places(&[0, 1]), &track));
        }
        assert!(!events.iter().any(|e| matches!(e, Event::Hit { .. })), "{events:?}");
        karts[1].speed = 10.0;
        let events = run(&mut paddock, &mut karts, &[false, false], &track, 60);
        let hits = events.iter().filter(|e| matches!(e, Event::Hit { kart: 1, kind: Kind::Oil })).count();
        assert_eq!(hits, 1, "{events:?}");
        assert_eq!(paddock.things().len(), 1, "oil stays");
    }

    #[test]
    fn a_kart_knocked_back_into_its_own_oil_does_not_slip() {
        let track = track();
        let mut paddock = Paddock::new(&track, 1, 1);
        paddock.hold(0, Some(Item::Oil));
        let mut karts = [kart(30.0, 0.0)];
        paddock.step(&mut karts, &[true], &at_places(&[0]), &track);
        paddock.step(&mut karts, &[false], &at_places(&[0]), &track);
        let oil = paddock.things()[0];
        // Thrown back across it, a second later.
        karts[0] = Kart { x: oil.x, z: oil.z + 3.0, speed: -8.0, ..Kart::default() };
        let events = run(&mut paddock, &mut karts, &[false], &track, 60);
        assert!(!events.iter().any(|e| matches!(e, Event::Hit { .. })), "{events:?}");
    }

    #[test]
    fn a_smoke_canister_pours_a_colour_for_a_while() {
        let track = track();
        let mut paddock = Paddock::new(&track, 1, 7);
        paddock.hold(0, Some(Item::Smoke));
        let mut karts = [kart(30.0, 10.0)];
        assert_eq!(paddock.smoke(0), None);
        paddock.step(&mut karts, &[true], &at_places(&[0]), &track);
        paddock.step(&mut karts, &[false], &at_places(&[0]), &track);
        let colour = paddock.smoke(0).expect("smoking");
        assert!(colour > 0 && colour <= 0xff_ffff);
        for _ in 0..(SMOKE / DT) as usize + 1 {
            paddock.step(&mut karts, &[false], &at_places(&[0]), &track);
        }
        assert_eq!(paddock.smoke(0), None);
    }

    #[test]
    fn things_go_after_their_time() {
        let track = track();
        let mut paddock = Paddock::new(&track, 1, 1);
        paddock.hold(0, Some(Item::Cone));
        let mut karts = [kart(30.0, 0.0)];
        paddock.step(&mut karts, &[true], &at_places(&[0]), &track);
        paddock.step(&mut karts, &[false], &at_places(&[0]), &track);
        karts[0].z = 120.0;
        for _ in 0..(CONE_LIFE / DT) as usize + 1 {
            paddock.step(&mut karts, &[false], &at_places(&[0]), &track);
        }
        assert!(paddock.things().is_empty());
    }

    #[test]
    fn an_ai_driver_throws_at_a_kart_ahead_and_drops_for_one_behind() {
        let track = track();
        let at = |z: f32| track.locate(0.0, z);
        let me = kart(50.0, 18.0);
        assert!(wants(Item::Tyre, 0, &[at(50.0), at(65.0)], &[], &me, &track));
        assert!(!wants(Item::Tyre, 0, &[at(50.0), at(40.0)], &[], &me, &track));
        assert!(wants(Item::Oil, 0, &[at(50.0), at(44.0)], &[], &me, &track));
        assert!(!wants(Item::Cone, 0, &[at(50.0), at(65.0)], &[], &me, &track));
        assert!(wants(Item::Nitro, 0, &[at(50.0)], &[], &me, &track));
        // Not into the end of the straight.
        assert!(!wants(Item::Nitro, 0, &[at(185.0)], &[], &kart(185.0, 18.0), &track));
    }

    /// How strong a draw is, roughly: what it does to the race for the one
    /// who uses it.
    fn strength(item: Item) -> f32 {
        match item {
            Item::Cone | Item::Oil | Item::Smoke | Item::Bumper => 1.0,
            Item::Tyre => 1.5,
            Item::Nitro => 2.0,
            Item::Stack => 3.0,
            Item::Rocket | Item::Drone => 4.0,
            Item::Push => 5.0,
        }
    }

    #[test]
    fn draws_follow_the_place_and_the_gap_to_the_leader() {
        let track = track();
        let draws = |standing: Standing| {
            let mut paddock = Paddock::new(&track, 8, 11);
            (0..4000).map(|_| paddock.draw(standing, 8)).collect::<Vec<_>>()
        };
        let mean = |items: &[Item]| items.iter().map(|&i| strength(i)).sum::<f32>() / items.len() as f32;
        let by_place: Vec<Vec<Item>> = (0..8).map(|place| draws(Standing { place, behind: 0.0 })).collect();
        // The leader gets defensive things: never a rocket, a drone or a push.
        let leader = &by_place[0];
        assert!(!leader.iter().any(|i| matches!(i, Item::Rocket | Item::Drone | Item::Push)), "the leader drew one");
        // Further back, stronger: the mean strength grows with the place.
        let means: Vec<f32> = by_place.iter().map(|d| mean(d)).collect();
        assert!(means.windows(2).all(|w| w[1] > w[0] - 0.02), "{means:?}");
        assert!(means[7] > means[0] + 1.0, "{means:?}");
        // Far behind counts as much as last: third place 150 m behind draws like last.
        let far = mean(&draws(Standing { place: 2, behind: 150.0 }));
        assert!(far > means[2] + 0.5 && (far - means[7]).abs() < 0.2, "far {far}, {means:?}");
        // Every item comes out somewhere in the field.
        for item in Item::ALL {
            assert!(by_place.iter().flatten().any(|&i| i == item), "{item:?} never drawn");
        }
    }

    #[test]
    fn a_tyre_held_behind_stops_a_tyre_from_behind_and_is_gone() {
        let track = track();
        let mut paddock = Paddock::new(&track, 2, 1);
        paddock.hold(0, Some(Item::Tyre));
        paddock.hold(1, Some(Item::Tyre));
        // Kart 0 ahead, holding its tyre behind; kart 1 throws at it.
        let mut karts = [kart(60.0, 12.0), kart(40.0, 12.0)];
        let mut events = paddock.step(&mut karts, &[true, true], &at_places(&[0, 1]), &track);
        assert!(events.contains(&Event::Trailing { kart: 0, item: Item::Tyre }));
        for _ in 0..90 {
            // The karts stand: the tyre comes up from behind.
            events.extend(paddock.step(&mut karts, &[true, false], &at_places(&[0, 1]), &track));
        }
        assert!(events.contains(&Event::Blocked { kart: 0, kind: Kind::Tyre }), "{events:?}");
        assert!(!events.contains(&Event::Hit { kart: 0, kind: Kind::Tyre }));
        assert_eq!(karts[0].spin, 0.0, "not spun: {:?}", karts[0]);
        assert_eq!(paddock.held(0), None, "the shield went");
        // Without a shield, the same tyre spins it.
        let mut paddock = Paddock::new(&track, 2, 1);
        paddock.hold(1, Some(Item::Tyre));
        let mut karts = [kart(60.0, 12.0), kart(40.0, 12.0)];
        let mut events = paddock.step(&mut karts, &[false, true], &at_places(&[0, 1]), &track);
        for _ in 0..90 {
            events.extend(paddock.step(&mut karts, &[false, false], &at_places(&[0, 1]), &track));
        }
        assert!(events.contains(&Event::Hit { kart: 0, kind: Kind::Tyre }), "{events:?}");
    }

    #[test]
    fn a_stack_throws_three_tyres_one_press_each() {
        let track = track();
        let mut paddock = Paddock::new(&track, 1, 1);
        paddock.hold(0, Some(Item::Stack));
        assert_eq!(paddock.count(0), STACK);
        let mut karts = [kart(20.0, 10.0)];
        let mut thrown = 0;
        for press in 0..3 {
            assert_eq!(paddock.count(0), STACK - press);
            paddock.step(&mut karts, &[true], &at_places(&[0]), &track);
            let events = paddock.step(&mut karts, &[false], &at_places(&[0]), &track);
            thrown += events.iter().filter(|e| matches!(e, Event::Used { item: Item::Stack, .. })).count();
        }
        assert_eq!(thrown, 3);
        assert_eq!(paddock.held(0), None);
        assert_eq!(paddock.things().iter().filter(|t| t.kind == Kind::Tyre).count(), 3);
    }

    #[test]
    fn a_drone_flies_to_the_leader_and_comes_down_on_it() {
        let track = track();
        let mut paddock = Paddock::new(&track, 3, 1);
        paddock.hold(2, Some(Item::Drone));
        // The leader is 150 m ahead, round a corner; the second is in between.
        let mut karts = [kart(170.0, 15.0), kart(100.0, 15.0), kart(20.0, 15.0)];
        let standings = at_places(&[0, 1, 2]);
        let mut events = paddock.step(&mut karts, &[false, false, true], &standings, &track);
        let mut highest: f32 = 0.0;
        for _ in 0..(DRONE_LIFE / DT) as usize {
            events.extend(paddock.step(&mut karts, &[false, false, false], &standings, &track));
            highest = paddock.things().iter().fold(highest, |h, t| h.max(t.height));
            if events.iter().any(|e| matches!(e, Event::Hit { .. })) {
                break;
            }
        }
        assert_eq!(
            events.iter().filter(|e| matches!(e, Event::Hit { .. })).collect::<Vec<_>>(),
            [&Event::Hit { kart: 0, kind: Kind::Drone }]
        );
        assert!(highest > DRONE_HEIGHT - 0.1, "it flew over the others: {highest}");
        assert!(karts[0].speed < 15.0 * 0.4 && karts[0].spin != 0.0, "{:?}", karts[0]);
        assert!(paddock.things().is_empty());
        // The leader's own drone goes nowhere.
        let mut paddock = Paddock::new(&track, 2, 1);
        paddock.hold(0, Some(Item::Drone));
        paddock.step(&mut [kart(50.0, 0.0), kart(20.0, 0.0)], &[true, false], &at_places(&[0, 1]), &track);
        assert!(paddock.things().is_empty() && paddock.held(0).is_none());
    }

    #[test]
    fn a_push_boosts_and_nothing_touches_the_kart_meanwhile() {
        let track = track();
        let mut paddock = Paddock::new(&track, 2, 1);
        paddock.hold(0, Some(Item::Push));
        paddock.hold(1, Some(Item::Drone));
        let mut karts = [kart(60.0, 12.0), kart(30.0, 12.0)];
        let mut events = paddock.step(&mut karts, &[true, true], &at_places(&[0, 1]), &track);
        assert!(karts[0].boost >= PUSH - DT && paddock.guard(0) > 0.0);
        // A cone dropped right in its way goes, and the drone does nothing.
        paddock.things.push(Thing {
            kind: Kind::Cone,
            x: 0.0,
            z: 63.0,
            vx: 0.0,
            vz: 0.0,
            height: 0.0,
            target: 0,
            life: 9.0,
            owner: 1,
            spare: 0.0,
            inside: 0,
        });
        events.extend(run(&mut paddock, &mut karts, &[false, false], &track, (GUARD / DT) as usize - 10));
        assert!(!events.iter().any(|e| matches!(e, Event::Hit { kart: 0, .. })), "{events:?}");
        assert!(events.contains(&Event::Blocked { kart: 0, kind: Kind::Cone }), "{events:?}");
        assert!(events.contains(&Event::Blocked { kart: 0, kind: Kind::Drone }), "{events:?}");
        assert_eq!(karts[0].spin, 0.0);
    }

    #[test]
    fn a_bumper_knocks_karts_beside_aside_and_not_those_further() {
        let track = track();
        let mut paddock = Paddock::new(&track, 3, 1);
        paddock.hold(0, Some(Item::Bumper));
        let mut karts = [kart(50.0, 15.0), Kart { x: 2.0, z: 50.5, speed: 15.0, ..Kart::default() }, kart(70.0, 15.0)];
        let events = paddock.step(&mut karts, &[true, false, false], &at_places(&[1, 0, 2]), &track);
        assert!(events.contains(&Event::Bumped { kart: 1, by: 0 }), "{events:?}");
        assert!(!events.iter().any(|e| matches!(e, Event::Bumped { kart: 2, .. })));
        let v = karts[1].velocity();
        assert!(v.0 > 3.0, "pushed away to +X: {v:?}");
        assert_eq!(karts[0].velocity(), kart(50.0, 15.0).velocity(), "the bumper's own kart is not moved");
    }

    #[test]
    fn an_ai_driver_holds_a_tyre_behind_with_a_kart_behind_and_throws_at_one_ahead() {
        let track = track();
        let at = |z: f32| track.locate(0.0, z);
        let mut paddock = Paddock::new(&track, 3, 1);
        paddock.hold(0, Some(Item::Tyre));
        let me = kart(50.0, 18.0);
        let standings = at_places(&[1, 2, 0]);
        // A kart close behind, none ahead: button down, the tyre goes behind.
        let field = [at(50.0), at(42.0), at(150.0)];
        assert!(paddock.button(0, &field, &standings, &me, &track));
        paddock.step(&mut [me, kart(42.0, 18.0), kart(150.0, 18.0)], &[true, false, false], &standings, &track);
        assert!(paddock.trailing(0));
        // Still followed: it keeps it there.
        assert!(paddock.button(0, &field, &standings, &me, &track));
        // A kart comes up ahead on the straight: button up, thrown.
        let field = [at(50.0), at(42.0), at(62.0)];
        assert!(!paddock.button(0, &field, &standings, &me, &track));
        // A drone when not leading, a bumper with a kart beside, a push on a straight.
        assert!(wants(Item::Drone, 0, &field, &standings, &me, &track));
        assert!(!wants(Item::Drone, 2, &field, &standings, &me, &track));
        assert!(wants(Item::Bumper, 0, &[at(50.0), at(51.0)], &standings, &me, &track));
        assert!(!wants(Item::Bumper, 0, &[at(50.0), at(60.0)], &standings, &me, &track));
        assert!(wants(Item::Push, 0, &[at(50.0)], &standings, &me, &track));
    }
}
