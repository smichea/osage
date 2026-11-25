// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;
use bevy::math::DVec3;

/// First intersection of the expanding direct wave with the camera's piecewise linear path.
pub(super) fn receiver(route: &Route, hz: u32, emission: u64, source: Vec3) -> (u64, Head, f64) {
    let hz = f64::from(hz);
    let t_emit = emission as f64 / f64::from(RATE);
    let source = source.as_dvec3();
    let first = route.camera.partition_point(|k| f64::from(k.tick) / hz <= t_emit).saturating_sub(1);
    for pair in route.camera[first..].windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let ta = f64::from(a.tick) / hz;
        let tb = f64::from(b.tick) / hz;
        let start = t_emit.max(ta);
        let span = tb - start;
        let velocity = (DVec3::from(b.eye.map(f64::from)) - DVec3::from(a.eye.map(f64::from))) / (tb - ta);
        let delta = DVec3::from(a.eye.map(f64::from)) + velocity * (start - ta) - source;
        let elapsed = start - t_emit;
        let outside = |t: f64| (delta + velocity * t).length_squared() - (SOUND * (elapsed + t)).powi(2);
        let a2 = velocity.length_squared() - SOUND * SOUND;
        // A faster listener can enter and then leave the sphere within a single segment.
        let last = if a2 > 0. { ((SOUND * SOUND * elapsed - delta.dot(velocity)) / a2).clamp(0., span) } else { span };
        if outside(last) > 0. {
            continue;
        }
        let (mut lo, mut hi) = (0., last);
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            if outside(mid) > 0. {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let at = start + hi;
        let fraction = ((at - ta) / (tb - ta)).clamp(0., 1.);
        let eye = source + delta + velocity * hi;
        let aim = DVec3::from(a.aim.map(f64::from)).lerp(DVec3::from(b.aim.map(f64::from)), fraction);
        return heard_pose(at, eye, aim, source);
    }
    let key = route.camera.last().expect("validated camera path");
    let eye = DVec3::from(key.eye.map(f64::from));
    heard_pose(t_emit + eye.distance(source) / SOUND, eye, DVec3::from(key.aim.map(f64::from)), source)
}

fn head_at(eye: DVec3, aim: DVec3) -> Head {
    let forward = (aim - eye).normalize_or(DVec3::NEG_Z);
    let right = forward.cross(DVec3::Y).normalize_or(DVec3::X);
    let v = |p: DVec3| vec3(p.x, p.y, p.z);
    Head { at: v(eye), forward: v(forward), right: v(right), up: v(right.cross(forward)) }
}

fn heard_pose(t: f64, eye: DVec3, aim: DVec3, source: DVec3) -> (u64, Head, f64) {
    ((t * f64::from(RATE)).round() as u64, head_at(eye, aim), eye.distance(source))
}

pub(super) struct EdgeArrival {
    pub frame: u64,
    pub head: Head,
    pub first_bend: Option<DVec3>,
    pub bend: DVec3,
    pub legs: Vec<f64>,
}

struct Segment {
    start: DVec3,
    along: DVec3,
    length: f64,
}

impl Segment {
    fn new(edge: &crate::object::AcousticEdge) -> Option<Self> {
        let start = DVec3::from(edge.a);
        let delta = DVec3::from(edge.b) - start;
        Some(Self { start, along: delta.try_normalize()?, length: delta.length() })
    }

    fn projection(&self, point: DVec3) -> (f64, f64) {
        let p = point - self.start;
        let axis = p.dot(self.along);
        (axis, (p - self.along * axis).length())
    }

    // Unfold the path in the plane of its two radii and clamp it to the finite segment.
    fn bend(&self, (s, r): (f64, f64), (t, q): (f64, f64)) -> DVec3 {
        let x = if r + q > 1e-12 { s + (t - s) * (r / (r + q)) } else { 0.5 * (s + t) };
        self.start + self.along * x.clamp(0., self.length)
    }
}

struct EdgeRay {
    source: DVec3,
    first: Segment,
    source_projection: (f64, f64),
    next: Option<Segment>,
}

struct Path {
    first: Option<DVec3>,
    last: DVec3,
    distance: f64,
}

impl EdgeRay {
    fn path(&self, eye: DVec3) -> Path {
        let Some(next) = &self.next else {
            let p = self.first.bend(self.source_projection, self.first.projection(eye));
            return Path { first: None, last: p, distance: self.source.distance(p) + p.distance(eye) };
        };
        let eye_projection = next.projection(eye);
        let points = |t: f64| {
            let p = self.first.start + self.first.along * t;
            let q = next.bend(next.projection(p), eye_projection);
            (p, q)
        };
        // Minimize over the second edge analytically. The remaining distance on the
        // first segment is convex. Its directional derivative brackets the minimum.
        let slope = |p: DVec3, q: DVec3| {
            self.first.along.dot((p - self.source).normalize_or_zero() + (p - q).normalize_or_zero())
        };
        let (a, b) = points(0.);
        let (p, q) = if slope(a, b) >= 0. {
            (a, b)
        } else {
            let (a, b) = points(self.first.length);
            if slope(a, b) <= 0. {
                (a, b)
            } else {
                let (mut lo, mut hi) = (0., self.first.length);
                for _ in 0..44 {
                    let mid = 0.5 * (lo + hi);
                    let (a, b) = points(mid);
                    if slope(a, b) < 0. { lo = mid } else { hi = mid }
                }
                points(0.5 * (lo + hi))
            }
        };
        Path { first: Some(p), last: q, distance: self.source.distance(p) + p.distance(q) + q.distance(eye) }
    }

    fn receive(&self, seconds: f64, eye: DVec3, aim: DVec3) -> EdgeArrival {
        let path = self.path(eye);
        let mut legs = Vec::with_capacity(3);
        let mut start = self.source;
        if let Some(p) = path.first {
            legs.push(start.distance(p));
            start = p;
        }
        legs.extend([start.distance(path.last), path.last.distance(eye)]);
        EdgeArrival {
            frame: (seconds * f64::from(RATE)).round() as u64,
            head: head_at(eye, aim),
            first_bend: path.first,
            bend: path.last,
            legs,
        }
    }
}

/// Earliest encounter with a wave constrained to one or two stationary finite edges.
pub(super) fn via_edges(
    route: &Route,
    hz: u32,
    emission: u64,
    source: DVec3,
    edge: &crate::object::AcousticEdge,
    next: Option<&crate::object::AcousticEdge>,
) -> Option<EdgeArrival> {
    let first = Segment::new(edge)?;
    let next = match next {
        Some(edge) => Some(Segment::new(edge)?),
        None => None,
    };
    let ray = EdgeRay { source, source_projection: first.projection(source), first, next };
    let hz = f64::from(hz);
    let emit = emission as f64 / f64::from(RATE);
    let first = route.camera.partition_point(|k| f64::from(k.tick) / hz <= emit).saturating_sub(1);
    for pair in route.camera[first..].windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let (ta, tb) = (f64::from(a.tick) / hz, f64::from(b.tick) / hz);
        let start = emit.max(ta);
        let span = tb - start;
        let origin = Vec3::from(a.eye).as_dvec3();
        let velocity = (Vec3::from(b.eye).as_dvec3() - origin) / (tb - ta);
        let eye_start = origin + velocity * (start - ta);
        let aim_at = |t: f64| {
            Vec3::from(a.aim).as_dvec3().lerp(Vec3::from(b.aim).as_dvec3(), ((t - ta) / (tb - ta)).clamp(0., 1.))
        };
        if velocity == DVec3::ZERO {
            let t = emit + ray.path(eye_start).distance / SOUND;
            if t >= start && t <= tb {
                return Some(ray.receive(t, eye_start, aim_at(t)));
            }
            continue;
        }
        let outside = |t: f64| ray.path(eye_start + velocity * t).distance - SOUND * (start + t - emit);
        let mut end = span;
        if outside(span) > 0. {
            if velocity.length_squared() <= SOUND * SOUND {
                continue;
            }
            // The minimized sum of distances is convex on a linear receiver segment.
            // A supersonic observer may enter and leave before this segment ends.
            let (mut l, mut r) = (0., span);
            for _ in 0..70 {
                let (u, v) = (l + (r - l) / 3., r - (r - l) / 3.);
                if outside(u) <= outside(v) {
                    r = v;
                } else {
                    l = u;
                }
            }
            end = 0.5 * (l + r);
            if outside(end) > 1e-10 {
                continue;
            }
        }
        let mut lo = 0.;
        for _ in 0..48 {
            let mid = 0.5 * (lo + end);
            if outside(mid) > 0. {
                lo = mid;
            } else {
                end = mid;
            }
        }
        let t = start + end;
        return Some(ray.receive(t, eye_start + velocity * end, aim_at(t)));
    }
    let key = route.camera.last().expect("validated camera keys");
    let eye = Vec3::from(key.eye).as_dvec3();
    Some(ray.receive(emit + ray.path(eye).distance / SOUND, eye, Vec3::from(key.aim).as_dvec3()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{replay::Key, scene::Sector};

    #[test]
    fn a_listener_can_cross_the_entire_wave_between_two_camera_keys() {
        let scene = Sector::read(crate::scene::DEFAULT).unwrap();
        let mut replay = Replay::read(crate::replay::DEFAULT, scene.colliders.iter().map(|s| s.id.as_str())).unwrap();
        let r = &mut replay.routes[0];
        r.ticks = 60;
        let key = |tick, x| Key { tick, eye: [x, 0., 0.], aim: [x, 0., -1.] };
        r.camera = vec![key(0, -100.), key(60, 900.)];
        let (arrival, _, d) = receiver(r, 60, 0, Vec3::ZERO);
        let time = 100. / (1000. + SOUND);
        assert_eq!(arrival, (time * f64::from(RATE)).round() as u64);
        assert!((d - SOUND * time).abs() < 1e-7);
        // On another path the wave reaches a stationary receiver after the last key.
        r.camera = vec![key(0, 500.), key(60, 900.)];
        assert_eq!(receiver(r, 60, 0, Vec3::ZERO).0, (900. / SOUND * f64::from(RATE)).round() as u64);
    }

    #[test]
    fn bent_wave_can_intercept_a_fast_receiver_before_it_leaves_the_edge_wavefront() {
        let scene = Sector::read(crate::scene::DEFAULT).unwrap();
        let mut replay = Replay::read(crate::replay::DEFAULT, scene.colliders.iter().map(|s| s.id.as_str())).unwrap();
        let r = &mut replay.routes[0];
        r.ticks = 60;
        let key = |tick, x| Key { tick, eye: [x, 0., 0.], aim: [x, 0., -1.] };
        r.camera = vec![key(0, -100.), key(60, 900.)];
        let edge = crate::object::AcousticEdge {
            a: [5., 0., -1.],
            b: [5., 0., 1.],
            wedge: None,
            id: "edge".into(),
            owner_path: vec![],
        };
        let hit = via_edges(r, 60, 0, DVec3::ZERO, &edge, None).unwrap();
        let time = 110. / (1000. + SOUND);
        assert_eq!(hit.frame, (time * f64::from(RATE)).round() as u64);
        assert_eq!(hit.bend, DVec3::new(5., 0., 0.));
        assert!((hit.legs.iter().sum::<f64>() - time * SOUND).abs() < 1e-7);
        r.camera = vec![key(0, 500.), key(60, 900.)];
        let hit = via_edges(r, 60, 0, DVec3::ZERO, &edge, None).unwrap();
        assert_eq!(hit.frame, (900. / SOUND * f64::from(RATE)).round() as u64);
    }
}
