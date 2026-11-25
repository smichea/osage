// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The solved capsule contact can precede overlap or fall between public physics ticks.
use super::{Touch, frame, point_velocity};
use bevy::math::Vec3;
use osage_dynamics::{Contact, Pose};
use osage_paris_voices::RATE;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct Step<'a> {
    pub tick: u32,
    pub hz: u32,
    pub substeps: u8,
    pub before: &'a [Pose],
    pub after: &'a [Pose],
    pub boxes: &'a [Pose],
    pub prior_boxes: &'a [Pose],
    pub contacts: &'a [Contact],
    pub swept: Option<f32>,
}

impl Step<'_> {
    pub(super) fn touches<'s>(
        &self,
        previous: &mut BTreeSet<(usize, usize)>,
        box_index: impl Fn(usize) -> Option<usize>,
        name: impl Fn(usize) -> Option<&'s str>,
    ) -> Vec<(usize, Touch<'s>)> {
        let start = frame(self.tick - 1, self.hz);
        let stop = frame(self.tick, self.hz);
        let mut now = BTreeSet::new();
        // A solver can transfer momentum while the shapes still have a positive gap.
        // Only attribute that response when one named surface can account for it.
        let sole_surface =
            self.contacts.first().map(|c| c.surface).filter(|&id| self.contacts.iter().all(|c| c.surface == id));
        let mut batches = BTreeMap::new();
        for c in self.contacts {
            let key = (c.surface, c.part);
            let point = Vec3::from(c.point);
            let normal = Vec3::from(c.normal);
            let mobile = box_index(c.surface);
            let relative = |parts: &[Pose], boxes: &[Pose]| {
                let other = mobile.and_then(|i| boxes.get(i)).map_or(Vec3::ZERO, |p| point_velocity(p, point));
                (other - point_velocity(&parts[c.part], point)).dot(normal)
            };
            let speed = relative(self.before, self.prior_boxes);
            let swept = self.swept.filter(|_| mobile == Some(0) && self.before.len() == 1);
            let response = self.before.len() == 1
                && sole_surface == Some(c.surface)
                && speed > 0.5
                && c.separation <= speed / (self.hz as f32 * f32::from(self.substeps))
                && speed - relative(self.after, self.boxes) > 0.5;
            let basis = if c.applied_offset_s.is_some() {
                "applied impulse"
            } else if swept.is_some() {
                "swept"
            } else if c.separation <= 0. {
                "touching"
            } else if response {
                "approach slowed"
            } else {
                continue;
            };
            if !now.insert(key) || previous.contains(&key) || speed <= 0.5 {
                continue;
            }
            let exact = c.applied_offset_s.or(swept);
            let at = exact
                .map_or_else(
                    || start + (stop - start) * u64::from(c.substep) / u64::from(self.substeps),
                    |s| start + (f64::from(s) * f64::from(RATE)).round() as u64,
                )
                .min(stop);
            // The velocity evidence spans the whole public tick.
            let at = if exact.is_none() && c.separation > 0. { stop } else { at };
            let Some(surface) = name(c.surface) else { continue };
            // Simultaneous points and limbs on one surface excite one impact voice.
            let hit = Touch { point, part: Some(c.part), surface, closing: speed, at, basis };
            let batch = batches.entry((at, surface, c.surface)).or_insert(hit);
            if speed > batch.closing {
                *batch = hit;
            }
        }

        *previous = now;
        batches.into_iter().map(|((_, _, surface), touch)| (surface, touch)).collect()
    }
}
