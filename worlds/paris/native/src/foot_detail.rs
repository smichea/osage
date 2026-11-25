// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use bevy::prelude::*;
use osage_humans::{PlannedHumanSteps, PlantFeet};
use osage_objects::DetailView;

/// Both installed body models in the curb replay displaced a forefoot by less
/// than 0.16 m relative to the raw clip. Paris budgets 0.20 m for this choice.
fn needs_steps(view: Option<DetailView>, feet: Vec3, planned: bool) -> bool {
    let Some(view) = view else { return true };
    let centre = (feet + Vec3::Y).as_dvec3();
    // Keep neighbours ready for an about-face and preserve the current stance.
    if centre.distance_squared(view.eye) <= 4.0 * 4.0 {
        return true;
    }
    let pixels = 0.20 * view.pixels_per_metre(centre, Vec3::new(0.8, 1.1, 0.8).as_dvec3());
    pixels > if planned { 1.0 } else { 2.0 }
}

pub(super) fn update(commands: &mut Commands, actor: Entity, view: Option<DetailView>, feet: Vec3, planned: bool) {
    match (planned, needs_steps(view, feet, planned)) {
        (false, true) => {
            commands.entity(actor).insert(PlannedHumanSteps::default());
        }
        (true, false) => {
            commands.entity(actor).remove::<(PlannedHumanSteps, PlantFeet)>();
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_turn_selects_the_other_six_walkers_in_a_symmetric_crowd() {
        for direction in [1., -1.] {
            let view = DetailView::new(
                Vec3::ZERO.as_dvec3(),
                (Vec3::Z * direction).as_dvec3(),
                60_f64.to_radians(),
                1080,
                0.05,
            );
            let mut groups = [0; 4];
            for (i, z) in [10., -10., 180., -180.].into_iter().enumerate() {
                for lane in 0..6 {
                    let feet = Vec3::new(lane as f32 - 2.5, 0., z);
                    groups[i] += usize::from(needs_steps(view, feet, false));
                }
            }
            assert_eq!(groups, if direction > 0. { [6, 0, 0, 0] } else { [0, 6, 0, 0] });
        }
    }

    #[test]
    fn resolution_turning_and_hysteresis_select_the_contact_solve() {
        let view = |height, forward: Vec3| {
            DetailView::new(Vec3::Y.as_dvec3(), forward.as_dvec3(), std::f64::consts::FRAC_PI_2, height, 0.05)
        };
        let v = view(1000, Vec3::Z);
        assert!(needs_steps(None, Vec3::Z * 500., false));
        assert!(needs_steps(v, Vec3::NEG_Z * 2., false));
        assert!(!needs_steps(v, Vec3::NEG_Z * 10., true));
        assert!(needs_steps(view(1000, Vec3::NEG_Z), Vec3::NEG_Z * 10., false));
        assert!(needs_steps(v, Vec3::Z * 70., true));
        assert!(!needs_steps(v, Vec3::Z * 70., false));
        assert!(!needs_steps(v, Vec3::Z * 150., true));
        assert!(needs_steps(view(3000, Vec3::Z), Vec3::Z * 140., false));
    }
}
