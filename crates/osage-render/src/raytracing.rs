// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! R11 — ray-traced lighting at T4 (D-093): Bevy Solari's direct and
//! indirect light, traced through the scene, on hardware that traces rays
//! (Vulkan or Direct3D 12 ray queries: an RTX card, a recent Radeon).
//! Without such hardware Solari stays out and the picture is T3's.
//!
//! Solari traces rays against meshes of exactly four attributes — position,
//! normal, texture coordinates, tangent — with 32-bit indices, and shades
//! them with the standard material. The scene's meshes carry more (vertex
//! colours: the land's paint, the weather's marks), so each gets a double
//! for the rays alone (`RaytracingMesh3d`), the drawn mesh unchanged; a
//! ground drawn with the ground material gets a stand-in for the rays: its
//! double, under a standard material of the ground's colour.

use std::collections::HashMap;

use bevy::{
    camera::CameraMainTextureUsages,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
    render::render_resource::TextureUsages,
    solari::prelude::{RaytracingMesh3d, SolariLighting, SolariPlugins},
};

use crate::{OsageCamera, Tier, ground::GroundMaterial};

pub struct RayTracingPlugin;

impl Plugin for RayTracingPlugin {
    fn build(&self, app: &mut App) {
        if app.world().get_resource::<Tier>().is_none_or(|tier| *tier < Tier::T4) {
            return;
        }
        app.add_plugins(SolariPlugins)
            .init_resource::<Doubles>()
            .add_systems(Update, (camera, doubles, stand_ins))
            .add_systems(PostUpdate, hidden.after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate));
    }
}

/// The double made for each mesh, once.
#[derive(Resource, Default)]
struct Doubles(HashMap<AssetId<Mesh>, Option<Handle<Mesh>>>);

/// A mesh drawn with the ground material, given its stand-in for the rays.
#[derive(Component)]
struct StoodIn;

/// A mesh that cannot be traced (not triangles): looked at once.
#[derive(Component)]
struct Untraced;

/// A mesh's double, kept while the mesh is hidden: Solari traces every
/// `RaytracingMesh3d`, shown or not, and a hidden shape — a kart's guard
/// bubble round it, the items it may carry — would shade what it hides.
#[derive(Component)]
struct HiddenDouble(Handle<Mesh>);

/// The scene's camera traces its light (Solari needs no multisampling and
/// a main texture it can write to). Solari draws every opaque material
/// deferred, so the camera has the deferred prepass whether or not the
/// hardware traces rays: without, the deferred picture is lit as usual.
fn camera(mut commands: Commands, cameras: Query<Entity, Added<OsageCamera>>) {
    for camera in &cameras {
        commands.entity(camera).insert((
            // ReSTIR: samples reused between pixels and frames. Without a
            // denoiser (DLSS's, not built here) the sunlit asphalt sparkled
            // with noise; the L4 had the frame time for it (16.7 ms of 33).
            SolariLighting { restir: true, ..default() },
            Msaa::Off,
            CameraMainTextureUsages::default().with(TextureUsages::STORAGE_BINDING),
        ));
    }
}

/// A mesh as Solari takes it: position, normal, texture coordinates and
/// tangent only, triangles, 32-bit indices; none for another topology or
/// a mesh without positions.
pub fn double(mesh: &Mesh) -> Option<Mesh> {
    if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
        return None;
    }
    let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION).cloned() else {
        return None;
    };
    let count = positions.len();
    let indices: Vec<u32> = match mesh.indices() {
        Some(indices) => indices.iter().map(|i| i as u32).collect(),
        None => (0..count as u32).collect(),
    };
    // Solari builds a structure for every mesh it can trace, and one of no
    // triangles is none it can build (it panics): an empty mesh is not doubled.
    if count == 0 || indices.len() < 3 {
        return None;
    }
    let mut double = Mesh::new(PrimitiveTopology::TriangleList, mesh.asset_usage)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_indices(Indices::U32(indices));
    match mesh.attribute(Mesh::ATTRIBUTE_NORMAL).cloned() {
        Some(normals) => double.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals),
        None => double.compute_normals(),
    }
    let uvs = mesh.attribute(Mesh::ATTRIBUTE_UV_0).cloned();
    let has_uvs = uvs.is_some();
    double.insert_attribute(
        Mesh::ATTRIBUTE_UV_0,
        uvs.unwrap_or(VertexAttributeValues::Float32x2(vec![[0.0, 0.0]; count])),
    );
    let tangents = mesh.attribute(Mesh::ATTRIBUTE_TANGENT).cloned();
    match tangents {
        Some(tangents) => double.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents),
        None if has_uvs && double.generate_tangents().is_ok() => {}
        None => double.insert_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0f32, 0.0, 0.0, 1.0]; count]),
    }
    Some(double)
}

/// The double of `mesh`, made once; none while it is not loaded, or if it
/// cannot be traced.
fn double_of(handle: &Handle<Mesh>, meshes: &mut Assets<Mesh>, doubles: &mut Doubles) -> Option<Option<Handle<Mesh>>> {
    if let Some(known) = doubles.0.get(&handle.id()) {
        return Some(known.clone());
    }
    let mesh = meshes.get(handle)?;
    let made = double(mesh).map(|double| meshes.add(double));
    doubles.0.insert(handle.id(), made.clone());
    Some(made)
}

/// Every mesh drawn with the standard material traced as its double.
#[allow(clippy::type_complexity)]
fn doubles(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut doubles: ResMut<Doubles>,
    drawn: Query<
        (Entity, &Mesh3d),
        (With<MeshMaterial3d<StandardMaterial>>, Without<RaytracingMesh3d>, Without<Untraced>, Without<HiddenDouble>),
    >,
) {
    for (entity, mesh) in &drawn {
        match double_of(&mesh.0, &mut meshes, &mut doubles) {
            Some(Some(double)) => {
                commands.entity(entity).insert(RaytracingMesh3d(double));
            }
            // Not to be traced: marked so, not looked at again.
            Some(None) => {
                commands.entity(entity).insert(Untraced);
            }
            None => {}
        }
    }
}

/// A hidden mesh leaves the traced scene; shown again, it comes back.
#[allow(clippy::type_complexity)]
fn hidden(
    mut commands: Commands,
    traced: Query<
        (Entity, &InheritedVisibility, &RaytracingMesh3d),
        Or<(Changed<InheritedVisibility>, Added<RaytracingMesh3d>)>,
    >,
    kept: Query<(Entity, &InheritedVisibility, &HiddenDouble), Changed<InheritedVisibility>>,
) {
    for (entity, visible, double) in &traced {
        if !visible.get() {
            commands.entity(entity).remove::<RaytracingMesh3d>().insert(HiddenDouble(double.0.clone()));
        }
    }
    for (entity, visible, double) in &kept {
        if visible.get() {
            commands.entity(entity).remove::<HiddenDouble>().insert(RaytracingMesh3d(double.0.clone()));
        }
    }
}

/// Every mesh drawn with the ground material gets a stand-in for the rays:
/// its double under a standard material of the ground's colour.
fn stand_ins(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut doubles: ResMut<Doubles>,
    grounds: Res<Assets<GroundMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    drawn: Query<(Entity, &Mesh3d, &MeshMaterial3d<GroundMaterial>), Without<StoodIn>>,
) {
    for (entity, mesh, material) in &drawn {
        let Some(found) = double_of(&mesh.0, &mut meshes, &mut doubles) else { continue };
        commands.entity(entity).insert(StoodIn);
        let (Some(double), Some(ground)) = (found, grounds.get(&material.0)) else { continue };
        // The ground's colour under its grain (a grain averages a half):
        // what light bounces off it.
        let colour = ground.base.base_color.to_linear() * 0.5;
        let stand_in = materials.add(StandardMaterial {
            base_color: colour.into(),
            perceptual_roughness: ground.base.perceptual_roughness,
            ..default()
        });
        commands.entity(entity).with_child((RaytracingMesh3d(double), MeshMaterial3d(stand_in), Transform::default()));
    }
}

#[cfg(test)]
mod tests {
    use bevy::asset::RenderAssetUsages;

    use super::*;

    #[test]
    fn a_coloured_mesh_has_a_double_of_the_four_attributes_solari_traces() {
        let coloured = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(
                Mesh::ATTRIBUTE_POSITION,
                vec![[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 3])
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32, 0.0, 0.0, 1.0]; 3])
            .with_inserted_indices(Indices::U16(vec![0, 2, 1]));
        let double = double(&coloured).expect("a double");
        let attributes: Vec<_> = double.attributes().map(|(a, _)| a.id).collect();
        assert_eq!(
            attributes,
            vec![
                Mesh::ATTRIBUTE_POSITION.id,
                Mesh::ATTRIBUTE_NORMAL.id,
                Mesh::ATTRIBUTE_UV_0.id,
                Mesh::ATTRIBUTE_TANGENT.id
            ]
        );
        assert!(matches!(double.indices(), Some(Indices::U32(i)) if i == &vec![0, 2, 1]));
        let lines = Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default());
        assert!(super::double(&lines).is_none());
        let empty = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new())
            .with_inserted_indices(Indices::U32(Vec::new()));
        assert!(super::double(&empty).is_none(), "no double of an empty mesh: Solari cannot build it");
    }
}
