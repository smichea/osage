// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{
    models::{Draw, Library, Page},
    tiles::{Index, Visual},
    visibility,
};
use crate::{
    people::Model,
    scene::Primitive,
    view::{self, MaterialFactors, Placement},
};
use anyhow::{Result, ensure};
use bevy::{
    app::PluginsState,
    mesh::Indices,
    prelude::*,
    render::{
        RenderApp,
        mesh::{RenderMesh, RenderMeshBufferInfo, allocator::MeshAllocator},
        render_asset::RenderAssets,
        render_resource::{IndexFormat, PollType},
        renderer::{RenderAdapterInfo, RenderDevice},
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
    transform::TransformSystems,
    window::ExitCondition,
    winit::WinitPlugin,
};
use osage_render::{
    OsageCamera, OsageRenderPlugin, Sun, Technique, Tier,
    resolution::{Output, Picture},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MeshGroup {
    Coarse(usize),
    Local(usize, usize),
    Shared(usize),
}
type MeshKey = (MeshGroup, usize);
type MaterialKey = ([u32; 6], u8, u32, Option<bool>, Option<crate::texture::TextureKey>);

fn material_key(factors: &MaterialFactors, reflected: bool) -> MaterialKey {
    let [r, g, b, a] = factors.color;
    (
        [r, g, b, a, factors.metallic, factors.roughness].map(f32::to_bits),
        factors.style.alpha_mode as u8,
        if factors.style.alpha_mode == crate::scene::SurfaceAlpha::Mask {
            factors.style.alpha_cutoff.to_bits()
        } else {
            0
        },
        (!factors.style.double_sided).then_some(reflected),
        factors.texture.as_ref().map(|t| t.key(factors.texture_cutoff())),
    )
}

struct Asset {
    handle: Handle<Mesh>,
    refs: usize,
    payload_bytes: u64,
    bounds: visibility::Bounds,
    box_mesh: bool,
}
#[derive(PartialEq, Eq)]
struct BindingKey {
    source_slot: usize,
    mesh: MeshKey,
    material: MaterialKey,
}

impl BindingKey {
    fn order(&self) -> (usize, usize) {
        (self.source_slot, self.mesh.1)
    }
}
struct Binding {
    piece: visibility::Piece,
    key: BindingKey,
}
struct Tile {
    fine: bool,
    revision: u64,
    draws: Vec<Binding>,
    owners: usize,
    shared_owners: usize,
}

pub struct Live {
    app: App,
    camera: Entity,
    actor: Vec<(usize, Entity)>,
    moving_box: Option<(Entity, [f32; 3])>,
    source_nodes: Vec<Entity>,
    meshes: BTreeMap<MeshKey, Asset>,
    materials: BTreeMap<MaterialKey, (Handle<StandardMaterial>, usize)>,
    images: crate::texture::Images,
    tiles: BTreeMap<usize, Tile>,
    samples: Vec<Value>,
    uploads: u64,
    uploaded_bytes: u64,
    freed: u64,
    retired: Vec<(AssetId<Mesh>, bool)>,
    initial_memory: Value,
    compact_indices: bool,
    reuse_bindings: bool,
    spawned_bindings: u64,
    retired_bindings: u64,
    reused_bindings: u64,
    publication_ns: u64,
}

fn memory(w: &World) -> Value {
    let dev = w.resource::<RenderDevice>();
    let alloc = w.resource::<MeshAllocator>();
    let report = dev.wgpu_device().generate_allocator_report();
    json!({"mesh_slab_count":alloc.slab_count(),"mesh_slab_capacity_bytes":alloc.slabs_size(),
        "named_base_color_allocations":report.as_ref().map(|r|r.allocations.iter().filter(|a|a.name == crate::texture::PAGED_LABEL).map(|a|a.size).collect::<Vec<_>>()),
        "device_allocated_bytes":report.as_ref().map(|r|r.total_allocated_bytes),
        "device_reserved_bytes":report.as_ref().map(|r|r.total_reserved_bytes)})
}

impl Live {
    pub fn bc7_limit(&self) -> Option<u32> {
        let w = self.app.world();
        crate::compressed_texture::bc7_limit(
            w.resource::<RenderDevice>(),
            w.resource::<bevy::render::renderer::RenderAdapter>(),
        )
    }

    pub fn new(
        size: [u32; 2],
        fov: f32,
        compact_indices: bool,
        reuse_bindings: bool,
        index: &Index,
        groups: visibility::Settings,
        model: &Model,
    ) -> Result<Self> {
        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
                .set(crate::compressed_texture::renderer(true))
                .disable::<WinitPlugin>()
                .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>(),
        )
        .insert_resource(Output(Some(UVec2::from_array(size))))
        .add_plugins(OsageRenderPlugin { tier: Tier::T2, without: vec![Technique::Upscaling] })
        .add_systems(
            PostUpdate,
            view::preserve_affines
                .after(TransformSystems::Propagate)
                .before(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
        );
        if groups.watch.is_some() || groups.withhold.is_some() {
            app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin);
        }
        app.insert_resource(groups)
            .init_resource::<visibility::Scene>()
            .init_resource::<visibility::Stats>()
            .add_systems(
                PostUpdate,
                visibility::cull.after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
            );
        let deadline = Instant::now() + Duration::from_secs(60);
        while app.plugins_state() == PluginsState::Adding {
            bevy::tasks::tick_global_task_pools_on_main_thread();
            ensure!(Instant::now() < deadline, "render plugins did not become ready");
            std::thread::yield_now();
        }
        app.finish();
        app.cleanup();
        let w = app.world_mut();
        // Cooked draws already carry world affines. Identity owner entities preserve the
        // original structural parentage without applying any source transform a second time.
        let source_nodes: Vec<_> = index
            .source_parents
            .iter()
            .enumerate()
            .map(|(node, _)| {
                w.spawn((Name::new(format!("source node {node}")), Transform::IDENTITY, Visibility::Inherited)).id()
            })
            .collect();
        for (node, parent) in index.source_parents.iter().enumerate() {
            if let Some(parent) = parent {
                w.entity_mut(source_nodes[node]).insert(ChildOf(source_nodes[*parent]));
            }
        }
        let camera = w
            .spawn((
                OsageCamera,
                Camera3d::default(),
                Projection::Perspective(PerspectiveProjection {
                    fov: fov.to_radians(),
                    near: 0.1,
                    far: 5000.,
                    ..default()
                }),
                Transform::from_xyz(0., 100., 10.).looking_at(Vec3::ZERO, Vec3::Y),
            ))
            .id();
        w.spawn((
            Sun,
            DirectionalLight { shadow_maps_enabled: true, ..default() },
            Transform::from_xyz(-480., 730., 350.).looking_at(Vec3::ZERO, Vec3::Y),
        ));
        let mut actor_textures = crate::texture::Images::new();
        let mut assets = bevy::ecs::system::SystemState::<(
            ResMut<Assets<Mesh>>,
            ResMut<Assets<StandardMaterial>>,
            ResMut<Assets<Image>>,
        )>::new(w);
        let (mut actor_meshes, mut actor_materials, mut actor_images) = assets.get_mut(w)?;
        let actor_draws =
            view::actor_draws(model, &mut actor_meshes, &mut actor_materials, &mut actor_textures, &mut actor_images);
        let actor = actor_draws
            .into_iter()
            .map(|draw| {
                let entity = w.spawn((Mesh3d(draw.mesh), MeshMaterial3d(draw.material), Transform::IDENTITY)).id();
                (draw.part, entity)
            })
            .collect();
        for _ in 0..4 {
            app.update();
        }
        let initial_memory = memory(app.sub_app(RenderApp).world());
        Ok(Self {
            app,
            camera,
            actor,
            moving_box: None,
            source_nodes,
            meshes: BTreeMap::new(),
            materials: BTreeMap::new(),
            images: Default::default(),
            tiles: BTreeMap::new(),
            samples: vec![],
            uploads: 0,
            uploaded_bytes: 0,
            freed: 0,
            retired: vec![],
            initial_memory,
            compact_indices,
            reuse_bindings,
            spawned_bindings: 0,
            retired_bindings: 0,
            reused_bindings: 0,
            publication_ns: 0,
        })
    }

    fn spawn(
        &mut self,
        key: BindingKey,
        pose: Mat4,
        p: &Primitive,
        factors: MaterialFactors,
        owner: Option<usize>,
    ) -> Binding {
        let asset = self.meshes.entry(key.mesh).or_insert_with(|| {
            let indices =
                if self.compact_indices { view::compact_indices(&p.indices) } else { Indices::U32(p.indices.clone()) };
            let index_bytes = match &indices {
                Indices::U16(_) => 2,
                Indices::U32(_) => 4,
            };
            let mesh = view::render_primitive_with_indices(p, indices);
            let bytes = (p.positions.len()
                * (24 + usize::from(p.colors.is_some()) * 16 + usize::from(p.uv.is_some()) * 8)
                + p.indices.len() * index_bytes) as u64;
            let handle = self.app.world_mut().resource_mut::<Assets<Mesh>>().add(mesh);
            self.uploads += 1;
            self.uploaded_bytes += bytes;
            let bounds = visibility::Bounds::from_mesh(p);
            Asset { handle, refs: 0, payload_bytes: bytes, bounds, box_mesh: visibility::solid_box(p, bounds) }
        });
        asset.refs += 1;
        let (material, refs) = self.materials.entry(key.material).or_insert_with(|| {
            let w = self.app.world_mut();
            let mut material = factors.material(&mut self.images, &mut w.resource_mut::<Assets<Image>>());
            if key.material.3 == Some(true) {
                material.cull_mode = Some(bevy::render::render_resource::Face::Front);
            }
            (w.resource_mut::<Assets<StandardMaterial>>().add(material), 0)
        });
        *refs += 1;
        let entity = self
            .app
            .world_mut()
            .spawn((
                Placement(pose),
                Transform::from_translation(pose.w_axis.truncate()),
                Mesh3d(asset.handle.clone()),
                MeshMaterial3d(material.clone()),
            ))
            .id();
        if let Some(entity_owner) = owner.and_then(|i| self.source_nodes.get(i)) {
            self.app.world_mut().entity_mut(entity).insert(ChildOf(*entity_owner));
        }
        let bounds = asset.bounds.world(pose);
        let piece = visibility::Piece {
            entity,
            owner,
            bounds,
            indices: p.indices.len(),
            blocker: asset.box_mesh && factors.fills_box() && visibility::axial(pose) && bounds.building(),
        };
        self.spawned_bindings += 1;
        Binding { piece, key }
    }

    fn release(&mut self, bindings: impl IntoIterator<Item = Binding>) {
        for b in bindings {
            self.retired_bindings += 1;
            self.app.world_mut().despawn(b.piece.entity);
            let asset = self.meshes.get_mut(&b.key.mesh).unwrap();
            asset.refs -= 1;
            if asset.refs == 0 {
                let id = asset.handle.id();
                self.app.world_mut().resource_mut::<Assets<Mesh>>().remove(id);
                self.retired.push((id, matches!(b.key.mesh.0, MeshGroup::Shared(_))));
                self.meshes.remove(&b.key.mesh);
                self.freed += 1;
            }
            let (h, n) = self.materials.get_mut(&b.key.material).unwrap();
            *n -= 1;
            if *n == 0 {
                self.app.world_mut().resource_mut::<Assets<StandardMaterial>>().remove(h.id());
                self.materials.remove(&b.key.material);
            }
        }
    }

    pub fn publish(
        &mut self,
        idx: &Index,
        parent: &Visual,
        fine: &BTreeMap<usize, Page>,
        library: &Library,
    ) -> Result<()> {
        let start = Instant::now();
        let mut changed = Vec::new();
        let mut old = vec![];
        let mut retired = vec![];
        for (t, tile) in idx.tiles.iter().enumerate() {
            let page = fine.get(&t);
            let revision = page.map_or(0, |p| p.revision);
            if self.tiles.get(&t).is_some_and(|r| r.fine == page.is_some() && r.revision == revision) {
                continue;
            }
            changed.push(t);
            // Fine placements occupy immutable source slots. A change of LOD on one
            // owner can leave the other owners in this same page entirely untouched.
            let previous = match self.tiles.remove(&t) {
                Some(prev) if self.reuse_bindings && prev.fine && page.is_some() && !tile.visual.lods.is_empty() => {
                    prev.draws
                }
                Some(prev) => {
                    old.push(prev);
                    Vec::new()
                }
                None => Vec::new(),
            };
            let mut previous = previous.into_iter().peekable();
            let mut next = Tile { fine: page.is_some(), revision, draws: vec![], owners: 0, shared_owners: 0 };
            let mut draw = |source_slot: usize,
                            mesh: MeshGroup,
                            parts: &[Primitive],
                            pose: Mat4,
                            variant: Option<usize>,
                            owner: Option<usize>|
             -> Result<()> {
                next.owners += 1;
                next.shared_owners += usize::from(matches!(mesh, MeshGroup::Shared(_)));
                for (slot, p) in parts.iter().enumerate() {
                    let factors = if let Some(i) = variant {
                        library.material(idx, idx.model_variants[i].materials[slot])?
                    } else {
                        MaterialFactors::from_primitive(p)
                    };
                    anyhow::ensure!(factors.texture.is_none() || p.uv.is_some(), "shared texture needs TEXCOORD_0");
                    let key = BindingKey {
                        source_slot,
                        mesh: (mesh, slot),
                        material: material_key(&factors, pose.as_dmat4().determinant() < 0.),
                    };
                    while let Some(missing) = previous.next_if(|b| b.key.order() < key.order()) {
                        retired.push(missing);
                    }
                    if let Some(before) = previous.next_if(|b| b.key.order() == key.order()) {
                        if before.key == key
                            && before.piece.owner == owner
                            && self.app.world().get::<Placement>(before.piece.entity).is_some_and(|p| p.0 == pose)
                        {
                            next.draws.push(before);
                            self.reused_bindings += 1;
                            continue;
                        }
                        retired.push(before);
                    }
                    next.draws.push(self.spawn(key, pose, p, factors, owner));
                }
                Ok(())
            };
            if let Some(page) = page {
                for (ordinal, item) in page.draws.iter().enumerate() {
                    let owner = page.owners.get(ordinal).copied();
                    match item {
                        Draw::Hidden => continue,
                        Draw::Local(j) => {
                            let s = &page.local.as_ref().ok_or_else(|| anyhow::anyhow!("fine mesh unavailable"))?.scene;
                            let i = &s.instances[*j];
                            draw(ordinal, MeshGroup::Local(t, i.mesh), &s.meshes[i.mesh], i.transform, None, owner)?;
                        }
                        Draw::Shared(i, pose) => {
                            let v = idx.model_variants.get(*i);
                            let key = v.map_or(*i, |a| a.resource);
                            let parts = library.models[key]
                                .as_deref()
                                .ok_or_else(|| anyhow::anyhow!("shared GPU source unavailable"))?;
                            draw(ordinal, MeshGroup::Shared(key), parts, *pose, v.map(|_| *i), owner)?;
                        }
                    }
                }
            }
            for n in tile.parent_first..tile.parent_first + tile.parent_count {
                if page.is_none_or(|p| p.needs_parent(parent.owners.get(n))) {
                    let i = &parent.scene.instances[n];
                    draw(
                        page.map_or(0, |p| p.draws.len()) + (n - tile.parent_first),
                        MeshGroup::Coarse(i.mesh),
                        &parent.scene.meshes[i.mesh],
                        i.transform,
                        None,
                        parent.owners.get(n).copied(),
                    )?;
                }
            }
            retired.extend(previous);
            self.tiles.insert(t, next);
        }
        // A single frame can move the last placement from one cell into another.
        // Acquire all new placements before dropping old references to the same mesh.
        for tile in old {
            self.release(tile.draws);
        }
        self.release(retired);
        if !changed.is_empty() {
            if !self.images.is_empty() {
                let bound: BTreeSet<_> = self.materials.keys().filter_map(|key| key.4).collect();
                self.images.retain(|k, _| bound.contains(k));
            }
            let world = self.app.world_mut();
            let full = world.resource::<visibility::Settings>().full_index;
            world
                .resource_mut::<visibility::Scene>()
                .update(idx.tiles.len(), &changed, full, |t| self.tiles[&t].draws.iter().map(|draw| draw.piece));
        }
        self.publication_ns = start.elapsed().as_nanos() as u64;
        Ok(())
    }

    pub fn add_moving_box(&mut self, half_extents_m: [f32; 3]) {
        let world = self.app.world_mut();
        let draw = world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            view::moving_box_draw(&mut meshes, &mut world.resource_mut::<Assets<StandardMaterial>>())
        });
        let entity = world.spawn((draw, Transform::IDENTITY)).id();
        self.moving_box = Some((entity, half_extents_m));
    }

    pub fn frame(
        &mut self,
        tick: u32,
        eye: Vec3,
        aim: Vec3,
        poses: &[osage_dynamics::Pose],
        box_pose: Option<&osage_dynamics::Pose>,
    ) -> Result<()> {
        let w = self.app.world_mut();
        w.entity_mut(self.camera).insert(Transform::from_translation(eye).looking_at(aim, Vec3::Y));
        for &(part, entity) in &self.actor {
            let p = poses.get(part).ok_or_else(|| anyhow::anyhow!("actor drawable references missing body {part}"))?;
            w.entity_mut(entity)
                .insert(Transform::from_translation(p.center.into()).with_rotation(Quat::from_array(p.rotation_xyzw)));
        }
        if let Some((entity, extents)) = self.moving_box {
            let pose = box_pose.ok_or_else(|| anyhow::anyhow!("missing moving box pose"))?;
            w.entity_mut(entity).insert(crate::physics::box_transform(pose, extents));
        }
        let start = Instant::now();
        self.app.update();
        let w = self.app.sub_app(RenderApp).world();
        let dev = w.resource::<RenderDevice>();
        dev.poll(PollType::Wait { submission_index: None, timeout: Some(Duration::from_secs(30)) })?;
        let elapsed = start.elapsed().as_nanos() as u64;
        let app = self.app.world();
        let cam = app.get::<bevy::render::sync_world::RenderEntity>(self.camera);
        let extracted = cam
            .and_then(|id| w.get::<bevy::render::view::RenderVisibleEntities>(id.id()))
            .and_then(|v| v.get::<Mesh3d>())
            .map(|c| c.iter_visible().count());
        let frustum = app.get::<bevy::camera::primitives::Frustum>(self.camera).is_some();
        let hierarchical_z = app.get::<bevy::render::occlusion_culling::OcclusionCulling>(self.camera).is_some();
        let visibility = &app.resource::<visibility::Stats>().last;
        // Hardware counters are returned asynchronously by Bevy. The matched measurements
        // hold each camera still for several frames before reading the latest completed pass.
        let opaque: BTreeMap<_, _> = app
            .resource::<bevy::diagnostic::DiagnosticsStore>()
            .iter()
            .filter_map(|d| {
                let path = d.path().as_str();
                let (_, metric) = path.rsplit_once("/main_opaque_pass_3d/")?;
                Some((metric, d.value()?))
            })
            .collect();
        let allocator = w.resource::<MeshAllocator>();
        let gpu = w.resource::<RenderAssets<RenderMesh>>();
        let mut vertices = 0u64;
        let mut indices = 0u64;
        let mut logical = 0u64;
        let mut u16_primitives = 0;
        let mut ready = 0;
        let mut shared = 0;
        let mut shared_v = 0;
        let mut shared_i = 0;
        let mut vertex_buffers = BTreeSet::new();
        let mut index_buffers = BTreeSet::new();
        for (key, a) in &self.meshes {
            let id = a.handle.id();
            let (Some(m), Some(v), Some(ix)) =
                (gpu.get(id), allocator.mesh_vertex_slice(&id), allocator.mesh_index_slice(&id))
            else {
                continue;
            };
            let RenderMeshBufferInfo::Indexed { count, index_format } = m.buffer_info else {
                anyhow::bail!("unindexed paged mesh")
            };
            let stride = m.layout.0.layout().array_stride;
            let index_stride = if index_format == IndexFormat::Uint16 {
                u16_primitives += 1;
                2u64
            } else {
                4u64
            };
            let vb = u64::from(v.range.end - v.range.start) * stride;
            let ib = u64::from(ix.range.end - ix.range.start) * index_stride;
            vertices += vb;
            indices += ib;
            logical += u64::from(m.vertex_count) * stride + u64::from(count) * index_stride;
            vertex_buffers.insert(v.buffer.id());
            index_buffers.insert(ix.buffer.id());
            if matches!(key.0, MeshGroup::Shared(_)) {
                shared += 1;
                shared_v += vb;
                shared_i += ib;
            }
            ready += 1;
        }
        // Removed main-world handles may outlive their owners in the render world. Retain
        // each retired identity until all three independent renderer lookups return absent.
        self.retired.retain(|(id, _)| {
            gpu.get(*id).is_some()
                || allocator.mesh_vertex_slice(id).is_some()
                || allocator.mesh_index_slice(id).is_some()
        });
        let expected: u64 = self.meshes.values().map(|a| a.payload_bytes).sum();
        ensure!(
            ready != self.meshes.len() || logical == expected,
            "rendered mesh layout differs from the submitted geometry"
        );
        let gpu_images = w.resource::<RenderAssets<bevy::render::texture::GpuImage>>();
        let mut texture_bytes = 0u64;
        let mut ready_images = 0;
        let mut texture_formats = BTreeMap::<String, usize>::new();
        for handle in self.images.values() {
            if let Some(image) = gpu_images.get(handle.id()) {
                ready_images += 1;
                let d = &image.texture_descriptor;
                texture_bytes +=
                    crate::compressed_texture::payload_bytes(d.format, d.size.width, d.size.height, d.mip_level_count);
                *texture_formats.entry(format!("{:?}", d.format)).or_default() += 1;
            }
        }
        self.samples.push(json!({"tick":tick,"actor_bodies":poses.len(),"actor_draw_primitives":self.actor.len(),"texture_assets":self.images.len(),"prepared_textures":ready_images,"texture_payload_bytes":texture_bytes,"texture_formats":texture_formats,"geometry_assets":self.meshes.len(),"prepared_assets":ready,"group_visibility":visibility,"latest_completed_opaque_pass":opaque,"render_world_camera_meshes":extracted,
            "frustum_component":frustum,"bevy_hierarchical_z_component":hierarchical_z,"u16_primitives":u16_primitives,"u32_primitives":ready-u16_primitives,
            "retired_gpu_ids_pending":self.retired.len(),"retired_shared_gpu_ids_pending":self.retired.iter().filter(|(_,s)|*s).count(),
            "camera_eye":eye.to_array(),"camera_aim":aim.to_array(),
            "shared_primitives":shared,"shared_vertex_slice_bytes":shared_v,"shared_index_slice_bytes":shared_i,
            "vertex_slice_bytes":vertices,"index_slice_bytes":indices,"drawable_payload_bytes":logical,
            "vertex_buffers":vertex_buffers.len(),"index_buffers":index_buffers.len(),
            "draw_owners":self.tiles.values().map(|t|t.owners).sum::<usize>(),"shared_draw_owners":self.tiles.values().map(|t|t.shared_owners).sum::<usize>(),
            "fine_cells":self.tiles.values().filter(|t|t.fine).count(),
            "placement_entities_spawned":self.spawned_bindings,"placement_entities_retired":self.retired_bindings,
            "placement_entities_reused":self.reused_bindings,"placement_publication_ns":self.publication_ns,
            "mesh_uploads":self.uploads,"mesh_payload_uploaded":self.uploaded_bytes,"evicted_assets":self.freed,
            "memory":memory(w),"wall_to_finished_gpu_frame_ns":elapsed}));
        Ok(())
    }

    pub fn capture(&mut self, path: &Path) -> Result<()> {
        let target = self.app.world().resource::<Picture>().image.clone();
        let path = path.to_owned();
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.app.world_mut().spawn(Screenshot::image(target)).observe(move |frame: On<ScreenshotCaptured>| {
            let r = frame
                .image
                .clone()
                .try_into_dynamic()
                .map_err(|e| e.to_string())
                .and_then(|img| img.save(&path).map_err(|e| e.to_string()));
            let _ = tx.send(r);
        });
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            self.app.update();
            if let Ok(r) = rx.try_recv() {
                return r.map_err(anyhow::Error::msg);
            }
        }
        anyhow::bail!("timed out waiting for the rendered frame")
    }

    pub fn report(&self) -> Value {
        let adapter = self.app.world().resource::<RenderAdapterInfo>();
        json!({"format":"ville-paris/paged-gpu/1","adapter":adapter.name,"driver":adapter.driver_info,
            "backend":format!("{:?}",adapter.backend),"source_tree_nodes":self.source_nodes.len(),"warmup_frames":4,"gpu_indices":if self.compact_indices {"compact"}else{"u32"},"gpu_bindings":if self.reuse_bindings {"stable"}else{"rebuild"},"baseline_memory":self.initial_memory,"frames":self.samples})
    }
}
