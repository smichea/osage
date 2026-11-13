// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! R1 — a photographed sky. A world that gives a [`Panorama`] (an
//! equirectangular HDR photograph of the sky, in linear light) gets it
//! instead of the computed atmosphere ([`crate::sky`]):
//!
//! - **every tier** sees it as the sky, and the land fades into the colour of
//!   its horizon with distance;
//! - **the sun** is found in it — where it is and how bright it is against
//!   the sky — and the world's [`Sun`] is turned and set to match;
//! - **light from the sky** ([`crate::sky_light`]): from T3, the scene is lit
//!   by the panorama with the sun taken out (it is a light of its own); below,
//!   by a constant light of the panorama's colour.
//!
//! A photograph has no absolute brightness. Its sun does: an unclipped
//! panorama keeps the ratio of sun to sky, so the whole panorama is scaled
//! for its sun to give [`SUN`] lux, and the sky then has its true brightness.
//! An overcast sky has no sun to go by: the world says how bright it is
//! overhead ([`Panorama::overhead`], cd/m²), the panorama is scaled to
//! that, and the sun gives what the brightest of its clouds gives — little.

use bevy::{
    camera::Exposure,
    color::ColorToComponents,
    light::{Skybox, light_consts::lux},
    pbr::{DistanceFog, FogFalloff},
    prelude::*,
};

use crate::{Look, OsageCamera, Sun, Tier, sky_light};

/// What the panorama's sun gives a surface facing it, lux.
pub const SUN: f32 = lux::DIRECT_SUNLIGHT;
/// Distance at which the land has faded halfway into the haze, metres: a
/// summer day's, which blues the hills a kilometre off.
const VISIBILITY: f32 = 3000.0;
/// Around the sun's centre, what is the sun: degrees.
const SUN_RADIUS: f32 = 2.5;
/// The light from the sky is at most this many times the sky's median.
const CEILING: f32 = 6.0;

/// An equirectangular photograph of the sky in linear light: `width` by
/// `height` pixels, row by row from straight up down to straight down;
/// across, a full turn, starting behind the viewer. `turn` turns it about
/// the vertical, radians: a world places its sun where it wants it.
#[derive(Resource, Clone)]
pub struct Panorama {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<[f32; 3]>,
    pub turn: f32,
    /// How bright the sky is straight up, cd/m², for a sky with no sun
    /// in it (overcast); none, the sun sets the scale.
    pub overhead: Option<f32>,
}

/// What the techniques use of the panorama, computed again when the
/// photograph changes.
#[derive(Resource)]
pub struct Prepared {
    /// The sky as seen.
    pub skybox: Handle<Image>,
    /// The light from the sky and the land, the sun left out: (matte, glossy).
    pub light: (Handle<Image>, Handle<Image>),
    /// cd/m² for one unit of the panorama.
    pub scale: f32,
    pub to_sun: Vec3,
    /// What the sun gives a surface facing it, lux: [`SUN`], but under an
    /// overcast sky.
    pub sun_lux: f32,
    pub sun_colour: Color,
    /// The constant light that stands in below T3.
    pub ambient: GlobalAmbientLight,
    /// The colour of the air at the horizon, as exposed for a sunny day.
    pub haze: Color,
}

pub struct PanoramaPlugin;

impl Plugin for PanoramaPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, (prepare_once, ambient).chain()).add_systems(
            Update,
            (again.run_if(resource_changed::<Panorama>), ambient.run_if(resource_changed::<Prepared>), camera, sun)
                .chain(),
        );
    }
}

fn prepare_once(
    mut commands: Commands,
    panorama: Res<Panorama>,
    tier: Res<Tier>,
    look: Res<Look>,
    mut images: ResMut<Assets<Image>>,
) {
    commands.insert_resource(prepare(&panorama, *tier, &look, &mut images));
}

/// Another photograph of the sky (a world raced under another sky): its
/// sun, light and skybox are worked out again, and cameras and the sun
/// take them. The first run is the photograph `prepare_once` prepared.
fn again(
    mut commands: Commands,
    panorama: Res<Panorama>,
    tier: Res<Tier>,
    look: Res<Look>,
    mut images: ResMut<Assets<Image>>,
    mut first: Local<bool>,
) {
    if !std::mem::replace(&mut *first, true) {
        return;
    }
    commands.insert_resource(prepare(&panorama, *tier, &look, &mut images));
}

fn prepare(panorama: &Panorama, tier: Tier, look: &Look, images: &mut Assets<Image>) -> Prepared {
    let face = if tier >= Tier::T3 { 1024 } else { 512 };
    let analysis = Analysis::of(panorama, look.ground.to_linear().to_vec3());
    let skybox = images.add(analysis.skybox(face));
    let (matte, glossy) = sky_light::cubemaps(|d| analysis.light(d));
    let scale = analysis.scale;
    info!(
        sun_elevation = analysis.to_sun.y.asin().to_degrees(),
        // From +Z towards +X, in the world: where a world turns the photograph to.
        sun_azimuth = analysis.to_sun.x.atan2(analysis.to_sun.z).to_degrees(),
        sun_colour = ?analysis.sun_colour.to_array(),
        sky_overhead = analysis.radiance(Vec3::Y) * scale,
        "panorama: sun found, sky in cd/m²"
    );
    let ambient = analysis.ambient();
    let haze = analysis.haze() * scale * Exposure::SUNLIGHT.exposure();
    Prepared {
        skybox,
        light: (images.add(matte), images.add(glossy)),
        scale,
        to_sun: analysis.to_sun,
        sun_lux: analysis.sun_illuminance * scale,
        sun_colour: Color::linear_rgb(analysis.sun_colour.x, analysis.sun_colour.y, analysis.sun_colour.z),
        ambient: GlobalAmbientLight {
            color: {
                let c = ambient / ambient.max_element().max(1e-6);
                Color::linear_rgb(c.x, c.y, c.z)
            },
            brightness: ambient.max_element() * scale,
            ..default()
        },
        haze: Color::linear_rgb(haze.x, haze.y, haze.z),
    }
}

/// Below T3, the panorama's colour as a constant light ([`crate::sky_light`]).
fn ambient(mut commands: Commands, tier: Res<Tier>, prepared: Res<Prepared>) {
    if *tier < Tier::T3 {
        commands.insert_resource(prepared.ambient.clone());
    }
}

fn camera(mut commands: Commands, prepared: Res<Prepared>, cameras: Query<(Entity, Ref<OsageCamera>)>) {
    for (camera, _) in cameras.iter().filter(|(_, c)| c.is_added() || prepared.is_changed()) {
        commands.entity(camera).insert((
            Skybox { image: Some(prepared.skybox.clone()), brightness: prepared.scale, ..default() },
            DistanceFog {
                color: prepared.haze,
                falloff: FogFalloff::from_visibility_colors(VISIBILITY, prepared.haze, prepared.haze),
                ..default()
            },
        ));
    }
}

fn sun(prepared: Res<Prepared>, mut suns: Query<(&mut Transform, &mut DirectionalLight, Ref<Sun>)>) {
    for (mut transform, mut light, _) in suns.iter_mut().filter(|(.., s)| s.is_added() || prepared.is_changed()) {
        *transform = Transform::default().looking_to(-prepared.to_sun, Vec3::Y);
        light.illuminance = prepared.sun_lux;
        light.color = prepared.sun_colour;
    }
}

/// The panorama measured: where its sun is, how bright, and its sky without it.
struct Analysis<'a> {
    panorama: &'a Panorama,
    /// Towards the sun, in the world (the panorama turned).
    to_sun: Vec3,
    /// The sun's colour, of luminance one.
    sun_colour: Vec3,
    /// The sky's colour just around the sun: what stands in for it.
    around_sun: Vec3,
    /// The brightest the light from the sky may be, in units of the
    /// panorama: a few times its median. The glow around the sun, outside
    /// its disc, would otherwise light glossy things like a second sun.
    ceiling: f32,
    /// cd/m² for one unit of the panorama.
    scale: f32,
    /// What the sun gives a surface facing it, in units of the panorama.
    sun_illuminance: f32,
    /// What the sky alone gives a surface facing up, in units of the panorama.
    sky_illuminance: f32,
    /// The linear colour of the land.
    ground: Vec3,
}

fn luminance(c: Vec3) -> f32 {
    0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z
}

impl<'a> Analysis<'a> {
    fn of(panorama: &'a Panorama, ground: Vec3) -> Self {
        let (w, h) = (panorama.width, panorama.height);
        let pixel = |x: usize, y: usize| Vec3::from(panorama.pixels[y * w + x]);
        let raw_direction = |x: usize, y: usize| {
            let longitude = (x as f32 + 0.5) / w as f32 * std::f32::consts::TAU - std::f32::consts::PI;
            let latitude = std::f32::consts::FRAC_PI_2 - (y as f32 + 0.5) / h as f32 * std::f32::consts::PI;
            Vec3::new(latitude.cos() * longitude.sin(), latitude.sin(), -latitude.cos() * longitude.cos())
        };
        // A pixel's solid angle shrinks towards the poles.
        let solid_angle = |y: usize| {
            (std::f32::consts::TAU / w as f32) * (std::f32::consts::PI / h as f32) * raw_direction(0, y).y.acos().sin()
        };

        // The sun: the brightest pixel of the upper half, and what is bright around it.
        let (mut brightest, mut at) = (0.0, (0, 0));
        for y in 0..h / 2 {
            for x in 0..w {
                let l = luminance(pixel(x, y));
                if l > brightest {
                    (brightest, at) = (l, (x, y));
                }
            }
        }
        let centre = raw_direction(at.0, at.1);
        let (cos_inner, cos_outer) = (SUN_RADIUS.to_radians().cos(), (2.0 * SUN_RADIUS).to_radians().cos());
        let (mut ring, mut ring_weight) = (Vec3::ZERO, 0.0);
        let (mut sun, mut sun_direction) = (Vec3::ZERO, Vec3::ZERO);
        let mut sky_illuminance = 0.0;
        let rows = (0..h).filter(|&y| raw_direction(0, y).y > -0.1);
        for y in rows {
            let omega = solid_angle(y);
            for x in 0..w {
                let (c, d) = (pixel(x, y), raw_direction(x, y));
                let cos = d.dot(centre);
                if cos > cos_inner {
                    sun += c * omega;
                    sun_direction += d * luminance(c) * omega;
                } else {
                    if cos > cos_outer {
                        ring += c * omega;
                        ring_weight += omega;
                    }
                    sky_illuminance += luminance(c) * d.y.max(0.0) * omega;
                }
            }
        }
        let around_sun = ring / ring_weight.max(1e-9);
        let mut upper: Vec<f32> = (0..h / 2)
            .step_by(4)
            .flat_map(|y| (0..w).step_by(4).map(move |x| (x, y)))
            .map(|(x, y)| luminance(pixel(x, y)))
            .collect();
        upper.sort_by(f32::total_cmp);
        let ceiling = CEILING * upper.get(upper.len() / 2).copied().unwrap_or(1.0).max(1e-6);
        // The sun's own light: what is above the sky around it, within its circle.
        let inner_area = std::f32::consts::TAU * (1.0 - cos_inner);
        let excess = (sun - around_sun * inner_area).max(Vec3::ZERO);
        let sun_illuminance = luminance(excess).max(1e-9);
        // The sky's light in the sun's circle, which the circle hid from the sum above.
        sky_illuminance += luminance(around_sun) * inner_area * centre.y.max(0.0);
        let turn = Quat::from_rotation_y(panorama.turn);
        let mut analysis = Self {
            panorama,
            to_sun: turn * sun_direction.normalize_or(centre),
            sun_colour: excess / sun_illuminance,
            around_sun,
            ceiling,
            scale: SUN / sun_illuminance,
            sun_illuminance,
            sky_illuminance,
            ground,
        };
        if let Some(overhead) = panorama.overhead {
            analysis.scale = overhead / analysis.radiance(Vec3::Y).max(1e-9);
        }
        analysis
    }

    /// The panorama in direction `d` of the world, bilinear, as photographed.
    fn radiance(&self, d: Vec3) -> f32 {
        luminance(self.sample(d))
    }

    fn sample(&self, d: Vec3) -> Vec3 {
        let p = self.panorama;
        let d = Quat::from_rotation_y(-p.turn) * d.normalize();
        let longitude = d.x.atan2(-d.z);
        let latitude = d.y.clamp(-1.0, 1.0).asin();
        let u = (longitude + std::f32::consts::PI) / std::f32::consts::TAU * p.width as f32 - 0.5;
        let v = ((std::f32::consts::FRAC_PI_2 - latitude) / std::f32::consts::PI * p.height as f32 - 0.5)
            .clamp(0.0, p.height as f32 - 1.0);
        let (x0, y0) = (u.floor(), v.floor());
        let (fx, fy) = (u - x0, v - y0);
        let at = |x: f32, y: f32| {
            let x = (x as isize).rem_euclid(p.width as isize) as usize;
            let y = (y as usize).min(p.height - 1);
            Vec3::from(p.pixels[y * p.width + x])
        };
        let top = at(x0, y0).lerp(at(x0 + 1.0, y0), fx);
        let bottom = at(x0, y0 + 1.0).lerp(at(x0 + 1.0, y0 + 1.0), fx);
        top.lerp(bottom, fy)
    }

    /// The light from direction `d`, the sun taken out. Below the horizon, the
    /// land, lit by the sun and the sky, fades from the horizon's haze.
    fn light(&self, d: Vec3) -> Vec3 {
        let d = d.normalize();
        let sky =
            if d.dot(self.to_sun) > (1.5 * SUN_RADIUS).to_radians().cos() { self.around_sun } else { self.sample(d) };
        let bright = luminance(sky);
        let sky = if bright > self.ceiling { sky * (self.ceiling / bright) } else { sky };
        if d.y >= 0.0 {
            return sky;
        }
        let horizon = self.sample(Vec3::new(d.x, 0.02, d.z));
        let lit = self.sun_illuminance * self.to_sun.y.max(0.0) + self.sky_illuminance;
        let land = self.ground * lit / std::f32::consts::PI;
        let below = (-d.y / 0.12).clamp(0.0, 1.0);
        horizon.lerp(land, below * below * (3.0 - 2.0 * below))
    }

    /// The sky as seen, the sun in it, `side` texels a face.
    fn skybox(&self, side: usize) -> Image {
        let mut data = Vec::with_capacity(6 * side * side * 8);
        let seen = |d: Vec3| if d.y >= 0.0 { self.sample(d) } else { self.light(d) };
        for face in 0..6 {
            sky_light::face(&seen, face, side, None, &mut data);
        }
        sky_light::cube(side, 1, data)
    }

    /// The constant light below T3: the sky's, with some of the land's.
    fn ambient(&self) -> Vec3 {
        let up = (0..64).map(|i| {
            let a = i as f32 / 64.0 * std::f32::consts::TAU;
            self.light(Vec3::new(a.cos(), 0.6, a.sin())) + self.light(Vec3::new(a.cos() * 0.3, 1.0, a.sin() * 0.3))
        });
        let sky = up.sum::<Vec3>() / 128.0;
        let land = self.light(Vec3::NEG_Y);
        sky * 0.8 + land * 0.2
    }

    /// The horizon's colour, all round.
    fn haze(&self) -> Vec3 {
        (0..64)
            .map(|i| {
                let a = i as f32 / 64.0 * std::f32::consts::TAU;
                self.sample(Vec3::new(a.cos(), 0.03, a.sin()))
            })
            .sum::<Vec3>()
            / 64.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A blue sky, brighter towards the horizon, with a small sun at
    /// `elevation` degrees in direction -Z, ten thousand times the sky.
    fn sky(elevation: f32, turn: f32) -> Panorama {
        let (width, height) = (512, 256);
        let sun = Vec3::new(0.0, elevation.to_radians().sin(), -elevation.to_radians().cos());
        let mut pixels = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let longitude = (x as f32 + 0.5) / width as f32 * std::f32::consts::TAU - std::f32::consts::PI;
                let latitude = std::f32::consts::FRAC_PI_2 - (y as f32 + 0.5) / height as f32 * std::f32::consts::PI;
                let d = Vec3::new(latitude.cos() * longitude.sin(), latitude.sin(), -latitude.cos() * longitude.cos());
                let mut c = Vec3::new(0.3, 0.5, 1.0) * (1.0 + (1.0 - d.y.max(0.0)).powi(3));
                if d.dot(sun) > 0.6f32.to_radians().cos() {
                    c = Vec3::new(1.0, 0.95, 0.85) * 10_000.0;
                }
                pixels.push(c.to_array());
            }
        }
        Panorama { width, height, pixels, turn, overhead: None }
    }

    #[test]
    fn the_sun_is_found_where_it_is() {
        let panorama = sky(40.0, 0.0);
        let analysis = Analysis::of(&panorama, Vec3::splat(0.2));
        let expected = Vec3::new(0.0, 40f32.to_radians().sin(), -40f32.to_radians().cos());
        assert!(analysis.to_sun.angle_between(expected).to_degrees() < 1.0, "{}", analysis.to_sun);
        // Turned, the sun turns with it.
        let turned_panorama = sky(40.0, std::f32::consts::FRAC_PI_2);
        let turned = Analysis::of(&turned_panorama, Vec3::splat(0.2));
        let expected = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * expected;
        assert!(turned.to_sun.angle_between(expected).to_degrees() < 1.0, "{}", turned.to_sun);
        assert!(turned.sample(expected).x > 1000.0, "the turned panorama is sampled turned");
    }

    #[test]
    fn the_sun_sets_the_brightness_of_the_sky() {
        let panorama = sky(40.0, 0.0);
        let analysis = Analysis::of(&panorama, Vec3::splat(0.2));
        assert!((analysis.sun_illuminance * analysis.scale - SUN).abs() < 1.0);
        // A sun ten thousand times the sky, about 0.6° across: a sky of a few thousand cd/m².
        let overhead = analysis.radiance(Vec3::Y) * analysis.scale;
        assert!((1_000.0..30_000.0).contains(&overhead), "{overhead}");
        assert!(analysis.sun_colour.x > analysis.sun_colour.z, "a warm sun: {}", analysis.sun_colour);
    }

    #[test]
    fn an_overcast_sky_is_as_bright_as_it_is_said_to_be_and_its_sun_gives_little() {
        // No sun in it: a sky a little brighter round one patch of cloud.
        let (width, height) = (256, 128);
        let pixels = (0..width * height)
            .map(|i| {
                let (x, y) = (i % width, i / width);
                let glow = if (x as i32 - 60).abs() < 10 && (y as i32 - 30).abs() < 6 { 1.4 } else { 1.0 };
                let up = if y < height / 2 { 1.0 } else { 0.3 };
                [0.8 * glow * up, 0.82 * glow * up, 0.85 * glow * up]
            })
            .collect();
        let panorama = Panorama { width, height, pixels, turn: 0.0, overhead: Some(2_500.0) };
        let analysis = Analysis::of(&panorama, Vec3::splat(0.2));
        let overhead = analysis.radiance(Vec3::Y) * analysis.scale;
        assert!((overhead - 2_500.0).abs() < 1.0, "{overhead}");
        let sun = analysis.sun_illuminance * analysis.scale;
        assert!(sun < SUN / 50.0, "a cloud is no sun: {sun} lux");
    }

    #[test]
    fn the_light_from_the_sky_leaves_the_sun_out() {
        let panorama = sky(40.0, 0.0);
        let analysis = Analysis::of(&panorama, Vec3::splat(0.2));
        let at_sun = analysis.light(analysis.to_sun);
        assert!(luminance(at_sun) < 10.0, "the sun is not in the sky's light: {at_sun}");
        assert!(luminance(analysis.sample(analysis.to_sun)) > 1000.0, "but it is in the sky seen");
        // Below, the land: darker than the sky, the colour of the ground.
        let green = Analysis::of(&panorama, Vec3::new(0.05, 0.2, 0.03));
        let down = green.light(Vec3::NEG_Y);
        assert!(down.y > down.x && down.y > down.z, "{down}");
        // No jump at the horizon.
        let (above, below) = (green.light(Vec3::new(1.0, 1e-3, 0.0)), green.light(Vec3::new(1.0, -1e-3, 0.0)));
        assert!(above.distance(below) < 0.1 * luminance(above), "{above} {below}");
    }

    #[test]
    fn the_glow_around_the_sun_lights_no_more_than_a_few_skies() {
        let mut panorama = sky(40.0, 0.0);
        // A glow ten degrees wide around the sun, a hundred times the sky.
        let sun = Vec3::new(0.0, 40f32.to_radians().sin(), -40f32.to_radians().cos());
        let (w, h) = (panorama.width, panorama.height);
        for y in 0..h {
            for x in 0..w {
                let longitude = (x as f32 + 0.5) / w as f32 * std::f32::consts::TAU - std::f32::consts::PI;
                let latitude = std::f32::consts::FRAC_PI_2 - (y as f32 + 0.5) / h as f32 * std::f32::consts::PI;
                let d = Vec3::new(latitude.cos() * longitude.sin(), latitude.sin(), -latitude.cos() * longitude.cos());
                let angle = d.angle_between(sun).to_degrees();
                if angle > 4.0 && angle < 10.0 {
                    panorama.pixels[y * w + x] = [100.0; 3];
                }
            }
        }
        let analysis = Analysis::of(&panorama, Vec3::splat(0.2));
        let glow = Quat::from_rotation_x(7f32.to_radians()) * analysis.to_sun;
        assert!(luminance(analysis.sample(glow)) > 50.0, "the glow is in the sky seen");
        assert!(luminance(analysis.light(glow)) <= analysis.ceiling * 1.001, "but not as bright in its light");
        assert!(
            (luminance(analysis.light(Vec3::Y)) - luminance(analysis.sample(Vec3::Y))).abs() < 1e-3,
            "the sky overhead is untouched"
        );
    }

    #[test]
    fn the_skybox_is_a_cubemap_of_the_size_asked() {
        let panorama = sky(40.0, 0.0);
        let analysis = Analysis::of(&panorama, Vec3::splat(0.2));
        let image = analysis.skybox(16);
        assert_eq!(image.data.as_ref().unwrap().len(), 6 * 16 * 16 * 8);
        assert_eq!(image.texture_descriptor.size.depth_or_array_layers, 6);
    }
}
