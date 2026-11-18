// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The light coming into the walker's block (D-136): its tiles asked of the
//! world's server of light as the walker goes (the server says which), or
//! read from the street's folder when walking alone; fetched and decoded
//! by threads of their own; the current block's put in two texture arrays
//! and drawn on the block's six faces, from inside (`light.wesl`). The same
//! light lights the block: what comes in from each direction, seen from
//! the block's centre, made into the camera's environment light.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    thread,
};

use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    image::ImageSampler,
    prelude::*,
    render::render_resource::{
        AsBindGroup, Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
    },
    shader::ShaderRef,
};
use crossbeam_channel::{Receiver, Sender};
use osage_hologram_stream::{http::Client, light};
use osage_world_paris::tiles::{self, Block, Face, Tile};

use crate::{NITS, Options, walker::Me};

/// How far ahead the light is asked for when walking alone, seconds.
const HORIZON: f32 = 2.0;
/// How often the light is asked for, seconds.
const PERIOD: f64 = 0.25;
const WORKERS: usize = 3;

pub struct LightPlugin;

impl Plugin for LightPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "light.wesl");
        app.add_plugins(MaterialPlugin::<IncomingLight>::default())
            .init_resource::<Store>()
            .init_resource::<LightServer>()
            .init_resource::<WalkerId>()
            .add_systems(Startup, (start, faces))
            .add_systems(Update, (ask, receive, arrays, lit).chain());
    }
}

/// The light on one face of the block.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct IncomingLight {
    /// The block's lowest corner, and which face (0 -x … 5 +z).
    #[uniform(0)]
    pub low: Vec4,
    /// The block's highest corner, and a probe's side in pixels.
    #[uniform(1)]
    pub high: Vec4,
    /// Each tile's layer, -1 where it has not come.
    #[uniform(2)]
    pub layers: [IVec4; 3],
    #[texture(3, dimension = "2d_array")]
    #[sampler(4)]
    pub colours: Handle<Image>,
    #[texture(5, dimension = "2d_array", sample_type = "float", filterable = false)]
    pub distances: Handle<Image>,
    #[uniform(6)]
    pub scale: Vec4,
    /// A trial (`--cube-probes`): the probes inside the walker's cube, where
    /// each stands and its layer (-1: none), instead of the block's tiles.
    #[uniform(7)]
    pub probes: [Vec4; 5],
    /// Their pictures' width and height, 1 when they are used, how many.
    #[uniform(8)]
    pub shape: Vec4,
}

impl Material for IncomingLight {
    fn fragment_shader() -> ShaderRef {
        "embedded://paris/light.wesl".into()
    }

    /// Drawn with what is seen through, after the opaque things (its alpha
    /// is 1): the faces show light already, which traced light's denoiser
    /// (T4), filtering the opaque picture by each surface's colour, blurred.
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}

/// The address of the world's server of light, once announced.
#[derive(Resource, Default)]
pub struct LightServer(pub Option<String>);

/// The walker's id in the world, once it has one.
#[derive(Resource, Default)]
pub struct WalkerId(pub Option<u64>);

/// The tiles that have come, decoded.
#[derive(Resource, Default)]
pub struct Store {
    tiles: HashMap<String, Arc<light::Drawn>>,
    asked: HashSet<String>,
    /// Tiles in the street's folder, walking alone.
    present: Option<HashSet<String>>,
    pub bytes: u64,
    pub received: usize,
    /// The block drawn now, and how many of its tiles it has.
    pub block: Option<Block>,
    pub drawn: usize,
    /// The block whose light lights the camera now.
    pub lit: Option<Block>,
    changed: bool,
}

#[derive(Clone)]
enum Source {
    Server(String),
    Folder(PathBuf),
}

enum Ask {
    Need { source: Source, walker: u64 },
    Tile { source: Source, key: String },
}

enum Got {
    Needed(Vec<String>),
    Tile { key: String, drawn: light::Drawn, bytes: u64 },
    Failed(String),
}

/// A block's environment light: its two cubemaps.
type Lit = (Block, Image, Image);

#[derive(Resource)]
struct Fetcher {
    asks: Sender<Ask>,
    gots: Receiver<Got>,
    /// The environment lights made from blocks' light, as they are done.
    lights: (Sender<Lit>, Receiver<Lit>),
}

/// The six faces of the block, as entities with their materials.
#[derive(Resource)]
struct Faces([(Entity, Handle<IncomingLight>); 6]);

/// A face's light for traced rays (T4 only): the mean of what its tiles
/// saw coming in, as a lamp the size of the face, the coarse light that
/// lights the block (D-136). Remade for each block.
#[derive(Component)]
struct Glow;

fn start(mut commands: Commands, options: Res<Options>, mut store: ResMut<Store>) {
    let (asks, to_workers) = crossbeam_channel::unbounded::<Ask>();
    let (from_workers, gots) = crossbeam_channel::unbounded::<Got>();
    for i in 0..WORKERS {
        let (to_workers, from_workers) = (to_workers.clone(), from_workers.clone());
        thread::Builder::new()
            .name(format!("paris-light-{i}"))
            .spawn(move || work(to_workers, from_workers))
            .expect("a light thread");
    }
    commands.insert_resource(Fetcher { asks, gots, lights: crossbeam_channel::unbounded() });
    if options.server.is_none() {
        let index = options.street.join("tiles/index.json");
        let present = std::fs::read_to_string(&index)
            .ok()
            .and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok())
            .unwrap_or_default();
        info!(tiles = present.len(), "paris: walking alone, the light from {}", index.display());
        store.present = Some(present.into_iter().collect());
    }
}

fn work(asks: Receiver<Ask>, gots: Sender<Got>) {
    let mut clients: HashMap<String, Client> = HashMap::new();
    for ask in asks {
        let got = match ask {
            Ask::Need { source: Source::Server(address), walker } => {
                let client = clients.entry(address.clone()).or_insert_with(|| Client::new(address));
                match client.get(&format!("/need?walker={walker}")).map(|b| serde_json::from_slice::<Vec<String>>(&b)) {
                    Ok(Ok(keys)) => Got::Needed(keys),
                    Ok(Err(e)) => Got::Failed(format!("need: {e}")),
                    Err(e) => Got::Failed(format!("need: {e}")),
                }
            }
            Ask::Need { source: Source::Folder(_), .. } => Got::Needed(Vec::new()),
            Ask::Tile { source, key } => {
                let mut fetch = |file: &str| -> std::io::Result<Vec<u8>> {
                    match &source {
                        Source::Server(address) => clients
                            .entry(address.clone())
                            .or_insert_with(|| Client::new(address.clone()))
                            .get(&format!("/h/{key}/{file}")),
                        Source::Folder(folder) => std::fs::read(folder.join(&key).join(file)),
                    }
                };
                match (fetch("colour.ivf"), fetch("depth.ivf"), fetch("mask.ivf")) {
                    (Ok(c), Ok(d), Ok(m)) => {
                        let bytes = (c.len() + d.len() + m.len()) as u64;
                        match light::decode(&c, &d, &m).map(|mut v| v.pop()) {
                            Ok(Some(drawn)) => Got::Tile { key, drawn, bytes },
                            Ok(None) => Got::Failed(format!("{key}: no picture")),
                            Err(e) => Got::Failed(format!("{key}: {e}")),
                        }
                    }
                    (Err(e), ..) | (_, Err(e), _) | (.., Err(e)) => Got::Failed(format!("{key}: {e}")),
                }
            }
        };
        if gots.send(got).is_err() {
            return;
        }
    }
}

fn source(options: &Options, server: &LightServer) -> Option<Source> {
    match &options.server {
        None => Some(Source::Folder(options.street.join("tiles"))),
        Some(_) => server.0.clone().map(Source::Server),
    }
}

/// A few times a second: what light the walker needs, asked of the server
/// (which knows where the walker is), or found here when walking alone.
#[allow(clippy::too_many_arguments)]
fn ask(
    time: Res<Time>,
    options: Res<Options>,
    server: Res<LightServer>,
    walker: Res<WalkerId>,
    me: Res<Me>,
    fetcher: Res<Fetcher>,
    mut store: ResMut<Store>,
    mut next: Local<f64>,
) {
    let now = time.elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + PERIOD;
    let Some(source) = source(&options, &server) else { return };
    match (&source, walker.0) {
        (Source::Server(_), Some(id)) => {
            let _ = fetcher.asks.send(Ask::Need { source, walker: id });
        }
        (Source::Folder(_), _) => {
            let wanted: Vec<String> = if options.cube_probes > 0 {
                cube_keys(Block::of(me.eyes()).cube, options.cube_probes)
            } else {
                tiles::needed(me.eyes(), me.speed, HORIZON).into_iter().map(|t| t.key()).collect()
            };
            for key in wanted {
                let there = store.present.as_ref().is_some_and(|p| p.contains(&key));
                if there && store.asked.insert(key.clone()) {
                    let _ = fetcher.asks.send(Ask::Tile { source: source.clone(), key });
                }
            }
        }
        _ => {}
    }
}

fn receive(options: Res<Options>, server: Res<LightServer>, fetcher: Res<Fetcher>, mut store: ResMut<Store>) {
    for got in fetcher.gots.try_iter() {
        match got {
            Got::Needed(keys) => {
                let Some(source) = source(&options, &server) else { continue };
                for key in keys {
                    if store.asked.insert(key.clone()) {
                        let _ = fetcher.asks.send(Ask::Tile { source: source.clone(), key });
                    }
                }
            }
            Got::Tile { key, drawn, bytes } => {
                store.bytes += bytes;
                store.received += 1;
                store.tiles.insert(key, Arc::new(drawn));
                store.changed = true;
            }
            Got::Failed(what) => warn!(what, "paris: the light"),
        }
    }
}

fn faces(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<IncomingLight>>,
    mut images: ResMut<Assets<Image>>,
) {
    let quad = meshes.add(Rectangle::new(1.0, 1.0));
    let (colours, distances) = empty(&mut images);
    let faces = Face::ALL.map(|face| {
        let material = materials.add(IncomingLight {
            low: Vec4::new(0.0, 0.0, 0.0, face as usize as f32),
            high: Vec4::new(1.0, 1.0, 1.0, 1.0),
            layers: [IVec4::splat(-1); 3],
            colours: colours.clone(),
            distances: distances.clone(),
            scale: Vec4::new(NITS, 0.0, 0.0, 0.0),
            probes: [Vec4::new(0.0, 0.0, 0.0, -1.0); 5],
            shape: Vec4::new(1.0, 1.0, 0.0, 0.0),
        });
        let entity = commands
            .spawn((
                Mesh3d(quad.clone()),
                MeshMaterial3d(material.clone()),
                Transform::default(),
                Visibility::Hidden,
                bevy::light::NotShadowCaster,
            ))
            .id();
        (entity, material)
    });
    commands.insert_resource(Faces(faces));
}

/// Arrays of one empty layer, until light comes.
fn empty(images: &mut Assets<Image>) -> (Handle<Image>, Handle<Image>) {
    (
        images.add(array((1, 1), 1, TextureFormat::Rgb10a2Unorm, vec![0; 4])),
        images.add(array((1, 1), 1, TextureFormat::R32Float, (-1f32).to_le_bytes().to_vec())),
    )
}

fn array((width, height): (u32, u32), layers: u32, format: TextureFormat, data: Vec<u8>) -> Image {
    let mut image = Image::new(
        Extent3d { width, height, depth_or_array_layers: layers },
        TextureDimension::D2,
        data,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_view_descriptor =
        Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::D2Array), ..default() });
    image.sampler = ImageSampler::linear();
    image
}

/// Where a tile sits on its face of a block: its slot among the face's.
fn slot(block: &Block, tile: &Tile) -> usize {
    let lo = block.cube - IVec3::new(1, 0, 1);
    let (a, b) = match tile.face.axis() {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let extent = IVec3::new(3, 1, 3);
    let nb = extent[b];
    ((tile.a - lo[a]) * nb + (tile.b - lo[b])) as usize
}

/// When the walker enters another block, or light comes for its own: the
/// block's tiles stacked into the arrays, its faces moved and told.
#[allow(clippy::too_many_arguments)]
fn arrays(
    mut commands: Commands,
    me: Res<Me>,
    mut store: ResMut<Store>,
    faces: Res<Faces>,
    fetcher: Res<Fetcher>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<IncomingLight>>,
    mut places: Query<(&mut Transform, &mut Visibility)>,
    glows: Query<Entity, With<Glow>>,
    quads: Query<&Mesh3d>,
    traced: Res<osage_render::TracedLight>,
    options: Res<Options>,
) {
    let block = Block::of(me.eyes());
    if store.block == Some(block) && !store.changed {
        return;
    }
    store.changed = false;
    if options.cube_probes > 0 {
        let probes = cube_probes(&store, &options, block.cube);
        let moved = store.block != Some(block);
        if !moved && probes.len() == store.drawn {
            return;
        }
        store.block = Some(block);
        store.drawn = probes.len();
        if let Some((centre, drawn)) = probes.first().cloned() {
            let done = fetcher.lights.0.clone();
            let _ = thread::Builder::new().name("paris-environment".to_owned()).spawn(move || {
                let (diffuse, specular) = environment_seen(&drawn, centre);
                let _ = done.send((block, diffuse, specular));
            });
        }
        let size = probes.first().map_or((1, 1), |(_, d)| (d.width, d.height));
        let (colours, distances) = if probes.is_empty() {
            empty(&mut images)
        } else {
            let mut colour = Vec::new();
            let mut distance = Vec::new();
            for (_, drawn) in &probes {
                colour.extend(drawn.radiance.iter().flat_map(|v| v.to_le_bytes()));
                distance.extend(drawn.distance.iter().flat_map(|v| v.to_le_bytes()));
            }
            let layers = probes.len() as u32;
            (
                images.add(array(size, layers, TextureFormat::Rgb10a2Unorm, colour)),
                images.add(array(size, layers, TextureFormat::R32Float, distance)),
            )
        };
        let mut at = [Vec4::new(0.0, 0.0, 0.0, -1.0); 5];
        for (layer, (centre, _)) in probes.iter().enumerate().take(5) {
            at[layer] = centre.extend(layer as f32);
        }
        let (lo, hi) = (block.min(), block.max());
        for (index, face) in Face::ALL.into_iter().enumerate() {
            let (entity, handle) = &faces.0[index];
            if let Some(mut material) = materials.get_mut(handle) {
                material.low = lo.extend(index as f32);
                material.high = hi.extend(size.0 as f32);
                material.probes = at;
                material.shape = Vec4::new(size.0 as f32, size.1 as f32, 1.0, probes.len().min(5) as f32);
                material.colours = colours.clone();
                material.distances = distances.clone();
            }
            place_face(&mut places, *entity, face, lo, hi, options.whole);
        }
        return;
    }
    let mine: Vec<(Tile, Arc<light::Drawn>)> =
        block.tiles().into_iter().filter_map(|t| store.tiles.get(&t.key()).map(|d| (t, d.clone()))).collect();
    let moved = store.block != Some(block);
    if !moved && mine.len() == store.drawn {
        return;
    }
    store.block = Some(block);
    store.drawn = mine.len();
    // The block lit by its own light, on a thread: what comes in from each
    // direction, seen from its centre.
    if !mine.is_empty() {
        let (tiles, done) = (mine.clone(), fetcher.lights.0.clone());
        let _ = thread::Builder::new().name("paris-environment".to_owned()).spawn(move || {
            let (diffuse, specular) = environment(&block, &tiles);
            let _ = done.send((block, diffuse, specular));
        });
    }
    let side = mine.first().map_or(1, |(_, d)| d.width);
    let (colours, distances) = if mine.is_empty() {
        empty(&mut images)
    } else {
        let mut colour = Vec::with_capacity(mine.len() * (side * side * 4) as usize);
        let mut distance = Vec::with_capacity(colour.capacity());
        for (_, drawn) in &mine {
            colour.extend(drawn.radiance.iter().flat_map(|v| v.to_le_bytes()));
            distance.extend(drawn.distance.iter().flat_map(|v| v.to_le_bytes()));
        }
        let layers = mine.len() as u32;
        (
            images.add(array((side, side), layers, TextureFormat::Rgb10a2Unorm, colour)),
            images.add(array((side, side), layers, TextureFormat::R32Float, distance)),
        )
    };
    for glow in &glows {
        commands.entity(glow).despawn();
    }
    let (lo, hi) = (block.min(), block.max());
    for (index, face) in Face::ALL.into_iter().enumerate() {
        let (entity, handle) = &faces.0[index];
        let mut layers = [-1i32; 12];
        for (layer, (tile, _)) in mine.iter().enumerate() {
            if tile.face == face {
                layers[slot(&block, tile)] = layer as i32;
            }
        }
        if let Some(mut material) = materials.get_mut(handle) {
            material.low = lo.extend(index as f32);
            material.high = hi.extend(side as f32);
            material.layers = [
                IVec4::new(layers[0], layers[1], layers[2], layers[3]),
                IVec4::new(layers[4], layers[5], layers[6], layers[7]),
                IVec4::new(layers[8], layers[9], layers[10], layers[11]),
            ];
            material.colours = colours.clone();
            material.distances = distances.clone();
        }
        place_face(&mut places, *entity, face, lo, hi, options.whole);
        let (sum, n) = mine
            .iter()
            .filter(|(t, _)| t.face == face)
            .map(|(_, d)| mean_radiance(d))
            .fold((Vec3::ZERO, 0), |(s, n), r| (s + r, n + 1));
        if let (Ok(quad), true) = (quads.get(*entity), traced.0 && n > 0) {
            let luminance = sum / n as f32 * NITS;
            let lamp = osage_render::Emitter(LinearRgba::rgb(luminance.x, luminance.y, luminance.z));
            let glow =
                commands.spawn((Mesh3d(quad.0.clone()), lamp, Transform::default(), Visibility::Inherited, Glow)).id();
            commands.entity(*entity).add_child(glow);
        }
    }
}

/// A face's quad on the block's surface, facing into the block.
fn place_face(
    places: &mut Query<(&mut Transform, &mut Visibility)>,
    entity: Entity,
    face: Face,
    lo: Vec3,
    hi: Vec3,
    whole: bool,
) {
    let normal = face.normal();
    let centre = (lo + hi) / 2.0 + normal * (hi - lo) / 2.0;
    let size = (hi - lo) * (Vec3::ONE - normal.abs());
    let (up, width, height) = match face.axis() {
        0 => (Vec3::Y, size.z, size.y),
        1 => (Vec3::Z, size.x, size.z),
        _ => (Vec3::Y, size.x, size.y),
    };
    if let Ok((mut place, mut visibility)) = places.get_mut(entity) {
        *place = Transform::from_translation(centre).looking_to(normal, up).with_scale(Vec3::new(width, height, 1.0));
        *visibility = if whole { Visibility::Hidden } else { Visibility::Inherited };
    }
}

/// The names of a cube's first `count` probes (`tools/bistro/cube-probes.py`).
fn cube_keys(cube: IVec3, count: usize) -> Vec<String> {
    (0..count.min(5)).map(|k| format!("c_{}_{}_{}_{k}", cube.x, cube.y, cube.z)).collect()
}

/// The cube's probes that have come, each with where it stands (its
/// `meta.json` in the street's folder: walking alone only).
fn cube_probes(store: &Store, options: &Options, cube: IVec3) -> Vec<(Vec3, Arc<light::Drawn>)> {
    cube_keys(cube, options.cube_probes)
        .into_iter()
        .filter_map(|key| {
            let drawn = store.tiles.get(&key)?.clone();
            let meta = std::fs::read_to_string(options.street.join("tiles").join(&key).join("meta.json")).ok()?;
            let meta: serde_json::Value = serde_json::from_str(&meta).ok()?;
            let c = meta["centre"].as_array()?;
            let at = |i: usize| c.get(i).and_then(|v| v.as_f64()).map(|v| v as f32);
            Some((Vec3::new(at(0)?, at(1)?, at(2)?), drawn))
        })
        .collect()
}

/// What comes into the block from each direction as a probe inside it saw
/// it (equirectangular: the middle column north, east to its right).
fn environment_seen(drawn: &light::Drawn, _centre: Vec3) -> (Image, Image) {
    let (w, h) = (drawn.width as usize, drawn.height as usize);
    let sample = |d: Vec3| -> Vec3 {
        let d = d.normalize_or(Vec3::Y);
        let lon = d.x.atan2(-d.z);
        let lat = d.y.clamp(-1.0, 1.0).asin();
        let u = 0.5 + lon / std::f32::consts::TAU;
        let v = 0.5 - lat / std::f32::consts::PI;
        let (col, row) = (((u * w as f32) as usize).min(w - 1), ((v * h as f32) as usize).min(h - 1));
        let packed = drawn.radiance[row * w + col];
        let band = |shift: u32| light::decode_radiance(((packed >> shift) & 1023) as f32 / 1023.0);
        Vec3::new(band(0), band(10), band(20))
    };
    osage_render::sky_light::cubemaps(sample)
}

/// The radiance a lamp on a tile shines with to light as the tile's
/// probe saw light come in: what a surface lying on the tile gathers, the
/// light of the half sphere out of the block weighed by the cosine of its
/// angle (the fisheye's circle within 90 of its 100 degrees; an
/// equidistant fisheye's pixel spans sin θ / θ of solid angle). A plain
/// mean left the cobbles a third too dark against Cycles: the bright sky
/// overhead counts most.
fn mean_radiance(drawn: &light::Drawn) -> Vec3 {
    let (w, h) = (drawn.width as usize, drawn.height as usize);
    let half_field = (tiles::PROBE_FOV / 2.0).to_radians();
    let (mut sum, mut weight) = (Vec3::ZERO, 0.0);
    let step = (w / 96).max(1);
    for row in (0..h).step_by(step) {
        for col in (0..w).step_by(step) {
            let (u, v) = ((col as f32 + 0.5) / w as f32 * 2.0 - 1.0, (row as f32 + 0.5) / h as f32 * 2.0 - 1.0);
            let theta = (u * u + v * v).sqrt() * half_field;
            if theta >= std::f32::consts::FRAC_PI_2 {
                continue;
            }
            let solid = if theta > 1e-4 { theta.sin() / theta } else { 1.0 };
            let w8 = theta.cos() * solid;
            let packed = drawn.radiance[row * w + col];
            let band = |shift: u32| light::decode_radiance(((packed >> shift) & 1023) as f32 / 1023.0);
            sum += Vec3::new(band(0), band(10), band(20)) * w8;
            weight += w8;
        }
    }
    if weight > 0.0 { sum / weight } else { Vec3::ZERO }
}

/// What comes into `block` from each direction, seen from its centre: the
/// radiance the nearest tile's probe saw that way, on the face the
/// direction leaves through; as the two cubemaps of an environment light.
fn environment(block: &Block, tiles: &[(Tile, Arc<light::Drawn>)]) -> (Image, Image) {
    let (lo, hi) = (block.min(), block.max());
    let centre = (lo + hi) / 2.0;
    let half_field = (tiles::PROBE_FOV / 2.0).to_radians();
    let sample = |d: Vec3| -> Vec3 {
        // Where the direction leaves the block.
        let t = |axis: usize| {
            let v = d[axis];
            if v.abs() < 1e-6 {
                f32::INFINITY
            } else if v > 0.0 {
                (hi[axis] - centre[axis]) / v
            } else {
                (lo[axis] - centre[axis]) / v
            }
        };
        let (tx, ty, tz) = (t(0), t(1), t(2));
        let (axis, along) = if tx <= ty && tx <= tz {
            (0, tx)
        } else if ty <= tz {
            (1, ty)
        } else {
            (2, tz)
        };
        let exit = centre + d * along;
        let face = Face::ALL[axis * 2 + usize::from(d[axis] > 0.0)];
        // The nearest tile of that face that has come.
        let Some((tile, drawn)) = tiles
            .iter()
            .filter(|(t, _)| t.face == face)
            .min_by(|a, b| a.0.centre().distance_squared(exit).total_cmp(&b.0.centre().distance_squared(exit)))
        else {
            return Vec3::ZERO;
        };
        let (normal, up) = (tile.face.normal(), tile.face.up());
        let right = normal.cross(up);
        let (x, y, z) = (d.dot(right), d.dot(up), d.dot(normal));
        let theta = Vec2::new(x, y).length().atan2(z);
        let r = theta / half_field;
        let phi = y.atan2(x);
        let (u, v) = (0.5 + 0.5 * r * phi.cos(), 0.5 - 0.5 * r * phi.sin());
        let (w, h) = (drawn.width as usize, drawn.height as usize);
        let (col, row) = (((u * w as f32) as usize).min(w - 1), ((v * h as f32) as usize).min(h - 1));
        let packed = drawn.radiance[row * w + col];
        let band = |shift: u32| light::decode_radiance(((packed >> shift) & 1023) as f32 / 1023.0);
        Vec3::new(band(0), band(10), band(20))
    };
    osage_render::sky_light::cubemaps(sample)
}

/// The environment light of the block the walker is in, once made.
fn lit(
    mut commands: Commands,
    fetcher: Res<Fetcher>,
    mut store: ResMut<Store>,
    mut images: ResMut<Assets<Image>>,
    cameras: Query<Entity, With<osage_render::OsageCamera>>,
) {
    for (block, diffuse, specular) in fetcher.lights.1.try_iter() {
        if store.block != Some(block) {
            continue;
        }
        let (diffuse_map, specular_map) = (images.add(diffuse), images.add(specular));
        for camera in &cameras {
            commands.entity(camera).insert(bevy::light::EnvironmentMapLight {
                diffuse_map: diffuse_map.clone(),
                specular_map: specular_map.clone(),
                intensity: NITS,
                ..default()
            });
        }
        store.lit = Some(block);
    }
}
