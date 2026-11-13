// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Particles: sparks and smoke from a drifting kart's rear tyres, flames
//! and smoke from its exhausts when it boosts, a rocket's fire and trail of
//! smoke, a smoke canister's coloured cloud, dust when it leaves the road. On the photographed circuit
//! ([`Photographed`]), as on a real one: no sparks, and tyre smoke a faint
//! veil; boost flames stay (a nitro does that to a real engine).
//!
//! A fixed pool of small squares that always face the camera. A particle
//! fades by shrinking, so that all of a kind share one material and are
//! drawn together. Nothing here is random: the same frames on every run.

use bevy::{
    asset::RenderAssetUsages,
    light::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use osage_kart_physics::turbo_level;
use osage_render::OsageCamera;

use crate::{
    karts::{EXHAUSTS, REAR_CONTACTS, ROCKET_NOZZLES},
    scene::{Course, Photographed, Pose},
};

/// Particles alive at once, at most.
const POOL: usize = 1200;

#[derive(Clone, Copy)]
enum Kind {
    /// Drift sparks by the level of mini-turbo charged: none yet, then
    /// blue, orange and violet (D-050).
    Spark(usize),
    /// Air rushing past a kart in another's wake: faint streaks.
    Wind,
    /// A stalled engine coughing black smoke.
    Soot,
    Smoke,
    Dust,
    /// Water thrown up by the wheels on a wet road (D-068): a pale mist.
    Spray,
    Flame,
    /// A rocket's fire: bigger and longer than a boost's flame.
    Fire,
    /// Smoke from an exhaust or a rocket: grey, rising, spreading.
    Exhaust,
    /// A smoke canister's: thick, of the colour given (0xRRGGBB), lingering.
    Tinted(u32),
}

#[derive(Component, Default)]
pub struct Particle {
    velocity: Vec3,
    /// Seconds left, and seconds it had.
    life: f32,
    span: f32,
    size: f32,
    /// How much it grows over its life, as a share of its size.
    growth: f32,
    gravity: f32,
}

#[derive(Resource)]
pub struct Pool {
    idle: Vec<Entity>,
    sparks: [Handle<StandardMaterial>; 4],
    wind: Handle<StandardMaterial>,
    soot: Handle<StandardMaterial>,
    smoke: Handle<StandardMaterial>,
    /// Tyre smoke on the photographed circuit: a faint grey veil.
    veil: Handle<StandardMaterial>,
    dust: Handle<StandardMaterial>,
    flame: Handle<StandardMaterial>,
    /// The soft dot every particle is, and the smoke colours made so far.
    dot: Handle<Image>,
    tinted: std::collections::HashMap<u32, Handle<StandardMaterial>>,
    /// Counts what was emitted: the source of this module's variety.
    emitted: u32,
}

impl Pool {
    /// A number in 0..1 that differs for every call, the same on every run.
    fn vary(&mut self) -> f32 {
        self.emitted = self.emitted.wrapping_add(1);
        let mut h = self.emitted.wrapping_mul(0x9E37_79B1);
        h = (h ^ (h >> 15)).wrapping_mul(0x85EB_CA77);
        ((h ^ (h >> 13)) & 0xffff) as f32 / 65535.0
    }

    fn spread(&mut self, amount: f32) -> Vec3 {
        Vec3::new(self.vary() - 0.5, self.vary() - 0.5, self.vary() - 0.5) * 2.0 * amount
    }
}

/// A soft round dot: white, fading to nothing at its edge.
fn dot() -> Image {
    let size = 48u32;
    let pixels: Vec<u8> = (0..size * size)
        .flat_map(|i| {
            let (x, y) = ((i % size) as f32 + 0.5, (i / size) as f32 + 0.5);
            let from_centre = Vec2::new(x - size as f32 / 2.0, y - size as f32 / 2.0).length() / (size as f32 / 2.0);
            let alpha = (1.0 - from_centre).clamp(0.0, 1.0).powf(1.5);
            [255, 255, 255, (alpha * 255.0) as u8]
        })
        .collect();
    let extent = Extent3d { width: size, height: size, depth_or_array_layers: 1 };
    Image::new(extent, TextureDimension::D2, pixels, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default())
}

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let dot = images.add(dot());
    let mut material = |colour: LinearRgba, alpha_mode: AlphaMode| {
        materials.add(StandardMaterial {
            base_color: colour.into(),
            base_color_texture: Some(dot.clone()),
            alpha_mode,
            unlit: true,
            cull_mode: None,
            ..default()
        })
    };
    // Lights add to what is behind them, brighter than white so that they glow.
    let light = |r: f32, g: f32, b: f32| LinearRgba::new(r * 3.0, g * 3.0, b * 3.0, 1.0);
    let smoke = material(LinearRgba::new(0.85, 0.85, 0.88, 0.32), AlphaMode::Blend);
    let pool = Pool {
        sparks: [light(1.0, 0.9, 0.55), light(0.2, 0.6, 1.0), light(1.0, 0.45, 0.08), light(0.75, 0.25, 1.0)]
            .map(|c| material(c, AlphaMode::Add)),
        wind: material(LinearRgba::new(0.5, 0.55, 0.6, 1.0), AlphaMode::Add),
        soot: material(LinearRgba::new(0.06, 0.06, 0.06, 0.6), AlphaMode::Blend),
        dust: material(LinearRgba::new(0.5, 0.42, 0.28, 0.55), AlphaMode::Blend),
        flame: material(light(1.0, 0.55, 0.15), AlphaMode::Add),
        smoke: smoke.clone(),
        // A racing kart's tyres barely smoke: a thin grey veil that spreads
        // and lingers, never a white puff.
        veil: material(LinearRgba::new(0.55, 0.55, 0.57, 0.09), AlphaMode::Blend),
        dot: dot.clone(),
        tinted: default(),
        emitted: 0,
        idle: {
            let square = meshes.add(Rectangle::new(1.0, 1.0));
            let hidden = || {
                (
                    Particle::default(),
                    Mesh3d(square.clone()),
                    MeshMaterial3d(smoke.clone()),
                    Transform::default(),
                    Visibility::Hidden,
                    NotShadowCaster,
                    NotShadowReceiver,
                )
            };
            (0..POOL).map(|_| commands.spawn(hidden()).id()).collect()
        },
    };
    commands.insert_resource(pool);
}

/// How dark the smoke was made last, and its materials' dry colours.
type Shaded = (f32, Vec<(Handle<StandardMaterial>, LinearRgba)>);

/// Unlit smoke glows in the gloom of a rain: as the sky darkens (D-068),
/// the smoke, the veil, the dust and the soot darken with it.
pub fn gloom(
    pool: Res<Pool>,
    rain: Option<Res<crate::rain::Rain>>,
    look: Option<Res<osage_render::Look>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut shaded: Local<Option<Shaded>>,
) {
    // Veiled by the rain, and exposed like the rest (D-069).
    let veiled = rain.map_or(0.0, |rain| rain.veiled * rain.wet());
    let shade = (1.0 - 0.6 * veiled) * look.map_or(1.0, |look| crate::scene::unlit(&look));
    let (done, dry) = shaded.get_or_insert_with(|| {
        let handles = [&pool.smoke, &pool.veil, &pool.dust, &pool.soot, &pool.wind];
        let dry = handles
            .into_iter()
            .filter_map(|h| materials.get(h).map(|m| (h.clone(), m.base_color.to_linear())))
            .collect();
        (1.0, dry)
    });
    if (shade - *done).abs() < 0.01 * done.max(0.01) {
        return;
    }
    *done = shade;
    for (handle, colour) in dry.iter() {
        if let Some(mut material) = materials.get_mut(handle) {
            material.base_color =
                LinearRgba::new(colour.red * shade, colour.green * shade, colour.blue * shade, colour.alpha).into();
        }
    }
}

/// What the karts throw up this frame.
#[allow(clippy::too_many_arguments)]
pub fn emit(
    time: Res<Time>,
    course: Option<Res<Course>>,
    photographed: Option<Res<Photographed>>,
    mut pool: ResMut<Pool>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut frame: Local<u32>,
    rain: Option<Res<crate::rain::Rain>>,
    karts: Query<(&Pose, &Transform)>,
    mut particles: Query<
        (&mut Particle, &mut Transform, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>),
        Without<Pose>,
    >,
) {
    *frame = frame.wrapping_add(1);
    if time.delta_secs() <= 0.0 {
        return;
    }
    let real = photographed.is_some();
    let mut launch = |pool: &mut Pool, kind: Kind, at: Vec3, velocity: Vec3| {
        let Some(entity) = pool.idle.pop() else { return };
        let Ok((mut particle, mut place, mut shown, mut material)) = particles.get_mut(entity) else { return };
        let (span, size, growth, gravity, look) = match kind {
            Kind::Spark(stage) => (0.22 + 0.2 * pool.vary(), 0.16, -0.6, 9.0, pool.sparks[stage].clone()),
            Kind::Wind => (0.18 + 0.1 * pool.vary(), 0.07, 0.0, 0.0, pool.wind.clone()),
            Kind::Soot => (0.7 + 0.4 * pool.vary(), 0.3, 2.5, -0.5, pool.soot.clone()),
            Kind::Smoke if real => (0.9 + 0.5 * pool.vary(), 0.45, 3.2, -0.25, pool.veil.clone()),
            Kind::Smoke => (0.4 + 0.25 * pool.vary(), 0.3, 1.8, -0.6, pool.smoke.clone()),
            Kind::Dust => (0.5 + 0.3 * pool.vary(), 0.5, 2.0, -0.3, pool.dust.clone()),
            // Gone before a chasing camera, 3.4 m behind at speed, gets there.
            Kind::Spray => (0.1 + 0.06 * pool.vary(), 0.25, 3.0, 3.0, pool.veil.clone()),
            Kind::Flame => (0.12 + 0.1 * pool.vary(), 0.42, -0.7, 0.0, pool.flame.clone()),
            Kind::Fire => (0.16 + 0.1 * pool.vary(), 0.6, -1.2, 0.0, pool.flame.clone()),
            Kind::Exhaust => (0.6 + 0.4 * pool.vary(), 0.25, 2.4, -0.4, pool.smoke.clone()),
            Kind::Tinted(colour) => (1.6 + 0.8 * pool.vary(), 0.5, 4.0, -0.2, pool.tinted[&colour].clone()),
        };
        *particle = Particle { velocity, life: span, span, size, growth, gravity };
        place.translation = at;
        place.scale = Vec3::splat(size);
        *shown = Visibility::Inherited;
        material.0 = look;
    };

    // A ghost leaves nothing behind it (D-062).
    for (pose, place) in karts.iter().filter(|(pose, _)| !pose.ghost) {
        let kart = &pose.kart;
        let (forward, speed) = (place.rotation * Vec3::Z, kart.speed);
        let behind = -forward * speed;
        if kart.drift != 0.0 {
            let stage = turbo_level(kart.charge);
            for contact in REAR_CONTACTS {
                let at = place.transform_point(contact);
                for _ in 0..if real { 0 } else { 2 } {
                    let fling = behind * 0.25 + Vec3::Y * (1.6 + pool.vary() * 2.2) + pool.spread(2.2);
                    launch(&mut pool, Kind::Spark(stage), at, fling);
                }
                if frame.is_multiple_of(if real { 3 } else { 4 }) {
                    let drift_up = Vec3::Y * 0.7 + pool.spread(0.5);
                    launch(&mut pool, Kind::Smoke, at + Vec3::Y * 0.1, behind * 0.12 + drift_up);
                }
            }
        }
        if kart.boost > 0.0 {
            for exhaust in EXHAUSTS {
                let at = place.transform_point(exhaust);
                let jet = -forward * 5.0 + pool.spread(0.6);
                launch(&mut pool, Kind::Flame, at, forward * speed + jet);
                if frame.is_multiple_of(2) {
                    let puff = -forward * 2.0 + Vec3::Y * 0.6 + pool.spread(0.4);
                    launch(&mut pool, Kind::Exhaust, at, forward * speed * 0.6 + puff);
                }
            }
        }
        // In a wake, air streams past the sides; a stalled engine coughs.
        if kart.draft > 0.15 {
            for side in [-1.0, 1.0] {
                let at =
                    place.transform_point(Vec3::new(side * (0.9 + 0.3 * pool.vary()), 0.4 + 0.6 * pool.vary(), 1.8));
                launch(&mut pool, Kind::Wind, at, forward * (speed - 14.0));
            }
        }
        if kart.stall > 0.0 && frame.is_multiple_of(3) {
            for exhaust in EXHAUSTS {
                let puff = Vec3::Y * 0.9 - forward * 0.8 + pool.spread(0.4);
                launch(&mut pool, Kind::Soot, place.transform_point(exhaust), puff);
            }
        }
        if kart.rocket > 0.0 {
            for nozzle in ROCKET_NOZZLES {
                let at = place.transform_point(nozzle);
                for _ in 0..2 {
                    let jet = -forward * 9.0 + pool.spread(1.0);
                    launch(&mut pool, Kind::Fire, at, forward * speed + jet);
                }
                // A trail of smoke that hangs where the kart was.
                let puff = Vec3::Y * 0.4 + pool.spread(0.6);
                launch(&mut pool, Kind::Exhaust, at - forward * 0.3, puff);
            }
        }
        if let Some(colour) = pose.smoke {
            if !pool.tinted.contains_key(&colour) {
                let channel = |shift: u32| ((colour >> shift) & 0xff) as f32 / 255.0;
                let tint = materials.add(StandardMaterial {
                    base_color: Color::srgba(channel(16), channel(8), channel(0), 0.55),
                    base_color_texture: Some(pool.dot.clone()),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    cull_mode: None,
                    ..default()
                });
                pool.tinted.insert(colour, tint);
            }
            // A thick cloud poured from the back, hanging where the kart was.
            for exhaust in EXHAUSTS {
                let puff = Vec3::Y * 0.5 + pool.spread(0.9);
                launch(&mut pool, Kind::Tinted(colour), place.transform_point(exhaust), forward * speed * 0.15 + puff);
            }
        }
        // On a wet road the rear wheels throw spray, more the faster (D-068).
        let wet = rain.as_ref().map_or(0.0, |rain| rain.wet());
        if wet > 0.3 && speed.abs() > 6.0 {
            for contact in REAR_CONTACTS {
                // Up and a little back: kept round the wheels, out of a chasing lens.
                let throw = behind * 0.12 + Vec3::Y * (0.9 + 0.8 * pool.vary()) + pool.spread(0.5);
                launch(&mut pool, Kind::Spray, place.transform_point(contact) + Vec3::Y * 0.15, throw * wet);
            }
        }
        let off_road = course.as_ref().is_some_and(|course| {
            course.0.track.locate(kart.x, kart.z).lateral.abs() > course.0.track.half_width + 0.8
        });
        if off_road && speed.abs() > 3.0 && frame.is_multiple_of(2) {
            for contact in REAR_CONTACTS {
                let puff = Vec3::Y * 0.8 + pool.spread(0.7);
                launch(&mut pool, Kind::Dust, place.transform_point(contact), behind * 0.1 + puff);
            }
        }
    }
}

/// Moves the particles, turns them to the camera, and retires the dead ones.
/// Frames from the start during which every particle material is drawn.
const PREWARM: u32 = 120;

/// Every particle material drawn once, in front of the camera and far too
/// small to see, for the first frames: their pipelines are made at the
/// start, not at the first puff of smoke. A bench, which compiles a
/// pipeline the frame it first needs it, would stall there (as it did, by
/// 0.75 s on the L4, when a level of detail first came into range:
/// `osage_props` warms those); a game would draw no smoke until it was
/// ready.
pub fn prewarm(
    pool: Res<Pool>,
    mut frame: Local<u32>,
    camera: Query<&Transform, (With<OsageCamera>, Without<Particle>)>,
    mut particles: Query<(&mut Transform, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>), With<Particle>>,
) {
    if *frame > PREWARM {
        return;
    }
    *frame += 1;
    let Some(camera) = camera.iter().next() else { return };
    let looks = pool.sparks.iter().chain([&pool.smoke, &pool.veil, &pool.dust, &pool.flame, &pool.wind, &pool.soot]);
    // The first idle particles are the last a launch takes.
    for (look, &entity) in looks.zip(&pool.idle) {
        let Ok((mut place, mut shown, mut material)) = particles.get_mut(entity) else { continue };
        if *frame > PREWARM {
            *shown = Visibility::Hidden;
            continue;
        }
        place.translation = camera.translation + camera.forward() * 1.0;
        place.scale = Vec3::splat(1e-4);
        *shown = Visibility::Inherited;
        material.0 = look.clone();
    }
}

pub fn update(
    time: Res<Time>,
    mut pool: ResMut<Pool>,
    camera: Query<&Transform, (With<OsageCamera>, Without<Particle>)>,
    mut particles: Query<(Entity, &mut Particle, &mut Transform, &mut Visibility)>,
) {
    let seconds = time.delta_secs();
    let facing = camera.iter().next().map_or(Quat::IDENTITY, |camera| camera.rotation);
    for (entity, mut particle, mut place, mut shown) in &mut particles {
        if particle.life <= 0.0 {
            continue;
        }
        particle.life -= seconds;
        if particle.life <= 0.0 {
            *shown = Visibility::Hidden;
            pool.idle.push(entity);
            continue;
        }
        particle.velocity.y -= particle.gravity * seconds;
        place.translation += particle.velocity * seconds;
        if place.translation.y < 0.04 {
            // It lands on the road and bounces a little.
            place.translation.y = 0.04;
            particle.velocity.y = particle.velocity.y.abs() * 0.35;
        }
        let age = 1.0 - particle.life / particle.span;
        // Born small, then its size; it fades by shrinking at the end.
        let fade = (age * 8.0).min(1.0) * ((1.0 - age) * 3.0).min(1.0);
        place.scale = Vec3::splat(particle.size * (1.0 + particle.growth * age) * fade);
        place.rotation = facing;
    }
}
