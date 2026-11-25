// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Evaluate independent targets on one two-link limb. Input and output coordinates
//! share the same parent frame; each row starts from the given three joint positions.
use anyhow::{Result, anyhow};
use bevy::prelude::*;
use osage_humans::solve_two_bone;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct Trial {
    initial: Option<[Vec3; 3]>,
    rows: Vec<Target>,
}
#[derive(Deserialize)]
struct Target {
    id: String,
    target: Vec3,
    initial: Option<[Vec3; 3]>,
    orientations: Option<[Quat; 3]>,
}
fn main() -> Result<()> {
    let trial: Trial = serde_json::from_reader(std::io::stdin().lock())?;
    let rows: Vec<_> = trial
        .rows
        .into_iter()
        .map(|row| -> Result<_> {
            let [base, mid, end] = row.initial.or(trial.initial).ok_or_else(|| anyhow!("missing initial pose"))?;
            let [upper, lower, tip] = row.orientations.unwrap_or([Quat::IDENTITY; 3]).map(Quat::normalize);
            let hip = Transform::from_translation(base).with_rotation(upper);
            let knee =
                Transform::from_translation(upper.inverse() * (mid - base)).with_rotation(upper.inverse() * lower);
            let ankle = Transform::from_translation(lower.inverse() * (end - mid)).with_rotation(lower.inverse() * tip);
            let rotations = solve_two_bone(hip, knee, ankle, row.target - end);
            let pose = rotations.map(|[h, k, a]| {
                let thigh = hip.with_rotation(h);
                let shin = thigh.mul_transform(knee.with_rotation(k));
                let tip = shin.mul_transform(ankle.with_rotation(a));
                [thigh.translation, shin.translation, tip.translation]
            });
            Ok(json!({"id":row.id,"target":row.target,"after":pose}))
        })
        .collect::<Result<_>>()?;
    serde_json::to_writer(std::io::stdout().lock(), &rows)?;
    Ok(())
}
