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
}

impl Material for DepthMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://osage_hologram/depth.wesl".into()
    }
}

/// Registers [`DepthMaterial`], for baking.
pub struct DepthPlugin;

impl Plugin for DepthPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "depth.wesl");
        app.add_plugins(MaterialPlugin::<DepthMaterial>::default());
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
            .add_systems(Update, (assemble, follow).chain());
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
    /// Cosine and sine of the object's turn about the vertical.
    #[uniform(103)]
    pub turn: Vec4,
    #[texture(104, dimension = "2d_array")]
    #[sampler(105)]
    pub views: Option<Handle<Image>>,
    /// Each view's depth (see [`DepthMaterial`]), read without filtering.
    #[texture(106, dimension = "2d_array", sample_type = "float", filterable = false)]
    pub depths: Option<Handle<Image>>,
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
}

/// Each folder's views and depths, stacked once and shared by every
/// hologram drawn from it.
#[derive(Resource, Default)]
struct Stacks(std::collections::HashMap<String, (Handle<Image>, Handle<Image>)>);

/// A hologram whose views and depths are loading.
#[derive(Component)]
struct Loading(Vec<Handle<Image>>, Vec<Handle<Image>>);

/// A hologram's material, once made.
#[derive(Component)]
struct Drawn(Handle<HologramMaterial>);

#[allow(clippy::type_complexity)]
/// Loads each new hologram's views and, once all are there, stacks them
/// into one texture array and puts the stand-in up.
#[allow(clippy::too_many_arguments)]
fn assemble(
    mut commands: Commands,
    server: Res<AssetServer>,
    new: Query<(Entity, &Hologram), (Without<Loading>, Without<Drawn>)>,
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
        let views =
            order.iter().map(|&(row, column)| server.load(format!("{}/{}", hologram.folder, Grid::name(row, column))));
        // Depth is a number, not a colour: read as it is.
        let depths = order.iter().map(|&(row, column)| {
            server
                .load_builder()
                .with_settings(|settings: &mut bevy::image::ImageLoaderSettings| settings.is_srgb = false)
                .load(format!("{}/{}", hologram.folder, Grid::depth_name(row, column)))
        });
        commands.entity(entity).insert(Loading(views.collect(), depths.collect()));
    }
    for (entity, hologram, Loading(views, depths)) in &loading {
        if !views.iter().chain(depths).all(|view| server.is_loaded_with_dependencies(view)) {
            continue;
        }
        let stacked = match stacks.0.get(&hologram.folder) {
            Some(pair) => Some(pair.clone()),
            None => stack(views.iter().filter_map(|view| images.get(view)))
                .zip(stack(depths.iter().filter_map(|depth| images.get(depth))))
                .map(|(colour, depth)| {
                    let pair = (images.add(colour), images.add(depth));
                    stacks.0.insert(hologram.folder.clone(), pair.clone());
                    pair
                }),
        };
        let Some((stack, depth_stack)) = stacked else {
            warn!(folder = hologram.folder, "osage-hologram: the views are not all of one size and kind");
            commands.entity(entity).remove::<Loading>().insert(Drawn(Handle::default()));
            continue;
        };
        let grid = &hologram.grid;
        let step = |v: &[f32]| if v.len() > 1 { v[1] - v[0] } else { 1.0 };
        let material = materials.add(ExtendedMaterial {
            base: StandardMaterial { unlit: true, alpha_mode: AlphaMode::Blend, ..default() },
            extension: Views {
                grid: Vec4::new(grid.azimuths[0], step(&grid.azimuths), grid.elevations[0], step(&grid.elevations)),
                shape: Vec4::new(
                    grid.azimuths.len() as f32,
                    grid.elevations.len() as f32,
                    if hologram.flat { 0.0 } else { 1.0 },
                    0.0,
                ),
                frame: Vec4::ZERO,
                turn: Vec4::new(1.0, 0.0, 0.0, 0.0),
                views: Some(stack),
                depths: Some(depth_stack),
            },
        });
        let (centre, half) = hologram.stand;
        let quad = meshes.add(Rectangle::new(half.x * 2.0, half.y * 2.0));
        commands.entity(entity).remove::<Loading>().insert(Drawn(material.clone())).with_child((
            Mesh3d(quad),
            MeshMaterial3d(material),
            Transform::from_translation(centre),
            bevy::light::NotShadowCaster,
        ));
        info!(folder = hologram.folder, views = views.len(), "osage-hologram: drawn");
    }
}

/// The views stacked into one texture array, if all are the same size and
/// kind of 8-bit RGBA.
fn stack<'a>(views: impl Iterator<Item = &'a Image>) -> Option<Image> {
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
        material.extension.turn = Vec4::new(yaw.cos(), yaw.sin(), 0.0, 0.0);
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
