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

use std::{
    collections::{HashMap, HashSet},
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
            .add_systems(Update, (ask, receive, arrays, lit).chain());
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
}

/// What the render world writes a frame, bytes: a probe's two layers
/// (64 MiB at 4096 × 2048) written at once held a frame 15 to 90 ms on an
/// Iris Xe, several at once up to 0.66 s.
const WRITTEN_A_FRAME: u64 = 16 << 20;

/// Writes, in bands of rows, the layers whose arrays are on the GPU, so
/// much a frame; the rest waits for the next.
fn write_layers(layers: Res<Layers>, images: Res<RenderAssets<GpuImage>>, queue: Res<RenderQueue>) {
    let Ok(mut uploads) = layers.0.lock() else { return };
    let Uploads { pending, written } = &mut *uploads;
    let mut spent = 0u64;
    pending.retain_mut(|layer| {
        if spent >= WRITTEN_A_FRAME {
            return true;
        }
        let Some(gpu) = images.get(layer.image) else { return true };
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
                origin: Origin3d { x: 0, y: layer.row, z: layer.layer },
                aspect: TextureAspect::All,
            },
            bytes,
            TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * width), rows_per_image: Some(rows) },
            Extent3d { width, height: rows, depth_or_array_layers: 1 },
        );
        spent += rows as u64 * row_bytes;
        layer.row += rows;
        if layer.row < height {
            return true;
        }
        *written.entry(layer.image).or_default() |= 1 << layer.layer;
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
    arrays: Option<(IVec3, Handle<Image>, Handle<Image>)>,
    sent: u8,
    written: u8,
    shown: u8,
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

/// A face's light for traced rays (T4 only), where the block is lit by its
/// surface tiles: the mean of what its tiles saw coming in, as a lamp the
/// size of the face, the coarse light that lights the block (D-136); with
/// its cube's probes the dome lights it instead. Remade for each block.
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

fn work(first: Receiver<Ask>, rest: Receiver<Ask>, gots: Sender<Got>) {
    give_way();
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
    // The probes written whole into the arrays since: drawn with them.
    if let (Some((_, colours, distances)), Ok(uploads)) = (store.arrays.clone(), layers.0.lock()) {
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
    // into its layer by the render world (`Layers`), a band a frame.
    if let Some((_, _, first)) = decoded.first() {
        if !matches!(&store.arrays, Some((cube, ..)) if *cube == block.cube) {
            let size = (first.width, first.height);
            store.arrays = Some((
                block.cube,
                images.add(layered(size, tiles::PROBES as u32, TextureFormat::Rgb10a2Unorm)),
                images.add(layered(size, tiles::PROBES as u32, TextureFormat::R32Float)),
            ));
            (store.sent, store.written) = (0, 0);
        }
        if let (Some((_, colours, distances)), Ok(mut uploads)) = (store.arrays.clone(), layers.0.lock()) {
            // What was on its way to arrays no longer filled goes.
            let mine = |image: &AssetId<Image>| *image == colours.id() || *image == distances.id();
            uploads.pending.retain(|l| mine(&l.image));
            uploads.written.retain(|image, _| mine(image));
            let sent = store.sent;
            for (k, _, drawn) in decoded.iter().filter(|(k, _, _)| sent & 1 << k == 0) {
                for (image, distance) in [(colours.id(), false), (distances.id(), true)] {
                    uploads.pending.push(Layer { image, layer: *k as u32, drawn: drawn.clone(), distance, row: 0 });
                }
                store.sent |= 1 << k;
            }
        }
    }
    // The probes the faces can show: written whole, the centre's first.
    let probes: Vec<_> = if store.written & 1 != 0 {
        decoded.iter().filter(|(k, _, _)| store.written & 1 << k != 0).cloned().collect()
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
        let shown = probes.iter().fold(0u8, |bits, (k, _, _)| bits | 1 << k);
        if store.block == Some(block) && same && shown == store.shown {
            return;
        }
        let relit = !same;
        store.block = Some(block);
        store.drawn = probes.len();
        store.cubed = true;
        store.probes_of = Some(block.cube);
        if let (true, Some((_, centre, drawn))) = (relit, probes.first().cloned()) {
            let (done, domed) = (fetcher.lights.0.clone(), dome.is_some());
            let _ = thread::Builder::new().name("paris-environment".to_owned()).spawn(move || {
                let (diffuse, specular) = environment_seen(&drawn);
                let dome = domed.then(|| (centre, dome_picture(&drawn)));
                let _ = done.send((block, diffuse, specular, dome));
            });
        }
        let size = probes.first().map_or((1, 1), |(_, _, d)| (d.width, d.height));
        let Some((_, colours, distances)) = store.arrays.clone() else { return };
        store.shown = shown;
        debug!(cube = ?block.cube, probes = probes.len(), ms = started.elapsed().as_micros() as f32 / 1000.0, "paris: the block's arrays told");
        // The probes that have come, the centre's first, each with its layer.
        let mut at = [Vec4::new(0.0, 0.0, 0.0, -1.0); 5];
        for (slot, (k, centre, _)) in probes.iter().enumerate().take(5) {
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

/// The cube's first `count` probes that have come, each with where it
/// stands, its centre's first; none until its centre's has.
fn cube_probes(store: &Store, count: usize, cube: IVec3) -> Vec<(usize, Vec3, Arc<light::Drawn>)> {
    let keys = tiles::probe_keys(cube);
    if !store.centres.contains_key(&keys[0]) || !store.tiles.contains_key(&keys[0]) {
        return Vec::new();
    }
    keys.into_iter()
        .take(count)
        .enumerate()
        .filter_map(|(k, key)| Some((k, *store.centres.get(&key)?, store.tiles.get(&key)?.clone())))
        .collect()
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
