// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! R10 — post-processing and anti-aliasing. All tiers render in HDR with a
//! filmic tonemapper and the exposure of a sunny day, since the sun has its
//! physical brightness ([`crate::sky`]).
//!
//! - T1, T2: FXAA, which needs no depth prepass; T2 adds bloom.
//! - T3+: temporal anti-aliasing, bloom and motion blur.
//!
//! Temporal anti-aliasing and ambient occlusion both need a depth prepass.
//! On the Intel Iris Xe reference (Mesa), the prepass and the main pass do
//! not always agree on a pixel's depth, which leaves speckles: one more
//! reason, with its cost, to keep T2 free of a prepass.

use bevy::{
    anti_alias::{fxaa::Fxaa, taa::TemporalAntiAliasing},
    camera::{Exposure, Hdr},
    core_pipeline::tonemapping::Tonemapping,
    post_process::{bloom::Bloom, motion_blur::MotionBlur},
    prelude::*,
};

use crate::{OsageCamera, Tier};

pub struct PostPlugin;

impl Plugin for PostPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, camera);
    }
}

fn camera(mut commands: Commands, tier: Res<Tier>, cameras: Query<Entity, Added<OsageCamera>>) {
    for camera in &cameras {
        let mut camera = commands.entity(camera);
        // A physical sun is about 100 000 lux: expose as a camera does in full sun.
        camera.insert((Hdr, Tonemapping::AgX, Exposure::SUNLIGHT, Msaa::Off));
        match *tier {
            Tier::T1 => camera.insert(Fxaa::default()),
            Tier::T2 => camera.insert((Fxaa::default(), Bloom::NATURAL)),
            Tier::T3 | Tier::T4 => {
                camera.insert((TemporalAntiAliasing::default(), Bloom::NATURAL, MotionBlur::default()))
            }
        };
    }
}
