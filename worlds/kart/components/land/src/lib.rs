// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The land around the kart circuit (knowledge: land around a track): level
//! where karts drive but over the track's crests, which it carries from
//! barrier to barrier and beyond (D-051); hills beyond that fade at the edge
//! of the modelled land, a knoll and a lake in the room the circuit leaves
//! inside its loop.
//! Geometry only — how far a point is from the track, how high the ground
//! is, where it is wet — for whoever draws it, dresses it or plants it.

use bevy::prelude::*;
use osage_kart_physics::{Crest, Track};

/// Side of the square of land modelled around the circuit, metres.
pub const LAND: f32 = 900.0;
/// Highest hills, metres.
pub const HILLS: f32 = 26.0;
/// Height of the knoll inside the circuit, metres.
pub const KNOLL: f32 = 7.0;
/// Depth of the lake's bed, and the level of its water, metres.
pub const LAKE_BED: f32 = 2.4;
pub const WATER: f32 = -0.45;
/// The sea's level, metres: the road runs along the top of its cliffs (D-061).
pub const SEA: f32 = -8.0;
/// How far under the cliff tops the sea's bed lies, metres.
const SEA_BED: f32 = 16.0;
/// How far beyond the verge the barriers stand, metres: where the physics
/// stops a kart's outline.
pub use osage_kart_physics::BARRIER;

/// A point of the centre line with the unit vectors ahead and to the right.
pub struct Frame {
    pub at: Vec2,
    pub forward: Vec2,
    pub right: Vec2,
    /// Distance from the first point, along the centre line.
    pub along: f32,
    /// How sharply the road turns here, radians per metre; positive: to the left.
    pub bend: f32,
    /// The road's height here (over a crest, or 0), metres, and its slope:
    /// rise per metre along the track.
    pub height: f32,
    pub slope: f32,
    /// A crest on the stretch to the next frame, and how far past its start
    /// this frame is (negative when it starts further on).
    pub crest: Option<(Crest, f32)>,
}

impl Frame {
    /// Pieces to draw the stretch to the next frame in, `length` metres on:
    /// one where the road is level, half-metre pieces over a crest.
    pub fn pieces(&self, length: f32) -> usize {
        if self.crest.is_some() { (length / 0.5).ceil().max(1.0) as usize } else { 1 }
    }

    /// The road's height a share `t` (0..1) of the way to the next frame,
    /// `length` metres on, as the physics has it.
    pub fn height_towards(&self, t: f32, length: f32) -> f32 {
        self.crest.and_then(|(crest, into)| crest.profile(into + t * length)).map_or(0.0, |(height, _)| height)
    }
}

pub fn frames(track: &Track) -> Vec<Frame> {
    let points = track.points();
    let n = points.len();
    let at = |i: usize| Vec2::from(points[i % n]);
    let mut along = 0.0;
    (0..n)
        .map(|i| {
            let (before, here, after) = (at(i + n - 1), at(i), at(i + 1));
            let forward = (after - before).normalize_or_zero();
            let (arriving, leaving) = ((here - before).normalize_or_zero(), (after - here).normalize_or_zero());
            // Heading grows to the left: with forward (sin h, cos h), a left turn has a negative cross product.
            let bend = -arriving.perp_dot(leaving).asin() / here.distance(after).max(0.1);
            let (height, slope) = track.lift(along);
            let length = here.distance(after);
            let crest = track.crests().iter().find_map(|crest| {
                // How far past the crest's start this frame is, the nearer way round.
                let into =
                    (along - crest.along + track.length() / 2.0).rem_euclid(track.length()) - track.length() / 2.0;
                (into < crest.length() && into + length > 0.0).then_some((*crest, into))
            });
            let frame =
                Frame { at: here, forward, right: Vec2::new(-forward.y, forward.x), along, bend, height, slope, crest };
            along += here.distance(after);
            frame
        })
        .collect()
}

/// Room clear of the track: a disc.
#[derive(Clone, Copy, Debug)]
pub struct Site {
    pub centre: Vec2,
    pub radius: f32,
}

/// The land: its height and its colour at any point, how far a point is from
/// the track, and the two places the circuit leaves room for inside its loop.
#[derive(Resource, Clone)]
pub struct Land {
    centre: Vec2,
    points: Vec<Vec2>,
    /// The track, for its crests: the ground follows them.
    track: Track,
    /// Beyond this distance from the centre line, the land may rise.
    flat: f32,
    /// A knoll in the widest place inside the circuit: where a landmark stands.
    pub knoll: Option<Site>,
    /// A lake in the next widest place.
    pub lake: Option<Site>,
    /// The sea, on a coast: none inland.
    pub sea: Option<Sea>,
}

/// The sea beside the land: beyond a shore that runs across one of the
/// land's axes, just past the track's furthest reach that way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sea {
    /// The way out to sea: one of ±X, ±Z (as a point of the plane, x and y).
    pub towards: Vec2,
    /// How far from the land's middle the shore runs, that way, before it wanders.
    pub shore: f32,
}

impl Land {
    pub fn new(frames: &[Frame], track: &Track) -> Self {
        let points: Vec<Vec2> = frames.iter().map(|f| f.at).collect();
        let (low, high) = points.iter().fold((Vec2::MAX, Vec2::MIN), |(low, high), p| (low.min(*p), high.max(*p)));
        let flat = track.half_width + track.verge + 5.0;
        let mut land =
            Self { centre: (low + high) / 2.0, points, track: track.clone(), flat, knoll: None, lake: None, sea: None };

        // The rooms inside the loop, widest first: every few metres, how far the track is.
        let mut rooms = Vec::new();
        let (columns, rows) = (((high.x - low.x) / 4.0) as usize, ((high.y - low.y) / 4.0) as usize);
        for j in 0..=rows {
            for i in 0..=columns {
                let centre = low + Vec2::new(i as f32, j as f32) * 4.0;
                if land.inside(centre) {
                    rooms.push(Site { centre, radius: land.distance_to_track(centre) });
                }
            }
        }
        rooms.sort_by(|a, b| b.radius.total_cmp(&a.radius));
        // What is built there keeps clear of the flat land along the track.
        let clear = |room: &Site, most: f32| Site { centre: room.centre, radius: (room.radius - flat - 6.0).min(most) };
        if let Some(widest) = rooms.first().filter(|room| room.radius > flat + 22.0) {
            land.knoll = Some(clear(widest, 30.0));
            let apart = |room: &&Site| room.centre.distance(widest.centre) > widest.radius + 0.8 * room.radius;
            land.lake = rooms.iter().find(apart).filter(|room| room.radius > flat + 18.0).map(|room| clear(room, 26.0));
        }
        land
    }

    /// The land on a coast: the sea on the side where the track comes
    /// nearest the edge of the land, its cliffs a little beyond the track.
    pub fn with_sea(mut self) -> Self {
        let ways = [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y];
        let reach = |way: Vec2| self.points.iter().map(|p| (*p - self.centre).dot(way)).fold(f32::MIN, f32::max);
        let towards = ways.into_iter().max_by(|a, b| reach(*a).total_cmp(&reach(*b))).unwrap_or(Vec2::X);
        self.sea = Some(Sea { towards, shore: reach(towards) + self.flat + 30.0 });
        self
    }

    /// How far out to sea a point is past the shore, metres (below zero:
    /// inland); none inland. The shore wanders in coves and points.
    pub fn offshore(&self, at: Vec2) -> Option<f32> {
        let sea = self.sea?;
        let from = at - self.centre;
        let across = from.dot(Vec2::new(-sea.towards.y, sea.towards.x));
        let wander =
            22.0 * (noise(Vec2::new(across / 70.0, 3.0)) - 0.5) + 8.0 * (noise(Vec2::new(across / 19.0, 9.0)) - 0.5);
        Some(from.dot(sea.towards) - sea.shore - wander)
    }

    /// The middle of the modelled land.
    pub fn centre(&self) -> Vec2 {
        self.centre
    }

    /// Distance from the nearest point of the centre line.
    pub fn distance_to_track(&self, at: Vec2) -> f32 {
        self.points.iter().map(|p| p.distance_squared(at)).fold(f32::MAX, f32::min).sqrt()
    }

    /// Whether a point is inside the loop of the circuit.
    pub fn inside(&self, at: Vec2) -> bool {
        let n = self.points.len();
        let crossings = (0..n).filter(|&i| {
            let (a, b) = (self.points[i], self.points[(i + 1) % n]);
            (a.y > at.y) != (b.y > at.y) && at.x < a.x + (b.x - a.x) * (at.y - a.y) / (b.y - a.y)
        });
        crossings.count() % 2 == 1
    }

    /// Where a point is relative to the lake's shore: under 1 inside it, 1 on
    /// the edge of its basin, above 1 outside. The shore is not a circle.
    pub fn shore(&self, at: Vec2) -> Option<f32> {
        let lake = self.lake?;
        let from = at - lake.centre;
        let angle = from.y.atan2(from.x);
        let outline = 0.84 + 0.11 * (2.0 * angle + 0.9).cos() + 0.05 * (5.0 * angle + 2.0).cos();
        Some(from.length() / (lake.radius * outline))
    }

    /// Height of the ground: the road's along the track (zero, but over a
    /// crest), hills beyond, zero again at the edge of the modelled land; a
    /// knoll and a lake inside the loop.
    pub fn height(&self, at: Vec2) -> f32 {
        let distance = self.distance_to_track(at);
        let rise = smooth((distance - self.flat) / 70.0);
        let edge = (at - self.centre).abs().max_element();
        let fade = smooth((LAND / 2.0 - edge) / 120.0);
        let hills = 0.5 * noise(at / 150.0) + 0.3 * noise(at / 61.0 + 17.0) + 0.2 * noise(at / 23.0 + 5.0);
        let mut height = HILLS * rise * fade * hills;
        if let Some(knoll) = self.knoll {
            height += KNOLL * smooth(1.0 - at.distance(knoll.centre) / knoll.radius);
        }
        if let Some(shore) = self.shore(at) {
            // No hill stands in the lake: the land comes down to the shore,
            // then goes under the water.
            height *= smooth((shore - 1.0) / 0.6);
            height -= LAKE_BED * smooth((1.0 - shore) / 0.5);
        }
        if let Some(off) = self.offshore(at) {
            // Towards the sea the hills settle into a plateau at the road's
            // height; at the shore, a cliff falls to the sea's bed.
            height *= 1.0 - smooth((off + 45.0) / 40.0);
            height -= SEA_BED * smooth(off / 7.0);
        }
        // A crest: the road's height across the flat land, fading into the
        // hills over the next 30 m.
        if !self.track.crests().is_empty() && distance < self.flat + 30.0 {
            let road = self.track.lift(self.track.locate(at.x, at.y).along).0;
            height += road * (1.0 - smooth((distance - self.flat) / 30.0));
        }
        height
    }

    /// Height of the road's surface at a point of the track, as the physics has it.
    pub fn road(&self, at: Vec2) -> f32 {
        self.track.lift(self.track.locate(at.x, at.y).along).0
    }

    /// Whether a point is in the lake or on its beach, or over the cliff's edge.
    pub fn wet(&self, at: Vec2) -> bool {
        self.shore(at).is_some_and(|shore| shore < 1.3) || self.offshore(at).is_some_and(|off| off > -2.0)
    }
}

/// 0 below 0, 1 above 1, and a smooth step between.
pub fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Value noise in 0..1, the same on every machine.
pub fn noise(p: Vec2) -> f32 {
    let cell = p.floor();
    let t = (p - cell).map(|t| t * t * (3.0 - 2.0 * t));
    let corner = |dx: f32, dy: f32| {
        let (x, y) = ((cell.x + dx) as i32 as u32, (cell.y + dy) as i32 as u32);
        let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
        h = (h ^ (h >> 15)).wrapping_mul(0x2C1B_3C6D);
        ((h ^ (h >> 13)) & 0xffff) as f32 / 65535.0
    };
    let (low, high) = (corner(0.0, 0.0).lerp(corner(1.0, 0.0), t.x), corner(0.0, 1.0).lerp(corner(1.0, 1.0), t.x));
    low.lerp(high, t.y)
}
#[cfg(test)]
mod tests {
    use osage_world_kart::Circuit;

    use super::*;

    fn land() -> (Land, Circuit) {
        let circuit = Circuit::new();
        let frames = frames(&circuit.track);
        (Land::new(&frames, &circuit.track), circuit)
    }

    #[test]
    fn the_land_is_the_roads_height_where_karts_drive() {
        let (land, circuit) = land();
        let reach = circuit.track.half_width + circuit.track.verge + BARRIER + 0.7;
        let mut crested = 0;
        for frame in frames(&circuit.track) {
            for lateral in [-reach, 0.0, reach] {
                let at = frame.at + frame.right * lateral;
                let road = circuit.track.lift(circuit.track.locate(at.x, at.y).along).0;
                assert!((land.height(at) - road).abs() < 0.02, "{} m high at {at}, the road {road}", land.height(at));
                crested += usize::from(road > 0.1);
            }
        }
        assert!(crested > 0, "the circuit has a crest");
    }

    #[test]
    fn between_frames_a_crest_is_drawn_as_the_physics_has_it() {
        let circuit = Circuit::new();
        let frames = frames(&circuit.track);
        let mut worst: f32 = 0.0;
        for (i, a) in frames.iter().enumerate() {
            let b = &frames[(i + 1) % frames.len()];
            let length = a.at.distance(b.at);
            for k in 0..=10 {
                let t = k as f32 / 10.0;
                let along = a.along + t * length;
                worst = worst.max((a.height_towards(t, length) - circuit.track.lift(along).0).abs());
            }
        }
        assert!(worst < 1e-3, "{worst} m off");
    }

    #[test]
    fn on_a_coast_the_road_runs_along_cliffs_above_the_sea() {
        let track = Circuit::new().track;
        let frames = frames(&track);
        let land = Land::new(&frames, &track).with_sea();
        let sea = land.sea.expect("a sea");
        // The track stays inland, on the plateau.
        for frame in &frames {
            assert!(land.offshore(frame.at).unwrap() < -25.0, "the track is kept from the cliff");
            assert!(!land.wet(frame.at));
        }
        // Out to sea, the bed lies under the sea's level; well inland, nothing changes.
        let out = land.centre() + sea.towards * (sea.shore + 60.0);
        assert!(land.height(out) < SEA - 4.0, "{}", land.height(out));
        assert!(land.wet(out));
        let inland = Land::new(&frames, &track);
        let far_side = land.centre() - sea.towards * 200.0;
        assert_eq!(land.height(far_side), inland.height(far_side));
    }

    #[test]
    fn the_circuit_leaves_room_for_a_knoll_and_a_lake() {
        let (land, circuit) = land();
        let (knoll, lake) = (land.knoll.expect("a knoll"), land.lake.expect("a lake"));
        let barrier = circuit.track.half_width + circuit.track.verge + BARRIER + 0.7;
        for site in [knoll, lake] {
            assert!(land.inside(site.centre), "{site:?} is inside the loop");
            assert!(land.distance_to_track(site.centre) - site.radius > barrier + 3.0, "{site:?} keeps clear");
        }
        assert!(knoll.centre.distance(lake.centre) > knoll.radius + lake.radius, "they do not overlap");
        assert!(!land.inside(Vec2::new(500.0, 0.0)));

        // The knoll stands above the track, the lake's bed lies under its water,
        // and its shore is above it all the way round.
        assert!(land.height(knoll.centre) >= KNOLL - 1e-3);
        assert!(land.height(lake.centre) < WATER - 1.0);
        for step in 0..48 {
            let around = Vec2::from_angle(step as f32 / 48.0 * std::f32::consts::TAU);
            let edge = lake.centre + around * lake.radius * 1.02;
            assert!(land.height(edge) > WATER, "the water is held in at {edge}");
            assert!(land.shore(edge).unwrap() >= 1.0);
        }
    }
}
