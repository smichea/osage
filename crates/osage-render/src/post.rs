// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! R10 — post-processing and anti-aliasing. All tiers render in HDR, with
//! the exposure of a sunny day since the sun has its physical brightness
//! ([`crate::sky`]), and with the world's [`Look`]: its tonemapper, its
//! saturation, its exposure offset.
//!
//! - T1, T2: FXAA, which needs no depth prepass; T2 adds bloom.
//! - T3+: temporal anti-aliasing, bloom and motion blur; and the lens the
//!   world asks for in its look (vignette, colour fringes).
//!
//! Temporal anti-aliasing and ambient occlusion both need a depth prepass.
//! On the Intel Iris Xe reference (Mesa), the prepass and the main pass do
//! not always agree on a pixel's depth, which leaves speckles: one more
//! reason, with its cost, to keep T2 free of a prepass.

use bevy::{
    anti_alias::{fxaa::Fxaa, taa::TemporalAntiAliasing},
    camera::{Exposure, Hdr},
    post_process::{
        bloom::Bloom,
        effect_stack::{ChromaticAberration, Vignette},
        motion_blur::MotionBlur,
    },
    prelude::*,
    render::view::{ColorGrading, ColorGradingGlobal},
};

use crate::{Look, OsageCamera, Tier};

pub struct PostPlugin;

impl Plugin for PostPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (camera, regrade.run_if(resource_changed::<Look>), steady_blur));
    }
}

/// How long the shutter is open, seconds: a camera's at speed, a 180°
/// shutter at 60 frames a second.
const EXPOSURE: f32 = 1.0 / 120.0;

/// Motion blur as a camera's: an exposure of the same length whatever the
/// frame rate. A fixed shutter angle (half a frame) blurred eight times as
/// much at 8 frames a second as at 60 (T3 on an integrated GPU, 2026-11-11).
fn steady_blur(time: Res<Time>, mut blurs: Query<&mut MotionBlur>) {
    let frame = time.delta_secs();
    if frame <= 0.0 {
        return;
    }
    for mut blur in &mut blurs {
        blur.shutter_angle = (EXPOSURE / frame).clamp(0.0, 0.5);
    }
}

/// Another look (a world raced under another sky, exposed for its light):
/// cameras take its exposure and saturation.
fn regrade(look: Res<Look>, mut gradings: Query<&mut ColorGrading, With<OsageCamera>>) {
    for mut grading in &mut gradings {
        grading.global.exposure = look.exposure;
        grading.global.post_saturation = look.saturation;
    }
}

fn camera(mut commands: Commands, tier: Res<Tier>, look: Res<Look>, cameras: Query<Entity, Added<OsageCamera>>) {
    for camera in &cameras {
        let mut camera = commands.entity(camera);
        let grading = ColorGradingGlobal { exposure: look.exposure, post_saturation: look.saturation, ..default() };
        // A physical sun is about 100 000 lux: expose as a camera does in full sun.
        camera.insert((Hdr, look.tonemapping, Exposure::SUNLIGHT, Msaa::Off));
        camera.insert(ColorGrading { global: grading, ..default() });
        match *tier {
            Tier::T1 => {
                camera.insert(Fxaa::default());
            }
            Tier::T2 => {
                camera.insert((Fxaa::default(), Bloom::NATURAL));
            }
            Tier::T3 | Tier::T4 => {
                camera.insert((TemporalAntiAliasing::default(), Bloom::NATURAL, MotionBlur::default()));
                if look.vignette > 0.0 {
                    camera.insert(Vignette { intensity: look.vignette, radius: 0.85, smoothness: 2.5, ..default() });
                }
                if look.fringe > 0.0 {
                    camera.insert(ChromaticAberration { intensity: look.fringe, ..default() });
                }
            }
        }
    }
}
