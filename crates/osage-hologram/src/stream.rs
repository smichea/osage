// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Holograms whose views come from a server, block by block (D-134,
//! `osage-hologram-stream`).
//!
//! An entity with [`Streamed`] names a hologram server and the hologram's
//! folder there. Its `stream.json` comes first: the hologram is made from
//! it, its views still missing. Then, a few times a second, the viewer's
//! place in the hologram's own frame and its speed go to the server, which
//! answers the blocks of views the viewer can see the hologram from before
//! long; those not yet asked for are fetched and decoded by threads of
//! their own, and their views go to the hologram's window as they come
//! (where one has not come yet, the nearest that has stands in for it).

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    thread,
};

use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use crossbeam_channel::{Receiver, Sender};
use osage_hologram_stream::{Block, Packed, Series, decode, http::Client};

use crate::{Drawn, Grid, Hologram, Sources, material_of, stack, stand_in, window};

/// How often the server is told where the viewer is, seconds.
const NEED_PERIOD: f64 = 0.25;
/// Threads that fetch and decode blocks.
const WORKERS: usize = 4;

/// A hologram streamed from the hologram server at `server` (`host:port`),
/// its folder there `path`; `window`: how many columns and rows of its views
/// it keeps on the GPU.
#[derive(Component, Clone, Debug)]
pub struct Streamed {
    pub server: String,
    pub path: String,
    pub window: (usize, usize),
    /// The stand-in it is drawn on, as [`Hologram::stand`] and
    /// [`Hologram::stand_depth`]; none: a box over its whole photographed
    /// sphere. A camera inside the stand-in sees nothing of it: a chunk of
    /// woods beside a track stands on a box over its trees only.
    pub stand: Option<((Vec3, Vec2), f32)>,
}

/// What has come of a streamed hologram.
#[derive(Component, Debug)]
pub struct Stream {
    pub packed: Arc<Packed>,
    /// Bytes of blocks received, and views.
    pub bytes: u64,
    pub views: usize,
    asked: HashSet<Block>,
    eye: Option<(Vec3, f64)>,
    next_need: f64,
}

#[derive(Resource)]
struct Fetcher {
    requests: Sender<Request>,
    replies: Receiver<Reply>,
}

enum Request {
    Packed { entity: Entity, server: String, path: String },
    Need { entity: Entity, server: String, query: String },
    Block { entity: Entity, server: String, path: String, block: Block, packed: Arc<Packed> },
}

/// Decoded views of a block: row, column, series, picture.
type Views = Vec<(u16, u16, Series, decode::Picture)>;

enum Reply {
    Packed { entity: Entity, packed: Packed },
    Needed { entity: Entity, blocks: Vec<Block> },
    Views { entity: Entity, bytes: u64, views: Views },
    Failed { entity: Entity, what: String },
}

/// Streams [`Streamed`] holograms (with [`crate::HologramPlugin`]).
pub struct StreamPlugin;

impl Plugin for StreamPlugin {
    fn build(&self, app: &mut App) {
        let (requests, to_workers) = crossbeam_channel::unbounded::<Request>();
        let (from_workers, replies) = crossbeam_channel::unbounded::<Reply>();
        for i in 0..WORKERS {
            let (to_workers, from_workers) = (to_workers.clone(), from_workers.clone());
            thread::Builder::new()
                .name(format!("hologram-stream-{i}"))
                .spawn(move || work(to_workers, from_workers))
                .expect("a hologram stream thread");
        }
        app.insert_resource(Fetcher { requests, replies })
            .add_systems(Update, (ask_packed, receive, need, put_up).chain().before(crate::slide));
    }
}

/// A worker: one kept-alive connection a server; requests in, replies out.
fn work(requests: Receiver<Request>, replies: Sender<Reply>) {
    let mut clients: HashMap<String, Client> = HashMap::new();
    for request in requests {
        let reply = match request {
            Request::Packed { entity, server, path } => {
                let client = clients.entry(server.clone()).or_insert_with(|| Client::new(server));
                match client.get(&format!("/h/{path}/{}", Packed::FILE)) {
                    Ok(bytes) => match serde_json::from_slice::<Packed>(&bytes) {
                        Ok(packed) => Reply::Packed { entity, packed },
                        Err(error) => Reply::Failed { entity, what: format!("{path}: {error}") },
                    },
                    Err(error) => Reply::Failed { entity, what: format!("{path}: {error}") },
                }
            }
            Request::Need { entity, server, query } => {
                let client = clients.entry(server.clone()).or_insert_with(|| Client::new(server));
                match client.get(&format!("/need?{query}")).map(|b| serde_json::from_slice::<Vec<Block>>(&b)) {
                    Ok(Ok(blocks)) => Reply::Needed { entity, blocks },
                    Ok(Err(error)) => Reply::Failed { entity, what: format!("need: {error}") },
                    Err(error) => Reply::Failed { entity, what: format!("need: {error}") },
                }
            }
            Request::Block { entity, server, path, block, packed } => {
                let client = clients.entry(server.clone()).or_insert_with(|| Client::new(server));
                match fetch(client, &path, block, &packed) {
                    Ok((bytes, views)) => Reply::Views { entity, bytes, views },
                    Err(error) => Reply::Failed { entity, what: format!("{path} {block:?}: {error}") },
                }
            }
        };
        if replies.send(reply).is_err() {
            return;
        }
    }
}

/// A block's views of every series the hologram is drawn from, decoded.
fn fetch(client: &mut Client, path: &str, block: Block, packed: &Packed) -> anyhow::Result<(u64, Views)> {
    let grid = packed.grid();
    let order = packed.tiling.views(block, grid.elevations.len() as u16, grid.azimuths.len() as u16);
    let mut get = |series: Series| client.get(&format!("/h/{path}/{}", block.file(series)));
    let colour = if packed.lit() { Series::Albedo } else { Series::View };
    let mut series = vec![colour];
    if packed.lit() {
        series.push(Series::Normal);
    }
    let mut bytes = 0;
    let mut views = Vec::new();
    for kind in series {
        let file = get(kind)?;
        bytes += file.len() as u64;
        for (&(row, column), picture) in order.iter().zip(decode::block(&file, kind)?) {
            views.push((row, column, kind, picture));
        }
    }
    let (depth, mask) = (get(Series::Depth)?, get(Series::Mask)?);
    bytes += (depth.len() + mask.len()) as u64;
    for (&(row, column), picture) in order.iter().zip(decode::depths(&depth, &mask)?) {
        views.push((row, column, Series::Depth, picture));
    }
    Ok((bytes, views))
}

fn ask_packed(fetcher: Res<Fetcher>, new: Query<(Entity, &Streamed), Added<Streamed>>) {
    for (entity, streamed) in &new {
        let _ = fetcher.requests.send(Request::Packed {
            entity,
            server: streamed.server.clone(),
            path: streamed.path.clone(),
        });
    }
}

/// The hologram a packed one makes, its views to come.
fn hologram_of(packed: &Packed, streamed: &Streamed) -> Hologram {
    let centre = Vec3::from(packed.centre);
    let grid = packed.grid();
    Hologram {
        folder: streamed.path.clone(),
        grid: Grid { azimuths: grid.azimuths, elevations: grid.elevations },
        centre,
        radius: packed.radius,
        // A box over the photographed sphere, as wide and deep as it.
        stand: streamed.stand.map_or((centre + Vec3::Z * packed.radius, Vec2::splat(packed.radius)), |s| s.0),
        flat: false,
        lit: packed.lit(),
        fade_in: None,
        aspect: packed.aspect,
        masked: true,
        stand_depth: streamed.stand.map_or(packed.radius * 2.0, |s| s.1),
        window: Some(streamed.window),
    }
}

fn receive(
    mut commands: Commands,
    fetcher: Res<Fetcher>,
    mut images: ResMut<Assets<Image>>,
    streamed: Query<&Streamed>,
    mut streams: Query<(&mut Stream, &mut Sources)>,
) {
    for reply in fetcher.replies.try_iter() {
        match reply {
            Reply::Packed { entity, packed } => {
                let Ok(streamed) = streamed.get(entity) else { continue };
                let hologram = hologram_of(&packed, streamed);
                let n = hologram.grid.len();
                info!(path = streamed.path, views = n, blocks = packed.bytes.len(), "osage-hologram: stream opened");
                commands.entity(entity).insert((
                    hologram,
                    Sources {
                        views: vec![None; n],
                        depths: vec![None; n],
                        normals: vec![None; n],
                        at: None,
                        size: (0, 0),
                        stale: true,
                        made: 0.0,
                        reduced: Default::default(),
                        filling: None,
                        layers: Vec::new(),
                        factor: 1,
                    },
                    Stream {
                        packed: Arc::new(packed),
                        bytes: 0,
                        views: 0,
                        asked: HashSet::new(),
                        eye: None,
                        next_need: 0.0,
                    },
                ));
            }
            Reply::Needed { entity, blocks } => {
                let (Ok(streamed), Ok((mut stream, _))) = (streamed.get(entity), streams.get_mut(entity)) else {
                    continue;
                };
                for block in blocks {
                    if stream.asked.insert(block) {
                        let _ = fetcher.requests.send(Request::Block {
                            entity,
                            server: streamed.server.clone(),
                            path: streamed.path.clone(),
                            block,
                            packed: stream.packed.clone(),
                        });
                    }
                }
            }
            Reply::Views { entity, bytes, views } => {
                let Ok((mut stream, mut sources)) = streams.get_mut(entity) else { continue };
                let columns = stream.packed.grid().azimuths.len();
                let lit = stream.packed.lit();
                stream.bytes += bytes;
                for (row, column, kind, picture) in views {
                    let format = match kind {
                        Series::View | Series::Albedo => TextureFormat::Rgba8UnormSrgb,
                        _ => TextureFormat::Rgba8Unorm,
                    };
                    let image = Image::new(
                        Extent3d { width: picture.width, height: picture.height, depth_or_array_layers: 1 },
                        TextureDimension::D2,
                        picture.rgba,
                        format,
                        RenderAssetUsages::MAIN_WORLD,
                    );
                    let handle = Some(images.add(image));
                    let i = row as usize * columns + column as usize;
                    match kind {
                        Series::View | Series::Albedo => {
                            // Unlit, the normals are never read: the colour stands in.
                            if !lit {
                                sources.normals[i] = handle.clone();
                            }
                            sources.views[i] = handle;
                            stream.views += 1;
                        }
                        Series::Normal => sources.normals[i] = handle,
                        Series::Depth | Series::Mask => sources.depths[i] = handle,
                    }
                }
                sources.stale = true;
            }
            Reply::Failed { entity, what } => warn!(?entity, what, "osage-hologram: streaming"),
        }
    }
}

/// Tells the server where the viewer is, in each streamed hologram's frame,
/// and how fast it goes.
fn need(
    time: Res<Time>,
    fetcher: Res<Fetcher>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut streams: Query<(Entity, &Streamed, &GlobalTransform, &mut Stream)>,
) {
    let Some((_, eye)) = cameras.iter().filter(|(c, _)| c.is_active).max_by_key(|(c, _)| c.order) else { return };
    let now = time.elapsed_secs_f64();
    for (entity, streamed, at, mut stream) in &mut streams {
        if now < stream.next_need {
            continue;
        }
        let local = at.affine().inverse().transform_point3(eye.translation());
        let speed = match stream.eye {
            Some((before, then)) if now > then => (local - before).length() / (now - then) as f32,
            _ => 0.0,
        };
        stream.eye = Some((local, now));
        stream.next_need = now + NEED_PERIOD;
        let query =
            format!("path={}&x={:.2}&y={:.2}&z={:.2}&speed={:.2}", streamed.path, local.x, local.y, local.z, speed);
        let _ = fetcher.requests.send(Request::Need { entity, server: streamed.server.clone(), query });
    }
}

/// Puts a streamed hologram up once a view of each kind it needs has come:
/// its first window from the nearest views; the window then slides as for
/// any hologram.
#[allow(clippy::type_complexity)]
fn put_up(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<crate::HologramMaterial>>,
    mut waiting: Query<(Entity, &Hologram, &mut Sources), (With<Stream>, Without<Drawn>)>,
) {
    for (entity, hologram, mut sources) in &mut waiting {
        let Some((width, height)) = hologram.window else { continue };
        let (columns, rows) = (hologram.grid.azimuths.len(), hologram.grid.elevations.len());
        let (width, height) = (width.min(columns).max(1), height.min(rows).max(1));
        let pick = |set: &Vec<Option<Handle<Image>>>| window(set, columns, (0, 0), (width, height));
        let (Some(views), Some(depths), Some(normals)) =
            (pick(&sources.views), pick(&sources.depths), pick(&sources.normals))
        else {
            continue;
        };
        let made = (
            stack(views.iter().filter_map(|v| images.get(v))),
            stack(depths.iter().filter_map(|v| images.get(v))),
            stack(normals.iter().filter_map(|v| images.get(v))),
        );
        let (Some(colour), Some(depth), Some(normal)) = made else { continue };
        let material =
            materials.add(material_of(hologram, (images.add(colour), images.add(depth), images.add(normal))));
        // The window slides to where it is seen from at once.
        sources.at = None;
        sources.stale = true;
        let (quad, centre) = stand_in(hologram, &mut meshes);
        commands.entity(entity).insert(Drawn(material.clone())).with_child((
            Mesh3d(quad),
            MeshMaterial3d(material),
            Transform::from_translation(centre),
            bevy::light::NotShadowCaster,
        ));
        info!(path = hologram.folder, "osage-hologram: streamed hologram drawn");
    }
}
