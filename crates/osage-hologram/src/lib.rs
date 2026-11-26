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
            .add_systems(Update, (assemble, follow, slide).chain());
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
    let mut data = Vec::new();
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
) {
    // The eye: the active 3D camera drawn last.
    let Some((_, eye)) = cameras.iter().filter(|(c, _)| c.is_active).max_by_key(|(c, _)| c.order) else { return };
    let eye = eye.translation();
    for (hologram, at, Drawn(material), mut sources) in &mut holograms {
        let Some((width, height)) = hologram.window else { continue };
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
        // (a chunk of woods by the track): the window widens to the angle
        // its sphere subtends, in columns and rows of the grid, and narrows
        // again, a little late, as it goes away.
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
        let size = |half: f32, least: usize, now: usize, n: usize| {
            let need = ((2.0 * half).ceil() as usize + 2).max(least).min(n);
            if now >= need && now <= need + 2 { now } else { need }
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
        let (x, y) = (first(fa, width, columns), first(fe, height, rows));
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
        let made = (
            whole(&views).and_then(|v| stack(v.into_iter())),
            whole(&depths).and_then(|v| stack(v.into_iter())),
            whole(&normals).and_then(|v| stack(v.into_iter())),
        );
        let (Some(colour), Some(depth), Some(normal)) = made else { continue };
        let Some(mut material) = materials.get_mut(material) else { continue };
        material.extension.views = Some(images.add(colour));
        material.extension.depths = Some(images.add(depth));
        material.extension.normals = Some(images.add(normal));
        material.extension.window = Vec4::new(x as f32, y as f32, width as f32, height as f32);
        sources.at = Some((x, y));
        sources.size = (width, height);
        sources.stale = false;
        sources.made = now;
    }
}

/// The views of a window of the grid (first column and row, columns and
/// rows), in the order the shader reads them: row by row. Where a view has
/// not come (a streamed hologram's), the nearest that has stands in for it;
/// none when no view has come at all.
pub(crate) fn window(
    set: &[Option<Handle<Image>>],
    columns: usize,
    (x, y): (usize, usize),
    (width, height): (usize, usize),
) -> Option<Vec<Handle<Image>>> {
    (y..y + height)
        .flat_map(|row| (x..x + width).map(move |column| (row, column)))
        .map(|(row, column)| nearest(set, columns, row, column))
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
