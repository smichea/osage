// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Arcade kart physics of the osage kart world. The server runs it natively
//! and clients run the same code compiled to WebAssembly to predict their own
//! kart (D-031): with fixed steps and `libm` for every non-trivial function,
//! both compute the same numbers.
//!
//! Conventions: Y up, the track lies in the X–Z plane; a kart with heading
//! `h` faces `(sin h, cos h)`, its right is `(-cos h, sin h)`, and steering
//! right makes `h` decrease.
//!
//! The driving is arcade (a speed along the kart, a slide across it); the
//! contacts are physical (D-045): a kart has an outline, a mass and a moment
//! of inertia, and a knock is an impulse at the point where it lands.
//!
//! Skill is rewarded (D-050): a drift charges a mini-turbo of three levels;
//! a crest taken fast enough lifts a kart off the road, where a press of the
//! drift button is a trick that boosts on landing; a kart in another's wake charges a
//! slipstream boost ([`slipstream`]); and the world judges each start
//! ([`launch`]): a boost for throttle in time, a stall for throttle too early.
//!
//! Karts differ (D-055): each carries a [`Build`] — top speed, acceleration,
//! handling, grip and weight as shares of the standard kart's — that its
//! driver and its body give it. The standard build changes nothing, bit for
//! bit.

#[cfg(target_arch = "wasm32")]
mod wasm;

use libm::{atan2f, cosf, fabsf, sinf, sqrtf};

/// One physics step, seconds: the kart world ticks at 60 Hz.
pub const DT: f32 = 1.0 / 60.0;

/// Top speed on the road, m/s (about 80 km/h).
pub const MAX_SPEED: f32 = 22.0;
const ACCELERATION: f32 = 16.0;
const BRAKING: f32 = 32.0;
const REVERSE_SPEED: f32 = 7.0;
const ROLLING: f32 = 4.0;
/// Top speed off the road.
const OFF_ROAD_SPEED: f32 = 9.0;
/// How fast a kart turns at full lock above [`FULL_STEER_SPEED`], rad/s.
pub const TURN_RATE: f32 = 2.0;
/// Speed under which steering weakens, m/s.
pub const FULL_STEER_SPEED: f32 = 8.0;
const GRIP: f32 = 10.0;
const DRIFT_MIN_SPEED: f32 = 10.0;
/// A drift turns tighter than plain steering: that is what it is for.
const DRIFT_TURN_RATE: f32 = 2.7;
const DRIFT_GRIP: f32 = 3.0;
/// Drift seconds for each level of mini-turbo, and the boost seconds each
/// gives on release.
pub const TURBO_CHARGES: [f32; 3] = [0.6, 1.2, 2.0];
pub const TURBO_BOOSTS: [f32; 3] = [0.5, 1.0, 1.6];
/// Pulled down in the air, m/s²: a little more than the Earth's, as karts in
/// games fall.
pub const GRAVITY: f32 = 18.0;
/// In the air, steering turns the kart this share of what it does on the ground.
const AIR_STEER: f32 = 0.25;
/// A trick in the air: the boost seconds it gives on landing.
pub const TRICK_BOOST: f32 = 0.7;
/// Slipstream: within this far ahead (m) and this far across (m) behind a
/// kart, above this speed (m/s), it charges; this many seconds of it give a
/// boost of this many seconds.
const DRAFT_AHEAD: (f32, f32) = (1.5, 10.0);
const DRAFT_ACROSS: f32 = 1.4;
const DRAFT_MIN_SPEED: f32 = 14.0;
pub const DRAFT_CHARGE: f32 = 1.2;
const DRAFT_BOOST: f32 = 0.6;
/// The start (judged by [`launch`]): throttle pressed at most this many
/// seconds before the lights go boosts for this long; pressed earlier than
/// this, the engine floods and the kart stalls for this long.
pub const LAUNCH_WINDOW: f32 = 0.4;
const LAUNCH_BOOST: f32 = 1.0;
pub const LAUNCH_EARLY: f32 = 1.5;
const LAUNCH_STALL: f32 = 0.8;
const BOOST_SPEED: f32 = 28.0;
const BOOST_ACCELERATION: f32 = 40.0;
/// A rocket strapped to the kart: faster than a boost, and pushing on
/// over grass more than a boost does.
pub const ROCKET_SPEED: f32 = 34.0;
const ROCKET_ACCELERATION: f32 = 30.0;

/// A kart's outline, for contacts: a capsule along the kart, its spine
/// reaching this far ahead of and behind the centre, this thick: 1.9 m long,
/// 1.3 m wide.
pub const KART_REACH: f32 = 0.3;
pub const KART_RADIUS: f32 = 0.65;
/// The moment of inertia about the vertical per kilogram, m²: less than a
/// uniform box of the kart's size (0.44), the driver's and the engine's
/// weight being near the middle. Every kart weighs the same.
const YAW_INERTIA: f32 = 0.3;
/// Karts touching: the share of the closing speed given back, and the
/// friction of bodywork and tyres rubbing.
const KART_RESTITUTION: f32 = 0.3;
const KART_FRICTION: f32 = 0.4;
/// Barriers of tyres give a little back; along them, a kart scrapes.
const WALL_RESTITUTION: f32 = 0.2;
const WALL_FRICTION: f32 = 0.3;
/// How fast the tyres stop a spin, per second.
const SPIN_GRIP: f32 = 4.0;
/// An impulse that ends a drift, m/s.
const KNOCK: f32 = 2.5;

/// Kerbs: this wide beyond the road's edge. A wheel on them keeps the road's
/// speed and loses some grip.
pub const KERB: f32 = 1.0;
/// The barriers' face stands this far beyond the verge: a kart's outline
/// stops there, so its centre comes no nearer than [`KART_RADIUS`].
pub const BARRIER: f32 = 0.9;
/// The wheels' contact patches from the kart's centre: across and along.
const WHEEL_ACROSS: f32 = 0.55;
const WHEEL_ALONG: f32 = 0.52;
/// Grip kept with all four wheels on kerbs, and on grass.
const KERB_GRIP: f32 = 0.65;
const GRASS_GRIP: f32 = 0.5;
/// A wet road (D-068): the tyres' grip and the brakes as shares of a dry
/// road's. A slide takes longer to scrub off, a stop longer to make.
pub const WET_GRIP: f32 = 0.6;
/// Hills (D-077): how much a slope takes from the top speed or adds to it
/// (per unit of slope, within 0.8 to 1.15 of it), and how hard it pulls a
/// kart back or on, m/s² per unit of slope — half of gravity: an arcade's.
pub const HILL_TOP: f32 = 1.0;
pub const HILL_PULL: f32 = 9.0;
/// Oily tyres (D-072): the grip they keep, and how much of the steering.
pub const SLICK_GRIP: f32 = 0.12;
pub const SLICK_STEER: f32 = 0.3;
pub const WET_BRAKING: f32 = 0.7;
/// Grass drags the wheels in it: with a side's two wheels in it, the kart
/// turns towards it by this many rad/s per m/s.
const GRASS_PULL: f32 = 0.02;

/// How a kart is built, each as a share of the standard kart's (1): top
/// speed (and boosted speed), acceleration, rate of turn (and of a drift),
/// grip (how fast a slide is scrubbed off), and weight (in a knock, the
/// heavier is moved less). A rocket's speed is the same for all.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Build {
    pub top: f32,
    pub accel: f32,
    pub handling: f32,
    pub grip: f32,
    pub weight: f32,
}

impl Build {
    /// The standard kart.
    pub const STANDARD: Build = Build { top: 1.0, accel: 1.0, handling: 1.0, grip: 1.0, weight: 1.0 };

    /// Each share kept within reason: a build is a flavour, not a cheat.
    pub fn sanitized(self) -> Build {
        let within = |v: f32, low: f32, high: f32| if v.is_finite() { v.clamp(low, high) } else { 1.0 };
        Build {
            top: within(self.top, 0.85, 1.15),
            accel: within(self.accel, 0.7, 1.4),
            handling: within(self.handling, 0.75, 1.3),
            grip: within(self.grip, 0.7, 1.4),
            weight: within(self.weight, 0.7, 1.4),
        }
    }
}

impl Default for Build {
    fn default() -> Self {
        Build::STANDARD
    }
}

/// A kart's physical state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Kart {
    pub x: f32,
    pub z: f32,
    pub heading: f32,
    /// Forward speed, m/s (negative when reversing).
    pub speed: f32,
    /// Sideways speed, m/s, positive to the right: the slide of a drift.
    pub slide: f32,
    /// -1 drifting left, 1 drifting right, 0 not drifting.
    pub drift: f32,
    /// Seconds of drift so far: enough of it gives a mini-turbo on release.
    pub charge: f32,
    /// Seconds of boost left.
    pub boost: f32,
    /// Seconds of rocket left (paddock objects): faster than a boost.
    pub rocket: f32,
    /// Rate of turn a knock gave, rad/s, positive to the left (as the
    /// heading grows); the tyres stop it.
    pub spin: f32,
    /// Height of the kart's base above level ground, m (the road's own height
    /// on a crest, more in the air), and speed upwards, m/s.
    pub height: f32,
    pub climb: f32,
    /// 1 when a trick was done in this jump: a boost on landing.
    pub trick: f32,
    /// Seconds the engine is stalled for (a start too early): no throttle.
    pub stall: f32,
    /// Seconds in another kart's wake so far: enough of it gives a boost.
    pub draft: f32,
    /// 1 when the drift button was held at the last step: a press is a change.
    pub held: f32,
    /// Seconds the tyres stay oily (D-072): they grip almost nothing, the
    /// kart slides on the way it went and the wheel barely turns it.
    pub slick: f32,
    /// How it is built: fixed for a race, set by the world.
    pub build: Build,
}

/// The level of mini-turbo a drift of `charge` seconds has reached: 0 (none) to 3.
pub fn turbo_level(charge: f32) -> usize {
    TURBO_CHARGES.iter().filter(|&&c| charge >= c).count()
}

/// A crest across the whole road, verges and barriers too (D-051): from
/// `along` the ground rises over `rise` metres to `height`, then falls back
/// over `fall` metres, smoothly (half a cosine each way: no edge, no step).
/// A kart leaves the road where the road curves away faster than it falls —
/// near the top, taken fast — and lands further on; a slow kart drives over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crest {
    pub along: f32,
    pub rise: f32,
    pub fall: f32,
    pub height: f32,
}

impl Crest {
    /// Metres along the track it covers.
    pub fn length(&self) -> f32 {
        self.rise + self.fall
    }

    /// The road's height and slope `into` metres past the crest's start;
    /// none off the crest.
    pub fn profile(&self, into: f32) -> Option<(f32, f32)> {
        if !(0.0..self.length()).contains(&into) {
            return None;
        }
        // Half a cosine up, then half a cosine down: 0 to π, π to 2π.
        let (span, turn) = if into < self.rise {
            (self.rise, core::f32::consts::PI * into / self.rise.max(f32::EPSILON))
        } else {
            let past = into - self.rise;
            (self.fall, core::f32::consts::PI * (1.0 + past / self.fall.max(f32::EPSILON)))
        };
        let height = self.height * 0.5 * (1.0 - cosf(turn));
        let slope = self.height * 0.5 * core::f32::consts::PI / span.max(f32::EPSILON) * sinf(turn);
        Some((height, slope))
    }
}

/// What a wheel runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ground {
    Road,
    Kerb,
    Grass,
}

/// What the driver does during one step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Controls {
    /// 1 full throttle, -1 brake then reverse.
    pub throttle: f32,
    /// 1 full right, -1 full left.
    pub steer: f32,
    /// The drift button is held.
    pub drift: bool,
}

impl Controls {
    /// Controls from anyone, made safe: finite and within range.
    pub fn sanitized(self) -> Self {
        let clean = |v: f32| if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 };
        Self { throttle: clean(self.throttle), steer: clean(self.steer), drift: self.drift }
    }
}

/// A closed circuit: its centre line, the half width of the road, and how far
/// beyond the road the walls stand.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    points: Vec<[f32; 2]>,
    /// Distance from the start to each point, along the centre line.
    distances: Vec<f32>,
    length: f32,
    pub half_width: f32,
    pub verge: f32,
    crests: Vec<Crest>,
    /// How wet the road is, 0 (dry) to 1 (rain): it grips and brakes less
    /// (D-068). Set by the world for a race.
    pub wet: f32,
    /// The road's height at each point of the centre line, m (D-077):
    /// hills. Empty: a level road.
    heights: Vec<f32>,
}

/// Where a position is relative to the track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    /// Distance along the centre line from the start, `0..length`.
    pub along: f32,
    /// Signed distance from the centre line, positive to the right of the
    /// direction of travel.
    pub lateral: f32,
}

impl Track {
    /// A track through `points`, closed from the last back to the first.
    pub fn new(points: Vec<[f32; 2]>, half_width: f32, verge: f32) -> Self {
        assert!(points.len() >= 3, "a track needs at least three points");
        let mut distances = Vec::with_capacity(points.len());
        let mut length = 0.0;
        for i in 0..points.len() {
            distances.push(length);
            length += distance(points[i], points[(i + 1) % points.len()]);
        }
        Self { points, distances, length, half_width, verge, crests: Vec::new(), wet: 0.0, heights: Vec::new() }
    }

    /// The same track with crests across its road.
    pub fn with_crests(mut self, crests: Vec<Crest>) -> Self {
        self.crests = crests;
        self
    }

    /// The same track, `wet` (0 dry, 1 in the rain).
    pub fn with_wet(mut self, wet: f32) -> Self {
        self.wet = if wet.is_finite() { wet.clamp(0.0, 1.0) } else { 0.0 };
        self
    }

    pub fn crests(&self) -> &[Crest] {
        &self.crests
    }

    /// The road's height at `along` (on a crest, or 0), and its slope
    /// (rise per metre along the track).
    pub fn lift(&self, along: f32) -> (f32, f32) {
        let crest = self
            .crests
            .iter()
            .find_map(|crest| crest.profile((along - crest.along).rem_euclid(self.length)))
            .unwrap_or((0.0, 0.0));
        if self.heights.is_empty() {
            return crest;
        }
        let hill = self.hill(along);
        (crest.0 + hill.0, crest.1 + hill.1)
    }

    /// The same track over hills (D-077): the road's height at each point
    /// of the centre line, m. Anything else than one finite height a point
    /// leaves it level.
    pub fn with_heights(mut self, heights: Vec<f32>) -> Self {
        let whole = heights.len() == self.points.len() && heights.iter().all(|h| h.is_finite());
        self.heights = if whole && heights.iter().any(|h| *h != 0.0) { heights } else { Vec::new() };
        self
    }

    /// The road's height at each point of the centre line; empty when level.
    pub fn heights(&self) -> &[f32] {
        &self.heights
    }

    /// The hills alone, `along` metres along the centre line: the road's
    /// height and its slope (rise per metre driven), crests left out.
    pub fn hill(&self, along: f32) -> (f32, f32) {
        if self.heights.is_empty() {
            return (0.0, 0.0);
        }
        let along = along.rem_euclid(self.length);
        let i = self.distances.partition_point(|&d| d <= along).saturating_sub(1);
        let j = (i + 1) % self.points.len();
        let segment = distance(self.points[i], self.points[j]).max(f32::EPSILON);
        let t = (along - self.distances[i]) / segment;
        let (a, b) = (self.heights[i], self.heights[j]);
        (a + t * (b - a), (b - a) / segment)
    }

    pub fn points(&self) -> &[[f32; 2]] {
        &self.points
    }

    pub fn length(&self) -> f32 {
        self.length
    }

    /// The closest point of the centre line to `(x, z)`.
    pub fn locate(&self, x: f32, z: f32) -> Spot {
        let mut best = (f32::INFINITY, Spot { along: 0.0, lateral: 0.0 });
        for (i, &a) in self.points.iter().enumerate() {
            let b = self.points[(i + 1) % self.points.len()];
            let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
            let segment = dx * dx + dz * dz;
            let t = if segment > 0.0 { (((x - a[0]) * dx + (z - a[1]) * dz) / segment).clamp(0.0, 1.0) } else { 0.0 };
            let (px, pz) = (a[0] + t * dx, a[1] + t * dz);
            let squared = (x - px) * (x - px) + (z - pz) * (z - pz);
            if squared < best.0 {
                let norm = sqrtf(segment).max(f32::EPSILON);
                // Right of the direction (dx, dz) is (-dz, dx).
                let lateral = ((x - px) * -dz + (z - pz) * dx) / norm;
                best = (squared, Spot { along: self.distances[i] + t * norm, lateral });
            }
        }
        best.1
    }

    /// What the ground is at `lateral` metres from the centre line.
    pub fn ground(&self, lateral: f32) -> Ground {
        let off = fabsf(lateral);
        if off <= self.half_width {
            Ground::Road
        } else if off <= self.half_width + KERB {
            Ground::Kerb
        } else {
            Ground::Grass
        }
    }

    /// The centre line point at `along` metres from the start, and the
    /// heading of the road there.
    pub fn at(&self, along: f32) -> ([f32; 2], f32) {
        let along = along.rem_euclid(self.length);
        let i = self.distances.partition_point(|&d| d <= along).saturating_sub(1);
        let (a, b) = (self.points[i], self.points[(i + 1) % self.points.len()]);
        let segment = distance(a, b).max(f32::EPSILON);
        let t = (along - self.distances[i]) / segment;
        ([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])], atan2f(b[0] - a[0], b[1] - a[1]))
    }
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    sqrtf((b[0] - a[0]) * (b[0] - a[0]) + (b[1] - a[1]) * (b[1] - a[1]))
}

fn dot(a: (f32, f32), b: (f32, f32)) -> f32 {
    a.0 * b.0 + a.1 * b.1
}

/// The kart's forward and right.
fn axes(heading: f32) -> ((f32, f32), (f32, f32)) {
    let (sin, cos) = (sinf(heading), cosf(heading));
    ((sin, cos), (-cos, sin))
}

/// The road's right where the centre line passes `spot`.
fn road_right(track: &Track, spot: Spot) -> (f32, f32) {
    let (_, road) = track.at(spot.along);
    (-cosf(road), sinf(road))
}

/// How a point at `r` from the centre moves when the kart turns at one
/// radian a second (to the left).
fn swept(r: (f32, f32)) -> (f32, f32) {
    (r.1, -r.0)
}

impl Kart {
    /// Its velocity on the ground.
    pub fn velocity(&self) -> (f32, f32) {
        let (forward, right) = axes(self.heading);
        (forward.0 * self.speed + right.0 * self.slide, forward.1 * self.speed + right.1 * self.slide)
    }

    fn set_velocity(&mut self, v: (f32, f32)) {
        let (forward, right) = axes(self.heading);
        (self.speed, self.slide) = (dot(v, forward), dot(v, right));
    }

    /// Takes an impulse `j` (per kilogram, m/s) at `r` from its centre, both
    /// on the ground's axes: its velocity and its spin change, and a hard
    /// enough knock ends a drift. For what a world throws at karts.
    pub fn take(&mut self, j: (f32, f32), r: (f32, f32)) {
        let v = self.velocity();
        self.set_velocity((v.0 + j.0, v.1 + j.1));
        self.spin += dot(swept(r), j) / YAW_INERTIA;
        if sqrtf(dot(j, j)) > KNOCK {
            self.drift = 0.0;
            self.charge = 0.0;
        }
    }

    /// The velocity of its point at `r` from its centre.
    fn velocity_at(&self, r: (f32, f32)) -> (f32, f32) {
        let (v, w) = (self.velocity(), swept(r));
        (v.0 + self.spin * w.0, v.1 + self.spin * w.1)
    }
}

/// The ground under each wheel: front left, front right, rear left, rear right.
pub fn wheels(kart: &Kart, track: &Track) -> [Ground; 4] {
    wheels_at(kart, track, track.locate(kart.x, kart.z))
}

fn wheels_at(kart: &Kart, track: &Track, spot: Spot) -> [Ground; 4] {
    let (forward, right) = axes(kart.heading);
    let across_road = road_right(track, spot);
    [(1.0, -1.0), (1.0, 1.0), (-1.0, -1.0), (-1.0, 1.0)].map(|(along, across)| {
        let offset = (
            forward.0 * along * WHEEL_ALONG + right.0 * across * WHEEL_ACROSS,
            forward.1 * along * WHEEL_ALONG + right.1 * across * WHEEL_ACROSS,
        );
        track.ground(spot.lateral + dot(offset, across_road))
    })
}

/// Advances `kart` by one step of [`DT`] seconds.
pub fn step(kart: &mut Kart, controls: Controls, track: &Track) {
    let Controls { mut throttle, steer, drift } = controls.sanitized();
    let pressed = drift && kart.held == 0.0;
    kart.held = if drift { 1.0 } else { 0.0 };
    if kart.stall > 0.0 {
        throttle = 0.0;
        kart.stall = (kart.stall - DT).max(0.0);
    }
    let here = track.locate(kart.x, kart.z);
    let road = track.lift(here.along).0;
    let flying = kart.height > road + 1e-3;
    // Each wheel feels its ground: grass slows the kart in proportion and
    // drags it towards the side it is on; kerbs and grass grip less. In the
    // air, nothing does.
    let ground = if flying { [Ground::Road; 4] } else { wheels_at(kart, track, here) };
    let count = |on: Ground, wheels: &[usize]| wheels.iter().filter(|&&i| ground[i] == on).count() as f32;
    let (grass_left, grass_right) = (count(Ground::Grass, &[0, 2]), count(Ground::Grass, &[1, 3]));
    let grass = (grass_left + grass_right) / 4.0;
    let kerb = count(Ground::Kerb, &[0, 1, 2, 3]) / 4.0;
    let build = kart.build.sanitized();
    let fastest = if kart.rocket > 0.0 {
        ROCKET_SPEED
    } else if kart.boost > 0.0 {
        BOOST_SPEED * build.top
    } else {
        MAX_SPEED * build.top
    };
    // Uphill a kart is slower, downhill faster (D-077): the slope it faces.
    let hill = if track.heights.is_empty() || flying {
        0.0
    } else {
        track.hill(here.along).1 * cosf(kart.heading - track.at(here.along).1)
    };
    let fastest = if hill != 0.0 { fastest * (1.0 - HILL_TOP * hill).clamp(0.8, 1.15) } else { fastest };
    let top = fastest - (fastest - OFF_ROAD_SPEED) * grass;
    let footing = 1.0 - (1.0 - KERB_GRIP) * kerb - (1.0 - GRASS_GRIP) * grass;
    // Rain takes grip from every tyre (D-068); a dry road, nothing (×1).
    let wet = track.wet.clamp(0.0, 1.0);
    let footing = footing * (1.0 - (1.0 - WET_GRIP) * wet);
    // Oil on the tyres takes nearly all of it, while it lasts (D-072).
    let oily = kart.slick > 0.0;
    let footing = if oily { footing * SLICK_GRIP } else { footing };
    if oily {
        kart.slick = (kart.slick - DT).max(0.0);
    }

    // Along the kart: wheels in the air neither drive nor brake.
    if flying {
    } else if throttle > 0.0 {
        if kart.speed < top {
            kart.speed = (kart.speed + throttle * (ACCELERATION * build.accel) * DT).min(top);
        }
    } else if throttle < 0.0 {
        let rate = if kart.speed > 0.0 { BRAKING * (1.0 - (1.0 - WET_BRAKING) * wet) } else { ACCELERATION / 2.0 };
        kart.speed = (kart.speed + throttle * rate * DT).max(-REVERSE_SPEED);
    } else {
        let roll = (ROLLING * DT).min(fabsf(kart.speed));
        kart.speed -= roll * kart.speed.signum();
    }
    // The slope pulls a moving kart back or on.
    if hill != 0.0 && (fabsf(kart.speed) > 0.5 || throttle != 0.0) {
        kart.speed -= HILL_PULL * hill * DT;
    }
    if kart.boost > 0.0 {
        kart.speed = (kart.speed + BOOST_ACCELERATION * DT).min((BOOST_SPEED * build.top).max(kart.speed));
        kart.boost = (kart.boost - DT).max(0.0);
    }
    if kart.rocket > 0.0 {
        kart.speed = (kart.speed + ROCKET_ACCELERATION * DT).min(ROCKET_SPEED.max(kart.speed));
        kart.rocket = (kart.rocket - DT).max(0.0);
    }
    if kart.speed > top {
        kart.speed -= (kart.speed - top) * (3.0 * DT).min(1.0);
    }

    // Drift: starts with the button, a direction and some speed; releasing
    // it after a long enough drift gives a mini-turbo.
    if flying {
        // A press of the button in the air is a trick.
        if pressed && kart.trick == 0.0 {
            kart.trick = 1.0;
        }
    } else if kart.drift == 0.0 {
        if drift && fabsf(steer) > 0.2 && kart.speed > DRIFT_MIN_SPEED {
            kart.drift = steer.signum();
            kart.charge = 0.0;
        }
    } else if !drift || kart.speed < DRIFT_MIN_SPEED * 0.7 {
        if drift {
            kart.charge = 0.0; // too slow: the drift is lost
        }
        let level = turbo_level(kart.charge);
        if level > 0 {
            kart.boost = kart.boost.max(TURBO_BOOSTS[level - 1]);
        }
        kart.drift = 0.0;
        kart.charge = 0.0;
    }

    // Turning: weaker at low speed, reversed when reversing.
    let steering =
        (fabsf(kart.speed) / FULL_STEER_SPEED).min(1.0) * kart.speed.signum() * if flying { AIR_STEER } else { 1.0 };
    let steering = if oily { steering * SLICK_STEER } else { steering };
    let (target_slide, grip) = if kart.drift != 0.0 {
        // Steering into the drift tightens it, against it widens it.
        let tightness = 1.0 + 0.45 * steer * kart.drift;
        kart.heading -= kart.drift * tightness * (DRIFT_TURN_RATE * build.handling) * steering * DT;
        if !flying {
            kart.charge += DT * tightness;
        }
        (-kart.drift * kart.speed * 0.12, DRIFT_GRIP * build.grip)
    } else {
        kart.heading -= steer * (TURN_RATE * build.handling) * steering * DT;
        (0.0, GRIP * build.grip)
    };
    let grip = if flying { grip * 0.1 } else { grip };
    kart.slide += (target_slide - kart.slide) * (grip * footing * DT).min(1.0);
    kart.heading += (grass_left - grass_right) / 2.0 * GRASS_PULL * kart.speed * DT;
    // A knock's spin turns the kart, which keeps going the way it went: the
    // speed it had becomes a slide that the tyres scrub off.
    if kart.spin != 0.0 {
        let v = kart.velocity();
        kart.heading += kart.spin * DT;
        kart.set_velocity(v);
        kart.spin -= kart.spin * (SPIN_GRIP * footing * DT).min(1.0);
        if fabsf(kart.spin) < 1e-3 {
            kart.spin = 0.0;
        }
    }
    kart.heading = wrap(kart.heading);

    let v = kart.velocity();
    kart.x += v.0 * DT;
    kart.z += v.1 * DT;
    walls(kart, track);
    fly(kart, track, flying, road);
}

/// Up and down, ballistically: a kart falls freely from where it was with
/// the speed upwards it had; if the road under it is higher than that, the
/// road carries it (how fast the road under it rose becomes its climb, in
/// whichever direction it drives). Over a crest
/// taken fast, the road falls away quicker than the kart: it flies, then
/// lands — with a boost if it did a trick.
fn fly(kart: &mut Kart, track: &Track, flying: bool, road: f32) {
    let surface = track.lift(track.locate(kart.x, kart.z).along).0;
    let falling = kart.height + kart.climb * DT - 0.5 * GRAVITY * DT * DT;
    if falling <= surface {
        if flying && kart.trick > 0.0 {
            kart.boost = kart.boost.max(TRICK_BOOST);
        }
        kart.height = surface;
        kart.climb = (surface - road) / DT;
        kart.trick = 0.0;
    } else {
        kart.height = falling;
        kart.climb -= GRAVITY * DT;
    }
}

/// Every kart in another's wake charges its slipstream; a full charge is a
/// boost. The world runs it after the karts have moved.
pub fn slipstream(karts: &mut [Kart]) {
    let wake: Vec<bool> = (0..karts.len())
        .map(|i| {
            let me = &karts[i];
            let (forward, right) = axes(me.heading);
            me.speed > DRAFT_MIN_SPEED
                && me.height < 0.05
                && karts.iter().enumerate().any(|(j, other)| {
                    let gap = (other.x - me.x, other.z - me.z);
                    let ahead = dot(gap, forward);
                    j != i
                        && other.speed > DRAFT_MIN_SPEED * 0.7
                        && ahead > DRAFT_AHEAD.0
                        && ahead < DRAFT_AHEAD.1
                        && fabsf(dot(gap, right)) < DRAFT_ACROSS
                })
        })
        .collect();
    for (kart, wake) in karts.iter_mut().zip(wake) {
        if wake {
            kart.draft += DT;
            if kart.draft >= DRAFT_CHARGE {
                kart.boost = kart.boost.max(DRAFT_BOOST);
                kart.draft = 0.0;
            }
        } else {
            kart.draft = (kart.draft - 2.0 * DT).max(0.0);
        }
    }
}

/// The start, as the lights go: `pressed` is how many seconds before it the
/// throttle was pressed and held down since (`None`: not held). In time is a
/// boost; too early floods the engine, which stalls.
pub fn launch(kart: &mut Kart, pressed: Option<f32>) {
    match pressed {
        Some(before) if before <= LAUNCH_WINDOW => kart.boost = kart.boost.max(LAUNCH_BOOST),
        Some(before) if before > LAUNCH_EARLY => kart.stall = LAUNCH_STALL,
        _ => {}
    }
}

/// The barriers: a kart's outline pushed back to their face, and an impulse
/// where it touches — it bounces a little off them, scrapes along them, and
/// spins if it hits them with a corner.
fn walls(kart: &mut Kart, track: &Track) {
    let spot = track.locate(kart.x, kart.z);
    let face = track.half_width + track.verge + BARRIER;
    let across_road = road_right(track, spot);
    let side = if spot.lateral >= 0.0 { 1.0 } else { -1.0 };
    let (forward, _) = axes(kart.heading);
    // The end of the kart's spine nearer the barrier.
    let towards = dot(forward, across_road) * side;
    let end = if towards >= 0.0 { KART_REACH } else { -KART_REACH };
    let depth = side * spot.lateral + end * towards + KART_RADIUS - face;
    if depth <= 0.0 {
        return;
    }
    // Back to the road, along the barrier's normal.
    let normal = (-side * across_road.0, -side * across_road.1);
    kart.x += normal.0 * depth;
    kart.z += normal.1 * depth;
    let r = (forward.0 * end - normal.0 * KART_RADIUS, forward.1 * end - normal.1 * KART_RADIUS);
    let v = kart.velocity_at(r);
    let closing = dot(v, normal);
    if closing >= 0.0 {
        return;
    }
    let along = (-normal.1, normal.0);
    let (bend, slew) = (dot(swept(r), normal), dot(swept(r), along));
    let push = -(1.0 + WALL_RESTITUTION) * closing / (1.0 + bend * bend / YAW_INERTIA);
    let scrape =
        (-dot(v, along) / (1.0 + slew * slew / YAW_INERTIA)).clamp(-WALL_FRICTION * push, WALL_FRICTION * push);
    kart.take((normal.0 * push + along.0 * scrape, normal.1 * push + along.1 * scrape), r);
}

/// The parameters along two spines, `a + s·u` and `b + t·v` with `s` and `t`
/// in `−KART_REACH…KART_REACH`, of their closest points.
fn closest(a: (f32, f32), u: (f32, f32), b: (f32, f32), v: (f32, f32)) -> (f32, f32) {
    let w = (a.0 - b.0, a.1 - b.1);
    let (cos, d, e) = (dot(u, v), dot(u, w), dot(v, w));
    let parallel = 1.0 - cos * cos;
    let s = if parallel > 1e-6 { ((cos * e - d) / parallel).clamp(-KART_REACH, KART_REACH) } else { 0.0 };
    let t = (cos * s + e).clamp(-KART_REACH, KART_REACH);
    ((cos * t - d).clamp(-KART_REACH, KART_REACH), t)
}

/// Karts collide: two overlapping outlines are pushed apart, and the karts
/// exchange an impulse where they touch — momentum kept, a little of the
/// closing speed given back, bodywork and tyres rubbing — so that a knock
/// pushes, slows and spins each kart by where it lands.
pub fn bump(a: &mut Kart, b: &mut Kart) {
    let ((fa, _), (fb, _)) = (axes(a.heading), axes(b.heading));
    let (s, t) = closest((a.x, a.z), fa, (b.x, b.z), fb);
    let (pa, pb) = ((a.x + fa.0 * s, a.z + fa.1 * s), (b.x + fb.0 * t, b.z + fb.1 * t));
    let gap = (pb.0 - pa.0, pb.1 - pa.1);
    let distance = sqrtf(dot(gap, gap));
    let overlap = 2.0 * KART_RADIUS - distance;
    if overlap <= 0.0 {
        return;
    }
    let normal = if distance > 1e-4 { (gap.0 / distance, gap.1 / distance) } else { (fa.1, -fa.0) };
    let push = overlap / 2.0;
    (a.x, a.z) = (a.x - normal.0 * push, a.z - normal.1 * push);
    (b.x, b.z) = (b.x + normal.0 * push, b.z + normal.1 * push);
    // Where they touch, from each centre.
    let contact = ((pa.0 + pb.0) / 2.0, (pa.1 + pb.1) / 2.0);
    let (ra, rb) = ((contact.0 - a.x, contact.1 - a.z), (contact.0 - b.x, contact.1 - b.z));
    let (va, vb) = (a.velocity_at(ra), b.velocity_at(rb));
    let relative = (vb.0 - va.0, vb.1 - va.1);
    let closing = dot(relative, normal);
    if closing >= 0.0 {
        return;
    }
    let along = (-normal.1, normal.0);
    // Weights as shares of the standard kart's: the impulse is shared out
    // by mass, and each moment of inertia grows with its mass.
    let (ma, mb) = (a.build.sanitized().weight, b.build.sanitized().weight);
    let resistance = |n: (f32, f32)| {
        let (ka, kb) = (dot(swept(ra), n), dot(swept(rb), n));
        (1.0 / ma + 1.0 / mb) + (ka * ka / ma + kb * kb / mb) / YAW_INERTIA
    };
    let push = -(1.0 + KART_RESTITUTION) * closing / resistance(normal);
    let rub = (-dot(relative, along) / resistance(along)).clamp(-KART_FRICTION * push, KART_FRICTION * push);
    let j = (normal.0 * push + along.0 * rub, normal.1 * push + along.1 * rub);
    a.take((-j.0 / ma, -j.1 / ma), ra);
    b.take((j.0 / mb, j.1 / mb), rb);
}

/// A loose body — a thrown tyre, a cone — meets a kart: `at` its centre,
/// `velocity` its own, `radius` its size, `weight` as a share of a standard
/// kart's, `restitution` what it gives back of the closing speed. If it
/// touches the kart's outline closing on it, the two exchange an impulse
/// where they touch, as two karts do ([`bump`]): the kart pushed and spun by
/// where the body lands, the body sent off. The body's change of velocity,
/// and where it now is (out of the kart's outline); none if they do not meet.
pub fn struck(
    kart: &mut Kart,
    at: (f32, f32),
    velocity: (f32, f32),
    radius: f32,
    weight: f32,
    restitution: f32,
) -> Option<((f32, f32), (f32, f32))> {
    let (forward, _) = axes(kart.heading);
    let along = ((at.0 - kart.x) * forward.0 + (at.1 - kart.z) * forward.1).clamp(-KART_REACH, KART_REACH);
    let spine = (kart.x + forward.0 * along, kart.z + forward.1 * along);
    let gap = (at.0 - spine.0, at.1 - spine.1);
    let distance = sqrtf(dot(gap, gap));
    if distance >= KART_RADIUS + radius {
        return None;
    }
    // From the kart to the body; dead on the spine, out to the side.
    let normal = if distance > 1e-4 { (gap.0 / distance, gap.1 / distance) } else { (forward.1, -forward.0) };
    let out = (spine.0 + normal.0 * (KART_RADIUS + radius), spine.1 + normal.1 * (KART_RADIUS + radius));
    let r = (spine.0 + normal.0 * KART_RADIUS - kart.x, spine.1 + normal.1 * KART_RADIUS - kart.z);
    let mine = kart.velocity_at(r);
    let relative = (velocity.0 - mine.0, velocity.1 - mine.1);
    let closing = dot(relative, normal);
    if closing >= 0.0 {
        return Some(((0.0, 0.0), out));
    }
    let side = (-normal.1, normal.0);
    let (mk, mb) = (kart.build.sanitized().weight, weight.max(1e-3));
    let resistance = |n: (f32, f32)| {
        let k = dot(swept(r), n);
        1.0 / mk + 1.0 / mb + k * k / mk / YAW_INERTIA
    };
    let push = -(1.0 + restitution) * closing / resistance(normal);
    let rub = (-dot(relative, side) / resistance(side)).clamp(-KART_FRICTION * push, KART_FRICTION * push);
    let j = (normal.0 * push + side.0 * rub, normal.1 * push + side.1 * rub);
    kart.take((-j.0 / mk, -j.1 / mk), r);
    Some(((j.0 / mb, j.1 / mb), out))
}

/// An angle in `(-π, π]`.
pub fn wrap(angle: f32) -> f32 {
    atan2f(sinf(angle), cosf(angle))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 200 m × 100 m oval-ish rectangle, driven clockwise seen from above
    /// with the start on the long side heading +Z.
    fn track() -> Track {
        Track::new(vec![[0.0, 0.0], [0.0, 200.0], [-100.0, 200.0], [-100.0, 0.0]], 6.0, 3.0)
    }

    fn drive(kart: &mut Kart, controls: Controls, seconds: f32, track: &Track) {
        for _ in 0..(seconds / DT) as usize {
            step(kart, controls, track);
        }
    }

    const FULL: Controls = Controls { throttle: 1.0, steer: 0.0, drift: false };

    #[test]
    fn a_body_that_strikes_a_kart_keeps_their_momentum_and_leaves_it() {
        let mut kart = Kart { z: 10.0, speed: 12.0, ..Kart::default() };
        let before = kart.velocity();
        let (v, weight) = ((-18.0, 3.0), 0.25);
        let (change, out) = struck(&mut kart, (0.8, 10.4), v, 0.35, weight, 0.6).expect("they meet");
        let after = kart.velocity();
        // The kart's momentum lost is the body's gained (weights as shares).
        let (dk, db) = ((after.0 - before.0, after.1 - before.1), (change.0 * weight, change.1 * weight));
        assert!(fabsf(dk.0 + db.0) < 1e-4 && fabsf(dk.1 + db.1) < 1e-4, "{dk:?} {db:?}");
        // Pushed away from it, spun by where it landed, and the body outside.
        assert!(after.0 < before.0 && kart.spin != 0.0);
        assert!(struck(&mut kart.clone(), out, (0.0, 0.0), 0.34, weight, 0.6).is_none());
        // A body going away, or too far, does nothing.
        let mut still = Kart { z: 10.0, ..Kart::default() };
        assert_eq!(struck(&mut still, (0.8, 10.0), (5.0, 0.0), 0.35, 0.25, 0.6), Some(((0.0, 0.0), (1.0, 10.0))));
        assert!(struck(&mut still, (3.0, 10.0), (-5.0, 0.0), 0.35, 0.25, 0.6).is_none());
        assert_eq!(still.velocity(), (0.0, 0.0));
    }

    /// A kart built otherwise: one share changed.
    fn built(change: impl Fn(&mut Build)) -> Kart {
        let mut build = Build::STANDARD;
        change(&mut build);
        Kart { z: 10.0, build, ..Kart::default() }
    }

    #[test]
    fn a_build_sets_top_speed_acceleration_handling_and_grip() {
        let track = track();
        // Top speed: 10 % more.
        let mut fast = built(|b| b.top = 1.1);
        drive(&mut fast, FULL, 4.0, &track);
        assert!((fast.speed - 1.1 * MAX_SPEED).abs() < 0.05, "{fast:?}");
        // Acceleration: to 15 m/s sooner.
        let time_to = |mut kart: Kart| {
            (1..600).find(|_| {
                step(&mut kart, FULL, &track);
                kart.speed >= 15.0
            })
        };
        let (quick, standard) = (time_to(built(|b| b.accel = 1.3)), time_to(built(|_| {})));
        assert!(quick < standard, "{quick:?} {standard:?}");
        // Handling: turns further in a second.
        let turned = |mut kart: Kart| {
            kart.speed = 15.0;
            drive(&mut kart, Controls { steer: 1.0, ..FULL }, 0.5, &track);
            kart.heading.abs()
        };
        assert!(turned(built(|b| b.handling = 1.2)) > 1.1 * turned(built(|_| {})));
        // Grip: a slide is scrubbed off sooner.
        let scrubbed = |mut kart: Kart| {
            (kart.speed, kart.slide) = (15.0, 4.0);
            (1..600).find(|_| {
                step(&mut kart, FULL, &track);
                kart.slide.abs() < 0.2
            })
        };
        assert!(scrubbed(built(|b| b.grip = 1.3)) < scrubbed(built(|_| {})));
        // Outside reason, a build is brought back: no 3× top speed.
        let mut cheat = built(|b| b.top = 3.0);
        drive(&mut cheat, FULL, 4.0, &track);
        assert!(cheat.speed < 1.16 * MAX_SPEED, "{cheat:?}");
    }

    #[test]
    fn a_heavy_kart_is_moved_less_and_momentum_is_kept() {
        let mut heavy =
            Kart { x: 0.0, z: 10.0, speed: 0.0, build: Build { weight: 1.3, ..Build::STANDARD }, ..Kart::default() };
        let mut light = Kart {
            x: 1.2,
            z: 10.0,
            heading: std::f32::consts::FRAC_PI_2,
            speed: -6.0,
            build: Build { weight: 0.8, ..Build::STANDARD },
            ..Kart::default()
        };
        let momentum = |a: &Kart, b: &Kart| {
            let (va, vb) = (a.velocity(), b.velocity());
            (a.build.weight * va.0 + b.build.weight * vb.0, a.build.weight * va.1 + b.build.weight * vb.1)
        };
        let (before_h, before_l) = (heavy.velocity(), light.velocity());
        let before = momentum(&heavy, &light);
        bump(&mut heavy, &mut light);
        let after = momentum(&heavy, &light);
        assert!((before.0 - after.0).abs() < 1e-3 && (before.1 - after.1).abs() < 1e-3, "{before:?} {after:?}");
        let change = |a: (f32, f32), b: (f32, f32)| sqrtf((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2));
        let (dh, dl) = (change(before_h, heavy.velocity()), change(before_l, light.velocity()));
        assert!(dh > 0.1 && dl > 1.5 * dh, "the light one moves more: {dh} {dl}");
    }

    #[test]
    fn full_throttle_reaches_top_speed_on_a_straight() {
        let (track, mut kart) = (track(), Kart::default());
        drive(&mut kart, FULL, 3.0, &track);
        assert!((kart.speed - MAX_SPEED).abs() < 0.01, "{kart:?}");
        assert!(kart.z > 30.0 && kart.x.abs() < 0.01, "straight ahead: {kart:?}");
        assert!((track.locate(kart.x, kart.z).along - kart.z).abs() < 0.01);
    }

    #[test]
    fn steering_right_turns_right() {
        let (track, mut kart) = (track(), Kart { speed: 15.0, ..Kart::default() });
        drive(&mut kart, Controls { steer: 1.0, ..FULL }, 0.3, &track);
        assert!(kart.heading < 0.0 && kart.x < 0.0, "facing +Z, right is -X: {kart:?}");
    }

    #[test]
    fn a_long_drift_gives_a_long_mini_turbo() {
        let wide = Track { half_width: 60.0, ..track() }; // no wall within a drift's circle
        let (track, mut kart) = (wide, Kart { speed: 20.0, ..Kart::default() });
        let drifting = Controls { steer: 1.0, drift: true, ..FULL };
        step(&mut kart, drifting, &track);
        assert_eq!(kart.drift, 1.0);
        drive(&mut kart, Controls { steer: 0.0, ..drifting }, TURBO_CHARGES[2] + 0.1, &track);
        assert!(kart.slide < 0.0, "a right drift slides outwards, to the left: {kart:?}");
        assert_eq!(turbo_level(kart.charge), 3);
        step(&mut kart, FULL, &track);
        assert!(kart.boost > TURBO_BOOSTS[2] - 0.05 && kart.drift == 0.0, "{kart:?}");
        drive(&mut kart, FULL, 0.5, &track);
        assert!(kart.speed > MAX_SPEED, "boosted: {kart:?}");
    }

    #[test]
    fn each_level_of_drift_gives_a_longer_boost() {
        assert_eq!(turbo_level(0.0), 0);
        assert_eq!(turbo_level(TURBO_CHARGES[0]), 1);
        assert_eq!(turbo_level(TURBO_CHARGES[1] + 0.01), 2);
        let wide = Track { half_width: 60.0, ..track() };
        let mut boosts = Vec::new();
        for seconds in [0.3, TURBO_CHARGES[0] + 0.05, TURBO_CHARGES[1] + 0.05, TURBO_CHARGES[2] + 0.05] {
            let mut kart = Kart { speed: 20.0, ..Kart::default() };
            drive(&mut kart, Controls { steer: 1.0, drift: true, ..FULL }, DT, &wide);
            drive(&mut kart, Controls { steer: 0.0, drift: true, ..FULL }, seconds, &wide);
            step(&mut kart, FULL, &wide);
            boosts.push(kart.boost);
        }
        assert_eq!(boosts[0], 0.0, "too short a drift: {boosts:?}");
        assert!(boosts.windows(2).all(|w| w[1] > w[0]), "{boosts:?}");
    }

    /// The loop with a crest on its first straight: 12 m up to 1.4 m, 6 m down.
    fn crested() -> Track {
        track().with_crests(vec![Crest { along: 40.0, rise: 12.0, fall: 6.0, height: 1.4 }])
    }

    #[test]
    fn a_crest_is_smooth_and_closed() {
        let track = crested();
        assert_eq!(track.lift(40.0), (0.0, 0.0));
        let (top, flat) = track.lift(52.0);
        assert!((top - 1.4).abs() < 1e-5 && flat.abs() < 1e-5, "{top} {flat}");
        assert!(track.lift(58.0).0.abs() < 1e-5 && track.lift(58.5) == (0.0, 0.0));
        // No step anywhere: a centimetre along, the road rises a few millimetres at most.
        for i in 0..250 {
            let along = 38.0 + i as f32 * 0.1;
            let rise = (track.lift(along + 0.01).0 - track.lift(along).0).abs();
            assert!(rise < 0.004, "{rise} at {along}");
        }
    }

    /// Steps in the air and the most clearance over the road, for a kart
    /// arriving at the crest at `speed` with full throttle.
    fn over_the_crest(speed: f32) -> (u32, f32, Kart) {
        let track = crested();
        let mut kart = Kart { z: 20.0, speed, ..Kart::default() };
        let (mut airborne, mut clearance) = (0, 0.0_f32);
        for _ in 0..150 {
            step(&mut kart, Controls { throttle: if speed < MAX_SPEED { 0.0 } else { 1.0 }, ..FULL }, &track);
            let road = track.lift(track.locate(kart.x, kart.z).along).0;
            if kart.height > road + 1e-3 {
                airborne += 1;
                clearance = clearance.max(kart.height - road);
            }
        }
        (airborne, clearance, kart)
    }

    #[test]
    fn a_crest_taken_flat_out_lifts_the_kart_and_a_slow_one_stays_down() {
        let (airborne, clearance, kart) = over_the_crest(MAX_SPEED);
        assert!((28..45).contains(&airborne), "{airborne} steps in the air");
        assert!((0.6..1.4).contains(&clearance), "{clearance} m under the wheels");
        assert!(kart.height == 0.0 && kart.climb == 0.0, "landed: {kart:?}");
        assert_eq!(kart.boost, 0.0, "no trick, no boost");
        // No drive in the air: the speed it took off with, it keeps.
        assert!((kart.speed - MAX_SPEED).abs() < 0.5, "{kart:?}");
        // Slower than the crest's curve asks (about 18 m/s), the road holds it.
        for speed in [8.0, 12.0, 16.0] {
            assert_eq!(over_the_crest(speed).0, 0, "at {speed} m/s");
        }
        // Reversing over it, too: the road carries it either way.
        let track = crested();
        let mut kart = Kart { z: 60.0, speed: -6.0, ..Kart::default() };
        for _ in 0..240 {
            step(&mut kart, Controls { throttle: -1.0, ..FULL }, &track);
            let road = track.lift(track.locate(kart.x, kart.z).along).0;
            assert!(kart.height <= road + 1e-3, "{kart:?}");
        }
    }

    #[test]
    fn a_trick_in_the_air_boosts_on_landing() {
        let track = crested();
        let mut kart = Kart { z: 20.0, speed: MAX_SPEED, ..Kart::default() };
        let mut tricked = false;
        for _ in 0..120 {
            let flying = kart.height > track.lift(track.locate(kart.x, kart.z).along).0 + 1e-3;
            // The button pressed once, as the kart leaves the road.
            let press = flying && !tricked;
            tricked |= press;
            step(&mut kart, Controls { drift: press, ..FULL }, &track);
            if tricked && kart.height == 0.0 {
                break;
            }
        }
        assert!(tricked && kart.boost > TRICK_BOOST - 0.05, "{kart:?}");
        // Holding the button from before the crest is no trick: it must be pressed.
        let mut kart = Kart { z: 20.0, speed: MAX_SPEED, held: 1.0, ..Kart::default() };
        drive(&mut kart, Controls { drift: true, ..FULL }, 1.5, &track);
        assert_eq!(kart.boost, 0.0, "{kart:?}");
    }

    #[test]
    fn a_kart_in_anothers_wake_gets_a_boost() {
        let mut karts = [Kart { speed: 20.0, ..Kart::default() }, Kart { z: 5.0, speed: 20.0, ..Kart::default() }];
        let mut boosted = None;
        for i in 0..120 {
            slipstream(&mut karts);
            if karts[0].boost > 0.0 {
                boosted = Some(i);
                break;
            }
        }
        let steps = boosted.expect("a boost in the wake");
        assert!((steps as f32 * DT - DRAFT_CHARGE).abs() < 0.05, "{steps}");
        assert_eq!(karts[1].boost, 0.0, "the leader gets none");
        // Beside it, no wake.
        let mut karts =
            [Kart { speed: 20.0, ..Kart::default() }, Kart { x: 3.0, z: 5.0, speed: 20.0, ..Kart::default() }];
        for _ in 0..120 {
            slipstream(&mut karts);
        }
        assert_eq!(karts[0].boost + karts[0].draft, 0.0);
    }

    #[test]
    fn a_start_in_time_boosts_and_one_too_early_stalls() {
        let track = track();
        let mut good = Kart::default();
        launch(&mut good, Some(0.2));
        let mut early = Kart::default();
        launch(&mut early, Some(2.0));
        let mut plain = Kart::default();
        launch(&mut plain, Some(0.8));
        for kart in [&mut good, &mut early, &mut plain] {
            drive(kart, FULL, 1.0, &track);
        }
        assert!(good.speed > plain.speed + 3.0, "{good:?} {plain:?}");
        assert!(early.speed < plain.speed - 5.0, "{early:?} {plain:?}");
    }

    #[test]
    fn a_rocket_goes_faster_than_a_boost_and_runs_out() {
        let (track, mut kart) = (track(), Kart { speed: MAX_SPEED, rocket: 1.5, ..Kart::default() });
        drive(&mut kart, FULL, 1.4, &track);
        assert!(kart.speed > BOOST_SPEED + 3.0 && kart.speed <= ROCKET_SPEED, "{kart:?}");
        drive(&mut kart, FULL, 3.0, &track);
        assert!(kart.rocket == 0.0 && kart.speed < MAX_SPEED + 0.5, "back to top speed: {kart:?}");
    }

    /// D-068: on a wet road the brakes take longer and a slide lasts.
    #[test]
    fn a_wet_road_brakes_later_and_slides_longer() {
        let dry = track();
        let wet = track().with_wet(1.0);
        let stop = |track: &Track| {
            let mut kart = Kart { z: 10.0, speed: MAX_SPEED, ..Kart::default() };
            let brake = Controls { throttle: -1.0, ..FULL };
            let mut steps = 0;
            while kart.speed > 0.0 {
                step(&mut kart, brake, track);
                steps += 1;
            }
            (steps, kart.z)
        };
        let ((dry_steps, dry_z), (wet_steps, wet_z)) = (stop(&dry), stop(&wet));
        assert!(wet_steps > dry_steps && wet_z > dry_z + 2.0, "dry {dry_steps} {dry_z}, wet {wet_steps} {wet_z}");
        let slid = |track: &Track| {
            let mut kart = Kart { z: 10.0, speed: MAX_SPEED, slide: 6.0, ..Kart::default() };
            drive(&mut kart, FULL, 0.3, track);
            kart.slide
        };
        assert!(slid(&wet) > slid(&dry) * 2.0, "dry {}, wet {}", slid(&dry), slid(&wet));
        assert_eq!(track().with_wet(f32::NAN).wet, 0.0);
        assert_eq!(track().with_wet(3.0).wet, 1.0);
    }

    /// D-077: over hills the road rises and falls under the kart, which
    /// is slower uphill and faster downhill; a level track is as before.
    #[test]
    fn hills_carry_the_kart_and_change_its_speed() {
        // The rectangle's first side climbs 20 m over its 200 m, the third comes down.
        let hilly = track().with_heights(vec![0.0, 20.0, 20.0, 0.0]);
        assert_eq!(hilly.hill(100.0), (10.0, 0.1));
        assert_eq!(hilly.lift(250.0), (20.0, 0.0));
        assert_eq!(track().with_heights(vec![0.0; 4]).heights().len(), 0, "all level: no hills");
        assert_eq!(track().with_heights(vec![1.0]).heights().len(), 0, "not one a point: none");
        let run = |track: &Track, z: f32, heading: f32| {
            let mut kart = Kart { z, heading, speed: MAX_SPEED, height: track.lift(z).0, ..Kart::default() };
            drive(&mut kart, FULL, 3.0, track);
            kart
        };
        let (level, up) = (run(&track(), 20.0, 0.0), run(&hilly, 20.0, 0.0));
        assert!(up.speed < level.speed - 1.5, "slower uphill: {} against {}", up.speed, level.speed);
        let spot = hilly.locate(up.x, up.z);
        assert!((up.height - hilly.lift(spot.along).0).abs() < 1e-3 && up.height > 5.0, "on the road: {up:?}");
        // Down the same slope, the other way.
        let down = run(&hilly, 150.0, std::f32::consts::PI);
        assert!(down.speed > level.speed + 1.5, "faster downhill: {}", down.speed);
        assert!(down.height < 12.0, "{down:?}");
    }

    /// D-072: on oily tyres the kart slides on the way it went and the
    /// wheel barely turns it; once they are clean, it grips again.
    #[test]
    fn oily_tyres_slide_on_and_hardly_steer() {
        let track = Track { half_width: 60.0, ..track() };
        let turn = Controls { throttle: 1.0, steer: 1.0, drift: false };
        let run = |slick: f32| {
            let mut kart = Kart { z: 10.0, speed: 18.0, slide: 4.0, slick, ..Kart::default() };
            drive(&mut kart, turn, 0.4, &track);
            kart
        };
        let (clean, oily) = (run(0.0), run(1.0));
        assert!(oily.slide.abs() > clean.slide.abs() * 3.0, "the slide lasts: {} against {}", oily.slide, clean.slide);
        assert!(
            oily.heading.abs() < clean.heading.abs() * 0.5,
            "it hardly turns: {} against {}",
            oily.heading,
            clean.heading
        );
        assert!(oily.slick > 0.0 && oily.slick < 1.0, "the oil wears off: {}", oily.slick);
        assert_eq!(run(0.0), clean, "clean tyres: as before");
    }

    #[test]
    fn grass_slows_and_walls_stop() {
        let track = track();
        let mut kart = Kart { x: -8.5, z: 50.0, speed: MAX_SPEED, ..Kart::default() };
        drive(&mut kart, FULL, 2.0, &track);
        assert!(kart.speed < OFF_ROAD_SPEED + 1.0, "off the road: {kart:?}");

        // Head on into the barrier on the right: stopped at its face, bounced a little.
        let mut kart = Kart { z: 50.0, heading: -std::f32::consts::FRAC_PI_2, speed: 15.0, ..Kart::default() };
        drive(&mut kart, FULL, 1.0, &track);
        let face = track.half_width + track.verge + BARRIER;
        let spot = track.locate(kart.x, kart.z);
        assert!(spot.lateral + KART_RADIUS <= face + 1e-3, "the outline stops at the face: {spot:?}");
        assert!(spot.lateral + KART_RADIUS + KART_REACH > face - 0.05, "held against it: {spot:?}");
        assert!(kart.speed < 15.0 * 0.6, "head on, most of the speed is lost: {kart:?}");
    }

    #[test]
    fn a_glancing_blow_keeps_the_speed_and_turns_the_kart_along_the_barrier() {
        // Kerbs up to the barrier, no grass to slow the kart before it.
        let track = Track { half_width: 8.9, verge: 0.1, ..track() };
        // Heading +Z, turned 0.25 rad to the right, half a metre from the barrier's face.
        let face = track.half_width + track.verge + BARRIER;
        let mut kart = Kart { x: -(face - KART_RADIUS - 0.5), z: 50.0, heading: -0.25, speed: 20.0, ..Kart::default() };
        drive(&mut kart, Controls { throttle: 1.0, ..Controls::default() }, 0.6, &track);
        let spot = track.locate(kart.x, kart.z);
        assert!(spot.lateral + KART_RADIUS <= face + 1e-3, "{spot:?}");
        assert!(kart.speed > 14.0, "a glancing blow keeps most of the speed: {kart:?}");
        assert!(kart.heading > -0.1, "turned back along it: {kart:?}");
    }

    #[test]
    fn a_whole_side_in_the_grass_drags_the_kart_into_it() {
        let track = track();
        // The right wheels just past the kerb, the left ones on it: the kart
        // turns right, towards the grass, and is slower than on the road.
        let edge = track.half_width + KERB;
        let mut kart = Kart { x: -(edge + 0.1), z: 20.0, speed: MAX_SPEED, ..Kart::default() };
        let ground = wheels(&kart, &track);
        assert_eq!(ground, [Ground::Kerb, Ground::Grass, Ground::Kerb, Ground::Grass], "{kart:?}");
        drive(&mut kart, FULL, 0.25, &track);
        assert!(kart.heading < -0.05, "pulled to the right: {kart:?}");
        assert!(kart.speed < MAX_SPEED - 1.0 && kart.speed > OFF_ROAD_SPEED + 3.0, "half on the grass: {kart:?}");
        // On the kerbs alone the kart keeps its speed.
        let mut kart = Kart { x: -(track.half_width + 0.4), z: 20.0, speed: MAX_SPEED, ..Kart::default() };
        assert!(wheels(&kart, &track).iter().all(|g| *g != Ground::Grass));
        drive(&mut kart, FULL, 0.25, &track);
        assert!((kart.speed - MAX_SPEED).abs() < 0.01 && kart.heading == 0.0, "{kart:?}");
    }

    #[test]
    fn a_rear_end_knock_passes_the_speed_on_and_keeps_the_momentum() {
        let mut behind = Kart { z: 0.0, speed: 20.0, ..Kart::default() };
        let mut ahead = Kart { z: 2.0 * (KART_REACH + KART_RADIUS) - 0.05, speed: 12.0, ..Kart::default() };
        bump(&mut behind, &mut ahead);
        assert!(behind.speed < 20.0 && ahead.speed > 12.0, "{behind:?} {ahead:?}");
        assert!((behind.speed + ahead.speed - 32.0).abs() < 1e-4, "momentum kept");
        assert!(ahead.speed - behind.speed < 0.3 * 8.0 + 1e-4, "only a little given back");
        assert!(behind.spin.abs() < 1e-5 && ahead.spin.abs() < 1e-5, "straight on: no spin");
        assert!(ahead.z - behind.z >= 2.0 * (KART_REACH + KART_RADIUS) - 1e-4, "apart");
    }

    #[test]
    fn a_knock_on_the_rear_spins_the_kart_and_the_tyres_stop_it() {
        // Struck on its rear from the left (+X) by a kart crossing its path:
        // the rear goes right, the nose left.
        let mut striker =
            Kart { x: 1.25, z: -0.3, heading: -std::f32::consts::FRAC_PI_2, speed: 10.0, ..Kart::default() };
        let mut struck = Kart { speed: 10.0, ..Kart::default() };
        let energy = |k: &Kart| {
            let v = k.velocity();
            v.0 * v.0 + v.1 * v.1 + YAW_INERTIA * k.spin * k.spin
        };
        let before = energy(&striker) + energy(&struck);
        bump(&mut striker, &mut struck);
        assert!(struck.spin > 2.0, "{struck:?}");
        assert!(struck.velocity().0 < -2.0, "pushed to the right: {struck:?}");
        assert!(energy(&striker) + energy(&struck) < before, "no energy made");
        // It turns; then the tyres stop the spin and scrub the slide off.
        let wide = Track { half_width: 60.0, ..track() };
        drive(&mut struck, Controls::default(), 0.3, &wide);
        assert!(struck.heading > 0.4, "{struck:?}");
        drive(&mut struck, Controls::default(), 2.5, &wide);
        assert!(struck.spin == 0.0 && struck.slide.abs() < 0.05, "{struck:?}");
    }

    #[test]
    fn karts_side_by_side_rub_and_part() {
        // Two karts converging side by side at the same speed.
        let mut left = Kart { x: 1.25, speed: 20.0, slide: 1.0, ..Kart::default() };
        let mut right = Kart { x: 0.0, speed: 20.0, slide: -1.0, ..Kart::default() };
        bump(&mut left, &mut right);
        assert!(left.x - right.x >= 2.0 * KART_RADIUS - 1e-4, "apart");
        assert!(left.slide < 0.0 && right.slide > 0.0, "they part: {left:?} {right:?}");
        assert!((left.speed - 20.0).abs() < 0.5 && (right.speed - 20.0).abs() < 0.5, "and race on");
    }

    #[test]
    fn hostile_controls_are_harmless() {
        let (track, mut kart) = (track(), Kart::default());
        let nasty = Controls { throttle: f32::NAN, steer: f32::INFINITY, drift: true };
        drive(&mut kart, nasty, 1.0, &track);
        assert!([kart.x, kart.z, kart.heading, kart.speed].iter().all(|v| v.is_finite()), "{kart:?}");
        drive(&mut kart, Controls { throttle: 50.0, ..FULL }, 3.0, &track);
        assert!(kart.speed <= MAX_SPEED + 0.01);
    }

    #[test]
    fn the_track_knows_where_things_are() {
        let track = track();
        assert_eq!(track.length(), 600.0);
        let (point, heading) = track.at(250.0);
        assert_eq!(point, [-50.0, 200.0]);
        assert!((heading + std::f32::consts::FRAC_PI_2).abs() < 1e-6, "heading -X: {heading}");
        assert_eq!(track.at(650.0).0, [0.0, 50.0], "the circuit is closed");
        let spot = track.locate(3.0, 20.0);
        assert!((spot.along - 20.0).abs() < 1e-4 && (spot.lateral + 3.0).abs() < 1e-4, "{spot:?}");
    }
}
