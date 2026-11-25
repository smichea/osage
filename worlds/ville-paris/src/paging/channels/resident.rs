// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Verified acoustic pages placed on their original model nodes for impact propagation.
use super::{Acoustic, Faces, Profile, Spatial};
use crate::object::{AcousticBinding, AcousticGeometry};
use crate::paging::tiles::Index;
use anyhow::{Context, Result, ensure};
use osage_dynamics::StaticMesh;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(crate) type Bindings = BTreeSet<usize>;

pub(crate) struct Catalog {
    group: Vec<usize>,
    members: BTreeMap<usize, Vec<usize>>,
    by_id: BTreeMap<String, usize>,
    paths: Vec<Vec<String>>,
    materials: BTreeMap<usize, BTreeMap<String, (Vec<String>, String)>>,
}

fn root(parents: &mut [usize], mut i: usize) -> usize {
    while i != parents[i] {
        parents[i] = parents[parents[i]];
        i = parents[i];
    }
    i
}

impl Catalog {
    pub(crate) fn new(profile: &Profile, idx: &Index) -> Result<Self> {
        let spatial = profile.spatial.get_or_init(|| Spatial::new(profile));
        let names: BTreeMap<_, _> = profile.nodes.iter().map(|n| (n.node, n.id.as_str())).collect();
        let mut paths = Vec::new();
        let mut materials = BTreeMap::new();
        for n in &profile.nodes {
            let path = idx
                .path(Some(n.node))
                .into_iter()
                .map(|i| names.get(&i).context("missing acoustic ancestor").map(|s| (*s).to_owned()))
                .collect::<Result<Vec<_>>>()?;
            for c in &n.collision {
                materials
                    .entry(n.node)
                    .or_insert_with(BTreeMap::new)
                    .insert(c.id.clone(), (path.clone(), c.material.clone()));
            }
            paths.push(path);
        }
        let mut by_id = BTreeMap::new();
        let mut blocking = Vec::new();
        for (i, b) in spatial.bindings.iter().enumerate() {
            let a = &profile.nodes[b.node].acoustic[b.surface];
            by_id.insert(a.surface.id.clone(), i);
            if profile.acoustic_materials[&a.surface.material]
                .transmission
                .is_some_and(|t| t.into_iter().any(|v| v < 1.))
            {
                // Share the ray index's enclosure, including conservative legacy transforms.
                let (lo, hi) = b.bounds.map_or(([-f64::INFINITY; 3], [f64::INFINITY; 3]), |bounds| {
                    let lo = bounds.mins.to_array().map(f64::from);
                    let hi = bounds.maxs.to_array().map(f64::from);
                    let pad =
                        lo.into_iter().chain(hi).fold(1_f64, |n, v| n.max(v.abs())) * 8. * f64::from(f32::EPSILON);
                    (lo.map(|v| v - pad), hi.map(|v| v + pad))
                });
                blocking.push((i, lo, hi));
            }
        }
        // Missing adjacent triangles can turn a common seam into an apparent opening.
        // Include all possibly connected blocking surfaces before testing a boundary.
        blocking.sort_by(|a, b| a.1[0].total_cmp(&b.1[0]));
        let mut group: Vec<_> = (0..spatial.bindings.len()).collect();
        for (n, &(i, lo, hi)) in blocking.iter().enumerate() {
            for &(j, other_lo, other_hi) in &blocking[n + 1..] {
                if other_lo[0] > hi[0] {
                    break;
                }
                if (1..3).all(|k| lo[k] <= other_hi[k] && other_lo[k] <= hi[k]) {
                    let a = root(&mut group, i);
                    let b = root(&mut group, j);
                    group[b] = a;
                }
            }
        }
        let mut members = BTreeMap::<_, Vec<_>>::new();
        for i in 0..group.len() {
            let r = root(&mut group, i);
            members.entry(r).or_default().push(i);
        }
        Ok(Self { group, members, by_id, paths, materials })
    }

    pub(crate) fn add_rays(
        &self,
        profile: &Profile,
        rays: &[([f32; 3], [f32; 3])],
        blockers: &BTreeSet<String>,
        set: &mut Bindings,
    ) -> Result<()> {
        for &ray in rays {
            if ray.0 != ray.1 {
                set.extend(profile.candidates(ray)?.bindings);
            }
        }
        for id in blockers {
            if let Some(&slot) = self.by_id.get(id) {
                set.extend(&self.members[&self.group[slot]]);
            }
        }
        Ok(())
    }

    pub(crate) fn material(&self, owner: Option<usize>, id: &str) -> (Option<Vec<String>>, &str) {
        owner
            .and_then(|n| self.materials.get(&n))
            .and_then(|m| m.get(id))
            .map(|(p, m)| (Some(p.clone()), m.as_str()))
            .unwrap_or((None, "default"))
    }

    pub(crate) fn resource(&self, profile: &Profile, placement: usize) -> usize {
        profile.spatial.get().expect("catalogue initialized").bindings[placement].resource
    }

    pub(crate) fn place(&self, profile: &Profile, id: usize, data: &Page) -> Result<AcousticBinding> {
        let b = &profile.spatial.get().expect("catalogue initialized").bindings[id];
        let node = &profile.nodes[b.node];
        let a = &node.acoustic[b.surface];
        let mesh = data.placed_mesh(a, profile.placed_bounds)?;
        Ok(AcousticBinding::resident(
            node.node,
            self.paths[b.node].clone(),
            &a.surface,
            profile.acoustic_materials[&a.surface.material].transmission,
            Arc::new(mesh),
        ))
    }
}

pub(crate) struct Page(Faces);

impl Page {
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self> {
        let f = Faces::decode(bytes)?;
        ensure!(f.vertices.len() <= u32::MAX as usize, "too many acoustic vertices");
        Ok(Self(f))
    }

    fn placed_mesh(&self, surface: &Acoustic, placed_bounds: bool) -> Result<StaticMesh> {
        let reflected = surface.pose.matrix()?.determinant() < 0.;
        let vertices = self.0.placed_vertices(surface, placed_bounds)?;
        let triangles = self
            .0
            .triangles
            .iter()
            .map(|face| {
                let [a, b, c] = face.map(|i| i as u32);
                if reflected { [a, c, b] } else { [a, b, c] }
            })
            .collect();
        Ok(StaticMesh { id: surface.surface.id.clone(), vertices, triangles, material: None })
    }
}

pub(crate) fn geometry(parts: &Bindings, placed: &BTreeMap<usize, AcousticBinding>) -> AcousticGeometry {
    AcousticGeometry::from_bindings(parts.iter().map(|i| placed[i].clone()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::{Mat4, Vec3};
    use serde_json::json;

    #[test]
    fn reflected_acoustic_placements_preserve_face_orientation() {
        let page = Page::decode(br#"{"vertices":[[0,0,0],[1,0,0],[0,1,0]],"triangles":[[0,1,2]]}"#).unwrap();
        let mirror = Mat4::from_cols_array(&[-2., 0., 0., 0., 1., 3., 0., 0., 0., 0., 1., 0., 4., -2., 1., 1.]);
        for (matrix, order) in [
            (Mat4::IDENTITY, [0, 1, 2]),
            (mirror, [0, 2, 1]),
            (mirror * Mat4::from_scale(Vec3::new(1., -1., 1.)), [0, 1, 2]),
        ] {
            let corners = [Vec3::ZERO, Vec3::X, Vec3::Y].map(|p| matrix.transform_point3(p));
            let [a, b, c] = corners;
            let lo = a.min(b).min(c);
            let hi = a.max(b).max(c);
            let surface: Acoustic = serde_json::from_value(json!({
                "id": "panel", "mesh": 0, "source": "authored", "material": "wall", "resource": 0,
                "bounds": [lo.to_array(), hi.to_array()], "pose": {"matrix": matrix.to_cols_array()},
            }))
            .unwrap();
            let mesh = page.placed_mesh(&surface, true).unwrap();
            assert_eq!(mesh.vertices, corners.map(|v| v.to_array()));
            assert_eq!(mesh.triangles, [order]);
            let [a, b, c] = order.map(|i| corners[i as usize]);
            assert!((b - a).cross(c - a).dot(Vec3::Z) > 0.);
        }
        assert_eq!(page.0.triangles, [[0, 1, 2]]);
    }
}
