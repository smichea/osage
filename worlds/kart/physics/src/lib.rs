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
const TURN_RATE: f32 = 2.0;
/// Speed under which steering weakens, m/s.
const FULL_STEER_SPEED: f32 = 8.0;
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
/// Speed kept after hitting a wall head-on; a glancing blow keeps more.
const WALL_BOUNCE: f32 = 0.2;

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

/// Advances `kart` by one step of [`DT`] seconds.
pub fn step(kart: &mut Kart, controls: Controls, track: &Track) {
    let Controls { throttle, steer, drift } = controls.sanitized();
    let on_road = fabsf(track.locate(kart.x, kart.z).lateral) <= track.half_width;
    let top = if kart.boost > 0.0 {
        BOOST_SPEED
    } else if on_road {
        MAX_SPEED
    } else {
        OFF_ROAD_SPEED
    };

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
    kart.slide += (target_slide - kart.slide) * (grip * DT).min(1.0);
    kart.heading = wrap(kart.heading);

    let (forward, right) = ((sinf(kart.heading), cosf(kart.heading)), (-cosf(kart.heading), sinf(kart.heading)));
    kart.x += (forward.0 * kart.speed + right.0 * kart.slide) * DT;
    kart.z += (forward.1 * kart.speed + right.1 * kart.slide) * DT;

    // Walls: back onto the wall line, turned along it; the more head-on
    // the hit, the more speed is lost.
    let spot = track.locate(kart.x, kart.z);
    let limit = track.half_width + track.verge;
    if fabsf(spot.lateral) > limit {
        let (centre, road) = track.at(spot.along);
        let side = spot.lateral.signum() * limit;
        kart.x = centre[0] - cosf(road) * side;
        kart.z = centre[1] + sinf(road) * side;
        let mut off = wrap(kart.heading - road);
        if fabsf(off) > core::f32::consts::FRAC_PI_2 {
            off = wrap(off - core::f32::consts::PI); // driving the wrong way: along the wall all the same
        }
        kart.speed *= fabsf(cosf(off)).clamp(WALL_BOUNCE, 0.95);
        kart.heading = wrap(kart.heading - 0.3 * off);
        kart.slide = 0.0;
        kart.drift = 0.0;
        kart.charge = 0.0;
    }
}

/// Half the distance at which two karts touch, metres.
pub const KART_RADIUS: f32 = 0.9;

/// Karts bump: two overlapping karts are pushed apart and lose a little speed.
pub fn bump(a: &mut Kart, b: &mut Kart) {
    let (dx, dz) = (b.x - a.x, b.z - a.z);
    let distance = sqrtf(dx * dx + dz * dz);
    let overlap = 2.0 * KART_RADIUS - distance;
    if overlap <= 0.0 {
        return;
    }
    let (nx, nz) = if distance > 1e-4 { (dx / distance, dz / distance) } else { (1.0, 0.0) };
    let push = overlap / 2.0;
    (a.x, a.z) = (a.x - nx * push, a.z - nz * push);
    (b.x, b.z) = (b.x + nx * push, b.z + nz * push);
    a.speed *= 0.97;
    b.speed *= 0.97;
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
        let mut kart = Kart { x: -7.5, z: 50.0, speed: MAX_SPEED, ..Kart::default() };
        drive(&mut kart, FULL, 2.0, &track);
        assert!(kart.speed < OFF_ROAD_SPEED + 1.0, "off the road: {kart:?}");

        let mut kart = Kart { z: 50.0, heading: -std::f32::consts::FRAC_PI_2, speed: 15.0, ..Kart::default() };
        drive(&mut kart, FULL, 1.0, &track);
        let spot = track.locate(kart.x, kart.z);
        assert!((spot.lateral - (track.half_width + track.verge)).abs() < 0.01, "held at the wall: {spot:?}");
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
