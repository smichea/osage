// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{
    impact::{Live, Playback},
    people::Model,
    physics::{Actor, Physics},
    replay::{Clock, Grid, Replay},
    scene::{Primitive, Sector, SurfaceAlpha, SurfaceStyle},
};
use anyhow::{Result, ensure};
use bevy::{
    app::ScheduleRunnerPlugin,
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured},
    transform::TransformSystems,
    window::ExitCondition,
    winit::WinitPlugin,
};
use osage_render::{
    OsageCamera, OsageRenderPlugin, Sun, Technique, Tier,
    resolution::{Output, Picture},
};
use std::{collections::HashSet, path::PathBuf, time::Duration};

/// Cooked district placements already contain the complete source-to-world affine.
#[derive(Component)]
pub(crate) struct Placement(pub Mat4);

pub(crate) fn preserve_affines(mut placements: Query<(&Placement, &mut GlobalTransform), Added<Placement>>) {
    for (src, mut dst) in &mut placements {
        *dst = GlobalTransform::from(src.0);
    }
}

#[derive(Component)]
enum BodyMarker {
    Limb(usize),
    MovingSurface,
}

#[derive(Resource)]
struct Run {
    route: usize,
    tick: u32,
    physics: Physics,
    model: Model,
    actor: Actor,
    clock: Clock,
    paused: bool,
    full_detail: bool,
    shot: Option<PathBuf>,
}

impl Run {
    fn set(&mut self, r: &Replay, s: &Sector, index: usize) -> Result<()> {
        let actor = Actor::new(self.physics, &self.model, s, r, &r.routes[index])?;
        self.route = index;
        self.tick = 0;
        self.actor = actor;
        self.clock = Clock::default();
        self.paused = false;
        Ok(())
    }

    fn advance(&mut self, n: u32, r: &Replay, s: &Sector, g: &Grid, mut sound: Option<&mut Live>) -> Result<()> {
        let before = self.tick;
        let stop = self.tick.saturating_add(n).min(r.routes[self.route].ticks);
        while self.tick < stop {
            self.tick += 1;
            if let Some(out) = sound.as_deref_mut() {
                out.step(&mut self.actor, self.tick, r, &r.routes[self.route], s, g)?;
            } else {
                self.actor.advance(self.tick, r, s, g)?;
            }
        }
        if let Some(out) = sound {
            out.tail(n - (self.tick - before), r.tick_hz);
        }
        Ok(())
    }
}

pub struct Options {
    pub selected: usize,
    pub listening: Option<Playback>,
    pub shot: Option<PathBuf>,
    pub tick: u32,
    pub timing: Option<(PathBuf, serde_json::Value)>,
    pub full_detail: bool,
    pub physics: Physics,
    pub model: Model,
}

pub fn run(scene: Sector, replay: Replay, options: Options) -> Result<()> {
    run_with_materials(scene, replay, options, InstanceMaterials::default())
}

#[derive(Default, Resource)]
pub struct InstanceMaterials {
    pub variants: Vec<Vec<MaterialFactors>>,
    pub assignments: std::collections::BTreeMap<usize, usize>,
}
struct FaceMaterials {
    front: Handle<StandardMaterial>,
    reflected: Option<Handle<StandardMaterial>>,
}

impl FaceMaterials {
    fn new(materials: &mut Assets<StandardMaterial>, front: StandardMaterial, needs_reflected: bool) -> Self {
        let reflected = (needs_reflected && front.cull_mode.is_some()).then(|| {
            let mut reversed = front.clone();
            reversed.cull_mode = Some(bevy::render::render_resource::Face::Front);
            materials.add(reversed)
        });
        Self { front: materials.add(front), reflected }
    }

    fn at(&self, reflected: bool) -> &Handle<StandardMaterial> {
        if reflected && let Some(handle) = &self.reflected { handle } else { &self.front }
    }
}

pub struct MaterialFactors {
    pub color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub style: SurfaceStyle,
    pub texture: Option<crate::texture::Texture>,
}
impl MaterialFactors {
    pub(crate) fn from_primitive(p: &Primitive) -> Self {
        Self {
            color: p.color,
            metallic: p.metallic,
            roughness: p.roughness,
            style: p.style,
            texture: p.texture.clone(),
        }
    }

    /// A certified box has unit vertex alpha on every face. A mask must pass on its least opaque texel.
    pub(crate) fn fills_box(&self) -> bool {
        match self.style.alpha_mode {
            SurfaceAlpha::Opaque => true,
            SurfaceAlpha::Mask => {
                self.color[3] * self.texture.as_ref().map_or(1., |t| f32::from(t.pixels.min_alpha) / 255.)
                    >= self.style.alpha_cutoff
            }
            SurfaceAlpha::Blend => false,
        }
    }

    pub(crate) fn texture_cutoff(&self) -> Option<u8> {
        let cut = self.style.alpha_cutoff;
        if self.style.alpha_mode != SurfaceAlpha::Mask || cut <= 0. || self.color[3] < cut {
            return None;
        }
        // Match the GPU comparison after the same normalized-byte conversion and material factor.
        (1..=255u8).find(|&a| (f32::from(a) / 255.) * self.color[3] >= cut)
    }
    pub(crate) fn material(&self, cache: &mut crate::texture::Images, images: &mut Assets<Image>) -> StandardMaterial {
        let [r, g, b, a] = self.color;
        StandardMaterial {
            base_color: Color::linear_rgba(r, g, b, a),
            base_color_texture: self
                .texture
                .as_ref()
                .map(|texture| texture.handle(self.texture_cutoff(), cache, images)),
            alpha_mode: match self.style.alpha_mode {
                SurfaceAlpha::Opaque => AlphaMode::Opaque,
                SurfaceAlpha::Mask => AlphaMode::Mask(self.style.alpha_cutoff),
                SurfaceAlpha::Blend => AlphaMode::Blend,
            },
            double_sided: self.style.double_sided,
            cull_mode: (!self.style.double_sided).then_some(bevy::render::render_resource::Face::Back),
            metallic: self.metallic,
            perceptual_roughness: self.roughness,
            ..default()
        }
    }
}

pub fn run_with_materials(scene: Sector, replay: Replay, options: Options, overrides: InstanceMaterials) -> Result<()> {
    let Options { selected, shot, tick, timing, full_detail, physics, model, listening } = options;
    let size = UVec2::from_array(replay.viewport);
    let mut app = App::new();
    app.insert_resource(overrides);
    let offscreen = shot.is_some() || timing.is_some();
    let renderer = crate::compressed_texture::renderer(offscreen);
    if offscreen {
        let windowless = WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() };
        app.add_plugins(
            DefaultPlugins
                .set(windowless)
                .set(renderer)
                .disable::<WinitPlugin>()
                .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>(),
        )
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::ZERO));
    } else {
        let window = Window {
            title: format!("Osage · Ville-Paris · {physics} · {model}"),
            resolution: (size.x, size.y).into(),
            resizable: false,
            ..default()
        };
        app.add_plugins(DefaultPlugins.set(renderer).set(WindowPlugin { primary_window: Some(window), ..default() }));
    }
    if let Some(path) = shot.as_ref().or_else(|| timing.as_ref().map(|t| &t.0))
        && let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    if let Some((path, inputs)) = timing {
        app.insert_resource(Timed { path, inputs, warmup: 60, last: None, samples: Vec::new() })
            .add_systems(Update, measure.before(pose));
        use bevy::render::{Render, RenderApp, RenderSystems};
        app.sub_app_mut(RenderApp).add_systems(Render, wait_for_gpu.in_set(RenderSystems::Cleanup));
    } else if shot.is_none() {
        if let Some(output) = listening.and_then(|mode| Live::open(mode, &scene)) {
            app.insert_non_send(output);
        }
        app.add_systems(Update, drive.before(pose));
    }
    let mut run = Run {
        route: selected,
        tick: 0,
        physics,
        actor: Actor::new(physics, &model, &scene, &replay, &replay.routes[selected])?,
        model,
        clock: Clock::default(),
        paused: false,
        full_detail,
        shot,
    };
    let grid = Grid::new(&scene.supports);
    run.advance(tick, &replay, &scene, &grid, None)?;
    app.insert_resource(Output(Some(size)))
        .add_plugins(OsageRenderPlugin { tier: Tier::T2, without: vec![Technique::Upscaling] })
        .add_plugins(crate::compressed_texture::CompressionPlugin)
        .insert_resource(replay)
        .insert_resource(scene)
        .insert_resource(grid)
        .insert_resource(run)
        .add_systems(Startup, setup)
        .add_systems(
            PostUpdate,
            preserve_affines
                .after(TransformSystems::Propagate)
                .before(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
        )
        .add_systems(Update, (pose, capture).chain());
    if !full_detail {
        app.add_systems(Update, (choose_windows, choose_object_detail).after(pose).before(capture));
    }
    ensure!(app.run().is_success(), "render failed");
    Ok(())
}

#[derive(Component)]
struct ObjectDetail {
    owner: usize,
    current: usize,
    groups: Vec<Entity>,
}

fn choose_object_detail(
    scene: Res<Sector>,
    camera: Single<(&Transform, &Projection, &Camera), With<OsageCamera>>,
    mut instances: Query<&mut ObjectDetail>,
    mut visibility: Query<&mut Visibility>,
) {
    let Some(context) = &scene.object else { return };
    let (camera_position, projection, camera) = *camera;
    let Projection::Perspective(lens) = projection else { return };
    let Some(size) = camera.physical_viewport_size() else { return };
    let Some(view) = osage_objects::DetailView::new(
        camera_position.translation.as_dvec3(),
        camera_position.forward().as_vec3().as_dvec3(),
        f64::from(lens.fov),
        size.y,
        f64::from(lens.near),
    ) else {
        return;
    };
    for mut instance in &mut instances {
        let next = context.detail[&instance.owner].choose(instance.current, view);
        if next != instance.current {
            *visibility.get_mut(instance.groups[instance.current]).expect("visual level group") = Visibility::Hidden;
            *visibility.get_mut(instance.groups[next]).expect("visual level group") = Visibility::Inherited;
            instance.current = next;
        }
    }
}

#[derive(Component)]
struct WindowDetail {
    full: Handle<Mesh>,
    flat: Handle<Mesh>,
    distance_squared: f32,
}

// On the authored facades the outside of every pane is on one of four planes.
// Preserve its exact position, normal and colour. Only the depth of the recess
// disappears when a camera is at least 200 m clear of the building.
fn window_faces(part: &Primitive) -> Option<Vec<u32>> {
    let low = part.positions.iter().map(|&p| Vec3::from(p)).fold(Vec3::splat(f32::INFINITY), Vec3::min);
    let high = part.positions.iter().map(|&p| Vec3::from(p)).fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
    let mut out = Vec::new();
    for tri in part.indices.as_chunks::<3>().0 {
        let i = tri[0] as usize;
        let n = Vec3::from(part.normals[i]);
        let (axis, plane) = if n.x.abs() > 0.999 {
            (0, if n.x > 0. { high.x } else { low.x })
        } else if n.z.abs() > 0.999 {
            (2, if n.z > 0. { high.z } else { low.z })
        } else {
            continue;
        };
        if tri.iter().all(|&v| (part.positions[v as usize][axis] - plane).abs() < 1e-6) {
            out.extend_from_slice(tri);
        }
    }
    (!out.is_empty() && out.len() < part.indices.len()).then_some(out)
}

fn choose_windows(
    camera: Single<&Transform, With<OsageCamera>>,
    mut panes: Query<(&GlobalTransform, &WindowDetail, &mut Mesh3d)>,
) {
    for (instance, detail, mut mesh) in &mut panes {
        let far = camera.translation.distance_squared(instance.translation()) > detail.distance_squared;
        let wanted = if far { &detail.flat } else { &detail.full };
        if &mesh.0 != wanted {
            mesh.0 = wanted.clone();
        }
    }
}

pub(crate) fn compact_indices(values: &[u32]) -> Indices {
    // Bevy promotes the array when an index needs u32 or equals its reserved u16 restart value.
    let mut indices = Indices::U16(Vec::new());
    indices.extend(values.iter().copied());
    indices
}

pub(crate) fn render_primitive(part: &crate::scene::Primitive) -> Mesh {
    render_primitive_with_indices(part, compact_indices(&part.indices))
}

pub(crate) fn render_primitive_with_indices(part: &crate::scene::Primitive, indices: Indices) -> Mesh {
    let mut shape = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, part.positions.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, part.normals.clone())
        .with_inserted_indices(indices);
    if let Some(colors) = &part.colors {
        shape.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors.clone());
    }
    if let Some(uv) = &part.uv {
        shape.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv.clone());
    }
    shape
}

pub(crate) fn moving_box_draw(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> (Mesh3d, MeshMaterial3d<StandardMaterial>) {
    (
        Mesh3d(meshes.add(Cuboid::new(1., 1., 1.))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.09, 0.34, 0.56),
            metallic: 0.35,
            ..default()
        })),
    )
}

pub(crate) struct ActorDraw {
    pub part: usize,
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
}

/// Each imported rigid primitive is already expressed in its physical ancestor's bind frame.
pub(crate) fn actor_draws(
    model: &Model,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    textures: &mut crate::texture::Images,
    images: &mut Assets<Image>,
) -> Vec<ActorDraw> {
    if let Model::Imported(rig) = model {
        return rig
            .visuals
            .iter()
            .map(|(index, part)| {
                let mesh = meshes.add(render_primitive(part));
                let material = materials.add(MaterialFactors::from_primitive(part).material(textures, images));
                ActorDraw { part: *index, mesh, material }
            })
            .collect();
    }
    let coat = materials.add(StandardMaterial { base_color: Color::srgb(0.82, 0.19, 0.06), ..default() });
    let parts = model.parts().unwrap_or_default();
    if parts.is_empty() {
        let capsule = crate::physics::CAPSULE;
        return vec![ActorDraw {
            part: 0,
            mesh: meshes.add(Capsule3d::new(capsule.radius, 2. * capsule.half_segment)),
            material: coat,
        }];
    }
    let trousers = materials.add(StandardMaterial { base_color: Color::srgb(0.08, 0.15, 0.24), ..default() });
    let skin = materials.add(StandardMaterial { base_color: Color::srgb(0.70, 0.43, 0.29), ..default() });
    parts
        .iter()
        .enumerate()
        .map(|(index, p)| {
            let region = p.name.split('_').next().unwrap_or(p.name.as_str());
            let material = match region {
                "pelvis" | "thigh" | "calf" | "foot" => &trousers,
                "Head" | "hand" | "lowerarm" => &skin,
                _ => &coat,
            };
            let mesh = if p.half_segment == 0. {
                meshes.add(Sphere::new(p.radius))
            } else {
                meshes.add(Capsule3d::new(p.radius, 2. * p.half_segment))
            };
            ActorDraw { part: index, mesh, material: material.clone() }
        })
        .collect()
}

#[allow(clippy::too_many_arguments, reason = "Bevy supplies the scene and asset resources independently")]
fn setup(
    mut cmd: Commands,
    mut scene: ResMut<Sector>,
    r: Res<Replay>,
    run: Res<Run>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    overrides: Res<InstanceMaterials>,
    mut images: ResMut<Assets<Image>>,
    mut compressed: ResMut<crate::compressed_texture::Queue>,
    device: Res<bevy::render::renderer::RenderDevice>,
    adapter: Res<bevy::render::renderer::RenderAdapter>,
) {
    compressed.configure(&device, &adapter);
    let mut textures = crate::texture::Images::new();
    let reflected_draws: HashSet<_> = scene
        .instances
        .iter()
        .enumerate()
        .filter(|(_, i)| i.transform.as_dmat4().determinant() < 0.)
        .map(|(i, _)| i)
        .collect();
    let reflected_meshes: HashSet<_> = reflected_draws.iter().map(|&i| scene.instances[i].mesh).collect();
    let reflected_variants: HashSet<_> =
        reflected_draws.iter().filter_map(|i| overrides.assignments.get(i).copied()).collect();
    let mut models = Vec::new();
    for (mesh_id, mesh) in scene.meshes.iter_mut().enumerate() {
        let mut parts = Vec::new();
        for part in mesh {
            let shape = render_primitive(part);
            let factors = MaterialFactors::from_primitive(part);
            let mat = FaceMaterials::new(
                &mut materials,
                factors.material(&mut textures, &mut images),
                reflected_meshes.contains(&mesh_id),
            );
            if let Some(binding) = part.embedded_texture.take() {
                for handle in [&mat.front].into_iter().chain(mat.reflected.as_ref()) {
                    let image = compressed.register(&binding, factors.texture_cutoff(), handle, &images);
                    materials.get_mut(handle).expect("new material").base_color_texture = Some(image);
                }
            }
            let distant = if part.facade_glass && !run.full_detail {
                window_faces(part).map(|idx| meshes.add(shape.clone().with_inserted_indices(compact_indices(&idx))))
            } else {
                None
            };
            parts.push((meshes.add(shape), mat, distant));
        }
        models.push(parts);
    }
    compressed.start();
    let mut owners = Vec::new();
    let variants: Vec<Vec<_>> = overrides
        .variants
        .iter()
        .enumerate()
        .map(|(variant, colors)| {
            colors
                .iter()
                .map(|m| {
                    FaceMaterials::new(
                        &mut materials,
                        m.material(&mut textures, &mut images),
                        reflected_variants.contains(&variant),
                    )
                })
                .collect()
        })
        .collect();
    if let Some(context) = &scene.object {
        for (node, local) in context.profile.nodes.iter().zip(&context.render_locals) {
            let mut entity = cmd.spawn((Name::new(node.geometry.id.clone()), *local, Visibility::Inherited));
            if let Some(parent) = node.parent {
                entity.insert(ChildOf(owners[parent]));
            }
            owners.push(entity.id());
        }
    }
    for (index, inst) in scene.instances.iter().enumerate() {
        if let Some(context) = &scene.object {
            let owner = context.visual_nodes[index];
            if let Some(detail) = context.detail.get(&owner) {
                let mut groups = Vec::with_capacity(detail.levels.len());
                for (level, choice) in detail.levels.iter().enumerate() {
                    let visible = if level == 0 { Visibility::Inherited } else { Visibility::Hidden };
                    let group = cmd.spawn((ChildOf(owners[owner]), Transform::IDENTITY, visible)).id();
                    for (mesh, material, _) in &models[choice.mesh] {
                        cmd.spawn((ChildOf(group), Mesh3d(mesh.clone()), MeshMaterial3d(material.front.clone())));
                    }
                    groups.push(group);
                }
                cmd.entity(owners[owner]).insert(ObjectDetail { owner, current: 0, groups });
                continue;
            }
        }
        for (part, (mesh, mat, distant)) in models[inst.mesh].iter().enumerate() {
            let mat = overrides
                .assignments
                .get(&index)
                .map_or(mat, |&v| &variants[v][part])
                .at(reflected_draws.contains(&index));
            let mut entity = cmd.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone())));
            if let Some(context) = &scene.object {
                entity.insert((ChildOf(owners[context.visual_nodes[index]]), Transform::IDENTITY));
            } else {
                entity
                    .insert((Placement(inst.transform), Transform::from_translation(inst.transform.w_axis.truncate())));
            }
            if let Some(flat) = distant {
                let switch_m = 200.
                    + Vec3::new(
                        inst.transform.x_axis.truncate().length(),
                        inst.transform.y_axis.truncate().length(),
                        inst.transform.z_axis.truncate().length(),
                    )
                    .length()
                        / 2.;
                entity.insert(WindowDetail {
                    full: mesh.clone(),
                    flat: flat.clone(),
                    distance_squared: switch_m * switch_m,
                });
            }
        }
    }
    for draw in actor_draws(&run.model, &mut meshes, &mut materials, &mut textures, &mut images) {
        cmd.spawn((
            BodyMarker::Limb(draw.part),
            Mesh3d(draw.mesh),
            MeshMaterial3d(draw.material),
            run.actor.transform(draw.part),
        ));
    }
    if r.routes.iter().any(|route| route.moving_surface().is_some()) {
        cmd.spawn((BodyMarker::MovingSurface, moving_box_draw(&mut meshes, &mut materials), Visibility::Hidden));
    }
    let (eye, aim) = r.routes[run.route].camera_at(run.tick);
    cmd.spawn((
        OsageCamera,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: r.vertical_fov_deg.to_radians(),
            near: 0.1,
            far: 5000.,
            ..default()
        }),
        Transform::from_translation(eye).looking_at(aim, Vec3::Y),
    ));
    cmd.spawn((
        Sun,
        DirectionalLight { shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(-480., 730., 350.).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

#[allow(clippy::too_many_arguments, reason = "Bevy injects input, clocks, the live sound device and world state")]
fn drive(
    mut run: ResMut<Run>,
    r: Res<Replay>,
    s: Res<Sector>,
    grid: Res<Grid>,
    time: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut exit: MessageWriter<AppExit>,
    mut sound: Option<NonSendMut<Live>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
        return;
    }
    for (i, k) in [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3].into_iter().enumerate() {
        if i < r.routes.len() && keys.just_pressed(k) {
            if let Some(out) = sound.as_deref_mut() {
                out.reset();
            }
            if let Err(e) = run.set(&r, &s, i) {
                error!("{e}");
                exit.write(AppExit::error());
            }
            return;
        }
    }
    if keys.just_pressed(KeyCode::KeyR) {
        let i = run.route;
        if let Some(out) = sound.as_deref_mut() {
            out.reset();
        }
        if let Err(e) = run.set(&r, &s, i) {
            error!("{e}");
            exit.write(AppExit::error());
        }
        return;
    }
    if keys.just_pressed(KeyCode::Space) {
        run.paused = !run.paused;
        if let Some(out) = sound.as_deref() {
            out.pause(run.paused);
        }
    }
    if !run.paused {
        let n = run.clock.advance(time.delta().as_nanos(), r.tick_hz);
        if let Err(e) = run.advance(n, &r, &s, &grid, sound.as_deref_mut()) {
            error!("{e}");
            exit.write(AppExit::error());
        }
    }
}

fn pose(
    run: Res<Run>,
    r: Res<Replay>,
    mut cam: Single<&mut Transform, With<OsageCamera>>,
    mut markers: Query<(&BodyMarker, &mut Transform, &mut Visibility), Without<OsageCamera>>,
) {
    let (eye, aim) = r.routes[run.route].camera_at(run.tick);
    **cam = Transform::from_translation(eye).looking_at(aim, Vec3::Y);
    for (part, mut marker, mut visibility) in &mut markers {
        match part {
            BodyMarker::Limb(i) => *marker = run.actor.transform(*i),
            BodyMarker::MovingSurface => {
                if let Some(transform) = run.actor.moving_surface_transform(&r.routes[run.route]) {
                    *marker = transform;
                    *visibility = Visibility::Inherited;
                } else {
                    *visibility = Visibility::Hidden;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments, reason = "Bevy supplies the independent capture resources")]
fn capture(
    mut cmd: Commands,
    run: Res<Run>,
    scene: Res<Sector>,
    detail: Query<&ObjectDetail>,
    visible: Query<&Visibility>,
    picture: Option<Res<Picture>>,
    mut frames: Local<u32>,
    compressed: Res<crate::compressed_texture::Queue>,
) {
    if compressed.pending() {
        return;
    }
    let (Some(file), Some(picture)) = (&run.shot, picture) else { return };
    *frames += 1;
    if *frames == 80 {
        if let Some(context) = &scene.object
            && !context.detail.is_empty()
        {
            let mut selected = 0;
            let mut triangles = 0;
            for instance in &detail {
                for (&group, level) in instance.groups.iter().zip(&context.detail[&instance.owner].levels) {
                    if visible.get(group).is_ok_and(|v| *v != Visibility::Hidden) {
                        selected += 1;
                        triangles += level.triangles;
                    }
                }
            }
            info!("adaptive object meshes at capture: {selected}, triangles: {triangles}");
        }
        let path = file.clone();
        cmd.spawn(Screenshot::image(picture.image.clone())).observe(
            move |shot: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                let saved = shot
                    .image
                    .clone()
                    .try_into_dynamic()
                    .ok()
                    .and_then(|img| img.save(&path).map_err(|e| error!(%e,"image write failed")).ok());
                exit.write(if saved.is_some() { AppExit::Success } else { AppExit::error() });
            },
        );
    }
}

/// A completed offscreen frame is the unit. Take both timestamps on the main
/// thread, with render pipelining off and the preceding GPU submission finished.
#[derive(Resource)]
struct Timed {
    path: PathBuf,
    inputs: serde_json::Value,
    warmup: u32,
    last: Option<std::time::Instant>,
    samples: Vec<(u32, u64)>,
}

fn wait_for_gpu(device: Res<bevy::render::renderer::RenderDevice>) {
    use bevy::render::render_resource::PollType;
    device
        .poll(PollType::Wait { submission_index: None, timeout: Some(Duration::from_secs(30)) })
        .expect("the GPU did not finish the frame");
}

#[allow(clippy::too_many_arguments)]
fn measure(
    mut t: ResMut<Timed>,
    mut run: ResMut<Run>,
    r: Res<Replay>,
    scene: Res<Sector>,
    grid: Res<Grid>,
    picture: Res<Picture>,
    adapter: Res<bevy::render::renderer::RenderAdapterInfo>,
    mut exit: MessageWriter<AppExit>,
    compressed: Res<crate::compressed_texture::Queue>,
) {
    const STRIDE: u32 = 6;
    if compressed.pending() {
        return;
    }
    if t.warmup > 0 {
        t.warmup -= 1;
        return;
    }
    let now = std::time::Instant::now();
    let Some(prev) = t.last.replace(now) else { return };
    t.samples.push((run.tick, (now - prev).as_nanos() as u64));
    let route = &r.routes[run.route];
    if run.tick < route.ticks {
        if let Err(e) = run.advance(STRIDE, &r, &scene, &grid, None) {
            error!("{e}");
            exit.write(AppExit::error());
        }
        return;
    }
    let distribution = crate::bench::distribution(t.samples.iter().map(|s| s.1).collect());
    let data = serde_json::json!({
        "format":"ville-paris/render/1","inputs":t.inputs,"physics":run.physics.to_string(),"actor":run.model.to_string(),"route":route.id,"viewport":picture.size.to_array(),"tier":"T2",
        "upscaling":false,"sync_pipelines":true,"distant_window_faces":!run.full_detail,"vertical_fov_deg":r.vertical_fov_deg,"gpu":adapter.name,"driver":adapter.driver_info,
        "backend":format!("{:?}",adapter.backend),"warmup_frames":60,"stride_ticks":STRIDE,"tick_hz":r.tick_hz,
        "metric":"wall nanoseconds per completed offscreen frame, one GPU fence per frame","ns":distribution,
        "tick_and_ns":t.samples
    });
    let written = serde_json::to_vec_pretty(&data)
        .map_err(std::io::Error::other)
        .and_then(|bytes| std::fs::write(&t.path, bytes));
    if let Err(e) = written {
        error!(%e,"frame receipt write failed");
        exit.write(AppExit::error());
    } else {
        println!("{} frames, {} ns at p95", t.samples.len(), data["ns"]["p95"]);
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_index_packing_preserves_high_indices_and_facade_geometry() {
        assert!(matches!(compact_indices(&[0, 32, 65_534]), Indices::U16(_)));
        for high in [65_535, 65_536] {
            let packed = compact_indices(&[0, high, 1]);
            assert!(matches!(packed, Indices::U32(_)));
            assert_eq!(packed.iter().collect::<Vec<_>>(), [0, high as usize, 1]);
        }
        let city = Sector::read(crate::scene::DEFAULT).unwrap();
        for p in city.meshes.iter().flatten().filter(|p| p.facade_glass) {
            let mesh = render_primitive(p);
            assert!(matches!(mesh.indices(), Some(Indices::U16(_))));
            assert!(mesh.indices().unwrap().iter().eq(p.indices.iter().map(|&i| i as usize)));
            let far = window_faces(p).unwrap();
            assert!(matches!(compact_indices(&far), Indices::U16(_)));
        }
    }

    #[test]
    fn distant_facades_retain_the_outside_of_every_pane() {
        let sector = Sector::read(crate::scene::DEFAULT).unwrap();
        let glass: Vec<_> = sector.meshes.iter().flatten().filter(|p| p.facade_glass).collect();
        assert_eq!(glass.len(), 3);
        for part in glass {
            let flat = window_faces(part).unwrap();
            // Each authored window box has six faces with one visible outer pane.
            assert_eq!(flat.len() * 6, part.indices.len());
        }
    }
}
