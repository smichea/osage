// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A carpet: a model laid on a grid of cells around the camera — grass, as
//! seen up close — and moved with it. A fixed pool of instances, one per
//! cell of a square window; each instance always stands in the one cell of
//! the window that matches it modulo the window's size, so that when the
//! camera moves on, only the instances whose cell left the window move, to
//! the cells that entered it. The world says where the model grows and how
//! big: a function of a point on the ground.
//!
//! Places are a function of the cell alone (a seeded jitter), so the same
//! camera path gives the same carpet on every machine.

use std::sync::Arc;

use bevy::{
    camera::visibility::VisibilityRange,
    gltf::{Gltf, GltfMaterial, GltfMesh, GltfNode},
    light::NotShadowCaster,
    prelude::*,
};

use crate::{Use, pieces};

/// Where the model grows: for a point on the ground, its height and the
/// model's scale there, or `None` where nothing grows (a road, water).
pub type Ground = Arc<dyn Fn(Vec2) -> Option<(f32, f32)> + Send + Sync>;

/// A model laid around the camera, one in each cell of `spacing` metres,
/// out to `radius` metres.
#[derive(Component, Clone)]
pub struct Carpet {
    pub model: Handle<Gltf>,
    pub using: Use,
    pub spacing: f32,
    pub radius: f32,
    pub ground: Ground,
    /// A plain colour instead of the model's colour texture ([`crate::Scatter::tint`]).
    pub tint: Option<Color>,
}

impl Carpet {
    /// Cells along a side of the window.
    pub fn side(&self) -> i32 {
        ((2.0 * self.radius / self.spacing).ceil() as i32).max(1)
    }
}

/// An instance of a carpet: the slot it fills in the window, the cell it
/// stands in now.
#[derive(Component)]
pub(crate) struct Tuft {
    slot: IVec2,
    cell: Option<IVec2>,
}

/// A carpet whose instances are spawned.
#[derive(Component)]
pub(crate) struct Laid;

/// The cell of the window that instance `slot` stands in, for a window
/// whose lowest cell is `low`, `side` cells wide: the one equal to `slot`
/// modulo `side`.
pub(crate) fn cell_of(slot: IVec2, low: IVec2, side: i32) -> IVec2 {
    low + (slot - low).rem_euclid(IVec2::splat(side))
}

/// A repeatable number in 0..1 for a cell and a salt.
fn jitter(cell: IVec2, salt: u32) -> f32 {
    let mut h = (cell.x as u32).wrapping_mul(0x9E37_79B1)
        ^ (cell.y as u32).wrapping_mul(0x85EB_CA77)
        ^ salt.wrapping_mul(0xC2B2_AE3D);
    h = (h ^ (h >> 15)).wrapping_mul(0x2C1B_3C6D);
    h = (h ^ (h >> 12)).wrapping_mul(0x297A_2D39);
    ((h ^ (h >> 15)) & 0xff_ffff) as f32 / 16_777_216.0
}

/// Where the instance of `cell` stands, and how, or `None` if nothing grows there.
fn place(carpet: &Carpet, cell: IVec2) -> Option<Transform> {
    let at = (cell.as_vec2() + Vec2::new(jitter(cell, 1), jitter(cell, 2))) * carpet.spacing;
    let (height, scale) = (carpet.ground)(at)?;
    let size = scale * (0.75 + 0.5 * jitter(cell, 3));
    Some(
        Transform::from_xyz(at.x, height, at.y)
            .with_rotation(Quat::from_rotation_y(jitter(cell, 4) * std::f32::consts::TAU))
            .with_scale(Vec3::splat(size)),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn lay(
    mut commands: Commands,
    carpets: Query<(Entity, &Carpet), Without<Laid>>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    nodes: Res<Assets<GltfNode>>,
    meshes: Res<Assets<GltfMesh>>,
    gltf_materials: Res<Assets<GltfMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut waiting: ResMut<crate::Waiting>,
) {
    for (entity, carpet) in &carpets {
        if !server.is_loaded_with_dependencies(&carpet.model) {
            if !server.load_state(&carpet.model).is_failed() {
                waiting.0 += 1;
            } else {
                commands.entity(entity).insert(Laid);
            }
            continue;
        }
        let Some(variants) = gltfs.get(&carpet.model).and_then(|gltf| pieces(gltf, &nodes, &meshes, carpet.using))
        else {
            continue;
        };
        let looks: Vec<Vec<crate::Look>> = variants
            .iter()
            .map(|pieces| {
                pieces
                    .iter()
                    .map(|p| {
                        let source =
                            p.material.as_ref().and_then(|m| gltf_materials.get(m)).cloned().unwrap_or_default();
                        (p.mesh.clone(), materials.add(crate::finish(&source, carpet.tint, false)), p.place)
                    })
                    .collect()
            })
            .collect();
        let side = carpet.side();
        let mut laid = commands.entity(entity);
        laid.insert((Laid, Transform::IDENTITY, Visibility::Inherited));
        if looks.is_empty() {
            continue;
        }
        laid.with_children(|parent| {
            for j in 0..side {
                for i in 0..side {
                    let slot = IVec2::new(i, j);
                    let look = &looks[(j * side + i) as usize % looks.len()];
                    let mut tuft = parent.spawn((Tuft { slot, cell: None }, Transform::IDENTITY, Visibility::Hidden));
                    // Fading out towards the window's edge: no line where the carpet stops.
                    let fade = VisibilityRange {
                        start_margin: 0.0..0.0,
                        end_margin: carpet.radius * 0.7..carpet.radius * 0.95,
                        use_aabb: false,
                    };
                    tuft.with_children(|tuft| {
                        for (mesh, material, inside) in look {
                            tuft.spawn((
                                Mesh3d(mesh.clone()),
                                MeshMaterial3d(material.clone()),
                                *inside,
                                NotShadowCaster,
                                fade.clone(),
                            ));
                        }
                    });
                }
            }
        });
    }
}

/// Moves the instances whose cell left the window to the cells that entered it.
pub(crate) fn follow(
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    carpets: Query<(&Carpet, &Children), With<Laid>>,
    mut tufts: Query<(&mut Tuft, &mut Transform, &mut Visibility)>,
) {
    let Some(eye) = cameras.iter().next().map(|camera| camera.translation()) else { return };
    for (carpet, children) in &carpets {
        let side = carpet.side();
        let low = (eye.xz() / carpet.spacing).floor().as_ivec2() - IVec2::splat(side / 2);
        for child in children {
            let Ok((mut tuft, mut transform, mut visibility)) = tufts.get_mut(*child) else { continue };
            let cell = cell_of(tuft.slot, low, side);
            if tuft.cell == Some(cell) {
                continue;
            }
            tuft.cell = Some(cell);
            match place(carpet, cell) {
                Some(placed) => {
                    *transform = placed;
                    *visibility = Visibility::Inherited;
                }
                None => *visibility = Visibility::Hidden,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cell_of_the_window_has_exactly_one_instance() {
        let side = 7;
        for low in [IVec2::ZERO, IVec2::new(-3, 11), IVec2::new(-100, -42)] {
            let mut cells: Vec<IVec2> =
                (0..side).flat_map(|j| (0..side).map(move |i| cell_of(IVec2::new(i, j), low, side))).collect();
            cells.sort_by_key(|c| (c.x, c.y));
            cells.dedup();
            assert_eq!(cells.len(), (side * side) as usize, "one per cell");
            assert!(cells.iter().all(|c| c.cmpge(low).all() && c.cmplt(low + side).all()), "all inside the window");
        }
    }

    #[test]
    fn moving_one_cell_moves_one_row_of_instances() {
        let side = 9;
        let moved = (0..side)
            .flat_map(|j| (0..side).map(move |i| IVec2::new(i, j)))
            .filter(|&slot| cell_of(slot, IVec2::ZERO, side) != cell_of(slot, IVec2::new(1, 0), side))
            .count();
        assert_eq!(moved, side as usize);
    }

    #[test]
    fn a_place_is_the_same_every_time_and_stays_in_its_cell() {
        let carpet = Carpet {
            model: Handle::default(),
            using: Use::Variants,
            spacing: 0.5,
            radius: 4.0,
            ground: Arc::new(|at: Vec2| (at.x > 0.0).then_some((0.25, 2.0))),
            tint: None,
        };
        let cell = IVec2::new(3, -2);
        let (a, b) = (place(&carpet, cell).unwrap(), place(&carpet, cell).unwrap());
        assert_eq!(a, b);
        let at = a.translation.xz() / carpet.spacing;
        assert!(at.floor().as_ivec2() == cell, "{at}");
        assert!((a.translation.y - 0.25).abs() < 1e-6 && (1.5..=2.5).contains(&a.scale.x));
        assert!(place(&carpet, IVec2::new(-3, 0)).is_none(), "nothing grows where the ground says so");
    }
}
