// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The light coming into the walker's block (D-136): its tiles asked of the
//! world's server of light as the walker goes (the server says which), or
//! read from the street's folder when walking alone; fetched and decoded
//! by threads of their own; the current block's put in two texture arrays
//! and drawn on the block's six faces, from inside (`light.wesl`). The same
//! light lights the block: what comes in from each direction, seen from
//! the block's centre, made into the camera's environment light; traced
//! (T4), shone by a dome round the block, the rays' sky.
//!
//! A probe packed as tiles (D-141) comes at half its size first, doubled
//! into its layer, and as its tiles, fetched whole and decoded a few at a
//! time from where the walker looks outwards, each written over the half
//! where it belongs.

use std::{
    collections::{HashMap, HashSet},
    f32::consts::{PI, TAU},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};

use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    image::ImageSampler,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        render_asset::RenderAssets,
        render_resource::{
            AsBindGroup, Extent3d, Origin3d, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect,
            TextureDimension, TextureFormat, TextureUsages, TextureViewDescriptor, TextureViewDimension,
        },
        renderer::RenderQueue,
        texture::GpuImage,
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
/// A probe's tiles a thread decodes at once (D-141): about 35 ms.
const PART: usize = 4;
/// Decodings of tiles asked at once: enough to keep the threads busy, few
/// enough that the tiles follow where the walker looks.
const PARTS_OUT: usize = 2 * WORKERS;
/// How far from where the walker looks its cube's tiles are decoded
/// (D-141), radians: running, the middle of the picture (its corners lie
/// 55° off at 70° of field); walking, all of it, and a margin for the
/// probes standing beside the eyes; standing, all round.
const RUNNING_CONE: f32 = 35.0 * PI / 180.0;
const WALKING_CONE: f32 = 70.0 * PI / 180.0;

pub struct LightPlugin;

impl Plugin for LightPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "light.wesl");
        app.add_plugins(MaterialPlugin::<IncomingLight>::default())
            .init_resource::<Store>()
            .init_resource::<LightServer>()
            .init_resource::<WalkerId>()
            .init_resource::<Layers>()
            .add_systems(Startup, (start, faces, dome))
            .add_systems(Update, (ask, receive, arrays, ask_parts, lit).chain());
    }

    fn finish(&self, app: &mut App) {
        let layers = app.world().resource::<Layers>().clone();
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.insert_resource(layers).add_systems(Render, write_layers.in_set(RenderSystems::PrepareResources));
        }
    }
}

/// The probes' light to write into their layers of the cube's texture
/// arrays, shared with the render world, which writes them straight from
/// the decoded light: nothing is copied on the main thread, and a probe goes
/// to the GPU once, when it has come (stacking the cube's probes again into
/// new arrays each time one came held the picture up to 0.9 s).
#[derive(Resource, Clone, Default)]
struct Layers(Arc<Mutex<Uploads>>);

#[derive(Default)]
struct Uploads {
    /// What is still to write, in the order it came.
    pending: Vec<Layer>,
    /// Each array's layers written whole (a bit each): those the faces may
    /// read.
    written: HashMap<AssetId<Image>, u8>,
}

/// A probe's light, or its distances, for a layer of an array, and how many
/// of its rows are written.
struct Layer {
    image: AssetId<Image>,
    layer: u32,
    drawn: Arc<light::Drawn>,
    distance: bool,
    row: u32,
    /// Where its top left goes in the layer: the corner, a probe's light
    /// whole; or one of its tiles (D-141), where it belongs.
    at: UVec2,
    /// A probe's light whole (the faces may read the layer once it is
    /// written), not a tile over it.
    whole: bool,
}

/// A probe's light, whole, for its layer `k` of the cube's arrays.
fn push_whole(uploads: &mut Uploads, arrays: &Arrays, k: usize, drawn: &Arc<light::Drawn>) {
    let (_, _, colours, distances) = arrays;
    for (image, distance) in [(colours.id(), false), (distances.id(), true)] {
        let (drawn, at) = (drawn.clone(), UVec2::ZERO);
        uploads.pending.push(Layer { image, layer: k as u32, drawn, distance, row: 0, at, whole: true });
    }
}

/// What the render world writes a frame, bytes: a probe's two layers
/// (64 MiB at 4096 × 2048) written at once held a frame 15 to 90 ms on an
/// Iris Xe, several at once up to 0.66 s.
const WRITTEN_A_FRAME: u64 = 16 << 20;

/// Writes, in bands of rows, the layers whose arrays are on the GPU, so
/// much a frame; the rest waits for the next. A tile waits for its layer's
/// light whole, which would write over it.
fn write_layers(layers: Res<Layers>, images: Res<RenderAssets<GpuImage>>, queue: Res<RenderQueue>) {
    let Ok(mut uploads) = layers.0.lock() else { return };
    let Uploads { pending, written } = &mut *uploads;
    let mut spent = 0u64;
    // The layers a light whole is still to be written into, this frame.
    let mut unfinished: HashSet<(AssetId<Image>, u32)> = HashSet::new();
    let wait = |layer: &Layer, unfinished: &mut HashSet<(AssetId<Image>, u32)>| {
        if layer.whole {
            unfinished.insert((layer.image, layer.layer));
        }
        true
    };
    pending.retain_mut(|layer| {
        if spent >= WRITTEN_A_FRAME || (!layer.whole && unfinished.contains(&(layer.image, layer.layer))) {
            return wait(layer, &mut unfinished);
        }
        let Some(gpu) = images.get(layer.image) else { return wait(layer, &mut unfinished) };
        let (width, height) = (layer.drawn.width, layer.drawn.height);
        let row_bytes = 4 * width as u64;
        let rows = (((WRITTEN_A_FRAME - spent) / row_bytes).max(1) as u32).min(height - layer.row);
        let (from, to) = ((layer.row * width) as usize, ((layer.row + rows) * width) as usize);
        let bytes: &[u8] = if layer.distance {
            bytemuck::cast_slice(&layer.drawn.distance[from..to])
        } else {
            bytemuck::cast_slice(&layer.drawn.radiance[from..to])
        };
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &gpu.texture,
                mip_level: 0,
                origin: Origin3d { x: layer.at.x, y: layer.at.y + layer.row, z: layer.layer },
                aspect: TextureAspect::All,
            },
            bytes,
            TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * width), rows_per_image: Some(rows) },
            Extent3d { width, height: rows, depth_or_array_layers: 1 },
        );
        spent += rows as u64 * row_bytes;
        layer.row += rows;
        if layer.row < height {
            return wait(layer, &mut unfinished);
        }
        if layer.whole {
            *written.entry(layer.image).or_default() |= 1 << layer.layer;
        }
        false
    });
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
    /// The texture arrays of the cube the eyes are in, a layer a probe,
    /// filled as its probes come; which probes have been sent to them, which
    /// are written whole (a bit each), and which the faces show.
    arrays: Option<Arrays>,
    sent: u8,
    written: u8,
    shown: u8,
    /// The probes packed as tiles (D-141), by key: their files, and the
    /// tiles decoded so far.
    parts: HashMap<String, Parts>,
    /// Decodings of tiles asked and not answered.
    parts_out: usize,
    /// The layers whose light, at half its size, is being doubled to fill
    /// them (D-141), a bit each.
    doubling: u8,
    /// Each layer's tiles sent to the arrays (D-141), a bit each.
    tiled: [u128; tiles::PROBES],
    /// Whether a probe of the cube shows some of its light at half its size
    /// still, its tiles not all sent (D-141).
    pub halved: bool,
}

/// A cube's texture arrays: the cube, their size, the light's and the
/// distances'.
type Arrays = (IVec3, (u32, u32), Handle<Image>, Handle<Image>);

/// A probe packed as tiles (D-141): its three files, fetched whole, and the
/// tiles decoded where the walker looks.
struct Parts {
    files: Arc<[Vec<u8>; 3]>,
    /// Tiles across and down.
    grid: (u32, u32),
    /// The tiles asked to decode, a bit each.
    asked: u128,
    decoded: HashMap<usize, Arc<light::Drawn>>,
}

/// A probe's light whole for a layer of a cube's arrays, doubled from half
/// its size: the cube, the layer.
type Doubled = (IVec3, usize, Arc<light::Drawn>);

/// A probe of the walker's cube: its layer, where it stands, the light its
/// layer starts with, and whether that is at half its size.
type Probe = (usize, Vec3, Arc<light::Drawn>, bool);

#[derive(Clone)]
enum Source {
    Server(String),
    Folder(PathBuf),
}

enum Ask {
    Need {
        source: Source,
        walker: u64,
    },
    Tile {
        source: Source,
        key: String,
    },
    /// Some tiles of a probe packed as tiles (D-141), to decode.
    Part {
        key: String,
        files: Arc<[Vec<u8>; 3]>,
        tiles: Vec<usize>,
    },
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
    /// A probe packed as tiles (D-141), fetched, not decoded.
    Tiles {
        key: String,
        files: Arc<[Vec<u8>; 3]>,
        grid: (u32, u32),
        bytes: u64,
        centre: Option<Vec3>,
    },
    /// Some of its tiles, decoded (none if they could not be).
    Parts {
        key: String,
        tiles: Vec<(usize, light::Drawn)>,
    },
    Failed(String),
}

/// A block's environment light: its two cubemaps.
/// A block's environment light, made; with the dome's centre and picture
/// when it is lit by the probes of its cube and traced.
type Lit = (Block, Image, Image, Option<(Vec3, Image)>);

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
    /// Tiles of probes to decode (D-141): after what comes first, before the
    /// cubes' other probes.
    parts: Sender<Ask>,
    gots: Receiver<Got>,
    /// The environment lights made from blocks' light, as they are done.
    lights: (Sender<Lit>, Receiver<Lit>),
    /// The layers' light doubled from half its size, as it is done.
    doubled: (Sender<Doubled>, Receiver<Doubled>),
}

impl Fetcher {
    /// Where a tile or probe waits: a cube's other probes after the rest.
    fn queue(&self, key: &str) -> &Sender<Ask> {
        let probe = key.starts_with("c_") || key.starts_with("h_");
        if probe && !key.ends_with("_0") { &self.later } else { &self.asks }
    }
}

/// The six faces of the block, as entities with their materials.
#[derive(Resource)]
struct Faces([(Entity, Handle<IncomingLight>); 6]);

/// A face's light for traced rays (T4 only), where the block is lit by its
/// surface tiles: the mean of what its tiles saw coming in, as a lamp the
/// size of the face, the coarse light that lights the block (D-136); with
/// its cube's probes the dome lights it instead. Remade for each block.
#[derive(Component)]
struct Glow;

fn start(mut commands: Commands, options: Res<Options>, mut store: ResMut<Store>) {
    let (asks, first) = crossbeam_channel::unbounded::<Ask>();
    let (later, rest) = crossbeam_channel::unbounded::<Ask>();
    let (parts, tiles) = crossbeam_channel::unbounded::<Ask>();
    let (from_workers, gots) = crossbeam_channel::unbounded::<Got>();
    let (needs, asked) = crossbeam_channel::unbounded::<Ask>();
    {
        let (from_workers, (keep, never)) = (from_workers.clone(), crossbeam_channel::unbounded::<Ask>());
        thread::Builder::new()
            .name("paris-need".to_owned())
            .spawn(move || {
                let _keep = keep;
                work(asked, never.clone(), never, from_workers)
            })
            .expect("the need thread");
    }
    for i in 0..WORKERS {
        let (first, tiles, rest, from_workers) = (first.clone(), tiles.clone(), rest.clone(), from_workers.clone());
        thread::Builder::new()
            .name(format!("paris-light-{i}"))
            .spawn(move || work(first, tiles, rest, from_workers))
            .expect("a light thread");
    }
    commands.insert_resource(Fetcher {
        asks,
        later,
        needs,
        parts,
        gots,
        lights: crossbeam_channel::unbounded(),
        doubled: crossbeam_channel::unbounded(),
    });
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

/// Decoding gives way to the picture: a fetching thread, and the threads it
/// starts (rav1d's, the colour's conversion), which inherit it, run at a
/// lower priority than the frame's, so that a busy processor draws first and
/// decodes after.
fn give_way() {
    #[cfg(target_os = "linux")]
    // SAFETY: setpriority on the calling thread (`who` 0) changes nothing but
    // its scheduling weight; it reads and writes no memory of ours.
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 10);
    }
}

fn work(first: Receiver<Ask>, parts: Receiver<Ask>, rest: Receiver<Ask>, gots: Sender<Got>) {
    give_way();
    let mut clients: HashMap<String, Client> = HashMap::new();
    loop {
        // What comes first, first; then the tiles where the walker looks.
        let ask = match first.try_recv().or_else(|_| parts.try_recv()) {
            Ok(ask) => ask,
            Err(_) => crossbeam_channel::select! {
                recv(first) -> ask => match ask { Ok(ask) => ask, Err(_) => return },
                recv(parts) -> ask => match ask { Ok(ask) => ask, Err(_) => return },
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
                // A probe inside a cube (D-137) says where it stands, and
                // whether it is packed as tiles (D-141).
                let meta = if key.starts_with("c_") || key.starts_with("h_") { fetch("meta.json").ok() } else { None };
                let centre = meta.as_deref().and_then(centre);
                let grid = meta.as_deref().and_then(tiled);
                if let Some(grid) = grid {
                    let files = (fetch("tiles-colour.ivf"), fetch("tiles-depth.ivf"), fetch("tiles-mask.ivf"));
                    let got = match files {
                        (Ok(c), Ok(d), Ok(m)) => {
                            let bytes = (c.len() + d.len() + m.len()) as u64;
                            Got::Tiles { key, files: Arc::new([c, d, m]), grid, bytes, centre }
                        }
                        (Err(e), ..) | (_, Err(e), _) | (.., Err(e)) => Got::Failed(format!("{key}: {e}")),
                    };
                    if gots.send(got).is_err() {
                        return;
                    }
                    continue;
                }
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
            Ask::Part { key, files, tiles } => {
                let started = std::time::Instant::now();
                let [c, d, m] = &*files;
                let decoded = light::decode_tiles(c, d, m, &tiles);
                debug!(key, tiles = tiles.len(), ms = started.elapsed().as_millis() as u64, "paris: tiles decoded");
                match decoded {
                    Ok(drawn) => Got::Parts { key, tiles: tiles.into_iter().zip(drawn).collect() },
                    Err(e) => {
                        warn!(key, "paris: tiles: {e}");
                        Got::Parts { key, tiles: Vec::new() }
                    }
                }
            }
        };
        if gots.send(got).is_err() {
            return;
        }
    }
}

/// A probe's tiles across and down, from its `meta.json`, when it is packed
/// as tiles (D-141): at most 128, a bit each.
fn tiled(meta: &[u8]) -> Option<(u32, u32)> {
    let meta: serde_json::Value = serde_json::from_slice(meta).ok()?;
    let grid = meta["tiles"].as_array()?;
    let (columns, rows) = (grid.first()?.as_u64()? as u32, grid.get(1)?.as_u64()? as u32);
    (columns > 0 && rows > 0 && columns * rows <= 128).then_some((columns, rows))
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
            let wanted = tiles::needed_light(me.eyes(), me.velocity, HORIZON, |k| {
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
            Got::Tiles { key, files, grid, bytes, centre } => {
                debug!(key, "paris: light came, as tiles");
                store.bytes += bytes;
                store.received += 1;
                if let Some(centre) = centre {
                    store.centres.insert(key.clone(), centre);
                }
                store.parts.insert(key, Parts { files, grid, asked: 0, decoded: HashMap::new() });
            }
            Got::Parts { key, tiles } => {
                store.parts_out = store.parts_out.saturating_sub(1);
                if let Some(parts) = store.parts.get_mut(&key) {
                    for (t, drawn) in tiles {
                        parts.asked |= 1 << t;
                        parts.decoded.insert(t, Arc::new(drawn));
                    }
                }
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

/// Traced (T4), the light coming in as the sky of the rays (D-138): a
/// sphere round the block, facing in, shining what the centre probe saw
/// come in from each direction. The rays see the block alone (`street` cuts
/// what reaches out of it): one that leaves it meets the dome and takes the
/// light that came in from its own direction. (Solari's realtime path took
/// next to nothing from the camera's environment map, research 10; lamps on
/// the faces gave each face's light as one.)
#[derive(Resource)]
struct Dome {
    entity: Entity,
    material: Handle<StandardMaterial>,
}

/// The dome's radius, m: beyond the street seen, and within the 50 m
/// Solari's light cache traces its rays to (`world_cache_max_gi_ray_distance`);
/// farther, a ray of the cache that met nothing would take the environment
/// map's light besides the dome's, counted twice.
const DOME_RADIUS: f32 = 40.0;
/// Its picture of the light, columns (half as many rows).
const DOME_WIDTH: usize = 1024;

fn dome(
    mut commands: Commands,
    traced: Res<osage_render::TracedLight>,
    options: Res<Options>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
) {
    // The whole street drawn (`--whole`), its light is traced through it.
    if !traced.0 || options.whole {
        return;
    }
    let material = standard.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: LinearRgba::rgb(NITS, NITS, NITS),
        // A luminance already: the traced glows leave it as it is.
        emissive_exposure_weight: 1.0,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let entity = commands
        .spawn((
            Mesh3d(meshes.add(dome_mesh())),
            MeshMaterial3d(material.clone()),
            Transform::default(),
            Visibility::Hidden,
            bevy::light::NotShadowCaster,
        ))
        .id();
    commands.insert_resource(Dome { entity, material });
}

/// A sphere of `DOME_RADIUS`, facing in, its picture equirectangular as a
/// probe's: the middle column north (-z), east to its right, the top row up.
fn dome_mesh() -> Mesh {
    let (sectors, stacks) = (96usize, 48usize);
    let (mut positions, mut normals, mut uvs) = (Vec::new(), Vec::new(), Vec::new());
    for i in 0..=stacks {
        let v = i as f32 / stacks as f32;
        let lat = (0.5 - v) * std::f32::consts::PI;
        for j in 0..=sectors {
            let u = j as f32 / sectors as f32;
            let lon = (u - 0.5) * std::f32::consts::TAU;
            let d = Vec3::new(lat.cos() * lon.sin(), lat.sin(), -lat.cos() * lon.cos());
            positions.push((d * DOME_RADIUS).to_array());
            normals.push((-d).to_array());
            uvs.push([u, v]);
        }
    }
    let row = sectors + 1;
    let mut indices = Vec::with_capacity(stacks * sectors * 6);
    for i in 0..stacks {
        for j in 0..sectors {
            let (a, b, c, d) = (i * row + j, i * row + j + 1, (i + 1) * row + j, (i + 1) * row + j + 1);
            // Wound to face the centre.
            indices.extend([a, c, b, b, c, d].map(|k| k as u32));
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}

/// What a probe saw come in, as the dome's picture: `DOME_WIDTH` columns,
/// each texel the mean of the probe's under it, radiance in half floats.
fn dome_picture(drawn: &light::Drawn) -> Image {
    let curve: Vec<f32> = (0..1024).map(|c| light::decode_radiance(c as f32 / 1023.0)).collect();
    let (w, h) = (drawn.width as usize, drawn.height as usize);
    let (dw, dh) = (DOME_WIDTH.min(w), (DOME_WIDTH / 2).min(h));
    let (sx, sy) = ((w / dw).max(1), (h / dh).max(1));
    let mut data = Vec::with_capacity(dw * dh * 8);
    for y in 0..dh {
        for x in 0..dw {
            let mut sum = Vec3::ZERO;
            for yy in 0..sy {
                for xx in 0..sx {
                    let packed = drawn.radiance[(y * sy + yy) * w + x * sx + xx];
                    let band = |shift: u32| curve[((packed >> shift) & 1023) as usize];
                    sum += Vec3::new(band(0), band(10), band(20));
                }
            }
            let c = sum / (sx * sy) as f32;
            for v in [c.x, c.y, c.z, 1.0] {
                data.extend(half(v).to_le_bytes());
            }
        }
    }
    let size = Extent3d { width: dw as u32, height: dh as u32, depth_or_array_layers: 1 };
    let mut image =
        Image::new(size, TextureDimension::D2, data, TextureFormat::Rgba16Float, RenderAssetUsages::default());
    image.sampler = ImageSampler::linear();
    image
}

/// A light's value as a half float, rounded (no light is negative).
fn half(v: f32) -> u16 {
    let bits = v.max(0.0).to_bits();
    let exponent = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mantissa = bits & 0x7f_ffff;
    if exponent <= 0 {
        return 0;
    }
    if exponent >= 31 {
        return 0x7c00;
    }
    let value = ((exponent as u16) << 10) | (mantissa >> 13) as u16;
    if mantissa & 0x1000 != 0 { value + 1 } else { value }
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

/// Texture arrays of `layers` layers, uninitialised: written layer by layer
/// in the render world (`write_layers`).
fn layered((width, height): (u32, u32), layers: u32, format: TextureFormat) -> Image {
    let mut image = Image::new_uninit(
        Extent3d { width, height, depth_or_array_layers: layers },
        TextureDimension::D2,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    // The layers not written yet are cleared when first drawn: as a render
    // target, by the GPU at once, not by copying a buffer of zeros.
    image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
    image.texture_view_descriptor =
        Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::D2Array), ..default() });
    image.sampler = ImageSampler::linear();
    image
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
    dome: Option<Res<Dome>>,
    layers: Res<Layers>,
) {
    let block = Block::of(me.eyes());
    // The layers' light doubled from half its size (D-141), sent to them.
    for (cube, k, drawn) in fetcher.doubled.1.try_iter() {
        let bit = 1 << k;
        let Some(arrays) = store.arrays.clone().filter(|a| a.0 == cube && store.doubling & bit != 0) else { continue };
        store.doubling &= !bit;
        if let Ok(mut uploads) = layers.0.lock() {
            push_whole(&mut uploads, &arrays, k, &drawn);
            store.sent |= bit;
            store.changed = true;
        }
    }
    // The probes written whole into the arrays since: drawn with them.
    if let (Some((_, _, colours, distances)), Ok(uploads)) = (store.arrays.clone(), layers.0.lock()) {
        let written = |image: &Handle<Image>| uploads.written.get(&image.id()).copied().unwrap_or(0);
        let both = written(&colours) & written(&distances);
        if both != store.written {
            store.written = both;
            store.changed = true;
        }
    }
    if store.block == Some(block) && !store.changed {
        return;
    }
    store.changed = false;
    let started = std::time::Instant::now();
    // The cube's probes (D-137); while they are on their way, the probes
    // drawn already, of the cube just left, which stand in this block too;
    // where the cube has none, the block's surface tiles (D-136).
    let decoded =
        if options.cube_probes > 0 { cube_probes(&store, options.cube_probes, block.cube) } else { Vec::new() };
    // This cube's arrays, filled as its probes decode, each written once
    // into its layer by the render world (`Layers`), a band a frame; a
    // probe at half its size doubled first (D-141), its tiles written over
    // it as they are decoded.
    if let Some((_, _, first, half)) = decoded.first() {
        let scale = if *half { 2 } else { 1 };
        let size = (first.width * scale, first.height * scale);
        // Made again for the probes of another size.
        if !matches!(&store.arrays, Some((cube, made, ..)) if *cube == block.cube && *made == size) {
            store.arrays = Some((
                block.cube,
                size,
                images.add(layered(size, tiles::PROBES as u32, TextureFormat::Rgb10a2Unorm)),
                images.add(layered(size, tiles::PROBES as u32, TextureFormat::R32Float)),
            ));
            (store.sent, store.written, store.doubling, store.tiled) = (0, 0, 0, [0; tiles::PROBES]);
        }
        let store = &mut *store;
        if let (Some(arrays), Ok(mut uploads)) = (store.arrays.clone(), layers.0.lock()) {
            // What was on its way to arrays no longer filled goes.
            let (_, _, colours, distances) = &arrays;
            let mine = |image: &AssetId<Image>| *image == colours.id() || *image == distances.id();
            uploads.pending.retain(|l| mine(&l.image));
            uploads.written.retain(|image, _| mine(image));
            for (k, _, drawn, half) in &decoded {
                let bit = 1 << k;
                let scale = if *half { 2 } else { 1 };
                if (store.sent | store.doubling) & bit != 0 || (drawn.width * scale, drawn.height * scale) != size {
                    continue;
                }
                if *half {
                    // Doubled on a thread of its own: eight million pixels.
                    store.doubling |= bit;
                    let (done, drawn, cube, k) = (fetcher.doubled.0.clone(), drawn.clone(), block.cube, *k);
                    let _ = thread::Builder::new().name("paris-double".to_owned()).spawn(move || {
                        give_way();
                        let _ = done.send((cube, k, Arc::new(drawn.doubled())));
                    });
                } else {
                    push_whole(&mut uploads, &arrays, *k, drawn);
                    store.sent |= bit;
                }
            }
            // The tiles decoded, over their layers' light once it is on its way.
            for (k, key) in tiles::probe_keys(block.cube).iter().enumerate() {
                let Some(parts) = store.parts.get(key).filter(|_| store.sent & 1 << k != 0) else { continue };
                let columns = parts.grid.0 as usize;
                for (&t, drawn) in &parts.decoded {
                    let at = UVec2::new((t % columns) as u32 * drawn.width, (t / columns) as u32 * drawn.height);
                    if store.tiled[k] & 1 << t != 0 || at.x + drawn.width > size.0 || at.y + drawn.height > size.1 {
                        continue;
                    }
                    for (image, distance) in [(colours.id(), false), (distances.id(), true)] {
                        let (layer, drawn) = (k as u32, drawn.clone());
                        uploads.pending.push(Layer { image, layer, drawn, distance, row: 0, at, whole: false });
                    }
                    store.tiled[k] |= 1 << t;
                }
            }
        }
        let keys = tiles::probe_keys(block.cube);
        store.halved = decoded.iter().any(|(k, _, _, half)| {
            *half && store.parts.get(&keys[*k]).is_none_or(|p| store.tiled[*k].count_ones() < p.grid.0 * p.grid.1)
        });
    }
    // The probes the faces can show: written whole, the centre's first.
    let probes: Vec<Probe> = if store.written & 1 != 0 {
        decoded.iter().filter(|(k, ..)| store.written & 1 << k != 0).cloned().collect()
    } else {
        Vec::new()
    };
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
        // Drawn with the probes written, the centre's first.
        let same = store.probes_of == Some(block.cube) && store.cubed;
        let shown = probes.iter().fold(0u8, |bits, (k, ..)| bits | 1 << k);
        if store.block == Some(block) && same && shown == store.shown {
            return;
        }
        let relit = !same;
        store.block = Some(block);
        store.drawn = probes.len();
        store.cubed = true;
        store.probes_of = Some(block.cube);
        if let (true, Some((_, centre, drawn, _))) = (relit, probes.first().cloned()) {
            let (done, domed) = (fetcher.lights.0.clone(), dome.is_some());
            let _ = thread::Builder::new().name("paris-environment".to_owned()).spawn(move || {
                let (diffuse, specular) = environment_seen(&drawn);
                let dome = domed.then(|| (centre, dome_picture(&drawn)));
                let _ = done.send((block, diffuse, specular, dome));
            });
        }
        let Some((_, size, colours, distances)) = store.arrays.clone() else { return };
        store.shown = shown;
        debug!(cube = ?block.cube, probes = probes.len(), ms = started.elapsed().as_micros() as f32 / 1000.0, "paris: the block's arrays told");
        // The probes that have come, the centre's first, each with its layer.
        let mut at = [Vec4::new(0.0, 0.0, 0.0, -1.0); 5];
        for (slot, (k, centre, ..)) in probes.iter().enumerate().take(5) {
            at[slot] = centre.extend(*k as f32);
        }
        // Traced (T4), the block is lit by the dome (`lit`), shining what the
        // centre probe saw come in, direction by direction: no face is a lamp.
        for glow in &glows {
            commands.entity(glow).despawn();
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
    let moved = store.block != Some(block) || store.cubed;
    if !moved && mine.len() == store.drawn {
        return;
    }
    store.block = Some(block);
    store.drawn = mine.len();
    store.cubed = false;
    store.probes_of = None;
    store.shown = 0;
    // Lit by the tiles' lamps: the dome shines a probe's light.
    if let Some(dome) = &dome
        && let Ok((_, mut visibility)) = places.get_mut(dome.entity)
    {
        *visibility = Visibility::Hidden;
    }
    // The block lit by its own light, on a thread: what comes in from each
    // direction, seen from its centre.
    if !mine.is_empty() {
        let (tiles, done) = (mine.clone(), fetcher.lights.0.clone());
        let _ = thread::Builder::new().name("paris-environment".to_owned()).spawn(move || {
            let (diffuse, specular) = environment(&block, &tiles);
            let _ = done.send((block, diffuse, specular, None));
        });
    }
    let side = mine.first().map_or(1, |(_, d)| d.width);
    let (colours, distances) = if mine.is_empty() {
        empty(&mut images)
    } else {
        let mut colour = Vec::with_capacity(mine.len() * (side * side * 4) as usize);
        let mut distance = Vec::with_capacity(colour.capacity());
        for (_, drawn) in &mine {
            colour.extend_from_slice(bytemuck::cast_slice(&drawn.radiance));
            distance.extend_from_slice(bytemuck::cast_slice(&drawn.distance));
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

/// The cube's first `count` probes whose light has come, each with where it
/// stands, the light its layer starts with, whole or at half its size
/// (D-140; D-141: doubled, its tiles written over it), its centre's first;
/// none until its centre's has.
fn cube_probes(store: &Store, count: usize, cube: IVec3) -> Vec<Probe> {
    let came = |key: &str| Some((*store.centres.get(key)?, store.tiles.get(key)?.clone()));
    let probes: Vec<Probe> = tiles::probe_keys(cube)
        .iter()
        .take(count)
        .enumerate()
        .filter_map(|(k, key)| {
            let ((centre, drawn), half) =
                came(key).map(|c| (c, false)).or_else(|| came(&tiles::half_key(key)).map(|c| (c, true)))?;
            Some((k, centre, drawn, half))
        })
        .collect();
    if probes.first().is_some_and(|(k, ..)| *k == 0) { probes } else { Vec::new() }
}

/// The cube of a probe inside a cube, from its key (`c_X_Y_Z_K`).
fn probe_cube(key: &str) -> Option<IVec3> {
    let mut parts = key.split('_').skip(1).map(|v| v.parse::<i32>().ok());
    Some(IVec3::new(parts.next()??, parts.next()??, parts.next()??))
}

/// How far each tile of a probe (equirectangular, `columns` × `rows` tiles,
/// row by row from the top left) lies from `forward`, radians: from the
/// nearest of its corners, sides' middles and middle; none for the one it
/// is in.
fn tile_angles((columns, rows): (u32, u32), forward: Vec3) -> Vec<f32> {
    let forward = forward.normalize_or(Vec3::NEG_Z);
    let direction = |u: f32, v: f32| {
        let (lon, lat) = ((u - 0.5) * TAU, (0.5 - v) * PI);
        Vec3::new(lat.cos() * lon.sin(), lat.sin(), -lat.cos() * lon.cos())
    };
    // The tile the walker looks into.
    let (u, v) = (0.5 + forward.x.atan2(-forward.z) / TAU, 0.5 - forward.y.clamp(-1.0, 1.0).asin() / PI);
    let into = ((u * columns as f32) as u32).min(columns - 1) + ((v * rows as f32) as u32).min(rows - 1) * columns;
    let mut out = Vec::with_capacity((columns * rows) as usize);
    for row in 0..rows {
        for column in 0..columns {
            let mut nearest = PI;
            for (a, b) in [0.0, 0.5, 1.0].into_iter().flat_map(|a| [0.0, 0.5, 1.0].map(|b| (a, b))) {
                let d = direction((column as f32 + a) / columns as f32, (row as f32 + b) / rows as f32);
                nearest = nearest.min(d.dot(forward).clamp(-1.0, 1.0).acos());
            }
            out.push(if row * columns + column == into { 0.0 } else { nearest });
        }
    }
    out
}

/// Where the walker looks, the tiles of its cube's probes packed as tiles
/// (D-141), decoded from the middle of its view outwards, a few at a time:
/// running, the middle alone; walking, the whole view; standing, all round.
/// Then those of the centre of the cube it goes to, for when it gets there.
/// The tiles decoded for cubes left behind go.
fn ask_parts(
    me: Res<Me>,
    options: Res<Options>,
    fetcher: Res<Fetcher>,
    mut store: ResMut<Store>,
    mut last: Local<Option<IVec3>>,
) {
    if options.cube_probes == 0 {
        return;
    }
    let cube = Block::of(me.eyes()).cube;
    if *last != Some(cube) {
        *last = Some(cube);
        for (key, parts) in &mut store.parts {
            let near = probe_cube(key).is_some_and(|c| (c - cube).abs().max_element() <= 1);
            if !near && !parts.decoded.is_empty() {
                parts.decoded.clear();
                parts.asked = 0;
            }
        }
    }
    if store.parts_out >= PARTS_OUT {
        return;
    }
    let forward = Quat::from_euler(EulerRot::YXZ, me.heading, me.pitch, 0.0) * Vec3::NEG_Z;
    let speed = Vec2::new(me.velocity.x, me.velocity.z).length();
    let cone = if speed > tiles::RUNNING {
        RUNNING_CONE
    } else if speed < tiles::STANDING {
        PI
    } else {
        WALKING_CONE
    };
    let mut keys: Vec<String> = tiles::probe_keys(cube).into_iter().take(options.cube_probes).collect();
    let own = keys.len();
    // The next cube on its way, a metre and a half ahead at most.
    let way = Vec3::new(me.velocity.x, 0.0, me.velocity.z).normalize_or_zero();
    let next = (1..=15).map(|i| Block::of(me.eyes() + way * (i as f32 / 10.0)).cube).find(|c| *c != cube);
    if let Some(next) = next.filter(|_| speed >= tiles::STANDING) {
        keys.extend(tiles::probe_keys(next).into_iter().take(1));
    }
    // Each tile wanted: whether its cube is the walker's, how far from where
    // the walker looks, whose, which.
    let mut wanted: Vec<(bool, f32, usize, usize)> = Vec::new();
    let mut angles: HashMap<(u32, u32), Vec<f32>> = HashMap::new();
    for (order, key) in keys.iter().enumerate() {
        let Some(parts) = store.parts.get(key) else { continue };
        let angles = angles.entry(parts.grid).or_insert_with(|| tile_angles(parts.grid, forward));
        for (t, &angle) in angles.iter().enumerate() {
            if parts.asked & 1 << t == 0 && angle <= cone {
                wanted.push((order >= own, angle, order, t));
            }
        }
    }
    wanted.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
    let store = &mut *store;
    while store.parts_out < PARTS_OUT
        && let Some(&(_, _, order, _)) = wanted.first()
    {
        let tiles: Vec<usize> = wanted.iter().filter(|w| w.2 == order).take(PART).map(|w| w.3).collect();
        wanted.retain(|w| w.2 != order || !tiles.contains(&w.3));
        let Some(parts) = store.parts.get_mut(&keys[order]) else { break };
        for &t in &tiles {
            parts.asked |= 1 << t;
        }
        let _ = fetcher.parts.send(Ask::Part { key: keys[order].clone(), files: parts.files.clone(), tiles });
        store.parts_out += 1;
    }
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

/// The environment light of the block the walker is in, once made; traced,
/// the dome round it shining the light that came in.
#[allow(clippy::too_many_arguments)]
fn lit(
    mut commands: Commands,
    fetcher: Res<Fetcher>,
    mut store: ResMut<Store>,
    mut images: ResMut<Assets<Image>>,
    cameras: Query<Entity, With<osage_render::OsageCamera>>,
    dome: Option<Res<Dome>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut places: Query<(&mut Transform, &mut Visibility)>,
) {
    for (block, diffuse, specular, domed) in fetcher.lights.1.try_iter() {
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
        if let (Some(dome), Some((centre, picture))) = (dome.as_ref(), domed) {
            if let Some(mut material) = standard.get_mut(&dome.material) {
                material.emissive_texture = Some(images.add(picture));
            }
            if let Ok((mut place, mut visibility)) = places.get_mut(dome.entity) {
                place.translation = centre;
                *visibility = Visibility::Inherited;
            }
        }
        store.lit = Some(block);
    }
}

#[cfg(test)]
mod tests {
    use bevy::mesh::VertexAttributeValues;

    use super::*;

    #[test]
    fn the_tiles_nearest_where_the_walker_looks_come_first() {
        // Looking north, level: the four tiles meeting at the picture's
        // middle are looked into, the one behind is farthest.
        let angles = tile_angles((16, 8), Vec3::NEG_Z);
        assert_eq!(angles.len(), 128);
        for t in [3 * 16 + 7, 3 * 16 + 8, 4 * 16 + 7, 4 * 16 + 8] {
            assert!(angles[t] < 1e-3, "tile {t}: {}", angles[t]);
        }
        assert!(angles[3 * 16] > 140f32.to_radians(), "behind: {}", angles[3 * 16].to_degrees());
        // Looking east and a little up: into the tile there.
        let east = tile_angles((16, 8), Vec3::new(1.0, 0.2, 0.0));
        let (column, row) = ((0.75f32 * 16.0) as usize, ((0.5 - 0.2f32.atan2(1.0) / PI) * 8.0) as usize);
        assert_eq!(east[row * 16 + column], 0.0);
        assert_eq!(probe_cube("c_8_0_-21_3"), Some(IVec3::new(8, 0, -21)));
    }

    #[test]
    fn the_dome_faces_in_and_is_mapped_as_a_probe() {
        let mesh = dome_mesh();
        let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("positions")
        };
        let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else {
            panic!("normals")
        };
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else { panic!("uvs") };
        for ((p, n), uv) in positions.iter().zip(normals).zip(uvs) {
            let d = Vec3::from(*p) / DOME_RADIUS;
            assert!((d.length() - 1.0).abs() < 1e-4);
            assert!((Vec3::from(*n) + d).length() < 1e-4, "facing in");
            // A probe's picture: u = ½ + atan2(x, -z) / 2π, v = ½ - asin(y) / π.
            let v = 0.5 - d.y.clamp(-1.0, 1.0).asin() / std::f32::consts::PI;
            assert!((uv[1] - v).abs() < 1e-4, "{uv:?} {d}");
            if d.y.abs() < 0.999 && uv[0] > 1e-4 && uv[0] < 1.0 - 1e-4 {
                let u = 0.5 + d.x.atan2(-d.z) / std::f32::consts::TAU;
                assert!((uv[0] - u).abs() < 1e-4, "{uv:?} {d}");
            }
        }
        // Wound to face the centre: each triangle's normal points in.
        let Some(Indices::U32(indices)) = mesh.indices() else { panic!("indices") };
        assert!(indices.len() / 3 < 65_536, "a light Solari samples");
        for t in indices.chunks(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(positions[i as usize]));
            let normal = (b - a).cross(c - a);
            if normal.length() > 1e-3 {
                assert!(normal.dot((a + b + c) / 3.0) < 0.0);
            }
        }
    }

    #[test]
    fn the_dome_picture_is_the_probe_s_light_in_half_floats() {
        for (v, bits) in [(0.0f32, 0u16), (1.0, 0x3c00), (0.5, 0x3800), (64.0, 0x5400), (1.0 / 3.0, 0x3555)] {
            assert_eq!(half(v), bits, "{v}");
        }
        let c = light::encode_radiance(2.0);
        let q = (c * 1023.0).round() as u32;
        let drawn = light::Drawn {
            width: 8,
            height: 4,
            radiance: vec![q | (q << 10) | (q << 20) | (3 << 30); 32],
            distance: vec![1.0; 32],
        };
        let picture = dome_picture(&drawn);
        assert_eq!((picture.width(), picture.height()), (8, 4));
        let data = picture.data.as_ref().expect("data");
        let red = u16::from_le_bytes([data[0], data[1]]);
        // The probe's light as it came, ten bits of its curve.
        assert_eq!(red, half(light::decode_radiance(q as f32 / 1023.0)), "{red:#x}");
    }
}
