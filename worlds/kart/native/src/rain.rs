// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Rain as it is seen (design/04 V2.5, D-068). When the world says a race
//! is run in the rain — or `--rain` on the bench — rain falls round the
//! camera in streaks, the road and the painted concrete darken and shine
//! as wet surfaces do (darker, a third as rough: the sky shows in them),
//! the sun is veiled, the sky and the haze go grey. The road wets and dries
//! over some seconds; the karts throw spray ([`crate::effects`]).
//!
//! The drops are fixed in the world and fall through it, wrapped in a box
//! round the camera, so that they do not move with it; one mesh, built
//! again every frame.

use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::NoFrustumCulling,
    core_pipeline::Skybox,
    mesh::{Indices, PrimitiveTopology},
    pbr::DistanceFog,
    prelude::*,
};
use osage_render::{OsageCamera, Sun, ground::GroundMaterial};

use crate::scene::Stage;

/// Drops round the camera.
const DROPS: usize = 4000;
/// The box they fall in round the camera, m: across, and high.
const ACROSS: f32 = 34.0;
const HIGH: f32 = 14.0;
/// How fast they fall, m/s, how long a streak is, m, and how thin.
const FALL: f32 = 9.0;
const STREAK: f32 = 0.7;
const THIN: f32 = 0.007;
/// Drops nearer the eye than this are not drawn: so near, a streak is a
/// bar across the picture, m.
const NEAREST: f32 = 2.0;
/// Seconds the road takes to wet or dry.
const WETTING: f32 = 6.0;
/// A wet surface's colour and roughness as shares of its dry ones.
const WET_COLOUR: f32 = 0.58;
const WET_ROUGHNESS: f32 = 0.3;
/// In the rain: the sun's light, the sky's brightness as shares of a dry
/// day's, and how far one sees in the haze, m.
const VEILED_SUN: f32 = 0.22;
const GREY_SKY: f32 = 0.45;
const HAZE: f32 = 380.0;

/// Whether it rains, and how wet things are now (eased towards it).
#[derive(Resource, Default)]
pub struct Rain {
    pub falling: bool,
    wet: f32,
    /// How much the sun and sky are veiled here (none under an overcast
    /// sky, which is grey already): what unlit smoke darkens with.
    pub veiled: f32,
    /// The drops' mesh, once made.
    drops: Option<Handle<Mesh>>,
    /// Values as they were dry, and as last written, to scale from.
    sun: Remembered<f32>,
    sky: Remembered<f32>,
    /// The haze as it was dry, and the colour last written (a fog compares
    /// by its colour).
    fog: Option<(DistanceFog, Color)>,
    wetted: f32,
}

impl Rain {
    /// Rain from the start, everything already wet: the bench's `--rain`.
    pub fn already() -> Self {
        Self { falling: true, wet: 1.0, wetted: -1.0, ..default() }
    }

    /// How wet the road is, 0 to 1.
    pub fn wet(&self) -> f32 {
        self.wet
    }
}

/// A value someone else sets (a new sky, a new track): kept as dry when it
/// is not the one written here.
struct Remembered<T> {
    dry: Option<T>,
    written: Option<T>,
}

impl<T> Default for Remembered<T> {
    fn default() -> Self {
        Self { dry: None, written: None }
    }
}

impl<T: Clone + PartialEq> Remembered<T> {
    /// The dry value behind `now`, and writes `wet(dry)` in its place.
    fn scale(&mut self, now: &mut T, wet: impl Fn(&T) -> T) {
        if self.written.as_ref() != Some(now) {
            self.dry = Some(now.clone());
        }
        let Some(dry) = &self.dry else { return };
        let scaled = wet(dry);
        *now = scaled.clone();
        self.written = Some(scaled);
    }
}

/// The surfaces that wet, with their dry colour and roughness: the road
/// (a ground) and the painted concrete of kerbs and walls.
#[derive(Resource, Default)]
pub struct Surfaces {
    pub grounds: Vec<(Handle<GroundMaterial>, LinearRgba, f32)>,
    pub plain: Vec<(Handle<StandardMaterial>, LinearRgba, f32)>,
}

pub struct RainPlugin;

impl Plugin for RainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Rain>()
            .init_resource::<Surfaces>()
            .add_systems(Update, (wetten, fall).chain().in_set(Stage::Draw));
    }
}

/// A drop's place in the box from its index: x, z across, y high, each 0..1.
fn seat(i: usize) -> Vec3 {
    let mut h = (i as u32).wrapping_mul(0x9e37_79b9) ^ 0x7261_696e;
    let mut next = || {
        h = (h ^ (h >> 15)).wrapping_mul(0x85eb_ca77);
        h = (h ^ (h >> 13)).wrapping_mul(0xc2b2_ae3d);
        ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
    };
    Vec3::new(next(), next(), next())
}

/// Where drop `i` is at `time` round a camera at `eye`: fixed in the world
/// across, falling, both wrapped in the box round the eye.
pub fn drop_at(i: usize, eye: Vec3, time: f32) -> Vec3 {
    let s = seat(i);
    let wrap =
        |value: f32, centre: f32, size: f32| centre - size / 2.0 + (value - centre + size / 2.0).rem_euclid(size);
    let x = wrap(s.x * ACROSS * 7.0, eye.x, ACROSS);
    let z = wrap(s.z * ACROSS * 7.0, eye.z, ACROSS);
    let y = wrap(s.y * HIGH - time * FALL, eye.y, HIGH);
    Vec3::new(x, y, z)
}

/// The streaks of every drop, as quads facing the eye.
fn streaks(eye: Vec3, time: f32) -> Mesh {
    let mut positions = Vec::with_capacity(DROPS * 4);
    for i in 0..DROPS {
        let top = drop_at(i, eye, time);
        if (top - eye).with_y(0.0).length() < NEAREST {
            // Folded away: a quad of no size.
            positions.extend([top.to_array(); 4]);
            continue;
        }
        let bottom = top - Vec3::Y * STREAK;
        let towards = (eye - top).with_y(0.0).normalize_or(Vec3::Z);
        let side = Vec3::new(-towards.z, 0.0, towards.x) * THIN;
        for corner in [top - side, bottom - side, bottom + side, top + side] {
            positions.push(corner.to_array());
        }
    }
    let indices: Vec<u32> = (0..DROPS as u32).flat_map(|q| [0, 1, 3, 3, 1, 2].map(|k| q * 4 + k)).collect();
    let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_indices(Indices::U32(indices))
}

/// The drops, round the camera, while it rains.
#[allow(clippy::too_many_arguments)]
fn fall(
    mut commands: Commands,
    time: Res<Time>,
    mut rain: ResMut<Rain>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    camera: Query<&GlobalTransform, With<OsageCamera>>,
    mut shown: Query<(&mut Visibility, &MeshMaterial3d<StandardMaterial>), With<Drops>>,
    look: Option<Res<osage_render::Look>>,
) {
    let Ok(eye) = camera.single().map(|c| c.translation()) else { return };
    if rain.drops.is_none() {
        let mesh = meshes.add(streaks(eye, 0.0));
        let material = materials.add(StandardMaterial {
            base_color: Color::srgba(0.75, 0.78, 0.82, 0.1),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });
        commands.spawn((
            Drops,
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material),
            Transform::default(),
            bevy::light::NotShadowCaster,
            NoFrustumCulling,
            Visibility::Hidden,
        ));
        rain.drops = Some(mesh);
    }
    let falling = rain.falling;
    for (mut visibility, material) in &mut shown {
        visibility.set_if_neq(if falling { Visibility::Inherited } else { Visibility::Hidden });
        // Exposed like the rest: as light against a grey sky as a sunny one (D-069).
        let shade = (1.6 * look.as_ref().map_or(1.0, |look| crate::scene::unlit(look))).min(1.0);
        let wanted = Color::srgba(0.78 * shade, 0.8 * shade, 0.83 * shade, 0.13);
        if materials.get(&material.0).is_some_and(|m| m.base_color != wanted)
            && let Some(mut material) = materials.get_mut(&material.0)
        {
            material.base_color = wanted;
        }
    }
    if falling && let Some(mesh) = rain.drops.as_ref().and_then(|handle| meshes.get_mut(handle)) {
        let mut mesh = mesh;
        *mesh = streaks(eye, time.elapsed_secs());
    }
}

#[derive(Component)]
struct Drops;

/// Wets and dries the road, veils the sun, greys the sky.
#[allow(clippy::too_many_arguments)]
fn wetten(
    time: Res<Time>,
    mut rain: ResMut<Rain>,
    surfaces: Res<Surfaces>,
    mut grounds: ResMut<Assets<GroundMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut suns: Query<&mut DirectionalLight, With<Sun>>,
    mut cameras: Query<(Option<&mut Skybox>, Option<&mut DistanceFog>), With<OsageCamera>>,
    shown: Option<Res<crate::scene::SkyShown>>,
) {
    let target = if rain.falling { 1.0 } else { 0.0 };
    let step = time.delta_secs() / WETTING;
    rain.wet += (target - rain.wet).clamp(-step, step);
    let wet = rain.wet;
    if (wet - rain.wetted).abs() > 0.01 || surfaces.is_changed() || (wet != rain.wetted && wet % 1.0 == 0.0) {
        rain.wetted = wet;
        let colour = |dry: LinearRgba| dry * (1.0 - (1.0 - WET_COLOUR) * wet);
        let rough = |dry: f32| dry * (1.0 - (1.0 - WET_ROUGHNESS) * wet);
        for (handle, dry, roughness) in &surfaces.grounds {
            if let Some(mut material) = grounds.get_mut(handle) {
                material.base.base_color = colour(*dry).with_alpha(dry.alpha).into();
                material.base.perceptual_roughness = rough(*roughness);
            }
        }
        for (handle, dry, roughness) in &surfaces.plain {
            if let Some(mut material) = materials.get_mut(handle) {
                material.base_color = colour(*dry).with_alpha(dry.alpha).into();
                material.perceptual_roughness = rough(*roughness);
            }
        }
    }
    // Under a photographed overcast sky (D-069) the sun and the sky are
    // already grey; under another, they are veiled here. The haze is the
    // rain's either way.
    let overcast = shown.is_some_and(|shown| shown.sky == crate::scene::Sky::Overcast);
    let gloom = if overcast { 0.0 } else { wet };
    rain.veiled = gloom;
    let haze = wet;
    let rain = &mut *rain;
    for mut light in &mut suns {
        rain.sun.scale(&mut light.illuminance, |dry| dry * (1.0 - (1.0 - VEILED_SUN) * gloom));
    }
    for (skybox, fog) in &mut cameras {
        if let Some(mut skybox) = skybox {
            rain.sky.scale(&mut skybox.brightness, |dry| dry * (1.0 - (1.0 - GREY_SKY) * gloom));
        }
        if let Some(mut fog) = fog {
            if rain.fog.as_ref().is_none_or(|(_, written)| *written != fog.color) {
                rain.fog = Some((fog.clone(), fog.color));
            }
            if let Some((dry, written)) = &mut rain.fog {
                let colour = dry.color.mix(&Color::srgb(0.52, 0.55, 0.58), haze);
                fog.falloff = if haze > 0.0 {
                    let visibility = 1.0 / (1.0 / 3000.0 + haze / HAZE);
                    FogFalloff::from_visibility_colors(visibility, colour, colour)
                } else {
                    dry.falloff.clone()
                };
                fog.color = colour;
                *written = colour;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_fall_round_the_eye_and_stay_where_they_are_as_it_moves() {
        let eye = Vec3::new(100.0, 5.0, -40.0);
        for i in 0..200 {
            let at = drop_at(i, eye, 3.0);
            assert!((at.x - eye.x).abs() <= ACROSS / 2.0 && (at.z - eye.z).abs() <= ACROSS / 2.0, "{at:?}");
            assert!((at.y - eye.y).abs() <= HIGH / 2.0, "{at:?}");
            // A little later it is lower (or wrapped back to the top).
            let later = drop_at(i, eye, 3.05);
            assert!(later.y < at.y || later.y > at.y + HIGH * 0.9, "{at:?} {later:?}");
            // The eye moving a metre leaves it where it was, unless it wrapped.
            let moved = drop_at(i, eye + Vec3::X, 3.0);
            assert!(moved.x == at.x || (moved.x - at.x).abs() > ACROSS * 0.9, "{at:?} {moved:?}");
        }
    }

    #[test]
    fn a_value_set_elsewhere_is_kept_as_dry() {
        let mut sun = Remembered::default();
        let mut light = 100.0f32;
        sun.scale(&mut light, |dry| dry * 0.5);
        assert_eq!(light, 50.0);
        sun.scale(&mut light, |dry| dry * 0.5);
        assert_eq!(light, 50.0, "scaled from the dry value, not again");
        light = 80.0; // a new sky
        sun.scale(&mut light, |dry| dry * 0.5);
        assert_eq!(light, 40.0);
    }
}
