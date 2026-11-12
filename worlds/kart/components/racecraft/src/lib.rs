// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! How the kart world's AI drivers race. Each has a temperament
//! ([`Temper`]) and a state of mind ([`Mind`]); every physics step,
//! [`drive`] turns them, the kart and the field around it into controls:
//!
//! - **bends:** it brakes for the bends ahead in time to take them as fast as
//!   a kart can turn, less a margin that its nerve sets;
//! - **a racing line:** the outside of a bend to turn in, the inside at the
//!   apex, its own line on the straights;
//! - **traffic:** behind a slower kart it overtakes on the side with room —
//!   the aggressive dive for the inside of the bend into narrow gaps — or it
//!   waits; alongside another, the polite give room, the aggressive hold
//!   their line; with a kart close behind, a defender covers the inside;
//! - **mistakes:** at some bends, as rarely as its consistency allows, it
//!   brakes too late and runs wide, or it hesitates;
//! - **grudges:** a knock shakes it for a few seconds — a hothead races
//!   harder, a calm one lifts off.
//!
//! Deterministic like the physics it drives: no clock, no random numbers but
//! a hash of the driver's seed, `libm` for every non-trivial function.

use libm::{atan2f, cosf, fabsf, sinf, sqrtf};
use osage_kart_physics::{Controls, DT, KART_RADIUS, Kart, MAX_SPEED, Spot, TURN_RATE, Track, wrap};

/// A driver's temperament, each trait in 0…1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Temper {
    /// Share of the kart's top speed it wants on the straights.
    pub pace: f32,
    /// How near the limit it takes bends.
    pub nerve: f32,
    /// How readily it goes for a gap, and how narrow a gap it accepts.
    pub aggression: f32,
    /// How readily it covers its line from a kart behind.
    pub defence: f32,
    /// 1 never errs; lower, it errs at more bends (at most one in ten).
    pub consistency: f32,
    /// After a knock: 0 lifts off, 1 races harder.
    pub heat: f32,
    /// Drifts through bends for mini-turbos.
    pub drifts: bool,
}

impl Temper {
    pub const STEADY: Self =
        Self { pace: 0.9, nerve: 0.6, aggression: 0.5, defence: 0.5, consistency: 0.92, heat: 0.4, drifts: true };
    pub const HOTHEAD: Self =
        Self { pace: 0.95, nerve: 0.85, aggression: 0.9, defence: 0.8, consistency: 0.65, heat: 0.95, drifts: true };
    pub const CAUTIOUS: Self =
        Self { pace: 0.86, nerve: 0.4, aggression: 0.15, defence: 0.2, consistency: 0.96, heat: 0.1, drifts: false };
    pub const ROOKIE: Self =
        Self { pace: 0.84, nerve: 0.5, aggression: 0.4, defence: 0.3, consistency: 0.55, heat: 0.5, drifts: false };
    pub const VETERAN: Self =
        Self { pace: 0.93, nerve: 0.75, aggression: 0.6, defence: 0.9, consistency: 0.97, heat: 0.3, drifts: true };

    /// A temperament drawn from `seed`: varied, never absurd.
    pub fn of(seed: u32) -> Self {
        let draw = |k: u32| unit(hash(seed.wrapping_mul(0x9e37_79b9) ^ k.wrapping_mul(0x85eb_ca6b)));
        Self {
            pace: 0.83 + 0.12 * draw(1),
            nerve: 0.35 + 0.55 * draw(2),
            aggression: draw(3),
            defence: draw(4),
            consistency: 0.55 + 0.43 * draw(5),
            heat: draw(6),
            drifts: draw(7) > 0.35,
        }
    }
}

/// An integer hash with good mixing (a "lowbias32" finaliser).
fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

/// A hash as a number in 0…1.
fn unit(x: u32) -> f32 {
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// Another kart as a driver sees it: where it is on the track, how fast.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rival {
    pub spot: Spot,
    pub speed: f32,
}

/// The field: every kart located once, for all the drivers of a step.
pub fn field(track: &Track, karts: &[Kart]) -> Vec<Rival> {
    karts.iter().map(|k| Rival { spot: track.locate(k.x, k.z), speed: k.speed }).collect()
}

/// What a driver is doing about the karts around it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Plan {
    /// Racing its line.
    #[default]
    Race,
    /// Overtaking on a side: -1 left, 1 right.
    Pass { side: f32 },
    /// Waiting behind a kart it cannot pass.
    Wait,
    /// Covering a side from a kart behind.
    Defend { side: f32 },
}

/// A mistake under way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mistake {
    /// Brakes too late for the bend and runs wide.
    LateBraking,
    /// Lifts off for no reason.
    Hesitation,
}

/// A driver's state of mind, from one step to the next.
#[derive(Clone, Debug, Default)]
pub struct Mind {
    seed: u32,
    /// Where it steers to across the road, metres right of the centre line.
    lateral: Option<f32>,
    plan: Plan,
    /// Seconds the plan has held: nobody weaves.
    held: f32,
    mistake: Option<(Mistake, f32)>,
    /// The last bend judged for a mistake, by its distance along the track.
    judged: Option<f32>,
    bends: u32,
    mistakes: u32,
    shaken: f32,
    spin: f32,
}

impl Mind {
    pub fn new(seed: u32) -> Self {
        Self { seed, ..Self::default() }
    }

    pub fn plan(&self) -> Plan {
        self.plan
    }

    pub fn mistake(&self) -> Option<Mistake> {
        self.mistake.map(|(m, _)| m)
    }

    /// Mistakes made so far.
    pub fn mistakes(&self) -> u32 {
        self.mistakes
    }

    /// How shaken by a knock, 1 just after, 0 when over it.
    pub fn shaken(&self) -> f32 {
        self.shaken
    }
}

/// Braking a driver counts on, m/s² (the kart can do 32).
const BRAKING: f32 = 18.0;
/// The kart's centre stays this far inside the road's edge.
const EDGE: f32 = 1.1;
/// How fast a driver moves across the road, m/s.
const LANE_SPEED: f32 = 2.5;
/// The gap beside a kart a driver needs to pass, with no aggression.
const GAP: f32 = 2.0 * KART_RADIUS + 0.6;

/// The bend at `along`: its radius, and which side its inside is (-1 left,
/// 1 right).
fn bend(track: &Track, along: f32) -> (f32, f32) {
    let ((_, before), (_, after)) = (track.at(along - 6.0), track.at(along + 6.0));
    let turn = wrap(after - before);
    // A left-hand bend (the heading grows) has its inside on the left.
    (12.0 / fabsf(turn).max(1e-3), if turn > 0.0 { -1.0 } else { 1.0 })
}

/// Distance along the track from `from` to `to`, ahead, `0…length`.
fn ahead(track: &Track, from: f32, to: f32) -> f32 {
    (to - from).rem_euclid(track.length())
}

/// The controls of driver `me` of `field`, whose kart is `kart`.
pub fn drive(kart: &Kart, me: usize, field: &[Rival], temper: &Temper, mind: &mut Mind, track: &Track) -> Controls {
    let spot = field[me].spot;
    let room = track.half_width - EDGE;
    let lateral = *mind.lateral.get_or_insert(spot.lateral);
    let others = || field.iter().enumerate().filter(move |(i, _)| *i != me).map(|(_, r)| r);

    // A knock: the kart spins suddenly.
    if fabsf(kart.spin) > 0.8 && fabsf(kart.spin) > fabsf(mind.spin) + 0.5 {
        mind.shaken = 1.0;
    }
    mind.spin = kart.spin;
    mind.shaken = (mind.shaken - DT * 0.2).max(0.0);
    let aggression = (temper.aggression + mind.shaken * (temper.heat - 0.5)).clamp(0.0, 1.0);
    let mut pace = temper.pace * (1.0 - 0.12 * mind.shaken * (1.0 - temper.heat));
    mind.held += DT;

    // The sharpest bend in the next 60 m, and where it starts.
    let bends: Vec<_> = (0..=12).map(|d| bend(track, spot.along + 5.0 * d as f32)).collect();
    let sharpest = bends.iter().fold(f32::INFINITY, |s, b| s.min(b.0));
    let first = bends.iter().position(|b| b.0 < sharpest * 1.15).unwrap_or(0);
    let (at, inside) = (5.0 * first as f32, bends[first].1);

    // Mistakes: each tight bend, once, as rarely as consistency allows.
    if let Some((_, left)) = &mut mind.mistake {
        *left -= DT;
        if *left <= 0.0 {
            mind.mistake = None;
        }
    }
    if sharpest < 45.0 && (10.0..40.0).contains(&at) {
        let along = spot.along + at;
        let new = mind.judged.is_none_or(|judged| {
            let d = ahead(track, judged, along);
            d.min(track.length() - d) > 25.0
        });
        if new {
            mind.judged = Some(along);
            mind.bends += 1;
            let roll = hash(mind.seed ^ mind.bends.wrapping_mul(0x27d4_eb2d));
            if unit(roll) < (1.0 - temper.consistency) * 0.25 * (1.0 + mind.shaken) {
                mind.mistakes += 1;
                let kind = if roll & 1 == 0 { Mistake::LateBraking } else { Mistake::Hesitation };
                mind.mistake = Some((kind, 2.5));
            }
        }
    }
    let late = mind.mistake() == Some(Mistake::LateBraking);
    if mind.mistake() == Some(Mistake::Hesitation) {
        pace *= 0.75;
    }

    // The fastest the bends ahead allow: a kart turns at most TURN_RATE, so
    // a bend is taken at TURN_RATE times its radius at best (a little more,
    // the line using the road's width), less a margin.
    let margin = 0.62 + 0.25 * temper.nerve + if late { 0.3 } else { 0.0 };
    let braking = if late { BRAKING * 1.6 } else { BRAKING };
    let mut allowed = f32::INFINITY;
    for d in 0..=14 {
        let distance = 5.0 * d as f32;
        let (radius, _) = bend(track, spot.along + distance);
        let corner = margin * TURN_RATE * (radius + 3.0);
        allowed = allowed.min(sqrtf(corner * corner + 2.0 * braking * distance));
    }

    // The racing line: its own on the straights, the outside to turn in,
    // the inside at the apex; wide when it braked too late.
    let own = (unit(hash(mind.seed ^ 0x5bd1_e995)) - 0.5) * 0.5 * room;
    let mut wanted = if sharpest < 60.0 && at < 12.0 {
        inside * room * 0.7
    } else if sharpest < 60.0 && at < 45.0 {
        -inside * room * 0.5
    } else {
        own
    };
    if late {
        wanted = -inside * room * 0.95;
    }

    // Traffic: the nearest kart ahead in the way.
    let reach = 6.0 + kart.speed.max(0.0) * 0.7;
    let blocking = others()
        .map(|r| (r, ahead(track, spot.along, r.spot.along)))
        .filter(|(r, d)| *d > 0.3 && *d < reach && fabsf(r.spot.lateral - wanted.clamp(-room, room)) < 2.2)
        .fold(
            None,
            |best: Option<(&Rival, f32)>, (r, d)| if best.is_none_or(|(_, b)| d < b) { Some((r, d)) } else { best },
        );
    let plan = match blocking {
        Some((other, gap)) if other.speed < kart.speed + 1.5 => {
            let need = GAP - 0.5 * aggression;
            let room_on = |side: f32| room - side * other.spot.lateral;
            let prefer = if aggression > 0.6 && sharpest < 60.0 {
                inside
            } else if room_on(1.0) > room_on(-1.0) {
                1.0
            } else {
                -1.0
            };
            let side = [prefer, -prefer].into_iter().find(|&s| room_on(s) > need);
            // The cautious go only through a wide gap.
            let keen = |s: f32| aggression > 0.25 || room_on(s) > need + 1.0;
            match side {
                Some(s) if keen(s) => {
                    wanted = other.spot.lateral + s * (GAP + 0.2 - 0.6 * aggression);
                    Plan::Pass { side: s }
                }
                _ => {
                    // Follow it, a gap behind that shrinks with aggression.
                    let follow = other.speed + (gap - 5.0 + 2.0 * aggression) * 0.8;
                    allowed = allowed.min(follow.max(0.0));
                    Plan::Wait
                }
            }
        }
        _ => {
            // A kart close behind and faster: cover the inside of the bend.
            let pressed = others().any(|r| {
                let behind = ahead(track, r.spot.along, spot.along);
                behind > 0.3 && behind < 12.0 && r.speed > kart.speed - 1.0
            });
            if pressed && temper.defence > 0.3 && sharpest < 60.0 && at < 50.0 {
                wanted = inside * room * (0.3 + 0.5 * temper.defence);
                Plan::Defend { side: inside }
            } else {
                Plan::Race
            }
        }
    };
    // Plans hold a moment: no weaving, no dithering behind a kart.
    let settled = mind.held > 1.2 || matches!(mind.plan, Plan::Race | Plan::Wait);
    if plan != mind.plan && (settled || matches!(plan, Plan::Pass { .. })) {
        (mind.plan, mind.held) = (plan, 0.0);
    }
    if let Plan::Defend { side } = mind.plan {
        wanted = side * room * (0.3 + 0.5 * temper.defence);
    }
    // Alongside another: the polite give room, the aggressive hold on.
    if aggression < 0.7 {
        for r in others() {
            let d = ahead(track, spot.along, r.spot.along);
            let beside = d.min(track.length() - d) < 2.4 && fabsf(r.spot.lateral - spot.lateral) < 2.4;
            if beside {
                let away = if spot.lateral >= r.spot.lateral { 1.0 } else { -1.0 };
                wanted = r.spot.lateral + away * 2.4;
            }
        }
    }

    let wanted = wanted.clamp(-room, room);
    let lateral = lateral + (wanted - lateral).clamp(-LANE_SPEED * DT, LANE_SPEED * DT);
    mind.lateral = Some(lateral);

    let top = pace * MAX_SPEED + if kart.boost > 0.0 { 6.0 } else { 0.0 };
    let tight = sharpest < 40.0 && at < 15.0;
    let mut controls =
        steer(kart, spot, lateral, top.min(allowed), allowed, temper.drifts && tight && mind.plan != Plan::Wait, track);
    // Off a crest, the bold do a trick: a press of the button in the air.
    let flying = kart.height > track.lift(spot.along).0 + 1e-3;
    if flying && kart.trick == 0.0 && temper.nerve > 0.4 {
        controls.drift = kart.held == 0.0;
    }
    controls
}

/// The world's plain autopilot, for drivers who say only how to race: a
/// share of the top speed, a line across the road (-1 the left edge, 1 the
/// right), drifts or not. It brakes for bends like a steady driver and
/// knows nothing of the other karts.
pub fn follow(kart: &Kart, pace: f32, line: f32, drift: bool, track: &Track) -> Controls {
    let spot = track.locate(kart.x, kart.z);
    let mut allowed = f32::INFINITY;
    let mut tight = false;
    for d in 0..=14 {
        let distance = 5.0 * d as f32;
        let (radius, _) = bend(track, spot.along + distance);
        let corner = 0.77 * TURN_RATE * (radius + 3.0);
        allowed = allowed.min(sqrtf(corner * corner + 2.0 * BRAKING * distance));
        tight |= radius < 40.0 && distance < 15.0;
    }
    let lateral = line.clamp(-1.0, 1.0) * (track.half_width - 1.5);
    let top = pace.clamp(0.0, 1.0) * MAX_SPEED;
    steer(kart, spot, lateral, top.min(allowed), allowed, drift && tight, track)
}

/// Aims at a point ahead on the line `lateral`, holds `wish` and brakes
/// above `allowed`.
fn steer(kart: &Kart, spot: Spot, lateral: f32, wish: f32, allowed: f32, drift: bool, track: &Track) -> Controls {
    // Further ahead the faster, nearer in a tight bend: never across it.
    let (radius, _) = bend(track, spot.along + 4.0);
    let look = (6.0 + kart.speed.max(0.0) * 0.55).min(3.0 + 1.2 * radius);
    let (centre, road) = track.at(spot.along + look);
    // Right of the road is (-cos h, sin h).
    let target = [centre[0] - cosf(road) * lateral, centre[1] + sinf(road) * lateral];
    let error = wrap(atan2f(target[0] - kart.x, target[1] - kart.z) - kart.heading);
    let throttle = if wish <= 0.0 {
        if kart.speed > 0.5 { -1.0 } else { 0.0 }
    } else if kart.speed > allowed + 0.3 {
        -((kart.speed - allowed) / 3.0).min(1.0)
    } else if kart.speed < wish {
        1.0
    } else {
        0.0
    };
    Controls {
        throttle,
        steer: (-error * 2.2).clamp(-1.0, 1.0),
        drift: drift && fabsf(error) > 0.18 && kart.speed > 14.0,
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use osage_kart_physics::{Ground, bump, step, wheels};

    use super::*;

    /// Two 100 m straights joined by hairpins of 12 m radius, driven
    /// clockwise from the start heading +Z.
    fn stadium(half_width: f32) -> Track {
        hairpins(half_width, 12.0)
    }

    fn hairpins(half_width: f32, radius: f32) -> Track {
        let mut points = Vec::new();
        for i in 0..50 {
            points.push([0.0, 2.0 * i as f32]);
        }
        let r = radius;
        for i in 0..=18 {
            let a = PI * i as f32 / 18.0;
            points.push([-r + r * cosf(a), 100.0 + r * sinf(a)]);
        }
        for i in 1..50 {
            points.push([-2.0 * r, 100.0 - 2.0 * i as f32]);
        }
        for i in 0..18 {
            let a = PI + PI * i as f32 / 18.0;
            points.push([-r + r * cosf(a), r * sinf(a)]);
        }
        Track::new(points, half_width, 4.0)
    }

    /// Runs a race of `tempers` from a grid, steps at a time; returns the
    /// karts and minds after `seconds`, and the steps any kart had a wheel on
    /// the grass.
    fn race(track: &Track, tempers: &[Temper], grid: &[Kart], seconds: f32) -> (Vec<Kart>, Vec<Mind>, usize) {
        let mut karts = grid.to_vec();
        let mut minds: Vec<Mind> = (0..karts.len()).map(|i| Mind::new(i as u32 * 7 + 1)).collect();
        let mut grass = 0;
        for _ in 0..(seconds / DT) as usize {
            let field = field(track, &karts);
            let controls: Vec<_> =
                (0..karts.len()).map(|i| drive(&karts[i], i, &field, &tempers[i], &mut minds[i], track)).collect();
            for (kart, controls) in karts.iter_mut().zip(controls) {
                step(kart, controls, track);
                grass += wheels(kart, track).contains(&Ground::Grass) as usize;
            }
            for a in 0..karts.len() {
                for b in a + 1..karts.len() {
                    let (left, right) = karts.split_at_mut(b);
                    bump(&mut left[a], &mut right[0]);
                }
            }
        }
        (karts, minds, grass)
    }

    fn start(x: f32, z: f32) -> Kart {
        Kart { x, z, ..Kart::default() }
    }

    #[test]
    fn a_driver_who_never_errs_laps_without_leaving_the_road() {
        let track = stadium(6.0);
        for temper in [Temper::STEADY, Temper::HOTHEAD, Temper::CAUTIOUS, Temper::VETERAN] {
            let temper = Temper { consistency: 1.0, ..temper };
            let (karts, minds, grass) = race(&track, &[temper], &[start(0.0, 10.0)], 60.0);
            let laps = minds[0].bends / 2;
            assert_eq!(grass, 0, "{temper:?}: {:?}", karts[0]);
            assert!(laps >= 3, "{temper:?} laps {laps}");
            assert_eq!(minds[0].mistakes(), 0);
        }
    }

    #[test]
    fn hairpins_are_taken_slower_than_straights() {
        // At 22 m/s a kart turns on a 11 m radius: a hairpin of 7 m needs braking.
        let track = hairpins(2.5, 7.0);
        let temper = Temper { consistency: 1.0, ..Temper::STEADY };
        let mut kart = start(0.0, 10.0);
        let mut mind = Mind::new(1);
        let (mut fastest, mut in_hairpin) = (0.0f32, f32::INFINITY);
        for _ in 0..(20.0 / DT) as usize {
            let controls = drive(&kart, 0, &field(&track, &[kart]), &temper, &mut mind, &track);
            step(&mut kart, controls, &track);
            fastest = fastest.max(kart.speed);
            if kart.z > 102.0 {
                in_hairpin = in_hairpin.min(kart.speed);
            }
        }
        assert!(fastest > 0.85 * MAX_SPEED && in_hairpin < fastest - 4.0, "{fastest} {in_hairpin}");
    }

    #[test]
    fn the_aggressive_go_for_a_narrow_gap_and_the_cautious_wait() {
        // A slow kart in the middle of a narrow road, 6 m ahead.
        let track = stadium(2.6);
        let me = Kart { z: 20.0, speed: 15.0, ..Kart::default() };
        let slow = Kart { z: 26.0, speed: 10.0, ..Kart::default() };
        let field = field(&track, &[me, slow]);
        let mut bold = Mind::new(1);
        drive(&me, 0, &field, &Temper::HOTHEAD, &mut bold, &track);
        assert!(matches!(bold.plan(), Plan::Pass { .. }), "{:?}", bold.plan());
        let mut careful = Mind::new(1);
        drive(&me, 0, &field, &Temper::CAUTIOUS, &mut careful, &track);
        assert_eq!(careful.plan(), Plan::Wait);
        // On a wide road both go.
        let track = stadium(6.0);
        let field = super::field(&track, &[me, slow]);
        let mut careful = Mind::new(1);
        drive(&me, 0, &field, &Temper::CAUTIOUS, &mut careful, &track);
        assert!(matches!(careful.plan(), Plan::Pass { .. }), "{:?}", careful.plan());
    }

    #[test]
    fn a_fast_driver_works_through_a_slow_one() {
        let track = stadium(6.0);
        let slow = Temper { pace: 0.6, consistency: 1.0, ..Temper::CAUTIOUS };
        let fast = Temper { consistency: 1.0, ..Temper::HOTHEAD };
        let (karts, ..) = race(&track, &[slow, fast], &[start(0.0, 20.0), start(0.0, 10.0)], 10.0);
        let along = |k: &Kart| track.locate(k.x, k.z).along;
        // Within 10 s, the fast one is past and ahead.
        let lead = (along(&karts[1]) - along(&karts[0])).rem_euclid(track.length());
        assert!(lead > 5.0 && lead < track.length() / 2.0, "{lead}");
    }

    #[test]
    fn a_defender_covers_the_inside_from_a_kart_close_behind() {
        let track = stadium(6.0);
        // 30 m before the right-hand hairpin, a faster kart 5 m behind.
        let me = Kart { z: 70.0, speed: 15.0, ..Kart::default() };
        let chaser = Kart { z: 65.0, speed: 17.0, ..Kart::default() };
        let field = field(&track, &[me, chaser]);
        let mut mind = Mind::new(3);
        drive(&me, 0, &field, &Temper::VETERAN, &mut mind, &track);
        assert_eq!(mind.plan(), Plan::Defend { side: 1.0 }, "the inside of a right-hander is on the right");
    }

    #[test]
    fn mistakes_come_as_often_as_consistency_allows() {
        let track = stadium(6.0);
        let count = |consistency: f32| {
            let temper = Temper { consistency, ..Temper::STEADY };
            let (mut mistakes, mut bends) = (0, 0);
            for seed in 1..=6 {
                let mut kart = start(0.0, 10.0);
                let mut mind = Mind::new(seed);
                for _ in 0..(120.0 / DT) as usize {
                    let controls = drive(&kart, 0, &field(&track, &[kart]), &temper, &mut mind, &track);
                    step(&mut kart, controls, &track);
                }
                (mistakes, bends) = (mistakes + mind.mistakes(), bends + mind.bends);
            }
            (mistakes, bends)
        };
        assert_eq!(count(1.0).0, 0);
        // One bend in six on average (0.7 × 0.25).
        let (mistakes, bends) = count(0.3);
        let rate = mistakes as f32 / bends as f32;
        assert!((0.08..0.3).contains(&rate), "{mistakes} in {bends}");
    }

    #[test]
    fn a_knock_makes_a_hothead_bolder_and_a_calm_driver_lift() {
        let track = stadium(6.0);
        let me = Kart { z: 20.0, speed: 15.0, ..Kart::default() };
        let field = field(&track, &[me]);
        let knocked = Kart { spin: 3.0, ..me };
        let mut hot = Mind::new(1);
        drive(&me, 0, &field, &Temper::HOTHEAD, &mut hot, &track);
        drive(&knocked, 0, &field, &Temper::HOTHEAD, &mut hot, &track);
        assert!(hot.shaken() > 0.9);
        let mut calm = Mind::new(1);
        let calm_temper = Temper::CAUTIOUS;
        let before = drive(&Kart { speed: 0.84 * MAX_SPEED, ..me }, 0, &field, &calm_temper, &mut calm, &track);
        drive(&knocked, 0, &field, &calm_temper, &mut calm, &track);
        let after = drive(&Kart { speed: 0.84 * MAX_SPEED, ..me }, 0, &field, &calm_temper, &mut calm, &track);
        assert!(before.throttle > 0.0 && after.throttle <= 0.0, "a shaken calm driver lifts: {before:?} {after:?}");
    }

    #[test]
    fn the_same_race_twice_is_the_same() {
        let track = stadium(6.0);
        let tempers = [Temper::of(1), Temper::of(2), Temper::of(3), Temper::of(4)];
        let grid = [start(-2.0, 30.0), start(2.0, 25.0), start(-2.0, 20.0), start(2.0, 15.0)];
        let (a, ..) = race(&track, &tempers, &grid, 15.0);
        let (b, ..) = race(&track, &tempers, &grid, 15.0);
        assert_eq!(a, b);
    }

    #[test]
    fn drawn_tempers_stay_within_reason() {
        for seed in 0..200 {
            let t = Temper::of(seed);
            assert!((0.83..=0.95).contains(&t.pace) && (0.55..=0.98).contains(&t.consistency), "{t:?}");
            assert!([t.nerve, t.aggression, t.defence, t.heat].iter().all(|v| (0.0..=1.0).contains(v)));
        }
        assert_ne!(Temper::of(1), Temper::of(2));
    }
}
