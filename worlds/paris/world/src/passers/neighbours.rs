// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{COMFORT, HORIZON, Motion, clearance};
use bevy::math::{Vec2, Vec3, Vec3Swizzles};

/// Positions stay at the start of the tick while accepted velocities change.
pub(super) struct Neighbours {
    axes: [Vec<usize>; 2],
    fastest: Vec2,
}

impl Neighbours {
    pub(super) fn new(people: &[Motion]) -> Self {
        // A direct scan is cheaper than building and searching indices for a
        // handful of residents, including the two on the default street.
        if people.len() <= 8 {
            return Self { axes: Default::default(), fastest: Vec2::ZERO };
        }
        let axes = std::array::from_fn(|axis| {
            let mut order: Vec<_> = (0..people.len()).collect();
            order.sort_unstable_by(|&a, &b| people[a].at.xz()[axis].total_cmp(&people[b].at.xz()[axis]));
            order
        });
        let fastest = people.iter().fold(Vec2::ZERO, |m, p| m.max(p.velocity.abs()));
        Self { axes, fastest }
    }

    pub(super) fn accept(&mut self, velocity: Vec2) {
        self.fastest = self.fastest.max(velocity.abs());
    }

    pub(super) fn measure(&self, people: &[Motion], me: usize, v: Vec2, dt: f32) -> (f32, f32) {
        let at: Vec3 = people[me].at;
        if self.axes[0].is_empty() {
            return clearance(at, v, people[..me].iter().chain(&people[me + 1..]), dt);
        }
        // Bound the relative displacement separately on each axis. A walker
        // still outside comfort after this travel cannot change any steering score.
        let travel = (self.fastest + v.abs()) * HORIZON.max(dt);
        let strip = |axis: usize| {
            let order = &self.axes[axis];
            let delta = |i: usize| at.xz()[axis] - people[i].at.xz()[axis];
            let start = order.partition_point(|&i| delta(i) - travel[axis] > COMFORT);
            let end = order[start..].partition_point(|&i| -delta(i) - travel[axis] <= COMFORT);
            &order[start..start + end]
        };
        let x = strip(0);
        let z = strip(1);
        let (shorter, other) = if x.len() <= z.len() { (x, 1) } else { (z, 0) };
        let relevant = shorter
            .iter()
            .copied()
            .filter(|&i| i != me && (at.xz()[other] - people[i].at.xz()[other]).abs() - travel[other] <= COMFORT);
        clearance(at, v, relevant.map(|i| &people[i]), dt)
    }
}
