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
/// Drift seconds for a short and a long mini-turbo.
pub const SHORT_TURBO_CHARGE: f32 = 0.7;
pub const LONG_TURBO_CHARGE: f32 = 1.5;
const BOOST_SPEED: f32 = 28.0;
const BOOST_ACCELERATION: f32 = 40.0;

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
/// Grass drags the wheels in it: with a side's two wheels in it, the kart
/// turns towards it by this many rad/s per m/s.
const GRASS_PULL: f32 = 0.02;

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
    /// Rate of turn a knock gave, rad/s, positive to the left (as the
    /// heading grows); the tyres stop it.
    pub spin: f32,
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
        Self { points, distances, length, half_width, verge }
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

    /// Takes an impulse `j` (per kilogram) at `r` from its centre.
    fn take(&mut self, j: (f32, f32), r: (f32, f32)) {
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
    let Controls { throttle, steer, drift } = controls.sanitized();
    // Each wheel feels its ground: grass slows the kart in proportion and
    // drags it towards the side it is on; kerbs and grass grip less.
    let ground = wheels_at(kart, track, track.locate(kart.x, kart.z));
    let count = |on: Ground, wheels: &[usize]| wheels.iter().filter(|&&i| ground[i] == on).count() as f32;
    let (grass_left, grass_right) = (count(Ground::Grass, &[0, 2]), count(Ground::Grass, &[1, 3]));
    let grass = (grass_left + grass_right) / 4.0;
    let kerb = count(Ground::Kerb, &[0, 1, 2, 3]) / 4.0;
    let fastest = if kart.boost > 0.0 { BOOST_SPEED } else { MAX_SPEED };
    let top = fastest - (fastest - OFF_ROAD_SPEED) * grass;
    let footing = 1.0 - (1.0 - KERB_GRIP) * kerb - (1.0 - GRASS_GRIP) * grass;

    // Along the kart.
    if throttle > 0.0 {
        if kart.speed < top {
            kart.speed = (kart.speed + throttle * ACCELERATION * DT).min(top);
        }
    } else if throttle < 0.0 {
        let rate = if kart.speed > 0.0 { BRAKING } else { ACCELERATION / 2.0 };
        kart.speed = (kart.speed + throttle * rate * DT).max(-REVERSE_SPEED);
    } else {
        let roll = (ROLLING * DT).min(fabsf(kart.speed));
        kart.speed -= roll * kart.speed.signum();
    }
    if kart.boost > 0.0 {
        kart.speed = (kart.speed + BOOST_ACCELERATION * DT).min(BOOST_SPEED);
        kart.boost = (kart.boost - DT).max(0.0);
    }
    if kart.speed > top {
        kart.speed -= (kart.speed - top) * (3.0 * DT).min(1.0);
    }

    // Drift: starts with the button, a direction and some speed; releasing
    // it after a long enough drift gives a mini-turbo.
    if kart.drift == 0.0 {
        if drift && fabsf(steer) > 0.2 && kart.speed > DRIFT_MIN_SPEED {
            kart.drift = steer.signum();
            kart.charge = 0.0;
        }
    } else if !drift || kart.speed < DRIFT_MIN_SPEED * 0.7 {
        if drift {
            kart.charge = 0.0; // too slow: the drift is lost
        }
        kart.boost = kart.boost.max(if kart.charge >= LONG_TURBO_CHARGE {
            1.2
        } else if kart.charge >= SHORT_TURBO_CHARGE {
            0.6
        } else {
            0.0
        });
        kart.drift = 0.0;
        kart.charge = 0.0;
    }

    // Turning: weaker at low speed, reversed when reversing.
    let steering = (fabsf(kart.speed) / FULL_STEER_SPEED).min(1.0) * kart.speed.signum();
    let (target_slide, grip) = if kart.drift != 0.0 {
        // Steering into the drift tightens it, against it widens it.
        let tightness = 1.0 + 0.45 * steer * kart.drift;
        kart.heading -= kart.drift * tightness * DRIFT_TURN_RATE * steering * DT;
        kart.charge += DT * tightness;
        (-kart.drift * kart.speed * 0.12, DRIFT_GRIP)
    } else {
        kart.heading -= steer * TURN_RATE * steering * DT;
        (0.0, GRIP)
    };
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
    let resistance = |n: (f32, f32)| {
        let (ka, kb) = (dot(swept(ra), n), dot(swept(rb), n));
        2.0 + (ka * ka + kb * kb) / YAW_INERTIA
    };
    let push = -(1.0 + KART_RESTITUTION) * closing / resistance(normal);
    let rub = (-dot(relative, along) / resistance(along)).clamp(-KART_FRICTION * push, KART_FRICTION * push);
    let j = (normal.0 * push + along.0 * rub, normal.1 * push + along.1 * rub);
    a.take((-j.0, -j.1), ra);
    b.take(j, rb);
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
        drive(&mut kart, Controls { steer: 0.0, ..drifting }, LONG_TURBO_CHARGE + 0.1, &track);
        assert!(kart.slide < 0.0, "a right drift slides outwards, to the left: {kart:?}");
        step(&mut kart, FULL, &track);
        assert!(kart.boost > 1.0 && kart.drift == 0.0, "{kart:?}");
        drive(&mut kart, FULL, 0.5, &track);
        assert!(kart.speed > MAX_SPEED, "boosted: {kart:?}");
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
