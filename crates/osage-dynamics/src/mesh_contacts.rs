// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Rapier contacts on finite triangle meshes, including exposed rims and seams between pages.

use crate::page_edges::{PageEdges, identity};
use rapier3d::parry::{
    math::Pose,
    query::{
        self as q,
        details::{NormalConstraints, contact_manifolds_trimesh_shape_shapes},
    },
    shape::{Shape, TriMesh, TriMeshBuilderError},
};
use rapier3d::prelude::*;

pub(crate) fn coplanar(a: Vector, b: Vector) -> bool {
    a.dot(b) > 0. && a.cross(b).length_squared() <= 1e-10
}

/// Weld equal positions and record actual face adjacency. Ambiguous triangle fans
/// retain their geometry and can still be checked locally.
pub fn collider(points: Vec<Vector>, indices: Vec<[u32; 3]>) -> Result<ColliderBuilder, TriMeshBuilderError> {
    let mut mesh = TriMesh::with_flags(points, indices, TriMeshFlags::FIX_INTERNAL_EDGES)?;
    // Parry keeps the triangles on a topology error. The query then checks the few
    // incident faces it needs directly, including nonmanifold or inconsistent fans.
    let _ = mesh.set_flags(mesh.flags() | TriMeshFlags::HALF_EDGE_TOPOLOGY);
    Ok(ColliderBuilder::new(SharedShape::new(mesh)))
}

/// The outer dispatcher delegates the BVH and persistent manifold handling to Parry.
/// A child carries the current mesh while Parry evaluates its individual triangles.
#[derive(Default)]
pub struct MeshQueries<'a> {
    mesh: Option<&'a TriMesh>,
    flipped: bool,
    pages: Option<PageEdges>,
    external: Option<&'a [u8]>,
}

impl MeshQueries<'static> {
    pub fn for_pages(pages: PageEdges) -> Self {
        Self { pages: Some(pages), ..Self::default() }
    }
}

// Rapier also uses this dispatcher for standard distance, intersection and sweep queries.
impl q::QueryDispatcher for MeshQueries<'_> {
    fn intersection_test(
        &self,
        relative_pose: &Pose,
        shape1: &dyn Shape,
        shape2: &dyn Shape,
    ) -> Result<q::ShapeIntersection, q::Unsupported> {
        q::DefaultQueryDispatcher.intersection_test(relative_pose, shape1, shape2)
    }
    fn distance(
        &self,
        relative_pose: &Pose,
        shape1: &dyn Shape,
        shape2: &dyn Shape,
    ) -> Result<q::ShapeDistance, q::Unsupported> {
        q::DefaultQueryDispatcher.distance(relative_pose, shape1, shape2)
    }
    fn contact(
        &self,
        relative_pose: &Pose,
        shape1: &dyn Shape,
        shape2: &dyn Shape,
        distance: f32,
    ) -> Result<Option<q::Contact>, q::Unsupported> {
        q::DefaultQueryDispatcher.contact(relative_pose, shape1, shape2, distance)
    }
    fn closest_points(
        &self,
        relative_pose: &Pose,
        shape1: &dyn Shape,
        shape2: &dyn Shape,
        distance: f32,
    ) -> Result<q::ClosestPoints, q::Unsupported> {
        q::DefaultQueryDispatcher.closest_points(relative_pose, shape1, shape2, distance)
    }
    fn cast_shapes(
        &self,
        relative_pose: &Pose,
        velocity: Vector,
        shape1: &dyn Shape,
        shape2: &dyn Shape,
        options: q::ShapeCastOptions,
    ) -> Result<Option<q::ShapeCastHit>, q::Unsupported> {
        q::DefaultQueryDispatcher.cast_shapes(relative_pose, velocity, shape1, shape2, options)
    }
    fn cast_shapes_nonlinear(
        &self,
        motion1: &q::NonlinearRigidMotion,
        shape1: &dyn Shape,
        motion2: &q::NonlinearRigidMotion,
        shape2: &dyn Shape,
        start: f32,
        end: f32,
        stop: bool,
    ) -> Result<Option<q::ShapeCastHit>, q::Unsupported> {
        q::DefaultQueryDispatcher.cast_shapes_nonlinear(motion1, shape1, motion2, shape2, start, end, stop)
    }
}

/// A smooth weld joins two coplanar triangles that traverse their shared edge in opposite directions.
fn boundary(mesh: &TriMesh, id: u32, n: Vector) -> [bool; 3] {
    let Some(topology) = mesh.topology() else {
        let ids = mesh.indices()[id as usize];
        return std::array::from_fn(|edge| {
            let (a, b) = (ids[edge], ids[(edge + 1) % 3]);
            let mut shared = false;
            for (other, tri) in mesh.indices().iter().enumerate() {
                if other == id as usize {
                    continue;
                }
                for j in 0..3 {
                    if (tri[j], tri[(j + 1) % 3]) == (a, b) {
                        return true;
                    }
                    if (tri[j], tri[(j + 1) % 3]) == (b, a) {
                        if shared || mesh.triangle(other as u32).normal().is_none_or(|m| !coplanar(n, m)) {
                            return true;
                        }
                        shared = true;
                    }
                }
            }
            !shared
        });
    };
    let mut half = topology.faces[id as usize].half_edge;
    std::array::from_fn(|_| {
        let e = &topology.half_edges[half as usize];
        half = e.next;
        let Some(twin) = topology.half_edges.get(e.twin as usize) else {
            return true;
        };
        mesh.triangle(twin.face).normal().is_none_or(|m| !coplanar(n, m))
    })
}

fn incident_to_segment(a: Vector, b: Vector, p: Vector, eps2: f32) -> bool {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0., 1.);
    (a + ab * t).distance_squared(p) <= eps2
}

impl<MD: Default + Clone + Send + Sync, CD: Default + Copy + Send + Sync> q::PersistentQueryDispatcher<MD, CD>
    for MeshQueries<'_>
{
    fn contact_manifolds(
        &self,
        relative_pose: &Pose,
        shape1: &dyn Shape,
        shape2: &dyn Shape,
        prediction: f32,
        manifolds: &mut Vec<q::ContactManifold<MD, CD>>,
        workspace: &mut Option<q::ContactManifoldsWorkspace>,
    ) -> Result<(), q::Unsupported> {
        let (mesh, other, flipped) = if let Some(mesh) = shape1.as_trimesh() {
            (mesh, shape2, false)
        } else if let Some(mesh) = shape2.as_trimesh() {
            (mesh, shape1, true)
        } else {
            return q::DefaultQueryDispatcher.contact_manifolds(
                relative_pose,
                shape1,
                shape2,
                prediction,
                manifolds,
                workspace,
            );
        };
        if !other.is_convex() || !mesh.flags().contains(TriMeshFlags::FIX_INTERNAL_EDGES) {
            return q::DefaultQueryDispatcher.contact_manifolds(
                relative_pose,
                shape1,
                shape2,
                prediction,
                manifolds,
                workspace,
            );
        }
        let pages = self.pages.as_ref().map(|relative_pose| relative_pose.0.read().unwrap());
        let external = pages.as_ref().and_then(|db| db.meshes.get(&identity(mesh))).map(|r| r.internal.as_slice());
        contact_manifolds_trimesh_shape_shapes(
            &MeshQueries { mesh: Some(mesh), flipped, external, pages: None },
            relative_pose,
            shape1,
            shape2,
            prediction,
            manifolds,
            workspace,
        );
        Ok(())
    }
    fn contact_manifold_convex_convex(
        &self,
        relative_pose: &Pose,
        shape1: &dyn Shape,
        shape2: &dyn Shape,
        constraints1: Option<&dyn NormalConstraints>,
        constraints2: Option<&dyn NormalConstraints>,
        prediction: f32,
        manifold: &mut q::ContactManifold<MD, CD>,
    ) -> Result<(), q::Unsupported> {
        let Some(mesh) = self.mesh else {
            return q::DefaultQueryDispatcher.contact_manifold_convex_convex(
                relative_pose,
                shape1,
                shape2,
                constraints1,
                constraints2,
                prediction,
                manifold,
            );
        };
        let face_id = if self.flipped { manifold.subshape2 } else { manifold.subshape1 };
        let triangle = mesh.triangle(face_id);
        let Some(face_normal) = triangle.normal() else {
            return q::DefaultQueryDispatcher.contact_manifold_convex_convex(
                relative_pose,
                shape1,
                shape2,
                constraints1,
                constraints2,
                prediction,
                manifold,
            );
        };
        let mut boundary = boundary(mesh, face_id, face_normal);
        if let Some(mask) = self.external.map(|projected| projected[face_id as usize]) {
            for (i, is_boundary) in boundary.iter_mut().enumerate() {
                *is_boundary &= mask & (1 << i) == 0;
            }
        }
        let vertices = [triangle.a, triangle.b, triangle.c];
        if boundary.into_iter().any(|shape2| shape2) {
            let (other, other_pose) =
                if self.flipped { (shape1, relative_pose.inverse()) } else { (shape2, *relative_pose) };
            let bounds = other.compute_aabb(&other_pose);
            let center = bounds.center();
            let half = bounds.half_extents();
            let near = std::array::from_fn::<_, 3, _>(|i| {
                if !boundary[i] {
                    return false;
                }
                let start = vertices[i];
                let edge = vertices[(i + 1) % 3] - start;
                let offset = center - start;
                let side = face_normal.cross(edge);
                let along = offset.dot(edge);
                let along_radius = half.dot(edge.abs());
                // A prior CCD step may have stopped the object at the edge. Prediction extends its search.
                offset.dot(side).abs() <= half.dot(side.abs()) + prediction * side.length()
                    && along + along_radius + prediction * edge.length() >= 0.
                    && along - along_radius - prediction * edge.length() <= edge.length_squared()
            });
            if near.into_iter().any(|mesh_point| mesh_point) {
                // The solver can label a clipped witness as Face or Unknown. Locate the
                // unconstrained witness on the finite triangle to identify a physical rim.
                let mut raw: q::ContactManifold<(), CD> = q::ContactManifold::with_data(0, 0, ());
                q::DefaultQueryDispatcher.contact_manifold_convex_convex(
                    relative_pose,
                    shape1,
                    shape2,
                    None,
                    None,
                    prediction,
                    &mut raw,
                )?;
                let normal = if self.flipped { raw.local_n2 } else { raw.local_n1 };
                let scale = triangle.a.abs().max(triangle.b.abs()).max(triangle.c.abs()).max_element().max(1.);
                let tolerance_squared = (4. * f32::EPSILON * scale).powi(2);
                if normal.cross(face_normal).length_squared() > 1e-10
                    && raw.points.iter().any(|contact| {
                        let mesh_point = if self.flipped { contact.local_p2 } else { contact.local_p1 };
                        (0..3).any(|i| {
                            near[i]
                                && incident_to_segment(
                                    vertices[i],
                                    vertices[(i + 1) % 3],
                                    mesh_point,
                                    tolerance_squared,
                                )
                        })
                    })
                {
                    if raw.local_n1.dot(manifold.local_n1) > 0.99999 && raw.local_n2.dot(manifold.local_n2) > 0.99999 {
                        raw.match_contacts(&manifold.points);
                    }
                    manifold.local_n1 = raw.local_n1;
                    manifold.local_n2 = raw.local_n2;
                    manifold.points = raw.points;
                    return Ok(());
                }
            }
        }
        // A previously exposed edge can become an interior seam as a page arrives.
        // Invalidate a cached tangent that falls outside the restored face-normal cone.
        let previous = if self.flipped { manifold.local_n2 } else { manifold.local_n1 };
        if !manifold.points.is_empty()
            && let Some(constraint) = if self.flipped { constraints2 } else { constraints1 }
            && constraint
                .project_local_normal(previous)
                .is_none_or(|projected| projected.distance_squared(previous) > 1e-8)
        {
            manifold.clear();
        }
        q::DefaultQueryDispatcher.contact_manifold_convex_convex(
            relative_pose,
            shape1,
            shape2,
            constraints1,
            constraints2,
            prediction,
            manifold,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn slab() -> (Vec<Vector>, Vec<[u32; 3]>) {
        (
            vec![
                Vector::new(-1., 1., 1.),
                Vector::new(1., 1., 1.),
                Vector::new(1., 1., -1.),
                Vector::new(-1., 1., -1.),
            ],
            vec![[0, 1, 2], [0, 2, 3]],
        )
    }
    #[test]
    fn shared_diagonal_is_smooth_and_its_exposed_endpoints_and_creases_remain_finite() {
        let (mut points, triangles) = slab();
        let check = |points: Vec<Vector>, ts: Vec<[u32; 3]>| {
            let c = collider(points, ts).unwrap().build();
            let mesh = c.shape().as_trimesh().unwrap();
            boundary(mesh, 0, mesh.triangle(0).normal().unwrap())
        };
        assert_eq!(check(points.clone(), triangles.clone()), [true, true, false]);
        assert_eq!(check(points.clone(), vec![[0, 1, 2], [0, 2, 3], [0, 2, 3]]), [true, true, true]);
        points[3].y += 0.1;
        assert_eq!(check(points, triangles), [true, true, true]);
    }
    #[test]
    fn high_speed_capsule_hits_the_finite_rim_and_clears_the_outside_control() {
        for dt in [1. / 20., 1. / 60.] {
            for (x, hit) in [(1.14, true), (1.30, false)] {
                let mut sim = PhysicsWorld::new();
                sim.integration_parameters.dt = dt;
                sim.gravity = Vector::ZERO;
                sim.narrow_phase.set_query_dispatcher(MeshQueries::default());
                let (points, triangles) = slab();
                sim.insert_collider(collider(points, triangles).unwrap(), None);
                let (body, cap) = sim.insert(
                    RigidBodyBuilder::dynamic()
                        .translation(Vector::new(x, 2., 0.2))
                        .linvel(-55. * Vector::Y)
                        .ccd_enabled(true)
                        .soft_ccd_prediction(0.28),
                    ColliderBuilder::capsule_y(0.60, 0.28).mass(75.),
                );
                let mut impulse = 0.;
                for _ in 0..4 {
                    sim.step();
                    for pair in sim.contact_pairs_with(cap) {
                        for m in pair.solver_manifolds() {
                            for c in &m.data.solver_contacts {
                                impulse += m.points[c.contact_indices()[0] as usize].data.impulse;
                            }
                        }
                    }
                }
                assert_eq!(impulse > 1., hit);
                assert_eq!(sim.bodies[body].linvel().x > 0.1, hit);
            }
        }
    }
}
