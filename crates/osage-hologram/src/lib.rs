// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Holographic textures (issue #7): an object textured not by one image
//! but by how it looks from every viewing angle — a field of views —
//! compressed as a video is, with the step from one view to the next in
//! viewing angle where a video steps in time.
//!
//! This crate holds, for now, the first experiment's parts: the grid of
//! viewing directions an object is photographed from ([`Grid`]), and the
//! order the views are put in to make a "video" ([`Grid::path`]): along a
//! row of azimuths, back along the next, so that each view follows one a
//! step of angle away from it, as each frame of a film follows one a step
//! of time away.

pub mod stream;

use std::f32::consts::PI;

use bevy::{
    asset::embedded_asset,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

/// The viewing directions an object is photographed from: azimuths and
/// elevations in degrees, the object's front facing +Z.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    pub azimuths: Vec<f32>,
    pub elevations: Vec<f32>,
}

impl Grid {
    /// A grid from `from` to `to` degrees by `step`, in azimuth and in
    /// elevation.
    pub fn new(azimuth: (f32, f32, f32), elevation: (f32, f32, f32)) -> Self {
        let range = |(from, to, step): (f32, f32, f32)| {
            let n = ((to - from) / step).round() as usize;
            (0..=n).map(|i| from + i as f32 * step).collect::<Vec<_>>()
        };
        Self { azimuths: range(azimuth), elevations: range(elevation) }
    }

    /// A building's front: from 70° to either side, from 10° below the
    /// horizontal to 50° above, by 10° and 7.5°.
    pub fn facade() -> Self {
        Self::new((-70.0, 70.0, 10.0), (-10.0, 50.0, 7.5))
    }

    pub fn len(&self) -> usize {
        self.azimuths.len() * self.elevations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The unit direction from the object towards the camera, for an
    /// azimuth and an elevation in degrees: azimuth 0 and elevation 0 look
    /// at the object's front (+Z) square on.
    pub fn direction(azimuth: f32, elevation: f32) -> [f32; 3] {
        let (a, e) = (azimuth * PI / 180.0, elevation * PI / 180.0);
        [a.sin() * e.cos(), e.sin(), a.cos() * e.cos()]
    }

    /// Every view as (row, column) of (elevation, azimuth), in the order of
    /// the "video": along each row of azimuths, back along the next.
    pub fn path(&self) -> Vec<(usize, usize)> {
        let columns = self.azimuths.len();
        (0..self.elevations.len())
            .flat_map(|row| {
                let along: Vec<usize> =
                    if row % 2 == 0 { (0..columns).collect() } else { (0..columns).rev().collect() };
                along.into_iter().map(move |column| (row, column))
            })
            .collect()
    }

    /// A view's file name: its row and column, so that any order can be
    /// made from the files.
    pub fn name(row: usize, column: usize) -> String {
        format!("view-r{row:02}-c{column:02}.png")
    }

    /// The file of a view's depth (see [`DepthMaterial`]).
    pub fn depth_name(row: usize, column: usize) -> String {
        format!("depth-r{row:02}-c{column:02}.png")
    }

    /// The file of a view's colour without light (its albedo).
    pub fn albedo_name(row: usize, column: usize) -> String {
        format!("albedo-r{row:02}-c{column:02}.png")
    }

    /// The file of a view's normals (see [`NormalMaterial`]).
    pub fn normal_name(row: usize, column: usize) -> String {
        format!("normal-r{row:02}-c{column:02}.png")
    }
}

/// What `hologram-bake` photographs, given as a file (`HOLOGRAM_SCENE`):
/// models at places, in the hologram's own frame (its front facing +Z),
/// and the grid of directions to photograph it from.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Scene {
    pub models: Vec<SceneModel>,
    /// First azimuth, last, step; first elevation, last, step (degrees).
    pub grid: [f32; 6],
    /// Where the hologram stands in its world: translation, rotation
    /// (a quaternion x, y, z, w) — for whoever draws it.
    pub at: [f32; 7],
}

/// A model of a [`Scene`]: a glTF under the manifest's content, whether
/// each place takes one of its variants, and its places (x, y, z, a
/// quaternion x, y, z, w, a scale).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct SceneModel {
    pub path: String,
    pub variants: bool,
    pub places: Vec<[f32; 8]>,
}

/// How `hologram-bake` framed its views (written beside them as
/// `hologram.json`): the photographed sphere's centre and radius, the
/// views' height to width, the grid, and where the hologram stands.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Framing {
    pub centre: [f32; 3],
    pub radius: f32,
    pub aspect: f32,
    pub grid: [f32; 6],
    pub at: [f32; 7],
}

impl Framing {
    /// The file beside the views.
    pub const FILE: &str = "hologram.json";

    /// The hologram these views make, lit, told by depth (views on black),
    /// on a box over the photographed sphere's lower part.
    pub fn hologram(&self, folder: &str) -> Hologram {
        let [a0, a1, a2, e0, e1, e2] = self.grid;
        let centre = Vec3::from(self.centre);
        let half = Vec2::new(self.radius, self.radius * self.aspect);
        Hologram {
            folder: folder.to_owned(),
            grid: Grid::new((a0, a1, a2), (e0, e1, e2)),
            centre,
            radius: self.radius,
            stand: (Vec3::new(centre.x, centre.y, centre.z + self.radius * 0.7), half),
            flat: false,
            lit: true,
            fade_in: None,
            aspect: self.aspect,
            masked: true,
            stand_depth: self.radius * 1.4,
            window: None,
        }
    }

    /// Where the hologram stands in its world.
    pub fn transform(&self) -> Transform {
        let [x, y, z, qx, qy, qz, qw] = self.at;
        Transform::from_xyz(x, y, z).with_rotation(Quat::from_xyzw(qx, qy, qz, qw))
    }
}

/// A forest plot's trees: `count` trees of `kinds` kinds scattered over a
/// plot `width` wide along x (centred on 0) and `depth` deep behind z 0,
/// each turned and sized by a hash seeded with `seed` — the same at every
/// call. The places of each kind.
pub fn forest(width: f32, depth: f32, count: usize, seed: u64, kinds: usize) -> Vec<Vec<Transform>> {
    let mut places = vec![Vec::new(); kinds.max(1)];
    let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    let mut next = || {
        state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) as f32 / (1u64 << 31) as f32
    };
    for _ in 0..count {
        let (x, z) = ((next() - 0.5) * width, -next() * depth);
        let kind = (next() * kinds.max(1) as f32) as usize % kinds.max(1);
        let place = Transform::from_xyz(x, 0.0, z)
            .with_rotation(Quat::from_rotation_y(next() * std::f32::consts::TAU))
            .with_scale(Vec3::splat(0.8 + 0.5 * next()));
        places[kind].push(place);
    }
    places
}

/// What a view's depth picture holds, drawn by `depth.wesl`: for each
/// point of the object, how far it stands towards the camera from the
/// photographed sphere's centre, from -radius to +radius, in 16 bits over
/// red and green; blue where the object is.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct DepthMaterial {
    /// The sphere's centre and radius.
    #[uniform(0)]
    pub frame: Vec4,
    /// The direction towards the camera.
    #[uniform(1)]
    pub back: Vec4,
    /// The model's colour texture, whose alpha cuts leaves out of their
    /// cards where `cut.x` (the cutoff) is above 0.
    #[texture(2)]
    #[sampler(3)]
    pub cut_texture: Option<Handle<Image>>,
    #[uniform(4)]
    pub cut: Vec4,
}

impl Material for DepthMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://osage_hologram/depth.wesl".into()
    }
}

/// What a view's normal picture holds, drawn by `normal.wesl`: the
/// surface's direction at each point, as colour (n × 0.5 + 0.5).
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct NormalMaterial {
    /// As [`DepthMaterial`]'s: leaves cut out of their cards.
    #[texture(2)]
    #[sampler(3)]
    pub cut_texture: Option<Handle<Image>>,
    #[uniform(4)]
    pub cut: Vec4,
}

impl Material for NormalMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://osage_hologram/normal.wesl".into()
    }
}

/// Registers [`DepthMaterial`] and [`NormalMaterial`], for baking.
pub struct DepthPlugin;

impl Plugin for DepthPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "depth.wesl");
        embedded_asset!(app, "normal.wesl");
        app.add_plugins((MaterialPlugin::<DepthMaterial>::default(), MaterialPlugin::<NormalMaterial>::default()));
    }
}

/// A point's depth from its depth picture's texel (red, green: 16 bits),
/// from -radius to +radius; none where the ground is (blue 0).
pub fn decode_depth(texel: [u8; 4], radius: f32) -> Option<f32> {
    (texel[2] > 127).then(|| ((texel[0] as f32 * 256.0 + texel[1] as f32) / 65535.0) * 2.0 * radius - radius)
}

/// Draws [`Hologram`]s.
pub struct HologramPlugin;

impl Plugin for HologramPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "hologram.wesl");
        app.add_plugins(MaterialPlugin::<HologramMaterial>::default())
            .init_resource::<Stacks>()
            .init_resource::<LayerWrites>()
            .add_systems(Update, (assemble, follow, slide, fill).chain());
        if let Some(render) = app.get_sub_app_mut(bevy::render::RenderApp) {
            render
                .init_resource::<LayerWrites>()
                .add_systems(bevy::render::ExtractSchedule, extract_layer_writes)
                .add_systems(bevy::render::Render, write_layers.in_set(bevy::render::RenderSystems::PrepareResources));
        }
    }
}

/// A hologram's material: unlit, its colour from the views.
pub type HologramMaterial = ExtendedMaterial<StandardMaterial, Views>;

/// The views, as a texture array, and where they were taken from.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct Views {
    /// First azimuth, azimuth step, first elevation, elevation step, degrees.
    #[uniform(100)]
    pub grid: Vec4,
    /// Columns, rows.
    #[uniform(101)]
    pub shape: Vec4,
    /// The photographed sphere's centre in the world, and its radius.
    #[uniform(102)]
    pub frame: Vec4,
    /// Cosine and sine of the object's turn about the vertical; the
    /// distances it fades in over.
    #[uniform(103)]
    pub turn: Vec4,
    #[texture(104, dimension = "2d_array")]
    #[sampler(105)]
    pub views: Option<Handle<Image>>,
    /// Each view's depth (see [`DepthMaterial`]), read without filtering.
    #[texture(106, dimension = "2d_array", sample_type = "float", filterable = false)]
    pub depths: Option<Handle<Image>>,
    /// Each view's normals (see [`NormalMaterial`]), for a lit hologram.
    #[texture(107, dimension = "2d_array")]
    pub normals: Option<Handle<Image>>,
    /// The views' height to width.
    #[uniform(108)]
    pub framing: Vec4,
    /// The window of views the arrays hold: first column, first row,
    /// columns, rows (columns 0: the whole grid).
    #[uniform(109)]
    pub window: Vec4,
}

impl MaterialExtension for Views {
    fn fragment_shader() -> ShaderRef {
        "embedded://osage_hologram/hologram.wesl".into()
    }
}

/// An object drawn from its views (a folder written by `hologram-bake`
/// with `HOLOGRAM_KEY=1`): placed by its transform (a turn about the
/// vertical only), on a rectangle standing at `stand` (in the object's
/// frame: its centre and half size, facing +Z).
#[derive(Component, Clone, Debug)]
#[require(Transform, Visibility)]
pub struct Hologram {
    /// The views' folder, under the asset manifest's content.
    pub folder: String,
    pub grid: Grid,
    /// The photographed sphere, in the object's frame: centre and radius.
    pub centre: Vec3,
    pub radius: f32,
    pub stand: (Vec3, Vec2),
    /// Drawn on the stand-in, without the views' depth (for comparison).
    pub flat: bool,
    /// Lit where it is drawn, by the world's lights, from the views of its
    /// colour without light and of its normals; else drawn with the light
    /// it was photographed in.
    pub lit: bool,
    /// Faded in from the first distance to the second, m, where its model
    /// gives way to it (the model's own range fading it out over the same);
    /// none: always whole.
    pub fade_in: Option<(f32, f32)>,
    /// The views' height to width (as baked with `HOLOGRAM_ASPECT`).
    pub aspect: f32,
    /// Its views baked on black (`HOLOGRAM_KEY=black`), where it is told by
    /// its depth pictures: for fine things (foliage) whose thousands of
    /// edges a magenta ground would tint.
    pub masked: bool,
    /// How deep the stand-in is behind its front, m: 0 a rectangle; more,
    /// a box, its front at `stand`'s centre (for an object rays can enter
    /// from above, such as a forest).
    pub stand_depth: f32,
    /// Only a window of this many columns and rows of views on the GPU,
    /// around the direction it is seen from, slid as the camera moves
    /// (issue #8); none: every view.
    pub window: Option<(usize, usize)>,
}

/// Each folder's views, depths and normals (lit or not), stacked once and
/// shared by every hologram drawn from it.
#[derive(Resource, Default)]
struct Stacks(std::collections::HashMap<(String, bool), Stack>);

/// A folder's stacked views, depths and normals.
pub(crate) type Stack = (Handle<Image>, Handle<Image>, Handle<Image>);

/// A hologram whose views, depths and normals are loading.
#[derive(Component)]
struct Loading(Vec<Handle<Image>>, Vec<Handle<Image>>, Vec<Handle<Image>>);

/// A hologram's material, once made.
#[derive(Component)]
pub(crate) struct Drawn(pub(crate) Handle<HologramMaterial>);

/// A windowed hologram's views, depths and normals on the CPU, by row and
/// column of its grid (none where a streamed view has not come yet), and
/// the window its arrays hold now (first column, first row) and its size.
/// A reduced view's key: its view, depth and normals, and by how much.
pub(crate) type Reduced = (AssetId<Image>, AssetId<Image>, AssetId<Image>, usize);

#[derive(Component)]
pub(crate) struct Sources {
    pub(crate) views: Vec<Option<Handle<Image>>>,
    pub(crate) depths: Vec<Option<Handle<Image>>>,
    pub(crate) normals: Vec<Option<Handle<Image>>>,
    pub(crate) at: Option<(usize, usize)>,
    pub(crate) size: (usize, usize),
    /// Views have come since the window was made: it is made again, at
    /// most once every [`RESTACK`] s (a wide window of large views is
    /// hundreds of megabytes; streamed blocks come many a second).
    pub(crate) stale: bool,
    /// When the window was last made, s.
    pub(crate) made: f64,
    /// Views reduced for a wide window, by their view, depth and normal
    /// and how much: made once each.
    pub(crate) reduced: std::collections::HashMap<Reduced, std::sync::Arc<Pictures>>,
    /// A new window being filled on the GPU.
    pub(crate) filling: Option<Filling>,
    /// The views, depths and normals each layer of the arrays holds, and
    /// by how much they are reduced.
    pub(crate) layers: Vec<[AssetId<Image>; 3]>,
    pub(crate) factor: usize,
}

#[allow(clippy::type_complexity)]
/// Loads each new hologram's views and, once all are there, stacks them
/// into one texture array and puts the stand-in up.
#[allow(clippy::too_many_arguments)]
fn assemble(
    mut commands: Commands,
    server: Res<AssetServer>,
    new: Query<(Entity, &Hologram), (Without<Loading>, Without<Drawn>, Without<stream::Stream>)>,
    loading: Query<(Entity, &Hologram, &Loading)>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<HologramMaterial>>,
    mut stacks: ResMut<Stacks>,
) {
    for (entity, hologram) in &new {
        let order = (0..hologram.grid.elevations.len())
            .flat_map(|row| (0..hologram.grid.azimuths.len()).map(move |column| (row, column)));
        let order: Vec<_> = order.collect();
        let views = order.iter().map(|&(row, column)| {
            let name = if hologram.lit { Grid::albedo_name(row, column) } else { Grid::name(row, column) };
            // Kept on the CPU only: they are stacked into one array, and the
            // stack alone goes to the GPU (each view uploaded on its own as
            // well doubled the GPU's memory for nothing).
            server
                .load_builder()
                .with_settings(|settings: &mut bevy::image::ImageLoaderSettings| {
                    settings.asset_usage = bevy::asset::RenderAssetUsages::MAIN_WORLD;
                })
                .load(format!("{}/{name}", hologram.folder))
        });
        // Depth is a number, not a colour: read as it is.
        let depths = order.iter().map(|&(row, column)| {
            server
                .load_builder()
                .with_settings(|settings: &mut bevy::image::ImageLoaderSettings| {
                    settings.is_srgb = false;
                    settings.asset_usage = bevy::asset::RenderAssetUsages::MAIN_WORLD;
                })
                .load(format!("{}/{}", hologram.folder, Grid::depth_name(row, column)))
        });
        let normals = order.iter().map(|&(row, column)| {
            server
                .load_builder()
                .with_settings(|settings: &mut bevy::image::ImageLoaderSettings| {
                    settings.is_srgb = false;
                    settings.asset_usage = bevy::asset::RenderAssetUsages::MAIN_WORLD;
                })
                .load(format!("{}/{}", hologram.folder, Grid::normal_name(row, column)))
        });
        commands.entity(entity).insert(Loading(views.collect(), depths.collect(), normals.collect()));
    }
    for (entity, hologram, Loading(views, depths, normals)) in &loading {
        if !views.iter().chain(depths).chain(normals).all(|view| server.is_loaded_with_dependencies(view)) {
            continue;
        }
        let key = (hologram.folder.clone(), hologram.lit);
        // Windowed: its own small arrays, made when it is first seen.
        let windowed = hologram.window.is_some();
        let stacked = match stacks.0.get(&key).filter(|_| !windowed) {
            Some(three) => Some(three.clone()),
            None => {
                let colour = stack(views.iter().filter_map(|view| images.get(view)));
                let depth = stack(depths.iter().filter_map(|depth| images.get(depth)));
                let normal = stack(normals.iter().filter_map(|normal| images.get(normal)));
                match (colour, depth, normal) {
                    (Some(colour), Some(depth), Some(normal)) if !windowed => {
                        let three = (images.add(colour), images.add(depth), images.add(normal));
                        stacks.0.insert(key, three.clone());
                        Some(three)
                    }
                    // Windowed: its first window, the grid's first columns
                    // and rows, until it slides to where it is seen from.
                    (Some(_), Some(_), Some(_)) => {
                        let (columns, rows) = (hologram.grid.azimuths.len(), hologram.grid.elevations.len());
                        let (width, height) = hologram.window.unwrap_or((columns, rows));
                        let (width, height) = (width.min(columns).max(1), height.min(rows).max(1));
                        let first = |set: &Vec<Handle<Image>>| {
                            let set: Vec<_> = set.iter().cloned().map(Some).collect();
                            window(&set, columns, (0, 0), (width, height))
                                .unwrap_or_default()
                                .into_iter()
                                .filter_map(|h| images.get(&h).cloned())
                                .collect::<Vec<_>>()
                        };
                        let (v, d, n) = (first(views), first(depths), first(normals));
                        match (stack(v.iter()), stack(d.iter()), stack(n.iter())) {
                            (Some(colour), Some(depth), Some(normal)) => {
                                Some((images.add(colour), images.add(depth), images.add(normal)))
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }
        };
        let Some((stack, depth_stack, normal_stack)) = stacked else {
            warn!(folder = hologram.folder, "osage-hologram: the views are not all of one size and kind");
            commands.entity(entity).remove::<Loading>().insert(Drawn(Handle::default()));
            continue;
        };
        let material = materials.add(material_of(hologram, (stack, depth_stack, normal_stack)));
        if windowed {
            let some = |set: &Vec<Handle<Image>>| set.iter().cloned().map(Some).collect();
            commands.entity(entity).insert(Sources {
                views: some(views),
                depths: some(depths),
                normals: some(normals),
                at: Some((0, 0)),
                size: hologram.window.unwrap_or_default(),
                stale: false,
                made: 0.0,
                reduced: Default::default(),
                filling: None,
                layers: Vec::new(),
                factor: 1,
            });
        }
        let (quad, centre) = stand_in(hologram, &mut meshes);
        commands.entity(entity).remove::<Loading>().insert(Drawn(material.clone())).with_child((
            Mesh3d(quad),
            MeshMaterial3d(material),
            Transform::from_translation(centre),
            bevy::light::NotShadowCaster,
        ));
        info!(folder = hologram.folder, views = views.len(), "osage-hologram: drawn");
    }
}

/// A hologram's material, drawing from these stacks of views, depths and
/// normals (its whole grid, or its first window).
pub(crate) fn material_of(hologram: &Hologram, (views, depths, normals): Stack) -> HologramMaterial {
    let grid = &hologram.grid;
    let step = |v: &[f32]| if v.len() > 1 { v[1] - v[0] } else { 1.0 };
    ExtendedMaterial {
        base: StandardMaterial {
            unlit: !hologram.lit,
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.85,
            ..default()
        },
        extension: Views {
            grid: Vec4::new(grid.azimuths[0], step(&grid.azimuths), grid.elevations[0], step(&grid.elevations)),
            shape: Vec4::new(
                grid.azimuths.len() as f32,
                grid.elevations.len() as f32,
                if hologram.flat { 0.0 } else { 1.0 },
                if hologram.lit { 1.0 } else { 0.0 },
            ),
            frame: Vec4::ZERO,
            turn: Vec4::new(1.0, 0.0, 0.0, 0.0),
            views: Some(views),
            depths: Some(depths),
            normals: Some(normals),
            framing: Vec4::new(hologram.aspect, if hologram.masked { 1.0 } else { 0.0 }, 0.0, 0.0),
            window: match hologram.window {
                Some((width, height)) => Vec4::new(
                    0.0,
                    0.0,
                    width.min(grid.azimuths.len()).max(1) as f32,
                    height.min(grid.elevations.len()).max(1) as f32,
                ),
                None => Vec4::ZERO,
            },
        },
    }
}

/// The shape a hologram is drawn on, and where its centre stands.
pub(crate) fn stand_in(hologram: &Hologram, meshes: &mut Assets<Mesh>) -> (Handle<Mesh>, Vec3) {
    let (centre, half) = hologram.stand;
    if hologram.stand_depth > 0.0 {
        let box_centre = centre - Vec3::Z * hologram.stand_depth / 2.0;
        (meshes.add(Cuboid::new(half.x * 2.0, half.y * 2.0, hologram.stand_depth)), box_centre)
    } else {
        (meshes.add(Rectangle::new(half.x * 2.0, half.y * 2.0)), centre)
    }
}

/// The views stacked into one texture array, if all are the same size and
/// kind of 8-bit RGBA.
pub(crate) fn stack<'a>(views: impl Iterator<Item = &'a Image>) -> Option<Image> {
    let views: Vec<&Image> = views.collect();
    let mut data = Vec::with_capacity(views.iter().map(|v| v.data.as_ref().map_or(0, Vec::len)).sum());
    let mut first: Option<(u32, u32, bevy::render::render_resource::TextureFormat)> = None;
    let mut layers = 0;
    for view in views {
        let kind = (view.width(), view.height(), view.texture_descriptor.format);
        if first.is_some_and(|f| f != kind) || view.data.is_none() {
            return None;
        }
        first = Some(kind);
        data.extend_from_slice(view.data.as_ref()?);
        layers += 1;
    }
    let (width, height, format) = first?;
    let mut image = Image::new(
        bevy::render::render_resource::Extent3d { width, height: height * layers, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        data,
        format,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    image.reinterpret_stacked_2d_as_array(layers).ok()?;
    Some(image)
}

/// The most a window's three arrays may weigh, bytes: a wider one has its
/// views halved, or quartered, as it is stacked.
const WINDOW_BUDGET: usize = 768 << 20;

/// Layers of a window's arrays to write on the GPU, in place.
#[derive(Resource, Default)]
pub(crate) struct LayerWrites(Vec<LayerWrite>);

pub(crate) struct LayerWrite {
    array: AssetId<Image>,
    layer: u32,
    width: u32,
    height: u32,
    data: std::sync::Arc<Vec<u8>>,
    /// Frames it has waited for its array to reach the GPU.
    tries: u8,
    /// Writes of a window being filled still to reach the GPU.
    left: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
}

/// The layer writes of the frame, taken from the main world.
fn extract_layer_writes(mut main: ResMut<bevy::render::MainWorld>, mut pending: ResMut<LayerWrites>) {
    if let Some(mut writes) = main.get_resource_mut::<LayerWrites>() {
        pending.0.append(&mut writes.0);
    }
}

/// Writes each layer into its array on the GPU; one whose array has not
/// reached it yet waits a few frames.
fn write_layers(
    mut pending: ResMut<LayerWrites>,
    gpu_images: Res<bevy::render::render_asset::RenderAssets<bevy::render::texture::GpuImage>>,
    queue: Res<bevy::render::renderer::RenderQueue>,
) {
    use bevy::render::render_resource::{
        Extent3d, Origin3d, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect,
    };
    // Done with, written or not: a window being filled is told.
    let done = |write: &LayerWrite| {
        if let Some(left) = &write.left {
            left.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
        }
        false
    };
    pending.0.retain_mut(|write| {
        let Some(gpu) = gpu_images.get(write.array) else {
            write.tries += 1;
            return write.tries < 8 || done(write);
        };
        if write.layer >= gpu.texture.depth_or_array_layers()
            || (gpu.texture.width(), gpu.texture.height()) != (write.width, write.height)
        {
            return done(write);
        }
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &gpu.texture,
                mip_level: 0,
                origin: Origin3d { x: 0, y: 0, z: write.layer },
                aspect: TextureAspect::All,
            },
            &write.data,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * write.width),
                rows_per_image: Some(write.height),
            },
            Extent3d { width: write.width, height: write.height, depth_or_array_layers: 1 },
        );
        done(write)
    });
}

/// By how much a window's views are reduced, 1, 2 or 4, for its three
/// arrays of `weight` bytes to come within `budget`.
pub(crate) fn reduction(weight: usize, budget: usize) -> usize {
    [1usize, 2, 4].into_iter().find(|f| weight / (f * f) <= budget).unwrap_or(4)
}

/// A view, its depth and its normals reduced by `factor`: a reduced texel
/// is the one of its square nearest the camera, in all three, so that no
/// colour of the ground bleeds into an edge and no depth is an average of
/// a leaf and what is behind it. None unless all three are 8-bit RGBA of
/// one size.
pub(crate) fn reduce(factor: usize, view: &Image, depth: &Image, normal: &Image) -> Option<[Image; 3]> {
    let (w, h) = (depth.width() as usize, depth.height() as usize);
    let four = |i: &Image| i.texture_descriptor.format.block_copy_size(None) == Some(4);
    if [view, normal].iter().any(|i| (i.width() as usize, i.height() as usize) != (w, h))
        || ![view, depth, normal].into_iter().all(four)
    {
        return None;
    }
    let (dv, dd, dn) = (view.data.as_ref()?, depth.data.as_ref()?, normal.data.as_ref()?);
    let (sw, sh) = (w / factor, h / factor);
    let (mut v, mut d, mut n) = (vec![0u8; sw * sh * 4], vec![0u8; sw * sh * 4], vec![0u8; sw * sh * 4]);
    for y in 0..sh {
        for x in 0..sw {
            // The texel of the square nearest the camera, or its first.
            let mut best = ((y * factor) * w + x * factor) * 4;
            let mut nearest = -1i32;
            for dy in 0..factor {
                let row = ((y * factor + dy) * w + x * factor) * 4;
                for dx in 0..factor {
                    let i = row + dx * 4;
                    let near = if dd[i + 2] > 127 { dd[i] as i32 * 256 + dd[i + 1] as i32 } else { -1 };
                    if near > nearest {
                        (nearest, best) = (near, i);
                    }
                }
            }
            let o = (y * sw + x) * 4;
            v[o..o + 4].copy_from_slice(&dv[best..best + 4]);
            d[o..o + 4].copy_from_slice(&dd[best..best + 4]);
            n[o..o + 4].copy_from_slice(&dn[best..best + 4]);
        }
    }
    let small = |data: Vec<u8>, like: &Image| {
        Image::new(
            bevy::render::render_resource::Extent3d { width: sw as u32, height: sh as u32, depth_or_array_layers: 1 },
            bevy::render::render_resource::TextureDimension::D2,
            data,
            like.texture_descriptor.format,
            bevy::asset::RenderAssetUsages::MAIN_WORLD,
        )
    };
    Some([small(v, view), small(d, depth), small(n, normal)])
}

/// How often a window is made again for views that have come, at most, s.
const RESTACK: f64 = 1.0;

/// Slides each windowed hologram's window of views to the direction the
/// camera sees it from, when the views that direction needs leave it: its
/// arrays made again from the views on the CPU.
fn slide(
    time: Res<Time>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut holograms: Query<(&Hologram, &GlobalTransform, &Drawn, &mut Sources)>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<HologramMaterial>>,
    mut writes: ResMut<LayerWrites>,
) {
    // The eye: the active 3D camera drawn last.
    let Some((_, eye)) = cameras.iter().filter(|(c, _)| c.is_active).max_by_key(|(c, _)| c.order) else { return };
    let eye = eye.translation();
    for (hologram, at, Drawn(material), mut sources) in &mut holograms {
        let Some((width, height)) = hologram.window else { continue };
        // A new window still being filled: it is drawn first.
        if sources.filling.is_some() {
            continue;
        }
        let grid = &hologram.grid;
        let (columns, rows) = (grid.azimuths.len(), grid.elevations.len());
        let (width, height) = (width.min(columns).max(1), height.min(rows).max(1));
        // The direction it is seen from, in its own frame, and where that
        // falls in the grid.
        let centre = at.transform_point(hologram.centre);
        let local = at.affine().inverse().transform_vector3(eye - centre).normalize_or(Vec3::Z);
        let (azimuth, elevation) = (local.x.atan2(local.z).to_degrees(), local.y.clamp(-1.0, 1.0).asin().to_degrees());
        let place = |angle: f32, axis: &[f32], n: usize| {
            let step = if axis.len() > 1 { axis[1] - axis[0] } else { 1.0 };
            ((angle - axis[0]) / step).clamp(0.0, (n - 1) as f32)
        };
        let (fa, fe) = (place(azimuth, &grid.azimuths, columns), place(elevation, &grid.elevations, rows));
        // Seen near, the hologram spans more directions than its window
        // (a chunk of woods by the track): the window widens to the whole
        // grid when the angle its sphere subtends needs more than its own.
        let distance = (eye - centre).length().max(1e-3);
        let scale = at.affine().matrix3.x_axis.length();
        let half_span = |half: f32, axis: &[f32]| {
            let step = if axis.len() > 1 { axis[1] - axis[0] } else { 1.0 };
            (half * scale / distance).min(1.0).asin().to_degrees() / step
        };
        let (sa, se) = (
            half_span(hologram.radius, &grid.azimuths),
            half_span(hologram.radius * hologram.aspect, &grid.elevations),
        );
        // Two sizes only, the window's own and the whole grid's: a window
        // resized is made again whole, so as the eye comes near it goes to
        // the whole grid at once, and back only once well away.
        let size = |half: f32, least: usize, now: usize, n: usize| {
            let need = ((2.0 * half).ceil() as usize + 2).min(n);
            let least = least.min(n);
            if need > least || (now == n && need * 2 > least) { n } else { least }
        };
        let (width, height) = (size(sa, width, sources.size.0, columns), size(se, height, sources.size.1, rows));
        let needed = |f: f32, half: f32, n: usize| {
            ((f - half).floor().max(0.0) as usize, ((f + half).floor() as usize + 1).min(n - 1))
        };
        let ((c0, c1), (r0, r1)) = (needed(fa, sa, columns), needed(fe, se, rows));
        let inside = sources.at.is_some_and(|(x, y)| c0 >= x && c1 < x + width && r0 >= y && r1 < y + height);
        let now = time.elapsed_secs_f64();
        if inside && sources.size == (width, height) && (!sources.stale || now - sources.made < RESTACK) {
            continue;
        }
        let first =
            |f: f32, size: usize, n: usize| ((f.floor() as i64 - (size as i64 - 2) / 2).max(0) as usize).min(n - size);
        // Views have come into a window still in place: only its layers
        // whose views changed are written, on the GPU, not the arrays made
        // again (hundreds of megabytes sent at once).
        // A window of the same size slid: its layers wrap round it, so
        // only those of the views it gained are written too.
        let refresh = sources.size == (width, height) && sources.layers.len() == width * height;
        let (x, y) = match sources.at {
            Some(at) if inside && refresh => at,
            _ => (first(fa, width, columns), first(fe, height, rows)),
        };
        let pick = |set: &Vec<Option<Handle<Image>>>| window(set, columns, (x, y), (width, height));
        let (Some(views), Some(depths), Some(normals)) =
            (pick(&sources.views), pick(&sources.depths), pick(&sources.normals))
        else {
            continue;
        };
        let whole = |set: &[Handle<Image>]| -> Option<Vec<&Image>> {
            let got: Vec<&Image> = set.iter().filter_map(|v| images.get(v)).collect();
            (got.len() == width * height).then_some(got)
        };
        let (Some(v), Some(d), Some(n)) = (whole(&views), whole(&depths), whole(&normals)) else { continue };
        let weight: usize =
            [&v, &d, &n].into_iter().flat_map(|set| set.iter()).map(|i| i.data.as_ref().map_or(0, Vec::len)).sum();
        let factor = reduction(weight, WINDOW_BUDGET);
        // The layers' pictures: the views themselves, or reduced; each
        // taken once, a few a frame, one a thread, and kept to be written
        // as often as needed without being copied again.
        let key = |i: usize| (views[i].id(), depths[i].id(), normals[i].id(), factor);
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).min(4);
        let missing: Vec<usize> = (0..views.len()).filter(|&i| !sources.reduced.contains_key(&key(i))).collect();
        let more = missing.len() > threads;
        let missing = &missing[..missing.len().min(threads)];
        let done: Vec<(usize, Option<Pictures>)> = std::thread::scope(|scope| {
            let parts: Vec<_> = missing
                .iter()
                .map(|&i| {
                    let (v, d, n) = (&v, &d, &n);
                    scope.spawn(move || (i, Pictures::of(factor, v[i], d[i], n[i])))
                })
                .collect();
            parts.into_iter().filter_map(|p| p.join().ok()).collect()
        });
        for (i, pictures) in done {
            if let Some(pictures) = pictures {
                sources.reduced.insert(key(i), std::sync::Arc::new(pictures));
            }
        }
        if more {
            continue;
        }
        let layers: Vec<[AssetId<Image>; 3]> =
            (0..views.len()).map(|i| [views[i].id(), depths[i].id(), normals[i].id()]).collect();
        let pictures: Option<Vec<std::sync::Arc<Pictures>>> =
            (0..views.len()).map(|i| sources.reduced.get(&key(i)).cloned()).collect();
        let Some(pictures) = pictures else { continue };
        let now_arrays = materials.get(material).and_then(|m| {
            Some([
                m.extension.views.as_ref()?.id(),
                m.extension.depths.as_ref()?.id(),
                m.extension.normals.as_ref()?.id(),
            ])
        });
        if let (true, true, Some(arrays)) = (refresh, factor == sources.factor, now_arrays) {
            for (layer, (ids, picture)) in layers.iter().zip(&pictures).enumerate() {
                if sources.layers[layer] != *ids {
                    writes.0.extend(picture.writes(arrays, layer as u32, None));
                }
            }
            sources.layers = layers;
            sources.stale = false;
            sources.made = now;
            if sources.at != Some((x, y)) {
                sources.at = Some((x, y));
                if let Some(mut material) = materials.get_mut(material) {
                    material.extension.window = Vec4::new(x as f32, y as f32, width as f32, height as f32);
                }
            }
            continue;
        }
        // A new window: its arrays made empty on the GPU and filled a few
        // layers a frame, the old ones drawn until they are full.
        let first = &pictures[0];
        let mut empty = |k: usize| {
            images.add(Image::new_uninit(
                bevy::render::render_resource::Extent3d {
                    width: first.width,
                    height: first.height,
                    depth_or_array_layers: pictures.len() as u32,
                },
                bevy::render::render_resource::TextureDimension::D2,
                first.formats[k],
                bevy::asset::RenderAssetUsages::RENDER_WORLD,
            ))
        };
        sources.filling = Some(Filling {
            arrays: [empty(0), empty(1), empty(2)],
            at: (x, y),
            size: (width, height),
            factor,
            layers,
            pictures,
            next: 0,
            left: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        });
        sources.stale = false;
        sources.made = now;
    }
}

/// How many layers of a new window are sent to the GPU a frame.
const FILL_PER_FRAME: usize = 4;

/// A new window being filled: its arrays, where it stands, its layers'
/// pictures, how many have been sent and how many writes are still to
/// reach the GPU.
pub(crate) struct Filling {
    arrays: [Handle<Image>; 3],
    at: (usize, usize),
    size: (usize, usize),
    factor: usize,
    layers: Vec<[AssetId<Image>; 3]>,
    pictures: Vec<std::sync::Arc<Pictures>>,
    next: usize,
    left: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

/// Sends a few more layers of each window being filled and, once all have
/// reached the GPU, draws the hologram from it.
fn fill(
    mut holograms: Query<(&Drawn, &mut Sources)>,
    mut materials: ResMut<Assets<HologramMaterial>>,
    mut writes: ResMut<LayerWrites>,
) {
    for (Drawn(material), mut sources) in &mut holograms {
        let Some(filling) = sources.filling.as_mut() else { continue };
        let arrays = [filling.arrays[0].id(), filling.arrays[1].id(), filling.arrays[2].id()];
        let end = (filling.next + FILL_PER_FRAME).min(filling.pictures.len());
        for layer in filling.next..end {
            filling.left.fetch_add(3, std::sync::atomic::Ordering::AcqRel);
            writes.0.extend(filling.pictures[layer].writes(arrays, layer as u32, Some(filling.left.clone())));
        }
        filling.next = end;
        if end < filling.pictures.len() || filling.left.load(std::sync::atomic::Ordering::Acquire) > 0 {
            continue;
        }
        let Some(filling) = sources.filling.take() else { continue };
        let Some(mut material) = materials.get_mut(material) else { continue };
        let [views, depths, normals] = filling.arrays;
        material.extension.views = Some(views);
        material.extension.depths = Some(depths);
        material.extension.normals = Some(normals);
        let (x, y) = filling.at;
        let (width, height) = filling.size;
        material.extension.window = Vec4::new(x as f32, y as f32, width as f32, height as f32);
        sources.at = Some((x, y));
        sources.size = (width, height);
        sources.layers = filling.layers;
        sources.factor = filling.factor;
    }
}

/// A view's picture, its depth's and its normals', as a layer of a
/// window's arrays: the views' own, or reduced.
pub(crate) struct Pictures {
    width: u32,
    height: u32,
    formats: [bevy::render::render_resource::TextureFormat; 3],
    data: [std::sync::Arc<Vec<u8>>; 3],
}

impl Pictures {
    fn of(factor: usize, view: &Image, depth: &Image, normal: &Image) -> Option<Self> {
        let formats = [view, depth, normal].map(|i| i.texture_descriptor.format);
        let ([v, d, n], width, height) = if factor == 1 {
            let (width, height) = (depth.width(), depth.height());
            if [view, normal].iter().any(|i| (i.width(), i.height()) != (width, height)) {
                return None;
            }
            ([view.data.clone()?, depth.data.clone()?, normal.data.clone()?], width, height)
        } else {
            let [mut v, mut d, mut n] = reduce(factor, view, depth, normal)?;
            let (width, height) = (d.width(), d.height());
            ([v.data.take()?, d.data.take()?, n.data.take()?], width, height)
        };
        Some(Self { width, height, formats, data: [v, d, n].map(std::sync::Arc::new) })
    }

    /// Its three writes into `arrays` at `layer`.
    fn writes(
        &self,
        arrays: [AssetId<Image>; 3],
        layer: u32,
        left: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
    ) -> impl Iterator<Item = LayerWrite> + '_ {
        (0..3).map(move |k| LayerWrite {
            array: arrays[k],
            layer,
            width: self.width,
            height: self.height,
            data: self.data[k].clone(),
            tries: 0,
            left: left.clone(),
        })
    }
}

/// The views of a window of the grid (first column and row, columns and
/// rows), in the order the shader reads them: each at its layer, wrapped round the window. Where a view has
/// not come (a streamed hologram's), the nearest that has stands in for it;
/// none when no view has come at all.
pub(crate) fn window(
    set: &[Option<Handle<Image>>],
    columns: usize,
    (x, y): (usize, usize),
    (width, height): (usize, usize),
) -> Option<Vec<Handle<Image>>> {
    // Each view at its layer, wrapped round the window's size (as the
    // shader reads it): a window slid by a column changes that column's
    // layers only.
    (0..width * height)
        .map(|layer| {
            let (wr, wc) = (layer / width, layer % width);
            let row = y + (wr + height - y % height) % height;
            let column = x + (wc + width - x % width) % width;
            nearest(set, columns, row, column)
        })
        .collect()
}

/// The view at (row, column) of a grid of `columns`, or the nearest there is.
fn nearest(set: &[Option<Handle<Image>>], columns: usize, row: usize, column: usize) -> Option<Handle<Image>> {
    let columns = columns.max(1);
    let rows = set.len().div_ceil(columns);
    let at = |r: usize, c: usize| set.get(r * columns + c).and_then(Clone::clone);
    if let Some(view) = at(row, column) {
        return Some(view);
    }
    // Ring after ring around it.
    for d in 1..rows.max(columns) as i64 {
        for dr in -d..=d {
            for dc in -d..=d {
                if dr.abs() != d && dc.abs() != d {
                    continue;
                }
                let (r, c) = (row as i64 + dr, column as i64 + dc);
                if r >= 0
                    && c >= 0
                    && (r as usize) < rows
                    && (c as usize) < columns
                    && let Some(view) = at(r as usize, c as usize)
                {
                    return Some(view);
                }
            }
        }
    }
    None
}

/// Keeps each hologram's material told where it stands and how it is turned.
#[allow(clippy::type_complexity)]
fn follow(
    holograms: Query<(&Hologram, &GlobalTransform, &Drawn), Or<(Changed<GlobalTransform>, Added<Drawn>)>>,
    mut materials: ResMut<Assets<HologramMaterial>>,
) {
    for (hologram, at, Drawn(material)) in &holograms {
        let Some(mut material) = materials.get_mut(material) else { continue };
        let centre = at.transform_point(hologram.centre);
        let forward = at.affine().transform_vector3(Vec3::Z).normalize_or(Vec3::Z);
        let yaw = forward.x.atan2(forward.z);
        material.extension.frame = centre.extend(hologram.radius);
        let (near, far) = hologram.fade_in.unwrap_or((0.0, 0.0));
        material.extension.turn = Vec4::new(yaw.cos(), yaw.sin(), near, far);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_s_layers_wrap_round_it() {
        let view = |i: u128| Some(Handle::<Image>::Uuid(bevy::asset::uuid::Uuid::from_u128(i + 1), Default::default()));
        // One row of five views; a window of three from the second.
        let set: Vec<_> = (0..5).map(view).collect();
        let held = window(&set, 5, (1, 0), (3, 1)).expect("views");
        // Column c at layer c modulo 3: the fourth view first.
        assert_eq!(held, vec![view(3).unwrap(), view(1).unwrap(), view(2).unwrap()]);
        // Slid by one: only the layer of the view it lost changes.
        let slid = window(&set, 5, (2, 0), (3, 1)).expect("views");
        assert_eq!(slid, vec![view(3).unwrap(), view(4).unwrap(), view(2).unwrap()]);
    }

    #[test]
    fn a_wide_window_is_reduced_to_its_budget() {
        assert_eq!(reduction(500, 1000), 1);
        assert_eq!(reduction(2800, 1000), 2);
        assert_eq!(reduction(10_000, 1000), 4);
        assert_eq!(reduction(100_000, 1000), 4);
    }

    #[test]
    fn a_reduced_texel_is_the_nearest_of_its_square() {
        let image = |texels: [[u8; 4]; 4]| {
            Image::new(
                bevy::render::render_resource::Extent3d { width: 2, height: 2, depth_or_array_layers: 1 },
                bevy::render::render_resource::TextureDimension::D2,
                texels.concat(),
                bevy::render::render_resource::TextureFormat::Rgba8Unorm,
                bevy::asset::RenderAssetUsages::MAIN_WORLD,
            )
        };
        // Depths: nothing, far, nearest, near; the colour says which.
        let depth = image([[0, 0, 0, 255], [10, 0, 255, 255], [200, 0, 255, 255], [100, 0, 255, 255]]);
        let colour = image([[1, 1, 1, 255], [2, 2, 2, 255], [3, 3, 3, 255], [4, 4, 4, 255]]);
        let [v, d, n] = reduce(2, &colour, &depth, &colour).expect("reduced");
        assert_eq!((v.width(), v.height()), (1, 1));
        assert_eq!(v.data.as_deref(), Some(&[3, 3, 3, 255][..]));
        assert_eq!(d.data.as_deref(), Some(&[200, 0, 255, 255][..]));
        assert_eq!(n.data.as_deref(), Some(&[3, 3, 3, 255][..]));
    }

    #[test]
    fn the_facade_grid_has_its_views() {
        let grid = Grid::facade();
        assert_eq!(grid.azimuths.len(), 15);
        assert_eq!(grid.elevations.len(), 9);
        assert_eq!(grid.len(), 135);
        assert_eq!(*grid.azimuths.first().unwrap(), -70.0);
        assert_eq!(*grid.elevations.last().unwrap(), 50.0);
    }

    #[test]
    fn the_path_visits_every_view_once_each_a_step_from_the_last() {
        let grid = Grid::facade();
        let path = grid.path();
        assert_eq!(path.len(), grid.len());
        let mut seen = std::collections::HashSet::new();
        for pair in path.windows(2) {
            let ((r0, c0), (r1, c1)) = (pair[0], pair[1]);
            assert_eq!(r0.abs_diff(r1) + c0.abs_diff(c1), 1, "{pair:?}");
        }
        for view in path {
            assert!(seen.insert(view));
        }
    }

    #[test]
    fn a_forest_is_the_same_at_every_call_and_inside_its_plot() {
        let a = forest(60.0, 30.0, 120, 7, 3);
        assert_eq!(a, forest(60.0, 30.0, 120, 7, 3));
        assert_eq!(a.iter().map(Vec::len).sum::<usize>(), 120);
        for place in a.iter().flatten() {
            assert!(place.translation.x.abs() <= 30.0 && (-30.0..=0.0).contains(&place.translation.z));
        }
    }

    #[test]
    fn depth_decodes_across_the_sphere() {
        assert_eq!(decode_depth([0, 0, 255, 255], 10.0), Some(-10.0));
        assert_eq!(decode_depth([255, 255, 255, 255], 10.0), Some(10.0));
        assert!((decode_depth([128, 0, 255, 255], 10.0).unwrap()).abs() < 0.01);
        assert_eq!(decode_depth([128, 0, 0, 255], 10.0), None);
    }

    #[test]
    fn directions_are_unit_and_look_at_the_front() {
        let [x, y, z] = Grid::direction(0.0, 0.0);
        assert!((x, y, z) == (0.0, 0.0, 1.0));
        let [x, y, z] = Grid::direction(30.0, 20.0);
        assert!(((x * x + y * y + z * z) - 1.0).abs() < 1e-5);
        assert!(x > 0.0 && y > 0.0 && z > 0.0);
    }
}
