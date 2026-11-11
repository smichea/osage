// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! People at a circuit: spectators sitting in a grandstand or standing
//! along a fence, marshals at their posts. Whole figures sculpted from
//! blended pieces ([`Sculpture`]) — trunk, limbs, head and hair in one
//! surface — in a few poses, each sculpted once and shared. A world dresses
//! each one by painting its parts: shirt, trousers, skin, hair, shoes.
//!
//! A seated figure sits on the origin, the seat's top, facing +X; its feet
//! rest 0.45 m lower, on the tier in front. A standing figure stands on the
//! origin, facing +X. Summer clothes: short sleeves.

use std::sync::OnceLock;

use bevy::prelude::*;
use osage_procedural::{Sculpture, Shape};

/// How a spectator sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pose {
    /// Upright, hands on the knees.
    HandsOnKnees,
    /// Leaning back, arms folded.
    ArmsFolded,
    /// Upright, one arm raised: cheering a kart that passes.
    Cheering,
}

impl Pose {
    pub const ALL: [Pose; 3] = [Pose::HandsOnKnees, Pose::ArmsFolded, Pose::Cheering];
}

/// How someone stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stance {
    /// Arms down, watching.
    Watching,
    /// Arms folded.
    ArmsFolded,
    /// One arm raised, waving at a kart that passes.
    Waving,
    /// The left hand forward at waist height, holding a flag's staff (the
    /// world adds the flag): a marshal.
    HoldingFlag,
}

impl Stance {
    pub const ALL: [Stance; 4] = [Stance::Watching, Stance::ArmsFolded, Stance::Waving, Stance::HoldingFlag];
    /// Where the hand holding a flag closes, for [`Stance::HoldingFlag`].
    pub const FLAG_HAND: Vec3 = Vec3::new(0.34, 1.08, -0.2);
}

/// What a part of the surface is, for the world to paint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Wear {
    Shirt,
    Trousers,
    Skin,
    Hair,
    Shoes,
}

const WEAR: [Wear; 5] = [Wear::Shirt, Wear::Trousers, Wear::Skin, Wear::Hair, Wear::Shoes];

/// The size of the sculpting grid: a spectator is seen from the track, ten
/// metres off at the closest, where 3.5 cm is under two pixels; a crowd of
/// two hundred costs a few hundred thousand triangles.
const CELL: f32 = 0.035;

/// The figure of a seated spectator in `pose`, as one shape per part.
pub fn seated(pose: Pose) -> &'static [(Wear, Shape)] {
    static FIGURES: [OnceLock<Vec<(Wear, Shape)>>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    let slot = Pose::ALL.iter().position(|p| *p == pose).unwrap_or(0);
    FIGURES[slot].get_or_init(|| {
        sculpt(pose).shapes(CELL).into_iter().map(|(group, shape)| (WEAR[group.min(WEAR.len() - 1)], shape)).collect()
    })
}

/// The figure of someone standing in `stance`, as one shape per part.
pub fn standing(stance: Stance) -> &'static [(Wear, Shape)] {
    static FIGURES: [OnceLock<Vec<(Wear, Shape)>>; 4] =
        [OnceLock::new(), OnceLock::new(), OnceLock::new(), OnceLock::new()];
    let slot = Stance::ALL.iter().position(|s| *s == stance).unwrap_or(0);
    FIGURES[slot].get_or_init(|| {
        sculpt_standing(stance)
            .shapes(CELL)
            .into_iter()
            .map(|(group, shape)| (WEAR[group.min(WEAR.len() - 1)], shape))
            .collect()
    })
}

fn sculpt_standing(stance: Stance) -> Sculpture {
    let (shirt, trousers, skin, hair, shoes) = (0, 1, 2, 3, 4);
    let mut body = Sculpture::new(0.04);
    body.ellipsoid(trousers, Vec3::new(0.0, 0.95, 0.0), Vec3::new(0.12, 0.11, 0.17), Quat::IDENTITY)
        .ellipsoid(shirt, Vec3::new(0.0, 1.15, 0.0), Vec3::new(0.11, 0.14, 0.16), Quat::IDENTITY)
        .ellipsoid(shirt, Vec3::new(-0.01, 1.34, 0.0), Vec3::new(0.12, 0.15, 0.19), Quat::IDENTITY)
        .cone(skin, Vec3::new(-0.01, 1.45, 0.0), Vec3::new(0.0, 1.57, 0.0), 0.052, 0.048)
        .ellipsoid(skin, Vec3::new(0.02, 1.67, 0.0), Vec3::new(0.1, 0.118, 0.084), Quat::IDENTITY)
        .ellipsoid(hair, Vec3::new(-0.012, 1.7, 0.0), Vec3::new(0.098, 0.1, 0.09), Quat::IDENTITY);
    for side in [-1.0f32, 1.0] {
        let shoulder = Vec3::new(-0.01, 1.43, side * 0.19);
        let down =
            (Vec3::new(0.0, 1.16, side * 0.24), Vec3::new(0.04, 0.92, side * 0.23), Vec3::new(0.05, 0.84, side * 0.22));
        let (elbow, wrist, hand) = match (stance, side > 0.0) {
            (Stance::Watching, _) => down,
            (Stance::ArmsFolded, _) => (
                Vec3::new(0.06, 1.2, side * 0.21),
                Vec3::new(0.15, 1.27, -side * 0.12),
                Vec3::new(0.14, 1.28, -side * 0.17),
            ),
            (Stance::Waving, true) => (
                Vec3::new(-0.02, 1.72, side * 0.3),
                Vec3::new(0.02, 1.96, side * 0.26),
                Vec3::new(0.03, 2.03, side * 0.25),
            ),
            (Stance::HoldingFlag, false) => {
                (Vec3::new(0.12, 1.16, side * 0.24), Vec3::new(0.3, 1.1, side * 0.21), Stance::FLAG_HAND)
            }
            _ => down,
        };
        let hip = Vec3::new(0.0, 0.92, side * 0.1);
        let knee = Vec3::new(0.03, 0.5, side * 0.1);
        let ankle = Vec3::new(0.0, 0.1, side * 0.1);
        body.cone(shirt, Vec3::new(-0.01, 1.47, 0.0), shoulder, 0.06, 0.062)
            .cone(shirt, shoulder, shoulder.lerp(elbow, 0.55), 0.055, 0.05)
            .cone(skin, shoulder.lerp(elbow, 0.5), elbow, 0.044, 0.04)
            .cone(skin, elbow, wrist, 0.04, 0.03)
            .ellipsoid(skin, hand, Vec3::new(0.045, 0.05, 0.03), Quat::IDENTITY)
            .cone(trousers, hip, knee, 0.085, 0.06)
            .folded(0.003)
            .cone(trousers, knee, ankle, 0.056, 0.045)
            .cone(shoes, ankle + Vec3::new(-0.02, -0.04, 0.0), Vec3::new(0.18, 0.04, side * 0.11), 0.05, 0.042);
    }
    body
}

fn sculpt(pose: Pose) -> Sculpture {
    let (shirt, trousers, skin, hair, shoes) = (0, 1, 2, 3, 4);
    // Leaning back turns the trunk about Z: its top goes towards -X.
    let lean = Quat::from_rotation_z(if pose == Pose::ArmsFolded { 0.2 } else { 0.04 });
    let trunk = |p: Vec3| lean * p;
    let mut body = Sculpture::new(0.04);
    body.ellipsoid(trousers, Vec3::new(0.0, 0.08, 0.0), Vec3::new(0.13, 0.1, 0.17), Quat::IDENTITY)
        .ellipsoid(shirt, trunk(Vec3::new(0.0, 0.28, 0.0)), Vec3::new(0.11, 0.14, 0.16), lean)
        .ellipsoid(shirt, trunk(Vec3::new(-0.01, 0.47, 0.0)), Vec3::new(0.12, 0.15, 0.19), lean)
        .cone(skin, trunk(Vec3::new(-0.01, 0.58, 0.0)), trunk(Vec3::new(0.0, 0.7, 0.0)), 0.052, 0.048)
        .ellipsoid(skin, trunk(Vec3::new(0.02, 0.8, 0.0)), Vec3::new(0.1, 0.118, 0.084), lean)
        // Hair: a cap over the top and the back of the head.
        .ellipsoid(hair, trunk(Vec3::new(-0.012, 0.83, 0.0)), Vec3::new(0.098, 0.1, 0.09), lean);
    for side in [-1.0f32, 1.0] {
        let shoulder = trunk(Vec3::new(-0.01, 0.56, side * 0.19));
        let (elbow, wrist, hand) = match (pose, side > 0.0) {
            (Pose::HandsOnKnees, _) => (
                Vec3::new(0.12, 0.34, side * 0.22),
                Vec3::new(0.3, 0.13, side * 0.17),
                Vec3::new(0.36, 0.11, side * 0.16),
            ),
            // Folded: each forearm across to the other elbow.
            (Pose::ArmsFolded, _) => (
                trunk(Vec3::new(0.06, 0.33, side * 0.21)),
                trunk(Vec3::new(0.15, 0.39, -side * 0.12)),
                trunk(Vec3::new(0.14, 0.4, -side * 0.17)),
            ),
            // One arm raised, the other on the knee.
            (Pose::Cheering, true) => (
                Vec3::new(-0.02, 0.84, side * 0.3),
                Vec3::new(0.02, 1.08, side * 0.26),
                Vec3::new(0.03, 1.15, side * 0.25),
            ),
            (Pose::Cheering, false) => (
                Vec3::new(0.12, 0.34, side * 0.22),
                Vec3::new(0.3, 0.13, side * 0.17),
                Vec3::new(0.36, 0.11, side * 0.16),
            ),
        };
        let knee = Vec3::new(0.42, 0.06, side * 0.11);
        let ankle = Vec3::new(0.46, -0.38, side * 0.11);
        body.cone(shirt, trunk(Vec3::new(-0.01, 0.6, 0.0)), shoulder, 0.06, 0.062)
            // Short sleeves to halfway down the upper arm, then bare arms.
            .cone(shirt, shoulder, shoulder.lerp(elbow, 0.55), 0.055, 0.05)
            .cone(skin, shoulder.lerp(elbow, 0.5), elbow, 0.044, 0.04)
            .cone(skin, elbow, wrist, 0.04, 0.03)
            .ellipsoid(skin, hand, Vec3::new(0.045, 0.05, 0.03), Quat::IDENTITY)
            .cone(trousers, Vec3::new(0.0, 0.06, side * 0.09), knee, 0.085, 0.062)
            .folded(0.003)
            .cone(trousers, knee, ankle, 0.056, 0.042)
            .cone(shoes, ankle + Vec3::new(-0.01, -0.02, 0.0), Vec3::new(0.6, -0.42, side * 0.12), 0.045, 0.04);
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    fn bounds(figure: &[(Wear, Shape)]) -> (Vec3, Vec3) {
        let mut batch = osage_procedural::Batch::default();
        for (_, shape) in figure {
            batch.add(shape, Transform::IDENTITY, osage_procedural::Paint::plain(1.0, 1.0, 1.0));
        }
        let mesh = batch.merged();
        let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("positions")
        };
        positions
            .iter()
            .fold((Vec3::MAX, Vec3::MIN), |(low, high), p| (low.min(Vec3::from(*p)), high.max(Vec3::from(*p))))
    }

    #[test]
    fn every_pose_is_a_whole_seated_figure_in_its_seat() {
        for pose in Pose::ALL {
            let figure = seated(pose);
            let parts: Vec<Wear> = figure.iter().map(|(wear, _)| *wear).collect();
            assert_eq!(parts, WEAR.to_vec(), "{pose:?}: every part, in order");
            let (low, high) = bounds(figure);
            // Head about 0.9 m above the seat (higher with an arm raised), feet on the tier below.
            assert!((0.85..1.3).contains(&high.y), "{pose:?}: top at {}", high.y);
            assert!((-0.5..-0.4).contains(&low.y), "{pose:?}: feet at {}", low.y);
            // Within a seat's width (1.02 m apart) and depth.
            assert!(low.z > -0.42 && high.z < 0.42, "{pose:?}: {} to {} wide", low.z, high.z);
            assert!(low.x > -0.3 && high.x < 0.7, "{pose:?}: {} to {} deep", low.x, high.x);
        }
    }

    #[test]
    fn every_stance_is_a_whole_standing_figure() {
        for stance in Stance::ALL {
            let figure = standing(stance);
            let parts: Vec<Wear> = figure.iter().map(|(wear, _)| *wear).collect();
            assert_eq!(parts, WEAR.to_vec(), "{stance:?}: every part, in order");
            let (low, high) = bounds(figure);
            // On the ground, about 1.75 m tall, higher with an arm raised.
            assert!(low.y.abs() < 0.03, "{stance:?}: feet at {}", low.y);
            assert!((1.72..2.15).contains(&high.y), "{stance:?}: top at {}", high.y);
            assert!(low.z > -0.42 && high.z < 0.42, "{stance:?}: {} to {} wide", low.z, high.z);
        }
        // The flag's hand is where the figure's hand is.
        let (_, high) = bounds(standing(Stance::HoldingFlag));
        assert!(high.x > Stance::FLAG_HAND.x, "the hand reaches forward to {}", high.x);
    }

    #[test]
    fn figures_are_sculpted_once() {
        assert!(std::ptr::eq(seated(Pose::Cheering), seated(Pose::Cheering)));
    }
}
