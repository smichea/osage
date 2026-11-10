// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! R3 — ambient occlusion: screen-space, from T3; quality rises with the
//! tier. It needs a depth and normal prepass and MSAA off ([`crate::post`]).
//!
//! Measured on the Iris Xe reference at 1080p: about 10 ms at medium
//! quality, more than half of T2's budget, so T2 goes without.

use bevy::{
    pbr::{ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel},
    prelude::*,
};

use crate::{OsageCamera, Tier};

pub struct AmbientOcclusionPlugin;

impl Plugin for AmbientOcclusionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, camera);
    }
}

fn camera(mut commands: Commands, tier: Res<Tier>, cameras: Query<Entity, Added<OsageCamera>>) {
    let quality_level = match *tier {
        Tier::T1 | Tier::T2 => return,
        Tier::T3 => ScreenSpaceAmbientOcclusionQualityLevel::High,
        Tier::T4 => ScreenSpaceAmbientOcclusionQualityLevel::Ultra,
    };
    for camera in &cameras {
        commands.entity(camera).insert(ScreenSpaceAmbientOcclusion { quality_level, ..default() });
    }
}
