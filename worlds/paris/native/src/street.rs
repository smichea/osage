// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The street around the walker: Bistro's model, of which only the meshes
//! that reach into the walker's block of 9 cubes are drawn (D-136) — the
//! block's faces, drawn with the light coming in, hide all beyond them.
//! Lit by the sky Cycles lit the light with, at the same strength.

use std::collections::HashMap;

use bevy::{
    gltf::{Gltf, GltfMaterial, GltfMesh, GltfNode},
    light::{EnvironmentMapLight, GlobalAmbientLight},
    mesh::VertexAttributeValues,
    pbr::gltf::standard_material_from_gltf_material,
    prelude::*,
};
use osage_render::OsageCamera;
use osage_world_paris::tiles::Block;

use crate::{NITS, clip, walker::Me};

/// The street's model and its sky, under the asset manifest's content.
const MODEL: &str = "orca/bistro/out/bistro.gltf";
const SKY: &str = "orca/bistro/src/Bistro_v5_2/san_giuseppe_bridge_4k.hdr";
/// The sky's strength, as the scene's Falcor file and the bake set it.
const SKY_STRENGTH: f32 = 10.0;

pub struct StreetPlugin;

impl Plugin for StreetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load).add_systems(Update, (place, show, sky));
    }
}

#[derive(Resource)]
struct Model {
    gltf: Handle<Gltf>,
    sky: Handle<Image>,
    placed: bool,
    lit: bool,
}

/// A mesh of the street, the box it fills in the world, and the mesh
/// itself (drawn cut to the block when traced).
#[derive(Component)]
pub struct Piece {
    min: Vec3,
    max: Vec3,
    mesh: Handle<Mesh>,
}

/// A trial (`--ring`): a mesh of the street drawn round the block with the
/// light the probes saw of it, and the box it fills.
#[derive(Component)]
struct Around {
    min: Vec3,
    max: Vec3,
}

/// Whether the street is placed, for whoever waits on it.
#[derive(Resource, Default)]
pub struct Placed(pub bool);

fn load(mut commands: Commands, server: Res<AssetServer>, mut ambient: ResMut<GlobalAmbientLight>) {
    commands.insert_resource(Model { gltf: server.load(MODEL), sky: server.load(SKY), placed: false, lit: false });
    commands.init_resource::<Placed>();
    // The sky lights the street (below); no light from nowhere.
    ambient.brightness = 0.0;
}

/// Places every mesh of the model once it has loaded, hidden until the
/// walker's block reaches it.
#[allow(clippy::too_many_arguments)]
fn place(
    mut commands: Commands,
    mut model: ResMut<Model>,
    mut placed: ResMut<Placed>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    nodes: Res<Assets<GltfNode>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    gltf_materials: Res<Assets<GltfMaterial>>,
    meshes: Res<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    options: Res<crate::Options>,
    ring: Res<crate::light::Ring>,
) {
    if model.placed {
        return;
    }
    if server.load_state(&model.gltf).is_failed() {
        error!("the street did not load: {MODEL} (tools/bistro/export.py)");
        model.placed = true;
        return;
    }
    if !server.is_loaded_with_dependencies(&model.gltf) {
        return;
    }
    let Some(gltf) = gltfs.get(&model.gltf) else { return };
    let index: HashMap<AssetId<GltfNode>, usize> = gltf.nodes.iter().enumerate().map(|(i, h)| (h.id(), i)).collect();
    let mut children = vec![false; gltf.nodes.len()];
    for handle in &gltf.nodes {
        if let Some(node) = nodes.get(handle) {
            for child in &node.children {
                if let Some(&i) = index.get(&child.id()) {
                    children[i] = true;
                }
            }
        }
    }
    let mut converted: HashMap<AssetId<GltfMaterial>, Handle<StandardMaterial>> = HashMap::new();
    let mut stack: Vec<(usize, Transform)> =
        (0..gltf.nodes.len()).filter(|&i| !children[i]).map(|i| (i, Transform::IDENTITY)).collect();
    let mut count = 0;
    while let Some((i, parent)) = stack.pop() {
        let Some(node) = nodes.get(&gltf.nodes[i]) else { continue };
        let place = parent.mul_transform(node.transform);
        for child in &node.children {
            if let Some(&c) = index.get(&child.id()) {
                stack.push((c, place));
            }
        }
        let Some(mesh) = node.mesh.as_ref().and_then(|m| gltf_meshes.get(m)) else { continue };
        for primitive in &mesh.primitives {
            let material = match &primitive.material {
                Some(handle) => converted
                    .entry(handle.id())
                    .or_insert_with(|| {
                        let mut material = standard_material_from_gltf_material(
                            &gltf_materials.get(handle).cloned().unwrap_or_default(),
                        );
                        // Cycles' radiance to luminance, as the light's.
                        material.emissive *= NITS;
                        materials.add(material)
                    })
                    .clone(),
                None => Handle::default(),
            };
            let Some((lo, hi)) = meshes.get(&primitive.mesh).and_then(bounds) else { continue };
            let affine = place.compute_affine();
            let corners = (0..8).map(|k| {
                let c = Vec3::new(
                    if k & 1 == 0 { lo.x } else { hi.x },
                    if k & 2 == 0 { lo.y } else { hi.y },
                    if k & 4 == 0 { lo.z } else { hi.z },
                );
                affine.transform_point3(c)
            });
            let (min, max) = corners.fold((Vec3::MAX, Vec3::MIN), |(a, b), c| (a.min(c), b.max(c)));
            commands.spawn((
                Mesh3d(primitive.mesh.clone()),
                MeshMaterial3d(material),
                place,
                Visibility::Hidden,
                Piece { min, max, mesh: primitive.mesh.clone() },
            ));
            if options.ring {
                commands.spawn((
                    Mesh3d(primitive.mesh.clone()),
                    MeshMaterial3d(ring.0.clone()),
                    place,
                    Visibility::Hidden,
                    bevy::light::NotShadowCaster,
                    Around { min, max },
                ));
            }
            count += 1;
        }
    }
    info!(meshes = count, "paris: the street is placed");
    model.placed = true;
    placed.0 = true;
}

fn bounds(mesh: &Mesh) -> Option<(Vec3, Vec3)> {
    let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return None;
    };
    let (lo, hi) =
        positions.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| (lo.min(Vec3::from(*p)), hi.max(Vec3::from(*p))));
    (lo.cmple(hi).all()).then_some((lo, hi))
}

/// Shows the meshes that reach into the block drawn, hides the others.
/// Traced (T4), each is cut to the block: the rays see the block alone, and
/// what lies beyond comes in as the light its probes saw (`clip`). Drawn,
/// nothing changes: from inside the block the faces hide all beyond it.
#[allow(clippy::too_many_arguments)]
fn show(
    me: Res<Me>,
    options: Res<crate::Options>,
    light: Res<crate::light::Store>,
    traced: Res<osage_render::TracedLight>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut last: Local<Option<Block>>,
    mut pieces: Query<(&Piece, &Transform, &mut Visibility, &mut Mesh3d)>,
    mut around: Query<(&Around, &mut Visibility), Without<Piece>>,
    added: Query<(), Added<Piece>>,
) {
    // The block whose light is drawn: it stays a moment after the walker
    // steps out of its cube, until the next one's light has come.
    let block = light.block.unwrap_or_else(|| Block::of(me.eyes()));
    if *last == Some(block) && added.is_empty() {
        return;
    }
    *last = Some(block);
    let (lo, hi) = (block.min() - 0.01, block.max() + 0.01);
    let cut = traced.0 && !options.whole;
    // The trial's meshes round the block: those that reach into the ring.
    let out = Vec3::new(crate::light::RING - 4.5, 1e3, crate::light::RING - 4.5);
    for (piece, mut visibility) in &mut around {
        let reaches = piece.min.cmple(hi + out).all() && piece.max.cmpge(lo - out).all();
        let wanted = if reaches { Visibility::Inherited } else { Visibility::Hidden };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
    let mut parts = 0;
    for (piece, place, mut visibility, mut mesh) in &mut pieces {
        let reaches = options.whole || (piece.min.cmple(hi).all() && piece.max.cmpge(lo).all());
        let mut wanted = if reaches { Visibility::Inherited } else { Visibility::Hidden };
        if cut {
            let inside = piece.min.cmpge(lo).all() && piece.max.cmple(hi).all();
            let shape = if !reaches || inside {
                Some(piece.mesh.clone())
            } else {
                match meshes.get(&piece.mesh).map(|whole| clip::clip(whole, place, lo, hi)) {
                    Some(clip::Cut::Part(part)) => {
                        parts += 1;
                        Some(meshes.add(*part))
                    }
                    Some(clip::Cut::Nothing) => None,
                    Some(clip::Cut::Whole) | None => Some(piece.mesh.clone()),
                }
            };
            match shape {
                Some(shape) if mesh.0 != shape => mesh.0 = shape,
                Some(_) => {}
                None => wanted = Visibility::Hidden,
            }
        }
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
    if cut {
        debug!(cube = ?block.cube, parts, "paris: the street cut to the block for the rays");
    }
}

/// The sky's photograph as the light of the street: the same strength as
/// the bake's.
fn sky(
    mut commands: Commands,
    mut model: ResMut<Model>,
    mut images: ResMut<Assets<Image>>,
    cameras: Query<Entity, With<OsageCamera>>,
) {
    if model.lit {
        return;
    }
    let Some(photo) = images.get(&model.sky) else { return };
    let (width, height) = (photo.width() as usize, photo.height() as usize);
    let Some(data) = photo.data.as_ref() else { return };
    let pixels: Vec<[f32; 3]> = match photo.texture_descriptor.format.block_copy_size(None) {
        Some(16) => data
            .as_chunks::<16>()
            .0
            .iter()
            .map(|p| {
                let f = |i: usize| f32::from_le_bytes([p[i], p[i + 1], p[i + 2], p[i + 3]]);
                [f(0), f(4), f(8)]
            })
            .collect(),
        _ => {
            warn!("paris: the sky's photograph is not in 32-bit floats");
            model.lit = true;
            return;
        }
    };
    // A direction of the glTF's frame to the photograph, as Blender maps
    // its world: Blender's (x, y, z) is the glTF's (x, -z, y).
    let sample = |d: Vec3| -> Vec3 {
        let (bx, by, bz) = (d.x, -d.z, d.y);
        let u = 0.5 - by.atan2(bx) / std::f32::consts::TAU;
        let v = 0.5 + bz.clamp(-1.0, 1.0).asin() / std::f32::consts::PI;
        let column = ((u.rem_euclid(1.0)) * width as f32) as usize % width;
        let row = (((1.0 - v) * height as f32) as usize).min(height - 1);
        Vec3::from(pixels[row * width + column])
    };
    let (diffuse, specular) = osage_render::sky_light::cubemaps(sample);
    let (diffuse_map, specular_map) = (images.add(diffuse), images.add(specular));
    for camera in &cameras {
        commands.entity(camera).insert(EnvironmentMapLight {
            diffuse_map: diffuse_map.clone(),
            specular_map: specular_map.clone(),
            intensity: SKY_STRENGTH * NITS,
            ..default()
        });
    }
    if !cameras.is_empty() {
        model.lit = true;
        info!("paris: lit by the sky's photograph");
    }
}
