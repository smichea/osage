// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use bevy::prelude::*;
use osage_humans::triangle_support_lift;

/// An elevated rectangle from -2 to 2 metres across the street. Point queries
/// also supply the underlying road; only the top can increase the required lift.
pub(super) fn strip_lift(sole: [Vec3; 3], strip: [f32; 3], range: std::ops::RangeInclusive<f32>) -> Option<f32> {
    let [start, end, y] = strip;
    if !range.contains(&y) || sole.iter().all(|p| p.z < start) || sole.iter().all(|p| p.z > end) {
        return None;
    }
    if sole.iter().all(|p| (-2.0..=2.).contains(&p.x) && (start..=end).contains(&p.z)) {
        return sole.iter().map(|p| y - p.y).reduce(f32::max);
    }
    let [a, b, c, d] =
        [Vec3::new(-2., y, start), Vec3::new(2., y, start), Vec3::new(2., y, end), Vec3::new(-2., y, end)];
    [[a, b, c], [a, c, d]].into_iter().filter_map(|t| triangle_support_lift(sole, t)).reduce(f32::max)
}
