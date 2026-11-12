// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! R1 — light from the sky. Outdoors, what is not lit by the sun is lit by
//! the sky above and by the ground below: shadows are blue, undersides take
//! the colour of the grass, and glossy things mirror the horizon. A constant
//! ambient light gives none of that.
//!
//! - T1, T2: a constant ambient light of a clear sky's brightness. Lighting
//!   a scene from any cubemap costs 2 to 3 ms on the Iris Xe reference at
//!   T2, even from a cubemap of 8 × 8 texels: the cost is in the shader that
//!   reads it, not in the cubemap (bench, 2026-11-10).
//! - T3+: the atmosphere lights the scene, recomputed every frame — or the
//!   world's photograph of the sky, the sun taken out ([`crate::panorama`]).
//!   Where the world has neither (`--without sky`), a cubemap of a clear day
//!   stands in, computed once on the processor from the sun's direction and
//!   the world's ground colour ([`Look`]).
//!
//! Left out, every tier gets the constant ambient light.

use bevy::{
    asset::RenderAssetUsages,
    color::ColorToComponents,
    light::{AtmosphereEnvironmentMapLight, EnvironmentMapLight},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension},
};

use crate::{LeftOut, Look, OsageCamera, Sun, Technique, Tier, pace::Pace, panorama::Prepared};

/// Luminance of a clear sky overhead, cd/m²: what one unit of the cubemap stands for.
pub const CLEAR_SKY: f32 = 6000.0;
/// Side of a face of the mirror-like cubemap; each level of blur halves it.
const SHARP: usize = 64;
/// Side of a face of the cubemap that lights matte surfaces.
const SOFT: usize = 8;
/// Directions averaged for each blurred texel.
const SAMPLES: usize = 48;

pub struct SkyLightPlugin;

impl Plugin for SkyLightPlugin {
    fn build(&self, app: &mut App) {
        if app.world().get_resource::<Tier>().is_some_and(|tier| *tier >= Tier::T3) {
            app.insert_resource(GlobalAmbientLight::NONE).add_systems(Update, light);
        }
    }
}

/// The constant light that stands in when this technique is left out.
pub fn constant_ambient() -> GlobalAmbientLight {
    GlobalAmbientLight { color: Color::srgb(0.78, 0.87, 1.0), brightness: CLEAR_SKY, ..default() }
}

/// Gives each new camera its sky light, and computes it again when the sun turns.
#[allow(clippy::too_many_arguments)]
fn light(
    mut commands: Commands,
    tier: Res<Tier>,
    look: Res<Look>,
    left_out: Res<LeftOut>,
    mut images: ResMut<Assets<Image>>,
    panorama: Option<Res<Prepared>>,
    suns: Query<Ref<GlobalTransform>, With<Sun>>,
    cameras: Query<(Entity, Has<EnvironmentMapLight>, Has<AtmosphereEnvironmentMapLight>), With<OsageCamera>>,
    pace: Option<Res<Pace>>,
) {
    // Traded for a constant light by the frame-rate governor.
    if pace.is_some_and(|pace| !pace.rung().sky_light) {
        return;
    }
    // The world's photograph of the sky, where it gave one.
    if let Some(panorama) = panorama {
        for (camera, lit, _) in &cameras {
            if !lit || panorama.is_changed() {
                commands.entity(camera).insert(EnvironmentMapLight {
                    diffuse_map: panorama.light.0.clone(),
                    specular_map: panorama.light.1.clone(),
                    intensity: panorama.scale,
                    ..default()
                });
            }
        }
        return;
    }
    // The atmosphere does it, where there is one.
    if !left_out.0.contains(&Technique::Sky) {
        for (camera, _, lit) in &cameras {
            if !lit {
                let size = UVec2::splat(if *tier == Tier::T4 { 256 } else { 128 });
                commands.entity(camera).insert(AtmosphereEnvironmentMapLight { size, ..default() });
            }
        }
        return;
    }
    let Some(sun) = suns.iter().next() else { return };
    if !sun.is_changed() && !look.is_changed() && cameras.iter().all(|(_, lit, _)| lit) {
        return;
    }
    let to_sun = sun.back().as_vec3();
    let ground = look.ground.to_linear().to_vec3();
    let (diffuse, specular) = cubemaps(|direction| clear_day(direction, to_sun, ground));
    let (diffuse_map, specular_map) = (images.add(diffuse), images.add(specular));
    for (camera, ..) in &cameras {
        commands.entity(camera).insert(EnvironmentMapLight {
            diffuse_map: diffuse_map.clone(),
            specular_map: specular_map.clone(),
            intensity: CLEAR_SKY,
            ..default()
        });
    }
}

/// The light that arrives from `direction` on a clear day, the sun itself
/// left out (it is a light of its own). One unit is the luminance of the sky
/// overhead. `to_sun` points at the sun; `ground` is the linear colour of the
/// land, which the sun and the sky light and which shines back from below.
pub fn clear_day(direction: Vec3, to_sun: Vec3, ground: Vec3) -> Vec3 {
    const ZENITH: Vec3 = Vec3::new(0.30, 0.55, 1.0);
    const HORIZON: Vec3 = Vec3::new(1.05, 1.25, 1.45);
    const GLOW: Vec3 = Vec3::new(1.0, 0.86, 0.62);
    let up = direction.y;
    // The sky pales and brightens towards the horizon, and glows around the sun.
    let low = (1.0 - up.max(0.0)).powi(4);
    let near_sun = direction.dot(to_sun).max(0.0);
    let sky = ZENITH.lerp(HORIZON, low) + GLOW * (1.6 * near_sun.powi(48) + 0.25 * near_sun.powi(6));
    // The land is lit by the sun, more the higher it stands, and by the sky.
    // Grass sends back about a fifth of what falls on it.
    let land = ground * (0.5 + 1.4 * to_sun.y.max(0.0));
    // A band of haze joins them: the land far away is the colour of the air.
    let below = ((-up) / 0.12).clamp(0.0, 1.0);
    if up >= 0.0 { sky } else { HORIZON.lerp(land, below * below * (3.0 - 2.0 * below)) }
}

/// The direction of a point of a cube face, by the convention of cubemaps:
/// faces +X, -X, +Y, -Y, +Z, -Z, with `u` to the right and `v` down, each
/// in -1..1.
pub(crate) fn face_direction(face: usize, u: f32, v: f32) -> Vec3 {
    let direction = match face {
        0 => Vec3::new(1.0, -v, -u),
        1 => Vec3::new(-1.0, -v, u),
        2 => Vec3::new(u, 1.0, v),
        3 => Vec3::new(u, -1.0, -v),
        4 => Vec3::new(u, -v, 1.0),
        _ => Vec3::new(-u, -v, -1.0),
    };
    // Cubemaps are left-handed, the world is right-handed: Bevy looks a
    // world direction up with its z negated, so a texel stands for that.
    Vec3::new(direction.x, direction.y, -direction.z).normalize()
}

/// `SAMPLES` directions around `axis`, spread as a lobe `cos^power`: their
/// plain average is the light a surface of that gloss would gather.
fn lobe(axis: Vec3, power: f32) -> impl Iterator<Item = Vec3> {
    let (side, other) = axis.any_orthonormal_pair();
    (0..SAMPLES).map(move |i| {
        let cos = ((i as f32 + 0.5) / SAMPLES as f32).powf(1.0 / (power + 1.0));
        let sin = (1.0 - cos * cos).max(0.0).sqrt();
        // The golden angle spreads the samples evenly around the axis.
        let (s, c) = (i as f32 * 2.399_963).sin_cos();
        axis * cos + (side * c + other * s) * sin
    })
}

/// One face of one level: `side` by `side` texels of a sky blurred by `power`
/// (`None`: as it is), as half floats; brighter than half floats go is kept
/// at their largest.
pub(crate) fn face(sky: &impl Fn(Vec3) -> Vec3, face: usize, side: usize, power: Option<f32>, data: &mut Vec<u8>) {
    for y in 0..side {
        for x in 0..side {
            let centre = |i: usize| 2.0 * (i as f32 + 0.5) / side as f32 - 1.0;
            let direction = face_direction(face, centre(x), centre(y));
            let colour = match power {
                None => sky(direction),
                Some(power) => lobe(direction, power).map(sky).sum::<Vec3>() / SAMPLES as f32,
            };
            for channel in [colour.x, colour.y, colour.z, 1.0] {
                data.extend_from_slice(&half::f16::from_f32(channel.min(65_000.0)).to_le_bytes());
            }
        }
    }
}

/// The blur of each level of a mirror-like cubemap of `levels` levels: Bevy
/// reads level `roughness × (levels - 1)`. A roughness `r` spreads light like
/// `cos^(2 / r⁴ - 2)`.
fn blur(level: usize, levels: usize) -> Option<f32> {
    let roughness = level as f32 / (levels - 1) as f32;
    let power = 2.0 / roughness.powi(4).max(1e-6) - 2.0;
    // Sharper than the texels themselves: nothing to average.
    (power < 600.0).then_some(power.max(0.0))
}

/// A cubemap of half floats, `side` texels a face, from `data`: face after
/// face, each with its `levels` levels.
pub(crate) fn cube(side: usize, levels: usize, data: Vec<u8>) -> Image {
    let size = Extent3d { width: side as u32, height: side as u32, depth_or_array_layers: 6 };
    let usage = RenderAssetUsages::RENDER_WORLD;
    // Made with its first level's size, then given all of its levels.
    let first = vec![0; 6 * side * side * 8];
    let mut image = Image::new(size, TextureDimension::D2, first, TextureFormat::Rgba16Float, usage);
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels as u32;
    image.texture_view_descriptor =
        Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::Cube), ..default() });
    image
}

/// The two cubemaps Bevy lights a scene with: one for matte surfaces (each
/// texel the light gathered by a surface facing that way), one for glossy
/// ones (each level blurrier than the one before).
pub fn cubemaps(sky: impl Fn(Vec3) -> Vec3) -> (Image, Image) {
    // A matte surface gathers light as the cosine of its angle: power 1.
    let mut soft = Vec::new();
    for f in 0..6 {
        face(&sky, f, SOFT, Some(1.0), &mut soft);
    }
    // Face after face, each with all its levels: the order Bevy expects.
    let levels = SHARP.ilog2() as usize + 1;
    let mut sharp = Vec::new();
    for f in 0..6 {
        for level in 0..levels {
            face(&sky, f, SHARP >> level, blur(level, levels), &mut sharp);
        }
    }
    (cube(SOFT, 1, soft), cube(SHARP, levels, sharp))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRASS: Vec3 = Vec3::new(0.1, 0.3, 0.05);

    fn sun() -> Vec3 {
        Vec3::new(0.5, 0.6, 0.3).normalize()
    }

    #[test]
    fn faces_point_where_cubemaps_say() {
        // Face centres, with z negated as Bevy does when it looks a direction up.
        let centres = [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::NEG_Z, Vec3::Z];
        for (face, centre) in centres.into_iter().enumerate() {
            assert!(face_direction(face, 0.0, 0.0).distance(centre) < 1e-6, "face {face}");
        }
        // On a side face, down the image is down in the world.
        assert!(face_direction(0, 0.0, 0.9).y < 0.0);
    }

    #[test]
    fn a_lobe_stays_around_its_axis_and_narrows_with_gloss() {
        let axis = Vec3::new(0.3, 0.8, -0.5).normalize();
        let spread = |power: f32| lobe(axis, power).map(|d| d.dot(axis)).sum::<f32>() / SAMPLES as f32;
        assert!(lobe(axis, 1.0).all(|d| (d.length() - 1.0).abs() < 1e-4 && d.dot(axis) >= 0.0));
        // The mean cosine of cos^n is (n + 1) / (n + 2).
        assert!((spread(1.0) - 2.0 / 3.0).abs() < 0.02, "{}", spread(1.0));
        assert!(spread(100.0) > 0.98);
    }

    #[test]
    fn the_sky_is_blue_above_and_the_colour_of_the_land_below() {
        let sky = |direction| clear_day(direction, sun(), GRASS);
        let (up, down) = (sky(Vec3::Y), sky(Vec3::NEG_Y));
        assert!(up.z > up.x * 2.0, "blue overhead: {up}");
        assert!(down.y > down.x * 2.0 && down.y > down.z * 2.0, "green below: {down}");
        // Brighter towards the horizon and around the sun.
        let horizon = sky(Vec3::new(-0.5, 0.02, -0.3).normalize());
        assert!(horizon.length() > up.length());
        assert!(sky(sun()).length() > sky(Vec3::new(-sun().x, sun().y, -sun().z)).length());
        // No jump at the horizon.
        let (above, below) = (sky(Vec3::new(1.0, 1e-4, 0.0).normalize()), sky(Vec3::new(1.0, -1e-4, 0.0).normalize()));
        assert!(above.distance(below) < 0.05);
    }

    #[test]
    fn levels_blur_from_a_mirror_to_a_matte_surface() {
        let levels = SHARP.ilog2() as usize + 1;
        assert_eq!(blur(0, levels), None);
        assert_eq!(blur(levels - 1, levels), Some(0.0));
        let powers: Vec<f32> = (0..levels).filter_map(|level| blur(level, levels)).collect();
        assert!(powers.windows(2).all(|pair| pair[0] > pair[1]), "{powers:?}");
    }

    #[test]
    fn the_cubemaps_have_the_size_bevy_expects() {
        let (soft, sharp) = cubemaps(|direction| clear_day(direction, sun(), GRASS));
        assert_eq!(soft.data.as_ref().unwrap().len(), 6 * SOFT * SOFT * 8);
        let texels: usize = (0..7).map(|level| (SHARP >> level) * (SHARP >> level)).sum();
        assert_eq!(sharp.texture_descriptor.mip_level_count, 7);
        assert_eq!(sharp.data.as_ref().unwrap().len(), 6 * texels * 8);

        // A matte surface facing up gathers the sky; facing down, the land.
        let texel = |image: &Image, face: usize| -> Vec3 {
            let at = face * SOFT * SOFT * 8 + (SOFT / 2 * SOFT + SOFT / 2) * 8;
            let data = image.data.as_ref().unwrap();
            let channel = |i: usize| half::f16::from_le_bytes([data[at + 2 * i], data[at + 2 * i + 1]]).to_f32();
            Vec3::new(channel(0), channel(1), channel(2))
        };
        let (up, down) = (texel(&soft, 2), texel(&soft, 3));
        assert!(up.z > up.x && down.y > down.z, "up {up}, down {down}");
        // Averaging over half the sphere: between the darkest and the brightest of the sky.
        assert!(up.length() > 0.5 && up.length() < 5.0, "{up}");
    }
}
