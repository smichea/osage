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
/// Threads fetching and decoding light: a probe (4096 × 2048) takes about
/// a second of one; more than four starve the picture of the processor.
const WORKERS: usize = 4;

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
    /// Where each probe that has come stands (D-137: its `meta.json`).
    centres: HashMap<String, Vec3>,
    /// Whether the server has been asked what the walker needs and not answered.
    needing: bool,
    /// Whether the block drawn now takes its light from probes, and whose.
    cubed: bool,
    probes_of: Option<IVec3>,
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
    /// A tile or a probe, and where a probe stands.
    Tile {
        key: String,
        drawn: light::Drawn,
        bytes: u64,
        centre: Option<Vec3>,
    },
    Failed(String),
}

/// A block's environment light: its two cubemaps.
type Lit = (Block, Image, Image);

#[derive(Resource)]
struct Fetcher {
    /// What to fetch: the server's needs, the tiles and the cubes' centre
    /// probes first (a cube can be drawn with its centre's alone), then the
    /// other probes.
    asks: Sender<Ask>,
    later: Sender<Ask>,
    /// The server asked what the walker needs, on a thread of its own: behind
    /// probes a second long each, the answers came seconds late.
    needs: Sender<Ask>,
    gots: Receiver<Got>,
    /// The environment lights made from blocks' light, as they are done.
    lights: (Sender<Lit>, Receiver<Lit>),
}

impl Fetcher {
    /// Where a tile or probe waits: a cube's other probes after the rest.
    fn queue(&self, key: &str) -> &Sender<Ask> {
        if key.starts_with("c_") && !key.ends_with("_0") { &self.later } else { &self.asks }
    }
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
    let (asks, first) = crossbeam_channel::unbounded::<Ask>();
    let (later, rest) = crossbeam_channel::unbounded::<Ask>();
    let (from_workers, gots) = crossbeam_channel::unbounded::<Got>();
    let (needs, asked) = crossbeam_channel::unbounded::<Ask>();
    {
        let (from_workers, (keep, never)) = (from_workers.clone(), crossbeam_channel::unbounded::<Ask>());
        thread::Builder::new()
            .name("paris-need".to_owned())
            .spawn(move || {
                let _keep = keep;
                work(asked, never, from_workers)
            })
            .expect("the need thread");
    }
    for i in 0..WORKERS {
        let (first, rest, from_workers) = (first.clone(), rest.clone(), from_workers.clone());
        thread::Builder::new()
            .name(format!("paris-light-{i}"))
            .spawn(move || work(first, rest, from_workers))
            .expect("a light thread");
    }
    commands.insert_resource(Fetcher { asks, later, needs, gots, lights: crossbeam_channel::unbounded() });
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

fn work(first: Receiver<Ask>, rest: Receiver<Ask>, gots: Sender<Got>) {
    let mut clients: HashMap<String, Client> = HashMap::new();
    loop {
        // What comes first, first.
        let ask = match first.try_recv() {
            Ok(ask) => ask,
            Err(_) => crossbeam_channel::select! {
                recv(first) -> ask => match ask { Ok(ask) => ask, Err(_) => return },
                recv(rest) -> ask => match ask { Ok(ask) => ask, Err(_) => return },
            },
        };
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
                // A probe inside a cube (D-137) says where it stands.
                let centre =
                    if key.starts_with("c_") { fetch("meta.json").ok().and_then(|m| centre(&m)) } else { None };
                match (fetch("colour.ivf"), fetch("depth.ivf"), fetch("mask.ivf")) {
                    (Ok(c), Ok(d), Ok(m)) => {
                        let bytes = (c.len() + d.len() + m.len()) as u64;
                        let started = std::time::Instant::now();
                        let decoded = light::decode(&c, &d, &m).map(|mut v| v.pop());
                        debug!(key, ms = started.elapsed().as_millis() as u64, "paris: light decoded");
                        match decoded {
                            Ok(Some(drawn)) => Got::Tile { key, drawn, bytes, centre },
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

/// Where a probe stands, from its `meta.json`.
fn centre(meta: &[u8]) -> Option<Vec3> {
    let meta: serde_json::Value = serde_json::from_slice(meta).ok()?;
    let c = meta["centre"].as_array()?;
    let at = |i: usize| c.get(i).and_then(|v| v.as_f64()).map(|v| v as f32);
    Some(Vec3::new(at(0)?, at(1)?, at(2)?))
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
            // One question at a time.
            if !store.needing {
                store.needing = true;
                let _ = fetcher.needs.send(Ask::Need { source, walker: id });
            }
        }
        (Source::Folder(_), _) => {
            // The probes of the cubes within reach (D-137), or their blocks'
            // surface tiles where those are not baked (D-136).
            let probes = options.cube_probes > 0;
            let wanted = tiles::needed_light(me.eyes(), me.speed, HORIZON, |k| {
                store.present.as_ref().is_some_and(|p| p.contains(k)) && (probes || !k.starts_with("c_"))
            });
            for key in wanted {
                if store.asked.insert(key.clone()) {
                    let _ = fetcher.queue(&key).send(Ask::Tile { source: source.clone(), key });
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
                store.needing = false;
                let Some(source) = source(&options, &server) else { continue };
                debug!(new = keys.iter().filter(|k| !store.asked.contains(*k)).count(), "paris: light needed");
                for key in keys {
                    if store.asked.insert(key.clone()) {
                        let _ = fetcher.queue(&key).send(Ask::Tile { source: source.clone(), key });
                    }
                }
            }
            Got::Tile { key, drawn, bytes, centre } => {
                debug!(key, placed = centre.is_some(), "paris: light came");
                store.bytes += bytes;
                store.received += 1;
                if let Some(centre) = centre {
                    store.centres.insert(key.clone(), centre);
                }
                store.tiles.insert(key, Arc::new(drawn));
                store.changed = true;
            }
            Got::Failed(what) => {
                if what.starts_with("need") {
                    store.needing = false;
                }
                warn!(what, "paris: the light");
            }
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
    // The cube's probes (D-137); while they are on their way, the probes
    // drawn already, of the cube just left, which stand in this block too;
    // where the cube has none, the block's surface tiles (D-136).
    let probes =
        if options.cube_probes > 0 { cube_probes(&store, options.cube_probes, block.cube) } else { Vec::new() };
    // Neither this cube's probes (its centre's) nor its block's tiles all
    // here, and the eyes still inside the block drawn (a walker who has just
    // stepped into a neighbouring cube is): that block stays as it is, its
    // faces, its light and its meshes, until the new one's light has come.
    let tiled = block.tiles().iter().all(|t| store.tiles.contains_key(&t.key()));
    let holds = store.block.is_some_and(|drawn| drawn != block && drawn.contains(me.eyes()));
    debug!(cube = ?block.cube, probes = probes.len(), tiled, holds, "paris: the block's light");
    if probes.is_empty() && !tiled && holds {
        return;
    }
    if !probes.is_empty() {
        // Drawn again with the centre's probe alone, then once all have come:
        // stacking them costs the frame a few hundred megabytes of copying.
        let same = store.probes_of == Some(block.cube) && store.cubed;
        let all = probes.len() >= options.cube_probes.min(tiles::PROBES);
        if store.block == Some(block) && same && (probes.len() == store.drawn || !all) {
            return;
        }
        let relit = !same;
        store.block = Some(block);
        store.drawn = probes.len();
        store.cubed = true;
        store.probes_of = Some(block.cube);
        if let (true, Some((_, drawn))) = (relit, probes.first().cloned()) {
            let done = fetcher.lights.0.clone();
            let _ = thread::Builder::new().name("paris-environment".to_owned()).spawn(move || {
                let (diffuse, specular) = environment_seen(&drawn);
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
        for glow in &glows {
            commands.entity(glow).despawn();
        }
        let (lo, hi) = (block.min(), block.max());
        // Traced (T4), the block is lit by its faces as lamps: Solari's
        // realtime path barely lights it from its environment map (research
        // 10). Each face shines what the centre probe saw come in through it,
        // all scaled for the floor in the middle of the block to gather from
        // them what the probe saw come in.
        let lamps = match (traced.0, probes.first()) {
            (true, Some((centre, drawn))) => {
                let shine = Face::ALL.map(|face| seen_through(drawn, face.normal()));
                let scale = lamps_scale(drawn, &shine, *centre, &block);
                debug!(scale, "paris: the faces' lamps");
                Some(shine.map(|l| l * scale))
            }
            _ => None,
        };
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
            if let (Ok(quad), Some(lamps)) = (quads.get(*entity), lamps) {
                let luminance = lamps[index] * NITS;
                let lamp = osage_render::Emitter(LinearRgba::rgb(luminance.x, luminance.y, luminance.z));
                let glow = commands
                    .spawn((Mesh3d(quad.0.clone()), lamp, Transform::default(), Visibility::Inherited, Glow))
                    .id();
                commands.entity(*entity).add_child(glow);
            }
        }
        return;
    }
    let mine: Vec<(Tile, Arc<light::Drawn>)> =
        block.tiles().into_iter().filter_map(|t| store.tiles.get(&t.key()).map(|d| (t, d.clone()))).collect();
    let moved = store.block != Some(block) || store.cubed;
    if !moved && mine.len() == store.drawn {
        return;
    }
    store.block = Some(block);
    store.drawn = mine.len();
    store.cubed = false;
    store.probes_of = None;
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
            // The block's tiles, not probes.
            material.shape = Vec4::new(side as f32, side as f32, 0.0, 0.0);
            material.probes = [Vec4::new(0.0, 0.0, 0.0, -1.0); 5];
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

/// The cube's first `count` probes that have come, each with where it
/// stands, its centre's first; none until its centre's has.
fn cube_probes(store: &Store, count: usize, cube: IVec3) -> Vec<(Vec3, Arc<light::Drawn>)> {
    let keys = tiles::probe_keys(cube);
    if !store.centres.contains_key(&keys[0]) || !store.tiles.contains_key(&keys[0]) {
        return Vec::new();
    }
    keys.into_iter().take(count).filter_map(|k| Some((*store.centres.get(&k)?, store.tiles.get(&k)?.clone()))).collect()
}

/// What a probe inside the cube saw come in from direction `d`
/// (equirectangular: the middle column north, east to its right).
fn seen_towards(drawn: &light::Drawn, d: Vec3) -> Vec3 {
    let (w, h) = (drawn.width as usize, drawn.height as usize);
    let d = d.normalize_or(Vec3::Y);
    let lon = d.x.atan2(-d.z);
    let lat = d.y.clamp(-1.0, 1.0).asin();
    let u = 0.5 + lon / std::f32::consts::TAU;
    let v = 0.5 - lat / std::f32::consts::PI;
    let (col, row) = (((u * w as f32) as usize).min(w - 1), ((v * h as f32) as usize).min(h - 1));
    let packed = drawn.radiance[row * w + col];
    let band = |shift: u32| light::decode_radiance(((packed >> shift) & 1023) as f32 / 1023.0);
    Vec3::new(band(0), band(10), band(20))
}

/// How much the faces' lamps (`shine`, in `Face::ALL`'s order) must be
/// scaled for the floor in the middle of the block, under the centre probe,
/// to gather from them as much light as the probe saw come in: over the
/// half sphere above, weighed by the cosine, each direction's lamp being
/// the face it leaves the block by.
fn lamps_scale(drawn: &light::Drawn, shine: &[Vec3; 6], centre: Vec3, block: &Block) -> f32 {
    let luminance = |v: Vec3| 0.2126 * v.x + 0.7152 * v.y + 0.0722 * v.z;
    let floor = centre - Vec3::Y * osage_world_paris::walk::EYES;
    let (lo, hi) = (block.min(), block.max());
    let (mut seen, mut lit) = (0.0f32, 0.0f32);
    const N: usize = 48;
    for i in 0..N {
        for j in 0..N {
            // Directions spread as the cosine over the half sphere above.
            let (u, v) = ((i as f32 + 0.5) / N as f32, (j as f32 + 0.5) / N as f32);
            let (sine, phi) = (u.sqrt(), v * std::f32::consts::TAU);
            let d = Vec3::new(sine * phi.cos(), (1.0 - u).sqrt(), sine * phi.sin());
            let leave = |axis: usize| {
                if d[axis].abs() < 1e-6 {
                    f32::INFINITY
                } else if d[axis] > 0.0 {
                    (hi[axis] - floor[axis]) / d[axis]
                } else {
                    (lo[axis] - floor[axis]) / d[axis]
                }
            };
            let axis = (0..3).min_by(|&a, &b| leave(a).total_cmp(&leave(b))).unwrap_or(1);
            lit += luminance(shine[axis * 2 + usize::from(d[axis] > 0.0)]);
            seen += luminance(seen_towards(drawn, d));
        }
    }
    if lit > 0.0 { (seen / lit).clamp(0.25, 4.0) } else { 1.0 }
}

/// What a probe inside the cube saw come in through the half sphere
/// around `out`, weighed by the cosine: the radiance a lamp facing in from
/// that side of the block shines with.
fn seen_through(drawn: &light::Drawn, out: Vec3) -> Vec3 {
    let (w, h) = (drawn.width as usize, drawn.height as usize);
    let step = (w / 256).max(1);
    let (mut sum, mut weight) = (Vec3::ZERO, 0.0);
    for row in (0..h).step_by(step) {
        let lat = std::f32::consts::FRAC_PI_2 - (row as f32 + 0.5) / h as f32 * std::f32::consts::PI;
        for col in (0..w).step_by(step) {
            let lon = ((col as f32 + 0.5) / w as f32 - 0.5) * std::f32::consts::TAU;
            let d = Vec3::new(lat.cos() * lon.sin(), lat.sin(), -lat.cos() * lon.cos());
            let cosine = d.dot(out);
            if cosine <= 0.0 {
                continue;
            }
            // An equirectangular pixel spans the cosine of its latitude.
            let w8 = cosine * lat.cos();
            let packed = drawn.radiance[row * w + col];
            let band = |shift: u32| light::decode_radiance(((packed >> shift) & 1023) as f32 / 1023.0);
            sum += Vec3::new(band(0), band(10), band(20)) * w8;
            weight += w8;
        }
    }
    if weight > 0.0 { sum / weight } else { Vec3::ZERO }
}

/// What comes into the block from each direction as a probe inside it saw
/// it (equirectangular: the middle column north, east to its right).
fn environment_seen(drawn: &light::Drawn) -> (Image, Image) {
    osage_render::sky_light::cubemaps(|d| seen_towards(drawn, d))
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
