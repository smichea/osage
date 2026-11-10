// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! R2 — shadows: the sun's cascaded shadow maps, more cascades, reach and
//! resolution as the tier rises; contact shadows for small details at T3+.
//!
//! Measured on the Iris Xe reference at 1080p (T2): two cascades of 1024
//! over 80 m cost 2.6 ms, three of 2048 over 120 m 4.7 ms.

use bevy::{
    light::{CascadeShadowConfigBuilder, DirectionalLightShadowMap},
    pbr::ContactShadows,
    prelude::*,
};

use crate::{OsageCamera, Sun, Tier};

pub struct ShadowsPlugin;

impl Plugin for ShadowsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, shadow_map).add_systems(Update, (sun, camera));
    }
}

/// (cascades, reach in metres, cascade resolution)
fn settings(tier: Tier) -> (usize, f32, usize) {
    match tier {
        Tier::T1 => (2, 60.0, 1024),
        Tier::T2 => (2, 80.0, 1024),
        Tier::T3 => (4, 200.0, 2048),
        Tier::T4 => (4, 300.0, 4096),
    }
}

fn shadow_map(mut commands: Commands, tier: Res<Tier>) {
    commands.insert_resource(DirectionalLightShadowMap { size: settings(*tier).2 });
}

fn sun(mut commands: Commands, tier: Res<Tier>, mut suns: Query<(Entity, &mut DirectionalLight), Added<Sun>>) {
    let (cascades, reach, _) = settings(*tier);
    for (entity, mut light) in &mut suns {
        light.shadow_maps_enabled = true;
        let config = CascadeShadowConfigBuilder {
            num_cascades: cascades,
            first_cascade_far_bound: 12.0,
            maximum_distance: reach,
            ..default()
        };
        commands.entity(entity).insert(config.build());
    }
}

fn camera(mut commands: Commands, tier: Res<Tier>, cameras: Query<Entity, Added<OsageCamera>>) {
    if *tier >= Tier::T3 {
        for camera in &cameras {
            commands.entity(camera).insert(ContactShadows::default());
        }
    }
}
