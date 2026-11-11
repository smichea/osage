// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Props: models of the asset manifest (glTF), each placed many times. A
//! [`Scatter`] is one model and the places it stands; once the model has
//! loaded, every place gets the model's meshes, each with its material
//! converted once and shared by all, so that Bevy draws each mesh of the set
//! in one batch. A scatter can be seen only up to a distance, fading out
//! before it, and can cast shadows or not: grass a hundred metres off costs
//! nothing, a tyre wall casts its shadow.
//!
//! A [`Carpet`] lays a model around the camera instead, cell by cell, and
//! moves with it: grass as seen up close. [`Woods`] draw a model always
//! seen from afar as crossed cards, pictures of it: forests.
//!
//! The app loads models through its asset server, whose root must be the
//! manifest's content (`osage_content::root`); [`poly_haven`] names a Poly
//! Haven model there.

use std::collections::HashMap;

pub mod carpet;
pub mod impostor;
pub mod lod;

pub use carpet::Carpet;
pub use impostor::Woods;

use bevy::{
    camera::visibility::VisibilityRange,
    gltf::{Gltf, GltfMaterial, GltfMesh, GltfNode},
    light::NotShadowCaster,
    pbr::gltf::standard_material_from_gltf_material,
    prelude::*,
};

pub struct PropsPlugin;

impl Plugin for PropsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Converted>()
            .init_resource::<Waiting>()
            .add_systems(Update, (scatter, carpet::lay, impostor::plant).chain())
            .add_systems(PostUpdate, carpet::follow.before(bevy::transform::TransformSystems::Propagate));
    }
}

/// Poly Haven model `id` of the asset manifest, at 1k textures.
pub fn poly_haven(server: &AssetServer, id: &str) -> Handle<Gltf> {
    server.load(format!("polyhaven/{id}/{id}_1k.gltf"))
}

/// How a model is used at each place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Use {
    /// The whole model.
    #[default]
    Whole,
    /// One of its variants: libraries such as Poly Haven lay several plants
    /// or rocks side by side in one model, each a top-level node. Place `i`
    /// takes variant `i` modulo their number, brought to the origin.
    Variants,
}

/// One model, standing at many places.
#[derive(Component, Clone)]
pub struct Scatter {
    pub model: Handle<Gltf>,
    pub places: Vec<Transform>,
    pub using: Use,
    /// Seen up to this distance, metres, fading out over its last tenth;
    /// `None`: at any distance.
    pub within: Option<f32>,
    pub shadows: bool,
    /// Coarser meshes further off ([`lod`]): the whole mesh near, a
    /// simplified one further, a coarse one beyond (foliage thinned, solid
    /// things clustered). For dense models seen far away: trees, bushes, rocks.
    pub lods: bool,
    /// A plain colour instead of the model's colour texture: for tiny
    /// things (tufts of grass) whose texture, an atlas on black, would
    /// darken as it shrinks.
    pub tint: Option<Color>,
    /// Drawn opaque whatever the model says: libraries mark leaves made of
    /// real geometry as blended, with textures that have no alpha; blended,
    /// they would be drawn without depth and overlap out of order.
    pub opaque: bool,
    /// Lit as the ground it grows from ([`Carpet::lit_as_ground`]): grass.
    pub lit_as_ground: bool,
}

/// The model's material as a standard material, its colour texture
/// replaced by `tint` if there is one, opaque if asked.
pub(crate) fn finish(source: &GltfMaterial, tint: Option<Color>, opaque: bool) -> StandardMaterial {
    let mut material = standard_material_from_gltf_material(source);
    if let Some(tint) = tint {
        material.base_color = tint;
        material.base_color_texture = None;
    }
    if opaque {
        material.alpha_mode = AlphaMode::Opaque;
        material.base_color = material.base_color.with_alpha(1.0);
    }
    material
}

/// Scatters whose model is still loading. A benchmark waits for none before
/// it starts, so that it draws the same frames at every run.
#[derive(Resource, Default, Debug)]
pub struct Waiting(pub usize);

/// A scatter whose places have been given the model.
#[derive(Component)]
struct Scattered;

/// The standard material made of each glTF material, made once; the
/// coarser meshes made of each mesh, made once.
#[derive(Resource, Default)]
struct Converted(HashMap<AssetId<GltfMaterial>, Handle<StandardMaterial>>, HashMap<AssetId<Mesh>, Levels>);

/// The two coarser meshes of a mesh, and where its first two levels end.
#[derive(Clone)]
struct Levels {
    meshes: [Option<Handle<Mesh>>; 2],
    ends: [f32; 2],
}

/// Solid things, by clustering: how much of the mesh's size each coarser
/// level merges, and where each level ends, as a share of the distance a
/// scatter is seen at.
const COARSER: [f32; 2] = [0.025, 0.06];
const LEVEL_ENDS: [f32; 2] = [0.15, 0.45];
/// Aggregates (needles, leaves), by thinning: the share of pieces each
/// coarser level keeps, and where each level ends. A needle is thinner than
/// a pixel a few metres off, so the full mesh is needed only close by.
const KEPT: [f32; 2] = [0.15, 0.03];
const THINNED_ENDS: [f32; 2] = [0.04, 0.15];

/// A mesh with its material, where the model puts it.
pub(crate) type Look = (Handle<Mesh>, Handle<StandardMaterial>, Transform);

/// A mesh of a model where the model puts it.
struct Piece {
    mesh: Handle<Mesh>,
    material: Option<Handle<GltfMaterial>>,
    place: Transform,
}

/// A node of a model: its place among its parent's, its children, its mesh.
struct Node<M> {
    place: Transform,
    children: Vec<usize>,
    mesh: Option<M>,
}

/// Every mesh of a model with the place it has in the model, by variant:
/// each node's place composed with those of the nodes above it. With
/// `apart`, each top-level node is a variant of its own, brought to the
/// origin (its offset dropped, its turn and scale kept); otherwise the
/// whole model is one.
fn flatten<M: Clone>(nodes: &[Node<M>], apart: bool) -> Vec<Vec<(M, Transform)>> {
    let mut child = vec![false; nodes.len()];
    for node in nodes {
        for &c in &node.children {
            if let Some(slot) = child.get_mut(c) {
                *slot = true;
            }
        }
    }
    // A cycle is not a model: never visit a node twice.
    let mut seen = vec![false; nodes.len()];
    let mut variants = Vec::new();
    for root in (0..nodes.len()).filter(|&i| !child[i]) {
        let mut meshes = Vec::new();
        let mut stack = vec![(root, Transform::IDENTITY)];
        while let Some((i, above)) = stack.pop() {
            if std::mem::replace(&mut seen[i], true) {
                continue;
            }
            let node = &nodes[i];
            let mut place = above * node.place;
            if apart && i == root {
                place.translation = Vec3::ZERO;
            }
            if let Some(mesh) = &node.mesh {
                meshes.push((mesh.clone(), place));
            }
            stack.extend(node.children.iter().filter(|&&c| c < nodes.len()).map(|&c| (c, place)));
        }
        if !meshes.is_empty() {
            variants.push(meshes);
        }
    }
    if !apart && !variants.is_empty() {
        variants = vec![variants.concat()];
    }
    variants
}

fn pieces(gltf: &Gltf, nodes: &Assets<GltfNode>, meshes: &Assets<GltfMesh>, using: Use) -> Option<Vec<Vec<Piece>>> {
    let index: HashMap<AssetId<GltfNode>, usize> = gltf.nodes.iter().enumerate().map(|(i, h)| (h.id(), i)).collect();
    let mut tree = Vec::with_capacity(gltf.nodes.len());
    for handle in &gltf.nodes {
        let node = nodes.get(handle)?;
        tree.push(Node {
            place: node.transform,
            children: node.children.iter().filter_map(|c| index.get(&c.id()).copied()).collect(),
            mesh: node.mesh.clone(),
        });
    }
    let mut variants = Vec::new();
    for variant in flatten(&tree, using == Use::Variants) {
        let mut pieces = Vec::new();
        for (mesh, place) in variant {
            for primitive in &meshes.get(&mesh)?.primitives {
                pieces.push(Piece { mesh: primitive.mesh.clone(), material: primitive.material.clone(), place });
            }
        }
        variants.push(pieces);
    }
    Some(variants)
}

#[allow(clippy::too_many_arguments)]
fn scatter(
    mut commands: Commands,
    scatters: Query<(Entity, &Scatter), Without<Scattered>>,
    gltfs: Res<Assets<Gltf>>,
    nodes: Res<Assets<GltfNode>>,
    meshes: Res<Assets<GltfMesh>>,
    mut models: ResMut<Assets<Mesh>>,
    gltf_materials: Res<Assets<GltfMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut converted: ResMut<Converted>,
    mut waiting: ResMut<Waiting>,
    server: Res<AssetServer>,
) {
    waiting.0 = 0;
    for (entity, scatter) in &scatters {
        // Placed once whole, textures and all: nothing appears white first.
        if server.load_state(&scatter.model).is_failed() {
            warn!("a model did not load: {:?}", server.get_path(&scatter.model));
            commands.entity(entity).insert(Scattered);
            continue;
        }
        if !server.is_loaded_with_dependencies(&scatter.model) {
            waiting.0 += 1;
            continue;
        }
        let Some(variants) = gltfs.get(&scatter.model).and_then(|gltf| pieces(gltf, &nodes, &meshes, scatter.using))
        else {
            continue;
        };
        let far = scatter.within;
        // Each level, from near to far: its mesh index and the range it is seen in.
        let fade = |from: f32, to: f32| VisibilityRange {
            start_margin: from * 0.9..from,
            end_margin: to * 0.9..to,
            use_aabb: false,
        };
        let levels = |ends: [f32; 2]| -> Vec<(usize, Option<VisibilityRange>)> {
            match far {
                Some(far) if scatter.lods => vec![
                    (0, Some(VisibilityRange { start_margin: 0.0..0.0, ..fade(0.0, far * ends[0]) })),
                    (1, Some(fade(far * ends[0], far * ends[1]))),
                    (2, Some(fade(far * ends[1], far))),
                ],
                Some(far) => vec![(0, Some(VisibilityRange { start_margin: 0.0..0.0, ..fade(0.0, far) }))],
                None => vec![(0, None)],
            }
        };
        let mut material = |piece: &Piece| -> Handle<StandardMaterial> {
            let Some(handle) = &piece.material else { return Handle::default() };
            // A tinted, forced-opaque or ground-lit material is this
            // scatter's own: the shared one keeps the model's.
            if scatter.tint.is_some() || scatter.opaque || scatter.lit_as_ground {
                let source = gltf_materials.get(handle).cloned().unwrap_or_default();
                let mut material = finish(&source, scatter.tint, scatter.opaque);
                if scatter.lit_as_ground {
                    material.double_sided = false;
                    material.cull_mode = None;
                }
                return materials.add(material);
            }
            converted
                .0
                .entry(handle.id())
                .or_insert_with(|| {
                    let source = gltf_materials.get(handle).cloned().unwrap_or_default();
                    materials.add(finish(&source, scatter.tint, false))
                })
                .clone()
        };
        let looks: Vec<Vec<Look>> = variants
            .iter()
            .map(|pieces| {
                pieces
                    .iter()
                    .map(|p| {
                        let upright = models
                            .get(&p.mesh)
                            .filter(|_| scatter.lit_as_ground)
                            .map(|m| carpet::facing_up(m, p.place));
                        let mesh = upright.map_or_else(|| p.mesh.clone(), |m| models.add(m));
                        (mesh, material(p), p.place)
                    })
                    .collect()
            })
            .collect();
        // Each mesh at each level, and where its levels end; the coarser
        // ones made once per mesh, a level that leaves nothing keeping the
        // finer one.
        let mut level_meshes: HashMap<(AssetId<Mesh>, usize), Handle<Mesh>> = HashMap::new();
        let mut level_ends: HashMap<AssetId<Mesh>, [f32; 2]> = HashMap::new();
        for (mesh, ..) in looks.iter().flatten() {
            for level in 0..3 {
                level_meshes.insert((mesh.id(), level), mesh.clone());
            }
            level_ends.insert(mesh.id(), LEVEL_ENDS);
            if !scatter.lods {
                continue;
            }
            let Levels { meshes: [first, second], ends } = converted
                .1
                .entry(mesh.id())
                .or_insert_with(|| {
                    let Some(source) = models.get(mesh) else {
                        return Levels { meshes: [None, None], ends: LEVEL_ENDS };
                    };
                    // One seed for both levels: the pieces the coarser keeps are among those the finer keeps.
                    let thinned = KEPT.map(|keep| lod::thin(source, keep, 0x05a6e));
                    let (made, ends) = if thinned.iter().all(Option::is_some) {
                        (thinned, THINNED_ENDS)
                    } else {
                        let size = lod::extent(source);
                        (COARSER.map(|share| lod::simplify(source, size * share)), LEVEL_ENDS)
                    };
                    let triangles = |mesh: Option<&Mesh>| mesh.and_then(Mesh::indices).map_or(0, |i| i.len() / 3);
                    debug!(
                        "levels of detail of {:?}: {} {} {} triangles{}",
                        server.get_path(&scatter.model),
                        triangles(Some(source)),
                        triangles(made[0].as_ref()),
                        triangles(made[1].as_ref()),
                        if ends == THINNED_ENDS { ", thinned" } else { "" }
                    );
                    let [first, second] = made;
                    Levels { meshes: [first.map(|m| models.add(m)), second.map(|m| models.add(m))], ends }
                })
                .clone();
            let first = first.unwrap_or_else(|| mesh.clone());
            let second = second.unwrap_or_else(|| first.clone());
            level_meshes.insert((mesh.id(), 1), first);
            level_meshes.insert((mesh.id(), 2), second);
            level_ends.insert(mesh.id(), ends);
        }
        let mut scattered = commands.entity(entity);
        scattered.insert((Scattered, Transform::IDENTITY, Visibility::Inherited));
        if looks.is_empty() {
            continue;
        }
        scattered.with_children(|parent| {
            for (i, place) in scatter.places.iter().enumerate() {
                for (mesh, material, inside) in &looks[i % looks.len()] {
                    for (level, range) in &levels(level_ends[&mesh.id()]) {
                        let mesh = level_meshes[&(mesh.id(), *level)].clone();
                        let mut child =
                            parent.spawn((Mesh3d(mesh), MeshMaterial3d(material.clone()), *place * *inside));
                        if let Some(range) = range {
                            child.insert(range.clone());
                        }
                        if !scatter.shadows {
                            child.insert(NotShadowCaster);
                        }
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(place: Transform, children: Vec<usize>, mesh: Option<&'static str>) -> Node<&'static str> {
        Node { place, children, mesh }
    }

    #[test]
    fn a_mesh_takes_the_places_of_the_nodes_above_it() {
        let up = Transform::from_xyz(0.0, 2.0, 0.0);
        let right = Transform::from_xyz(1.0, 0.0, 0.0);
        // A root with a mesh and a child with a mesh; the child listed first.
        let tree = [node(right, vec![], Some("wheel")), node(up, vec![0], Some("body"))];
        let mut flat = flatten(&tree, false).concat();
        flat.sort_by_key(|(mesh, _)| *mesh);
        assert_eq!(flat.len(), 2);
        assert_eq!(flat[0].0, "body");
        assert_eq!(flat[0].1.translation, Vec3::new(0.0, 2.0, 0.0));
        assert_eq!(flat[1].0, "wheel");
        assert_eq!(flat[1].1.translation, Vec3::new(1.0, 2.0, 0.0), "the child under its parent");
    }

    #[test]
    fn nodes_without_meshes_still_place_their_children() {
        let tree = [
            node(Transform::from_scale(Vec3::splat(2.0)), vec![1], None),
            node(Transform::from_xyz(1.0, 0.0, 0.0), vec![], Some("leaf")),
        ];
        let flat = flatten(&tree, false).concat();
        assert_eq!(flat.len(), 1);
        assert_eq!(flat[0].1.translation, Vec3::new(2.0, 0.0, 0.0), "moved in its parent's scale");
    }

    #[test]
    fn a_broken_model_does_not_loop_or_reach_outside() {
        let tree = [node(Transform::IDENTITY, vec![1, 7], Some("a")), node(Transform::IDENTITY, vec![0], Some("b"))];
        // Each is the other's child: no root, nothing to draw, and no endless walk.
        assert!(flatten(&tree, false).is_empty());
        let rooted = [node(Transform::IDENTITY, vec![1, 9], Some("a")), node(Transform::IDENTITY, vec![1], Some("b"))];
        assert_eq!(flatten(&rooted, false).concat().len(), 2, "a self-loop and an index out of range are ignored");
    }

    #[test]
    fn variants_laid_side_by_side_each_come_to_the_origin() {
        // Three plants a metre apart, the last turned a quarter round, with a leaf of its own.
        let turned =
            Transform::from_xyz(2.0, 0.0, 0.0).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
        let tree = [
            node(Transform::IDENTITY, vec![], Some("a")),
            node(Transform::from_xyz(1.0, 0.0, 0.0), vec![], Some("b")),
            node(turned, vec![3], Some("c")),
            node(Transform::from_xyz(0.0, 0.0, 1.0), vec![], Some("c-leaf")),
        ];
        let variants = flatten(&tree, true);
        assert_eq!(variants.len(), 3);
        for variant in &variants {
            let root = variant.iter().find(|(m, _)| m.len() == 1).unwrap();
            assert!(root.1.translation.length() < 1e-5, "{variant:?}");
        }
        let c = variants.iter().find(|v| v.iter().any(|(m, _)| *m == "c")).unwrap();
        let leaf = c.iter().find(|(m, _)| *m == "c-leaf").unwrap().1.translation;
        assert!(leaf.distance(Vec3::new(1.0, 0.0, 0.0)) < 1e-5, "the leaf turns with its plant, at its origin: {leaf}");
        assert_eq!(flatten(&tree, false).len(), 1, "whole: one variant of everything");
    }
}
