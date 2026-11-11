// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! R1 — sky and sun, as they are seen. The sun has its physical brightness
//! at every tier. How the sky lights the scene is [`crate::sky_light`]'s.
//!
//! - T1: a flat sky colour and distance fog.
//! - T2: Bevy's physical atmosphere with small lookup tables — sky, sun disk
//!   and aerial perspective.
//! - T3+: the full atmosphere.
//!
//! A world that gives a photograph of the sky ([`crate::panorama::Panorama`])
//! gets it instead, at every tier.
//!
//! Measured on the Iris Xe reference at 1080p (bench/README): the atmosphere
//! costs about 4 ms with default tables — too much for T2's 16.7 ms, hence
//! the small ones.

use bevy::{
    light::{Atmosphere, SunDisk, atmosphere::ScatteringMedium, light_consts::lux},
    pbr::{AtmosphereSettings, DistanceFog, FogFalloff},
    prelude::*,
};

use crate::{OsageCamera, Sun, Tier, panorama};

const SKY_BLUE: Color = Color::srgb(0.53, 0.71, 0.88);

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        if app.world().contains_resource::<panorama::Panorama>() {
            app.add_plugins(panorama::PanoramaPlugin);
            return;
        }
        app.add_systems(Startup, spawn_atmosphere).add_systems(Update, (camera, sun));
    }
}

fn spawn_atmosphere(mut commands: Commands, tier: Res<Tier>, mut media: ResMut<Assets<ScatteringMedium>>) {
    if *tier >= Tier::T2 {
        commands.spawn(Atmosphere::earth(media.add(ScatteringMedium::earth(256, 256))));
    } else {
        commands.insert_resource(ClearColor(SKY_BLUE));
    }
}

fn camera(mut commands: Commands, tier: Res<Tier>, cameras: Query<Entity, Added<OsageCamera>>) {
    for camera in &cameras {
        let mut camera = commands.entity(camera);
        match *tier {
            Tier::T1 => camera.insert(DistanceFog {
                color: SKY_BLUE,
                falloff: FogFalloff::Linear { start: 120.0, end: 420.0 },
                ..default()
            }),
            Tier::T2 => camera.insert(AtmosphereSettings {
                transmittance_lut_size: UVec2::new(128, 64),
                transmittance_lut_samples: 20,
                multiscattering_lut_dirs: 32,
                multiscattering_lut_samples: 10,
                sky_view_lut_size: UVec2::new(200, 100),
                sky_view_lut_samples: 8,
                aerial_view_lut_size: UVec3::new(16, 16, 16),
                aerial_view_lut_samples: 5,
                sky_max_samples: 8,
                ..default()
            }),
            Tier::T3 | Tier::T4 => camera.insert(AtmosphereSettings::default()),
        };
    }
}

fn sun(mut commands: Commands, tier: Res<Tier>, mut suns: Query<(Entity, &mut DirectionalLight), Added<Sun>>) {
    for (entity, mut light) in &mut suns {
        if *tier >= Tier::T2 {
            // Above the atmosphere, which dims and tints it on the way down.
            light.illuminance = lux::RAW_SUNLIGHT;
            commands.entity(entity).insert(SunDisk::EARTH);
        } else {
            light.illuminance = lux::DIRECT_SUNLIGHT;
        }
    }
}
