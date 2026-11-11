// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! R2 — shadows: the sun's cascaded shadow maps, more cascades, reach and
//! resolution as the tier rises; contact shadows for small details and soft
//! shadows whose edges widen with distance (PCSS, smoothed by TAA) at T3+.
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

/// The sun's size for soft shadows, from T3.
const SUN_SIZE: f32 = 1.0;

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
        if *tier >= Tier::T3 {
            // Soft shadows (PCSS): the sun is half a degree wide, so a
            // shadow's edge softens the further it falls from what casts it.
            // 1.0 matched a real sun's penumbra on the kart circuit's fence
            // posts at evening; 4.0 blurred them away.
            light.soft_shadow_size = Some(SUN_SIZE);
        }
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
            // Bevy's temporal shadow filter hung the Iris Xe (Mesa) a few
            // hundred frames into a lap (2026-11-11): the default filter,
            // under TAA, keeps soft shadows clean enough.
            commands.entity(camera).insert(ContactShadows::default());
        }
    }
}
