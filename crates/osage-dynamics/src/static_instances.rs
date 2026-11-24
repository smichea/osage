// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

use super::StaticMesh;
use anyhow::Result;
use glam::Vec3;
use rapier3d::parry::{bounding_volume::Aabb, partitioning::Bvh};
use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};

/// Compare the complete ordered topology and translation-invariant geometry. Baked rotations,
/// reflections and shears can share only if the remaining difference is an exact translation.
fn key(mesh: &StaticMesh, relative: bool) -> u64 {
    let mut h = DefaultHasher::new();
    mesh.triangles.hash(&mut h);
    let anchor = if relative { mesh.vertices[0] } else { [0.; 3] };
    mesh.vertices.len().hash(&mut h);
    for p in &mesh.vertices {
        for axis in 0..3 {
            (f64::from(p[axis]) - f64::from(anchor[axis])).to_bits().hash(&mut h);
        }
    }
    h.finish()
}

fn translation(from: &StaticMesh, to: &StaticMesh) -> Option<[f32; 3]> {
    if from.vertices.len() != to.vertices.len() || from.triangles != to.triangles {
        return None;
    }
    let d: [f32; 3] = std::array::from_fn(|i| to.vertices[0][i] - from.vertices[0][i]);
    for (reference, actual) in from.vertices.iter().zip(&to.vertices) {
        for i in 0..3 {
            let (a, b) = (reference[i], actual[i]);
            if f64::from(b) - f64::from(a) != f64::from(d[i])
                || (d[i] == 0. && a.to_bits() != b.to_bits())
                || d[i] != 0. && ((a + d[i]).to_bits() != b.to_bits() || (b - d[i]).to_bits() != a.to_bits())
            {
                return None;
            }
        }
    }
    Some(d)
}

/// A world-space box enclosing the isolated actor and the full centre displacement considered by its next native step.
pub(super) struct Reach(Aabb);
impl Reach {
    pub fn new(
        center: [f32; 3],
        velocity: [f32; 3],
        radius: f32,
        dt: f32,
        gravity: f32,
        speed_cap: f32,
        margin: f32,
    ) -> Self {
        let center = Vec3::from(center);
        let speed = Vec3::from(velocity).as_dvec3().length() + f64::from(gravity) * f64::from(dt);
        let extent = (speed.max(f64::from(speed_cap)) * f64::from(dt) + f64::from(radius) + f64::from(margin)) as f32;
        let pad = center.abs().max(Vec3::splat(extent)).max(Vec3::ONE) * (16. * f32::EPSILON);
        let half = Vec3::splat(extent) + pad;
        Self(Aabb::new((center - half).to_array().into(), (center + half).to_array().into()))
    }
    fn overlaps(&self, lo: [f32; 3], hi: [f32; 3]) -> bool {
        Vec3::from(lo).cmple(Vec3::from_array(self.0.maxs.to_array())).all()
            && Vec3::from(hi).cmpge(Vec3::from_array(self.0.mins.to_array())).all()
    }
}

struct Template {
    remaining: usize,
    vertices: Vec<[f32; 3]>,
    triangles: Vec<[u32; 3]>,
}
struct Pending<H> {
    handle: H,
    template: usize,
    delta: [f32; 3],
    id: String,
    promoted: bool,
}
pub(super) struct Deferred<H> {
    remaining: usize,
    tree: Bvh,
    copies: Vec<Pending<H>>,
    templates: HashMap<usize, Template>,
}
impl<H: Copy> Deferred<H> {
    pub fn is_empty(&self) -> bool {
        self.remaining == 0
    }
    /// Promote before the solver can visit the placement. Its handle and surface binding survive.
    pub fn admit(&mut self, reach: &Reach, mut install: impl FnMut(H, &StaticMesh) -> Result<()>) -> Result<()> {
        for slot in self.tree.leaves(|n| reach.overlaps(n.mins().to_array(), n.maxs().to_array())) {
            let copy = &mut self.copies[slot as usize];
            if copy.promoted {
                continue;
            }
            let source = self.templates.get_mut(&copy.template).expect("live deferred mesh template");
            let vertices = source
                .vertices
                .iter()
                .map(|p| std::array::from_fn(|i| if copy.delta[i] == 0. { p[i] } else { p[i] + copy.delta[i] }))
                .collect();
            let mesh =
                StaticMesh { id: copy.id.clone(), material: None, vertices, triangles: source.triangles.clone() };
            install(copy.handle, &mesh)?;
            copy.promoted = true;
            self.remaining -= 1;
            source.remaining -= 1;
            if source.remaining == 0 {
                self.templates.remove(&copy.template);
            }
        }
        if self.remaining == 0 {
            self.tree = Bvh::default();
            self.copies = Vec::new();
        }
        Ok(())
    }
}

/// Build source geometry once for far placements. Without a certified reach, only bit-identical
/// shapes in the identical world frame can share a native allocation.
pub(super) fn insert_shapes<S, H: Copy>(
    meshes: &[StaticMesh],
    reach: Option<&Reach>,
    mut build: impl FnMut(&StaticMesh) -> Result<S>,
    mut install: impl FnMut(usize, &StaticMesh, &S, [f32; 3]) -> Result<H>,
) -> Result<Deferred<H>> {
    let mut groups: HashMap<u64, Vec<(usize, S)>> = HashMap::new();
    let mut bounds = Vec::new();
    let mut copies = Vec::new();
    let mut templates = HashMap::new();
    for (i, mesh) in meshes.iter().enumerate() {
        let candidates = groups.entry(key(mesh, reach.is_some())).or_default();
        if let Some((original, shape, offset)) = candidates
            .iter()
            .find_map(|(source, shape)| translation(&meshes[*source], mesh).map(|offset| (*source, shape, offset)))
        {
            if offset == [0.; 3] {
                install(i, mesh, shape, offset)?;
                continue;
            }
            let (lo, hi) = mesh
                .vertices
                .iter()
                .copied()
                .map(Vec3::from)
                .fold((Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)), |(lo, hi), p| {
                    (lo.min(p), hi.max(p))
                });
            if reach.is_some_and(|r| !r.overlaps(lo.to_array(), hi.to_array())) {
                let handle = install(i, mesh, shape, offset)?;
                let template = templates.entry(original).or_insert_with(|| Template {
                    remaining: 0,
                    vertices: meshes[original].vertices.clone(),
                    triangles: meshes[original].triangles.clone(),
                });
                template.remaining += 1;
                copies.push(Pending {
                    handle,
                    template: original,
                    delta: offset,
                    id: mesh.id.clone(),
                    promoted: false,
                });
                bounds.push(Aabb::new(lo.to_array().into(), hi.to_array().into()));
            } else {
                install(i, mesh, &build(mesh)?, [0.; 3])?;
            }
        } else {
            let shape = build(mesh)?;
            install(i, mesh, &shape, [0.; 3])?;
            candidates.push((i, shape));
        }
    }
    Ok(Deferred { remaining: copies.len(), tree: Bvh::from_leaves(Default::default(), &bounds), copies, templates })
}
#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use rapier3d::prelude::{ColliderBuilder, PhysicsWorld, SharedShape};
    use std::sync::Arc;

    fn sheet(d: f32) -> StaticMesh {
        StaticMesh {
            id: format!("sheet-{d}"),
            material: None,
            vertices: vec![[d, 0., 0.], [d, 0., 2.], [d + 2., 0., 0.], [d + 2., 0., 2.]],
            triangles: vec![[0, 1, 2], [2, 1, 3]],
        }
    }

    #[test]
    fn native_colliders_keep_independent_poses_materials_and_lifetimes() {
        let mut sim = PhysicsWorld::new();
        let meshes = [sheet(0.), sheet(16.)];
        let mut handles = Vec::new();
        let mut builds = 0;
        insert_shapes(
            &meshes,
            Some(&Reach::new([1000., 0., 0.], [0.; 3], 1., 1. / 60., 9.81, 400., 1.)),
            |m| {
                builds += 1;
                Ok(SharedShape::trimesh(m.vertices.iter().copied().map(Vec3::from).collect(), m.triangles.clone())?)
            },
            |slot, _, shape, delta| {
                handles.push(
                    sim.insert_collider(
                        ColliderBuilder::new(shape.clone())
                            .translation(delta.into())
                            .restitution(slot as f32 * 0.5)
                            .user_data(slot as u128),
                        None,
                    ),
                );
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(builds, 1);
        let (a, b) = (&sim.colliders[handles[0]], &sim.colliders[handles[1]]);
        assert!(Arc::ptr_eq(&a.shared_shape().0, &b.shared_shape().0));
        assert_eq!((a.user_data, b.user_data), (0, 1));
        assert_eq!(b.position().translation.x, 16.);
        assert_eq!((a.restitution(), b.restitution()), (0., 0.5));
        drop(sim.remove_collider(handles[0]));
        let remaining = &sim.colliders[handles[1]];
        assert_eq!(remaining.compute_aabb().mins.x, 16.);
        assert_eq!(remaining.shape().as_trimesh().unwrap().indices(), meshes[1].triangles);
    }

    #[test]
    fn pending_translations_restore_each_exact_world_vertex_once_before_contact() {
        let meshes = [sheet(0.), sheet(16.), sheet(32.)];
        let area = |x| Reach::new([x, 0., 0.], [0.; 3], 1., 1. / 60., 9.81, 2., 0.02);
        let mut locations = Vec::new();
        let mut d = insert_shapes(
            &meshes,
            Some(&area(0.)),
            |m| Ok(m.id.clone()),
            |i, _, s, at| {
                locations.push((s.clone(), at));
                Ok(i)
            },
        )
        .unwrap();
        assert_eq!((d.remaining, d.templates.len()), (2, 1));
        let mut admitted = Vec::new();
        for x in [16., 16., 32.] {
            d.admit(&area(x), |i, m| {
                assert_eq!(m.vertices, meshes[i].vertices);
                assert_eq!(m.triangles, meshes[i].triangles);
                admitted.push(i);
                Ok(())
            })
            .unwrap();
        }
        assert_eq!(admitted, vec![1, 2]);
        assert!(d.is_empty() && d.templates.is_empty() && d.tree.is_empty());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn jolt_removing_a_body_preserves_the_other_placed_shape() {
        let mut world = oxijolt::PhysicsWorld::new(oxijolt::WorldSettings::default().single_threaded()).unwrap();
        let meshes = [sheet(0.), sheet(16.)];
        let mut ids = Vec::new();
        let mut built = 0;
        insert_shapes(
            &meshes,
            Some(&Reach::new([1000., 0., 0.], [0.; 3], 1., 1. / 60., 9.81, 500., 1.)),
            |mesh| {
                built += 1;
                let points = mesh.vertices.iter().copied().map(oxijolt::Vec3::from).collect::<Vec<_>>();
                Ok(oxijolt::Shape::new_mesh(&points, &mesh.triangles)?.0)
            },
            |_, _, s, delta| {
                let id = world.create_body(s, &oxijolt::BodySettings::new_static().position(delta.into()))?;
                ids.push(id);
                Ok(id)
            },
        )
        .unwrap();
        assert_eq!(built, 1);
        world.remove_body(ids[0]).unwrap();
        let ray = oxijolt::RayCast::new([16.5, 2., 0.5].into(), [0., -4., 0.].into());
        let hit = world.cast_ray(&ray, &Default::default()).unwrap().unwrap();
        assert_eq!(hit.body, ids[1]);
        assert_eq!(hit.fraction, 0.5);
    }

    #[test]
    fn index_winding_height_and_nonrepresentable_offsets_keep_original_shapes() {
        let a = sheet(0.);
        let mut other = sheet(16.);
        other.triangles.swap(0, 1);
        assert!(translation(&a, &other).is_none());
        let mut other = sheet(16.);
        other.triangles[0].swap(0, 1);
        assert!(translation(&a, &other).is_none());
        let mut other = sheet(16.);
        other.vertices[3][1] = f32::from_bits(1);
        assert!(translation(&a, &other).is_none());
        let far = sheet(16777216.);
        let shifted = sheet(-1.);
        assert!(translation(&shifted, &far).is_none());
    }
}
