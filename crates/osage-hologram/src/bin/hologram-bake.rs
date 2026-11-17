// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Photographs a model of the asset manifest from every direction of a
//! [`Grid`], one picture per view, for the holographic textures experiment
//! (issue #7):
//!
//!     hologram-bake OUT [MODEL] [SIZE] [X,Y,Z,R]
//!
//! MODEL is a glTF file under the manifest's content (default: Poly
//! Haven's urban apartments facade), SIZE the pictures' side in pixels
//! (default 512), X,Y,Z,R the part of it framed — a sphere's centre and
//! radius, m (default: the whole model). Each view is orthographic — no perspective, the object
//! framed the same in every picture, so that a view differs from the next
//! only by the turn of the viewing angle — under one fixed sun, on a grey
//! ground colour, or on magenta with `HOLOGRAM_KEY=1` (drawn as nothing
//! by the hologram's shader). Without a window: the pictures are drawn off screen.

use std::{path::PathBuf, time::Duration};

use bevy::{
    app::ScheduleRunnerPlugin,
    asset::RenderAssetUsages,
    camera::{RenderTarget, ScalingMode, primitives::Aabb},
    prelude::*,
    render::{
        RenderPlugin,
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
        view::screenshot::{Screenshot, save_to_disk},
    },
    window::ExitCondition,
    winit::WinitPlugin,
};
use bevy::{
    camera::visibility::RenderLayers,
    core_pipeline::tonemapping::{DebandDither, Tonemapping},
    render::view::Msaa,
};
use osage_hologram::{DepthMaterial, DepthPlugin, Grid, NormalMaterial};

const FACADE: &str = "polyhaven/modular_urban_apartments_facade/modular_urban_apartments_facade_1k.gltf";
/// Frames drawn at a view before it is photographed, and after the last
/// before the program ends (for the pictures to be written).
const SETTLE: u32 = 4;
const WIND_DOWN: u32 = 60;

#[derive(Resource)]
struct Bake {
    out: PathBuf,
    size: u32,
    grid: Grid,
    path: Vec<(usize, usize)>,
    /// The view being photographed, and the frames it has been drawn.
    at: usize,
    frames: u32,
    /// The part framed: its centre and radius; the whole object's, once it
    /// has loaded, unless given.
    bounds: Option<(Vec3, f32)>,
    /// Whether the model has loaded and stands in place.
    loaded: bool,
    target: Handle<Image>,
    camera: Option<Entity>,
    /// The depth pictures: their target, camera and material (a twin of
    /// the model on its own layer, drawn by depth).
    depth_target: Handle<Image>,
    depth_camera: Option<Entity>,
    depth_material: Handle<DepthMaterial>,
    /// The pictures of the colour without light and of the normals, for a
    /// hologram lit where it is drawn: their targets and cameras (each a
    /// twin of the model on a layer of its own: 2 unlit, 3 by normals).
    albedo_target: Handle<Image>,
    albedo_camera: Option<Entity>,
    normal_target: Handle<Image>,
    normal_camera: Option<Entity>,
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(anyhow::Context::with_context(args.next(), || "hologram-bake OUT [MODEL] [SIZE]")?);
    let model = args.next().filter(|m| !m.is_empty()).unwrap_or_else(|| FACADE.to_owned());
    let size: u32 =
        anyhow::Context::with_context(args.next().map_or(Ok(512), |s| s.parse::<u32>()), || "SIZE in pixels")?;
    let framed = match args.next() {
        Some(region) => {
            let n: Vec<f32> = region.split(',').map(str::parse).collect::<Result<_, _>>()?;
            let [x, y, z, r] = n[..] else { anyhow::bail!("X,Y,Z,R: a centre and a radius") };
            Some((Vec3::new(x, y, z), r))
        }
        None => None,
    };
    std::fs::create_dir_all(&out)?;
    let root = anyhow::Context::with_context(
        osage_content::root(),
        || "the asset manifest content is not here (OSAGE_ASSETS)",
    )?;
    // The grid, from the environment for experiments: HOLOGRAM_GRID =
    // "AZ_FROM,AZ_TO,AZ_STEP,EL_FROM,EL_TO,EL_STEP" (default: a facade's).
    let grid = match std::env::var("HOLOGRAM_GRID") {
        Ok(text) => {
            let n: Vec<f32> = text.split(',').map(str::parse).collect::<Result<_, _>>()?;
            let [a0, a1, a2, e0, e1, e2] = n[..] else { anyhow::bail!("HOLOGRAM_GRID takes six numbers") };
            Grid::new((a0, a1, a2), (e0, e1, e2))
        }
        Err(_) => Grid::facade(),
    };
    let path = grid.path();
    let plugins = DefaultPlugins
        .build()
        .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
        .set(RenderPlugin { synchronous_pipeline_compilation: true, ..default() })
        .set(AssetPlugin { file_path: root.to_string_lossy().into_owned(), ..default() })
        .disable::<WinitPlugin>();
    App::new()
        .add_plugins(plugins)
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::ZERO))
        .add_plugins((osage_props::PropsPlugin, DepthPlugin))
        .insert_resource(Bake {
            out,
            size,
            grid,
            path,
            at: 0,
            frames: 0,
            bounds: framed,
            loaded: false,
            target: Handle::default(),
            camera: None,
            depth_target: Handle::default(),
            depth_camera: None,
            depth_material: Handle::default(),
            albedo_target: Handle::default(),
            albedo_camera: None,
            normal_target: Handle::default(),
            normal_camera: None,
        })
        .insert_resource(ModelPath(model))
        .add_systems(Startup, setup)
        .add_systems(Update, (measure, photograph).chain())
        .run();
    Ok(())
}

#[derive(Resource)]
struct ModelPath(String);

/// Saves a captured picture of numbers (a linear format Bevy's own saving
/// refuses) byte for byte: labelled sRGB only to be written, its bytes
/// unchanged.
fn save_numbers(path: PathBuf) -> impl FnMut(On<bevy::render::view::screenshot::ScreenshotCaptured>) {
    move |captured| {
        let mut image = captured.image.clone();
        image.texture_descriptor.format = TextureFormat::Rgba8UnormSrgb;
        match image.try_into_dynamic() {
            Ok(picture) => {
                if let Err(error) = picture.to_rgba8().save(&path) {
                    error!(?path, %error, "hologram-bake: cannot save a depth picture");
                }
            }
            Err(error) => error!(?path, %error, "hologram-bake: cannot read a depth picture"),
        }
    }
}

/// The views' height to width.
fn aspect() -> f32 {
    std::env::var("HOLOGRAM_ASPECT").ok().and_then(|a| a.parse().ok()).unwrap_or(1.0)
}

/// The pictures' ground: grey to measure compression, magenta to be keyed
/// out when the views are drawn.
fn background() -> Color {
    match std::env::var("HOLOGRAM_KEY").as_deref() {
        Ok("black") => Color::BLACK,
        Ok(_) => Color::srgb(1.0, 0.0, 1.0),
        Err(_) => Color::srgb(0.5, 0.5, 0.5),
    }
}

fn setup(
    mut commands: Commands,
    server: Res<AssetServer>,
    model: Res<ModelPath>,
    mut bake: ResMut<Bake>,
    mut images: ResMut<Assets<Image>>,
) {
    // A forest (HOLOGRAM_FOREST = "WIDTH,DEPTH,TREES,SEED"): the trees of
    // HOLOGRAM_TREES (glTF files, comma-separated) scattered over a plot
    // WIDTH wide along x (centred), DEPTH deep behind z 0, each turned and
    // sized by a seeded hash; else the one model at the origin.
    if let Ok(forest) = std::env::var("HOLOGRAM_FOREST") {
        let n: Vec<f32> = forest.split(',').filter_map(|v| v.parse().ok()).collect();
        let trees: Vec<String> =
            std::env::var("HOLOGRAM_TREES").unwrap_or_default().split(',').map(str::to_owned).collect();
        if let ([width, depth, count, seed], false) = (n.as_slice(), trees.is_empty()) {
            let (width, depth, count, seed) = (*width, *depth, *count, *seed);
            let places = osage_hologram::forest(width, depth, count as usize, seed as u64, trees.len());
            for (tree, places) in trees.iter().zip(places) {
                commands.spawn(osage_props::Scatter {
                    model: server.load(tree.clone()),
                    places,
                    using: osage_props::Use::Whole,
                    within: None,
                    shadows: true,
                    lods: false,
                    tint: None,
                    opaque: true,
                    lit_as_ground: false,
                });
            }
            info!(trees = count, kinds = trees.len(), "hologram-bake: a forest");
        }
    } else {
        commands.spawn(osage_props::Scatter {
            model: server.load(model.0.clone()),
            places: vec![Transform::IDENTITY],
            using: osage_props::Use::Whole,
            within: None,
            shadows: true,
            lods: false,
            tint: None,
            opaque: false,
            lit_as_ground: false,
        });
    }
    // One sun, high and to the left of the front, the same for every view:
    // the light is part of what is photographed.
    commands.spawn((
        // A forest has no shadow maps: redrawn per cascade, its hundreds
        // of millions of faces lost the integrated GPU.
        DirectionalLight {
            illuminance: 10_000.0,
            shadow_maps_enabled: std::env::var_os("HOLOGRAM_FOREST").is_none(),
            ..default()
        },
        Transform::from_xyz(-0.6, 1.0, 0.8).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // HOLOGRAM_ASPECT: the views' height to width (default 1), for a wide,
    // low object such as a strip of forest.
    let size =
        Extent3d { width: bake.size, height: (bake.size as f32 * aspect()).round() as u32, depth_or_array_layers: 1 };
    let mut target = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    target.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC;
    bake.target = images.add(target);
    // Depth is a number, not a colour: no sRGB.
    let mut depth = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::default(),
    );
    depth.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC;
    bake.depth_target = images.add(depth.clone());
    // The albedo is a colour (sRGB), the normals numbers.
    bake.albedo_target = images.add(target_like(size, TextureFormat::Rgba8UnormSrgb));
    bake.normal_target = images.add(depth);
}

fn target_like(size: Extent3d, format: TextureFormat) -> Image {
    let mut image = Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 255], format, RenderAssetUsages::default());
    image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC;
    image
}

/// Once the model is in place: its centre and the radius of the sphere
/// round it, from its meshes' boxes.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn measure(
    mut commands: Commands,
    mut bake: ResMut<Bake>,
    waiting: Res<osage_props::Waiting>,
    meshes: Query<(&Aabb, &GlobalTransform)>,
    drawn: Query<(&Mesh3d, &GlobalTransform, &MeshMaterial3d<StandardMaterial>)>,
    mut depth_materials: ResMut<Assets<DepthMaterial>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut normal_materials: ResMut<Assets<NormalMaterial>>,
) {
    if bake.loaded || waiting.0 > 0 || meshes.is_empty() {
        return;
    }
    bake.loaded = true;
    // The model's twin, drawn by depth, on its own layer.
    bake.depth_material = depth_materials.add(DepthMaterial::default());
    let normals = normal_materials.add(NormalMaterial {});
    let mut unlit = std::collections::HashMap::new();
    for (mesh, at, material) in &drawn {
        let place = at.compute_transform();
        commands.spawn((mesh.clone(), MeshMaterial3d(bake.depth_material.clone()), place, RenderLayers::layer(1)));
        // Its own colour, without light: the material made unlit.
        let flat = unlit
            .entry(material.0.id())
            .or_insert_with(|| {
                let mut copy = standard.get(&material.0).cloned().unwrap_or_default();
                copy.unlit = true;
                standard.add(copy)
            })
            .clone();
        commands.spawn((mesh.clone(), MeshMaterial3d(flat), place, RenderLayers::layer(2)));
        commands.spawn((mesh.clone(), MeshMaterial3d(normals.clone()), place, RenderLayers::layer(3)));
    }
    if bake.bounds.is_some() {
        return;
    }
    let (mut low, mut high) = (Vec3::MAX, Vec3::MIN);
    for (aabb, at) in &meshes {
        for corner in 0..8 {
            let c = Vec3::from(aabb.center);
            let h = Vec3::from(aabb.half_extents);
            let sign = Vec3::new(
                if corner & 1 == 0 { -1.0 } else { 1.0 },
                if corner & 2 == 0 { -1.0 } else { 1.0 },
                if corner & 4 == 0 { -1.0 } else { 1.0 },
            );
            let p = at.transform_point(c + h * sign);
            (low, high) = (low.min(p), high.max(p));
        }
    }
    let centre = (low + high) / 2.0;
    let radius = (high - low).length() / 2.0;
    info!(?low, ?high, radius, "hologram-bake: the model's bounds");
    bake.bounds = Some((centre, radius));
}

/// Each view in turn: the camera put there, a few frames drawn, a picture.
fn photograph(
    mut commands: Commands,
    mut bake: ResMut<Bake>,
    mut exit: MessageWriter<AppExit>,
    mut depth_materials: ResMut<Assets<DepthMaterial>>,
    mut cameras: Query<&mut Camera>,
) {
    let Some((centre, radius)) = bake.bounds.filter(|_| bake.loaded) else { return };
    if bake.at >= bake.path.len() {
        bake.frames += 1;
        if bake.frames > WIND_DOWN {
            info!(views = bake.path.len(), out = ?bake.out, "hologram-bake: done");
            exit.write(AppExit::Success);
        }
        return;
    }
    let (row, column) = bake.path[bake.at];
    let [x, y, z] = Grid::direction(bake.grid.azimuths[column], bake.grid.elevations[row]);
    let place = Transform::from_translation(centre + Vec3::new(x, y, z) * radius * 3.0).looking_at(centre, Vec3::Y);
    if let Some(mut material) = depth_materials.get_mut(&bake.depth_material) {
        material.frame = centre.extend(radius);
        material.back = Vec4::new(x, y, z, 0.0);
    }
    let projection = OrthographicProjection {
        scaling_mode: ScalingMode::Fixed { width: radius * 2.0, height: radius * 2.0 * aspect() },
        ..OrthographicProjection::default_3d()
    };
    match bake.depth_camera {
        None => {
            let camera = commands
                .spawn((
                    Camera3d::default(),
                    Camera { order: 1, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
                    RenderTarget::Image(bake.depth_target.clone().into()),
                    Projection::Orthographic(projection.clone()),
                    Tonemapping::None,
                    DebandDither::Disabled,
                    Msaa::Off,
                    RenderLayers::layer(1),
                    place,
                ))
                .id();
            bake.depth_camera = Some(camera);
        }
        Some(camera) if bake.frames == 0 => {
            commands.entity(camera).insert(place);
        }
        Some(_) => {}
    }
    // The albedo's camera: no tone mapping (the colour as it is), the
    // background magenta; the normals': numbers, black ground.
    for (layer, which) in [(2usize, 0usize), (3, 1)] {
        let existing = if which == 0 { bake.albedo_camera } else { bake.normal_camera };
        match existing {
            None => {
                let (target, clear) = if which == 0 {
                    (bake.albedo_target.clone(), background())
                } else {
                    (bake.normal_target.clone(), Color::BLACK)
                };
                let camera = commands
                    .spawn((
                        Camera3d::default(),
                        Camera { order: 1 + layer as isize, clear_color: ClearColorConfig::Custom(clear), ..default() },
                        RenderTarget::Image(target.into()),
                        Projection::Orthographic(projection.clone()),
                        Tonemapping::None,
                        DebandDither::Disabled,
                        RenderLayers::layer(layer),
                        place,
                    ))
                    .id();
                if which == 0 {
                    bake.albedo_camera = Some(camera);
                } else {
                    bake.normal_camera = Some(camera);
                }
            }
            Some(camera) if bake.frames == 0 => {
                commands.entity(camera).insert(place);
            }
            Some(_) => {}
        }
    }
    match bake.camera {
        None => {
            let camera = commands
                .spawn((
                    Camera3d::default(),
                    Camera { clear_color: ClearColorConfig::Custom(background()), ..default() },
                    RenderTarget::Image(bake.target.clone().into()),
                    Projection::Orthographic(projection),
                    place,
                ))
                .id();
            bake.camera = Some(camera);
        }
        Some(camera) if bake.frames == 0 => {
            commands.entity(camera).insert(place);
        }
        Some(_) => {}
    }
    // One picture at a time: each camera draws alone for SETTLE frames and
    // is photographed on its last — a heavy model drawn by four cameras in
    // one frame lost the GPU.
    let pass = (bake.frames / SETTLE) as usize;
    let order = [bake.camera, bake.depth_camera, bake.albedo_camera, bake.normal_camera];
    for (k, camera) in order.iter().enumerate() {
        if let Some(mut settings) = camera.and_then(|c| cameras.get_mut(c).ok()) {
            settings.is_active = k == pass;
        }
    }
    bake.frames += 1;
    if bake.frames.is_multiple_of(SETTLE) {
        let (target, file, numbers) = match pass {
            0 => (bake.target.clone(), Grid::name(row, column), false),
            1 => (bake.depth_target.clone(), Grid::depth_name(row, column), true),
            2 => (bake.albedo_target.clone(), Grid::albedo_name(row, column), false),
            _ => (bake.normal_target.clone(), Grid::normal_name(row, column), true),
        };
        let path = bake.out.join(file);
        if numbers {
            commands.spawn(Screenshot::image(target)).observe(save_numbers(path));
        } else {
            commands.spawn(Screenshot::image(target)).observe(save_to_disk(path));
        }
        if pass == 3 {
            bake.at += 1;
            bake.frames = 0;
        }
    }
}
