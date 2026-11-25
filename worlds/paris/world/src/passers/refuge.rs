// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{ARRIVAL, COMFORT, HORIZON, Patrol, Resident, WALK, Walk, YIELD_SECONDS};
use crate::walk::{EYES, RADIUS};
use bevy::math::{Vec2, Vec3, Vec3Swizzles};

/// A route out of single-file traffic, with room to stand clear of the lane.
pub(super) struct Refuge {
    peer: u64,
    path: Vec<Vec3>,
    next: usize,
    returning: bool,
    outward: Vec2,
    heading: Vec2,
}

impl Patrol {
    pub(super) fn traffic_goal(&mut self, walk: &Walk, at: Vec3, residents: &[Resident], index: usize) -> Vec3 {
        let goal = if self.refuge.is_some() { self.points[self.next] } else { self.destination(walk, at) };
        if self.refuge.is_none() && self.yield_remaining == YIELD_SECONDS {
            let me = &residents[index];
            let priority = |r: &Resident| (r.passages_yielded, std::cmp::Reverse(r.id));
            let others = residents.iter().filter(|r| !r.in_refuge && priority(r) > priority(me));
            self.refuge = Refuge::find(walk, at, goal, self, others);
        }
        if let Some(refuge) = &mut self.refuge {
            let peer = residents.binary_search_by_key(&refuge.peer, |r| r.id).ok().map(|i| &residents[i]);
            if let Some(waypoint) = refuge.destination(walk, at, goal, peer) {
                return waypoint;
            }
            self.passages_yielded = self.passages_yielded.saturating_add(1);
            self.refuge = None;
        }
        goal
    }
}

impl Refuge {
    fn find<'a>(
        walk: &Walk,
        at: Vec3,
        goal: Vec3,
        patrol: &Patrol,
        others: impl Iterator<Item = &'a Resident>,
    ) -> Option<Self> {
        let heading = (goal.xz() - at.xz()).try_normalize()?;
        let side = heading.perp() * (2. * RADIUS);
        if [side, -side].iter().any(|s| walk.clear_segment(at.xz(), at.xz() + s)) {
            return None;
        }
        let peer = others
            .filter(|peer| {
                let offset = peer.motion.at.xz() - at.xz();
                (peer.motion.at.y - at.y).abs() < EYES
                    && offset.dot(heading) > 0.
                    && offset.length() < WALK * HORIZON + COMFORT
                    && peer.towards.normalize_or_zero().dot(heading) < -0.5
            })
            .min_by(|a, b| at.distance_squared(a.motion.at).total_cmp(&at.distance_squared(b.motion.at)))?;
        let origin = if patrol.forward { patrol.points.first()? } else { patrol.points.last()? };
        let reverse = walk.route(at.xz(), origin.xz())?;
        let mut path = vec![at];
        let mut outward = -heading;
        for end in reverse.iter().skip(1) {
            let from = path.last()?.xz();
            let delta = end.xz() - from;
            let Some(dir) = delta.try_normalize() else { continue };
            outward = dir;
            let steps = (delta.length() / RADIUS).ceil() as usize;
            for i in 1..=steps {
                let p = from.lerp(end.xz(), i as f32 / steps as f32);
                if let Some(spot) = Self::spot(walk, p, dir) {
                    return Some(Self::new(peer.id, path, spot, dir, heading));
                }
            }
            path.push(*end);
        }
        // A patrol can start inside the bottleneck. Continue past that endpoint
        // along clear ground until there is a place to step aside or the floor ends.
        let mut p = origin.xz();
        loop {
            let q = p + outward * RADIUS;
            if q == p || !walk.clear_segment(p, q) {
                return None;
            }
            p = q;
            if let Some(spot) = Self::spot(walk, p, outward) {
                return Some(Self::new(peer.id, path, spot, outward, heading));
            }
        }
    }

    fn spot(walk: &Walk, p: Vec2, dir: Vec2) -> Option<[Vec3; 2]> {
        let lateral = dir.perp() * COMFORT;
        for side in [lateral, -lateral] {
            let q = p + side;
            if p.distance(q) >= 2. * RADIUS && walk.clear_segment(p, q) {
                return Some([Vec3::new(p.x, walk.ground(p)?, p.y), Vec3::new(q.x, walk.ground(q)?, q.y)]);
            }
        }
        None
    }

    fn new(peer: u64, mut path: Vec<Vec3>, spot: [Vec3; 2], outward: Vec2, heading: Vec2) -> Self {
        path.extend(spot);
        path.dedup();
        Self { peer, path, next: 1, returning: false, outward, heading }
    }

    fn destination(&mut self, walk: &Walk, at: Vec3, goal: Vec3, peer: Option<&Resident>) -> Option<Vec3> {
        if peer.is_none() && !self.returning {
            self.returning = true;
            self.next = self.next.saturating_sub(1);
        }
        if self.returning && walk.clear_segment(at.xz(), goal.xz()) {
            return None;
        }
        if at.xz().distance(self.path[self.next].xz()) >= ARRIVAL {
            return Some(self.path[self.next]);
        }
        if !self.returning && self.next + 1 == self.path.len() {
            let mouth = self.path[self.next - 1].xz();
            let clear = peer.is_none_or(|peer| {
                let p = peer.motion.at;
                let dir = peer.towards;
                (p.xz() - mouth).dot(self.outward) >= ARRIVAL
                    || ((p.xz() - at.xz()).dot(dir) > 0.
                        && p.xz().distance(at.xz()) > COMFORT
                        && dir.dot(self.heading) > 0.)
            });
            if !clear {
                return Some(at);
            }
            self.returning = true;
        }
        let next = if self.returning { self.next.checked_sub(1)? } else { self.next + 1 };
        if walk.clear_segment(at.xz(), self.path[next].xz()) {
            self.next = next;
        }
        Some(self.path[self.next])
    }
}
