// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

mod acoustic;
mod rays;
pub(crate) mod rooms;
pub(crate) use acoustic::Crossing;

use crate::scene::{Instance, Primitive, Sector};
use anyhow::{Context, Result, ensure};
use bevy::prelude::{Mat4, Transform, Vec3};
use osage_dynamics::StaticMesh;
use osage_objects::{
    AcousticSource, ActiveGeometry, BAND_HZ, DetailView, GeometryLevels, ObjectProfile, VisualDetail, VisualLevel,
    VisualLod, maximum_stretch,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct ExteriorWedge {
    #[serde(skip)]
    pub shell: (usize, usize),
    pub tolerance_m: f64,
    pub normals: [[f64; 3]; 2],
    pub adjacent_surface: String,
    pub adjacent_owner_path: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct AcousticEdge {
    pub a: [f64; 3],
    pub b: [f64; 3],
    pub wedge: Option<ExteriorWedge>,
    pub id: String,
    pub owner_path: Vec<String>,
}

#[derive(Clone)]
enum AcousticMesh {
    Proxy(String),
    Owned(std::sync::Arc<StaticMesh>),
}

#[derive(Clone)]
pub(crate) struct AcousticBinding {
    owner_path: Vec<String>,
    transmission: Option<[f64; 3]>,
    owner: usize,
    id: String,
    material: String,
    source: AcousticSource,
    mesh: AcousticMesh,
    spatial: std::sync::Arc<acoustic::SurfaceIndex>,
}

#[derive(Default)]
pub(crate) struct AcousticGeometry {
    acoustics: Vec<AcousticBinding>,
    acoustic_index: std::sync::OnceLock<acoustic::AcousticIndex>,
    pub(crate) rooms: Option<std::sync::Arc<rooms::Graph>>,
}

impl AcousticGeometry {
    pub(crate) fn from_bindings(acoustics: Vec<AcousticBinding>) -> Self {
        Self { acoustics, ..Default::default() }
    }
}

impl AcousticBinding {
    pub(crate) fn resident(
        owner: usize,
        owner_path: Vec<String>,
        surface: &osage_objects::AcousticSurface,
        bands: Option<[f32; 3]>,
        mesh: std::sync::Arc<StaticMesh>,
    ) -> Self {
        Self {
            owner,
            owner_path,
            transmission: bands.map(|t| t.map(f64::from)),
            id: surface.id.clone(),
            source: surface.source,
            material: surface.material.clone(),
            mesh: AcousticMesh::Owned(mesh),
            spatial: Default::default(),
        }
    }
}

pub struct ObjectContext {
    pub(super) profile: ObjectProfile,
    pub(super) visual_nodes: Vec<usize>,
    pub(super) detail: BTreeMap<usize, VisualDetail>,
    pub(super) render_locals: Vec<Transform>,
    // Each runtime mesh retains its node and binding in the canonical object tree.
    physical_bindings: BTreeMap<String, (usize, usize)>,
    pub(crate) sound: AcousticGeometry,
    levels: GeometryLevels,
    active: Vec<ActiveGeometry>,
}

pub(super) fn world_mesh(id: &str, parts: &[Primitive], matrix: Mat4) -> Result<StaticMesh> {
    let mut mesh = StaticMesh { id: id.into(), vertices: Vec::new(), triangles: Vec::new(), material: None };
    let reflected = matrix.determinant() < 0.;
    for part in parts {
        let base = u32::try_from(mesh.vertices.len())?;
        u32::try_from(mesh.vertices.len().checked_add(part.positions.len()).context("too many vertices")?)?;
        mesh.vertices.extend(part.positions.iter().map(|p| matrix.transform_point3(Vec3::from(*p)).to_array()));
        for tri in part.indices.as_chunks::<3>().0 {
            let [a, b, c] = tri.map(|i| base + i);
            mesh.triangles.push(if reflected { [a, c, b] } else { [a, b, c] });
        }
    }
    ensure!(mesh.vertices.iter().flatten().all(|p| p.is_finite()), "non-finite surface {id}");
    Ok(mesh)
}

pub(super) fn bake(parts: &[Primitive], matrix: Mat4) -> Result<Vec<Primitive>> {
    let normal_matrix = matrix.inverse().transpose();
    let mut transformed = parts.to_vec();
    for part in &mut transformed {
        for p in &mut part.positions {
            *p = matrix.transform_point3(Vec3::from(*p)).to_array();
        }
        for n in &mut part.normals {
            *n = normal_matrix.transform_vector3(Vec3::from(*n)).normalize_or_zero().to_array();
        }
        ensure!(
            part.positions.iter().chain(&part.normals).flatten().all(|v| v.is_finite()),
            "non-finite visual geometry"
        );
        if matrix.determinant() < 0. {
            for tri in part.indices.as_chunks_mut::<3>().0 {
                tri.swap(1, 2);
            }
        }
    }
    Ok(transformed)
}

pub fn sector(
    source: Vec<Vec<Primitive>>,
    profile: ObjectProfile,
    levels: GeometryLevels,
    adaptive: bool,
) -> Result<Sector> {
    let mut meshes: Vec<Vec<Primitive>> = Vec::new();
    let mut shared = HashMap::new();
    let mut local_bounds = HashMap::new();
    let mut detail = BTreeMap::new();
    let mut render_locals = Vec::with_capacity(profile.nodes.len());
    let mut render_worlds = Vec::with_capacity(profile.nodes.len());
    let mut instances = Vec::new();
    let mut visual_nodes = Vec::new();
    let mut colliders = Vec::new();
    let mut physical_bindings = BTreeMap::new();
    let mut acoustics = Vec::new();
    let mut triangles = 0;
    let active = profile.active_geometry(levels);
    for (owner, node) in profile.nodes.iter().enumerate() {
        let world = node.world_from_local;
        let object = &node.geometry;
        let parent = node.parent.map_or(Mat4::IDENTITY, |i| render_worlds[i]);
        let local = if node.parent.is_none_or(|i| profile.nodes[i].world_from_local == parent) {
            node.parent_from_local
        } else {
            parent.inverse() * world
        };
        // Bevy composes local TRS into a full affine global matrix. A sheared or
        // reflected *local* frame needs a residual on the mesh, carried into the
        // local frames of children as well. Ordinary instances share the same mesh.
        let candidate = Transform::from_matrix(local);
        let exact = local.determinant() > 0. && candidate.to_matrix().abs_diff_eq(local, 1e-5);
        let frame = if exact { candidate } else { Transform::from_translation(local.w_axis.truncate()) };
        let render_world = parent * frame.to_matrix();
        render_locals.push(frame);
        render_worlds.push(render_world);
        if let Some(index) = node.visual_at(levels.visual).filter(|_| active[owner].visual) {
            let mut materialize = |index: usize| -> Result<usize> {
                if exact {
                    return Ok(*shared.entry(index).or_insert_with(|| {
                        meshes.push(source[index].clone());
                        meshes.len() - 1
                    }));
                }
                meshes.push(bake(&source[index], frame.to_matrix().inverse() * local)?);
                Ok(meshes.len() - 1)
            };
            let mesh = materialize(index)?;
            let triangle_count = |index: usize| source[index].iter().map(|p| p.indices.len() / 3).sum::<usize>();
            let primary_triangles = triangle_count(index);
            triangles += primary_triangles;
            if adaptive && levels.visual == VisualLod::High {
                let mut offered =
                    vec![VisualLevel { level: VisualLod::High, mesh, triangles: primary_triangles, error_m: 0. }];
                let mut mesh_bounds = |index: usize| {
                    *local_bounds.entry(index).or_insert_with(|| {
                        source[index]
                            .iter()
                            .flat_map(|part| &part.positions)
                            .map(|&p| Vec3::from(p))
                            .fold((Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)), |(lo, hi), p| {
                                (lo.min(p), hi.max(p))
                            })
                    })
                };
                let mut bounds = None;
                for lod in &object.visual_lods {
                    let Some(error) = lod.max_deviation_m else { continue };
                    let triangles = triangle_count(lod.mesh);
                    if triangles >= primary_triangles {
                        continue;
                    }
                    let (lo, hi) = bounds.get_or_insert_with(|| mesh_bounds(index));
                    let (alt_lo, alt_hi) = mesh_bounds(lod.mesh);
                    *lo = lo.min(alt_lo);
                    *hi = hi.max(alt_hi);
                    offered.push(VisualLevel {
                        level: lod.level,
                        mesh: materialize(lod.mesh)?,
                        triangles,
                        error_m: error * maximum_stretch(world),
                    });
                }
                if let Some((lo, hi)) = bounds {
                    let (center, half_extents) = VisualDetail::world_bounds(lo, hi, world);
                    detail.insert(owner, VisualDetail { center, half_extents, levels: offered });
                }
            }
            instances.push(Instance { mesh, transform: render_world });
            visual_nodes.push(owner);
        }
        for (binding, shape) in object.collision.iter().enumerate().filter(|_| active[owner].collision) {
            let mut mesh = world_mesh(&shape.id, &source[shape.mesh], world)?;
            mesh.material = Some(profile.header.collision_materials[&shape.material]);
            colliders.push(mesh);
            physical_bindings.insert(shape.id.clone(), (owner, binding));
        }
        for shape in object.acoustic.iter().filter(|_| active[owner].acoustic) {
            let mesh = match shape.source {
                AcousticSource::CollisionProxy if !active[owner].collision => {
                    AcousticMesh::Owned(world_mesh(&shape.id, &source[shape.mesh], world)?.into())
                }
                AcousticSource::Authored => {
                    AcousticMesh::Owned(world_mesh(&shape.id, &source[shape.mesh], world)?.into())
                }
                AcousticSource::CollisionProxy => {
                    let physical =
                        object.collision.iter().find(|s| s.mesh == shape.mesh).context("missing acoustic proxy")?;
                    AcousticMesh::Proxy(physical.id.clone())
                }
            };
            acoustics.push(AcousticBinding {
                owner_path: profile
                    .node_path(owner)
                    .context("missing acoustic node")?
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
                transmission: profile.header.acoustic_materials[&shape.material].transmission.map(|v| v.map(f64::from)),
                owner,
                id: shape.id.clone(),
                material: shape.material.clone(),
                source: shape.source,
                mesh,
                spatial: Default::default(),
            });
        }
    }
    colliders.sort_by(|a, b| a.id.cmp(&b.id));
    let mut sound = AcousticGeometry::from_bindings(acoustics);
    sound.rooms = rooms::Graph::from_profile(&profile)?.map(std::sync::Arc::new);
    let object =
        Some(ObjectContext { profile, visual_nodes, detail, render_locals, physical_bindings, sound, levels, active });
    Ok(Sector { meshes, instances, supports: Vec::new(), colliders, triangles, object, bundled_bridge: false })
}

impl ObjectContext {
    pub fn contact_material(&self, id: &str) -> Option<&str> {
        let &(owner, binding) = self.physical_bindings.get(id)?;
        Some(&self.profile.nodes[owner].geometry.collision[binding].material)
    }

    pub fn collision_path(&self, id: &str) -> Option<Vec<&str>> {
        self.profile.node_path(self.physical_bindings.get(id)?.0)
    }
}

pub fn inspect_textures(bytes: &[u8]) -> Result<serde_json::Value> {
    let doc = crate::scene::read_glb(bytes)?;
    let profile = ObjectProfile::read(&doc.document)?;
    let meshes = crate::scene::read_meshes(&doc, profile.is_some(), None, false)?;
    let mut owners = vec![Vec::new(); meshes.len()];
    if let Some(p) = profile.as_ref() {
        for (i, node) in p.nodes.iter().enumerate() {
            if let Some(mesh) = node.visual {
                owners[mesh].push(serde_json::json!({"nodePath":p.node_path(i),"level":"high"}));
            }
            for lod in &node.geometry.visual_lods {
                owners[lod.mesh].push(serde_json::json!({"nodePath":p.node_path(i),"level":lod.level}));
            }
        }
    } else {
        for node in doc.nodes() {
            if let Some(mesh) = node.mesh() {
                owners[mesh.index()].push(serde_json::json!({"node":node.index(),"name":node.name()}));
            }
        }
    }
    let mut rows = Vec::new();
    for (mesh, parts) in meshes.iter().enumerate() {
        for (primitive, part) in parts.iter().enumerate() {
            if let Some(texture) = &part.texture {
                let cutoff = crate::view::MaterialFactors::from_primitive(part).texture_cutoff();
                rows.push(serde_json::json!({
                    "mesh":mesh, "primitive":primitive, "owners":owners[mesh],
                    "alphaCutoffByte":cutoff, "levels":texture.levels(cutoff)
                }));
            }
        }
    }
    Ok(serde_json::json!({"sceneSha256":crate::bench::sha(bytes),"textures":rows}))
}

pub fn inspect(scene: &Sector, rays: &[([f32; 3], [f32; 3])], views: &[DetailView]) -> Result<Value> {
    let context = scene.object.as_ref().context("input lacks the Osage object profile")?;
    let profile = &context.profile;
    let nodes: Vec<_> = profile
        .nodes
        .iter()
        .zip(&context.active)
        .map(|(node, active)| {
            json!({
                "parent":node.parent.map(|i| &profile.nodes[i].geometry.id),
                "children":node.children.iter().map(|&i| &profile.nodes[i].geometry.id).collect::<Vec<_>>(),
                "parentFromLocal":node.parent_from_local.to_cols_array_2d(),
                "visualMesh":node.visual, "selectedVisualMesh":node.visual_at(context.levels.visual).filter(|_| active.visual), "channels":node.geometry
            })
        })
        .collect();
    let mut result = json!({"object": profile.header, "roots":profile.roots.iter().map(|&i| &profile.nodes[i].geometry.id).collect::<Vec<_>>(), "frequencyHz":BAND_HZ, "nodes":nodes, "visualLod":context.levels.visual,"levels":context.levels,
        "visualTriangles":scene.triangles, "collisionTriangles":scene.colliders.iter().map(|m|m.triangles.len()).sum::<usize>()});
    if !views.is_empty() {
        let mut current = vec![0; profile.nodes.len()];
        let mut samples = Vec::new();
        for &view in views {
            let mut nodes = Vec::new();
            let mut total = scene.triangles;
            for (&owner, detail) in &context.detail {
                current[owner] = detail.choose(current[owner], view);
                let chosen = &detail.levels[current[owner]];
                let error = if chosen.error_m == 0. { 0. } else { chosen.error_m * detail.pixels_per_metre(view) };
                total = total - detail.levels[0].triangles + chosen.triangles;
                nodes.push(json!({"owner":profile.node_path(owner),"level":chosen.level,"triangles":chosen.triangles,"errorBoundPx":error}));
            }
            samples.push(json!({"triangles":total,"nodes":nodes}));
        }
        result["detailViews"] = json!(samples);
    }
    if let Some(rig) = &profile.header.articulation {
        result["resolvedRigBodies"] = serde_json::to_value(rig.resolve(&profile.nodes)?)?;
    }
    if !rays.is_empty() {
        let index = rays::Index::new(scene, context)?;
        let mut hits = Vec::with_capacity(rays.len());
        for &(a, b) in rays {
            let mut hit = index.query(a, b)?;
            if let Some(graph) = &context.sound.rooms {
                hit["roomPath"] = serde_json::to_value(graph.query(a, b))?;
            }
            hits.push(hit);
        }
        result["ray"] = hits.last().unwrap().clone();
        if hits.len() > 1 {
            result["rays"] = json!(hits);
        }
    }
    Ok(result)
}
