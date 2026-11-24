// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Shared physical edges of adjacent world-space pages.

use crate::mesh_contacts::coplanar;
use rapier3d::{parry::shape::TriMesh, prelude::*};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

type Key = (Option<usize>, Arc<str>, [u32; 3], [u32; 3]);
#[derive(Clone, Copy)]
struct Side {
    mesh: usize,
    page: [i32; 2],
    tri: usize,
    edge: usize,
    forward: bool,
    normal: Vector,
}

#[derive(Default)]
pub(crate) struct Record {
    pub(crate) internal: Vec<u8>,
    keys: Vec<Key>,
}

#[derive(Default)]
pub(crate) struct Data {
    pub(crate) meshes: HashMap<usize, Record>,
    segments: HashMap<Key, Vec<Side>>,
}

/// Adjacency of currently installed pieces of the same source surface.
#[derive(Clone, Default)]
pub struct PageEdges(pub(crate) Arc<RwLock<Data>>);

impl Data {
    fn update(&mut self, key: &Key) {
        let Some(uses) = self.segments.get(key) else {
            return;
        };
        let shared = match uses.as_slice() {
            [left, right] => {
                left.page != right.page && left.forward != right.forward && coplanar(left.normal, right.normal)
            }
            _ => false,
        };
        for s in uses {
            if let Some(record) = self.meshes.get_mut(&s.mesh) {
                let mask = &mut record.internal[s.tri];
                if shared {
                    *mask |= 1 << s.edge;
                } else {
                    *mask &= !(1 << s.edge);
                }
            }
        }
    }
}

// A SharedShape owns the mesh at a stable address until removal from the world.
// This is an identity key. No address stored here is dereferenced.
pub(crate) fn identity(mesh: &TriMesh) -> usize {
    mesh as *const TriMesh as usize
}

impl PageEdges {
    pub fn add(&self, mesh: &TriMesh, owner: Option<usize>, surface: &str, page: [i32; 2]) {
        let topology = mesh.topology();
        // An inconsistent fan elsewhere in a legacy mesh can prevent a global half-edge table.
        // Count each undirected edge once here to recover its valid open page cuts.
        let mut multiplicity = HashMap::new();
        if topology.is_none() {
            for ids in mesh.indices() {
                for i in 0..3 {
                    let (a, b) = (ids[i], ids[(i + 1) % 3]);
                    *multiplicity.entry((a.min(b), a.max(b))).or_insert(0u32) += 1;
                }
            }
        }
        let id = identity(mesh);
        let mut db = self.0.write().unwrap();
        if db.meshes.contains_key(&id) {
            return;
        }
        let source: Arc<str> = surface.into();
        let mut keys = Vec::new();
        db.meshes.insert(id, Record { internal: vec![0; mesh.num_triangles()], keys: Vec::new() });
        for (t, ids) in mesh.indices().iter().enumerate() {
            let Some(normal) = mesh.triangle(t as u32).normal() else {
                continue;
            };
            let mut half_edge = topology.map_or(0, |topo| topo.faces[t].half_edge);
            for i in 0..3 {
                let (a, b) = (ids[i], ids[(i + 1) % 3]);
                let open = if let Some(topo) = topology {
                    let edge = topo.half_edges[half_edge as usize];
                    half_edge = edge.next;
                    edge.twin == u32::MAX
                } else {
                    multiplicity[&(a.min(b), a.max(b))] == 1
                };
                if !open {
                    continue;
                }
                let bits = |v: Vector| v.to_array().map(|x| if x == 0. { 0 } else { x.to_bits() });
                let a = bits(mesh.vertices()[ids[i] as usize]);
                let b = bits(mesh.vertices()[ids[(i + 1) % 3] as usize]);
                let key = (owner, source.clone(), a.min(b), a.max(b));
                db.segments.entry(key.clone()).or_default().push(Side {
                    mesh: id,
                    page,
                    tri: t,
                    edge: i,
                    forward: a < b,
                    normal,
                });
                db.update(&key);
                keys.push(key);
            }
        }
        db.meshes.get_mut(&id).unwrap().keys = keys;
    }

    pub fn remove(&self, mesh: &TriMesh) {
        let mut db = self.0.write().unwrap();
        let id = identity(mesh);
        if let Some(record) = db.meshes.remove(&id) {
            for key in record.keys {
                if let Some(uses) = db.segments.get_mut(&key) {
                    uses.retain(|s| s.mesh != id);
                    if uses.is_empty() {
                        db.segments.remove(&key);
                    } else {
                        db.update(&key);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh_contacts::collider;
    #[test]
    fn neighboring_parts_share_only_opposite_edges_from_the_same_owner_until_eviction() {
        let p = |x, z| Vector::new(x, 0., z);
        let west = collider(vec![p(-1., 0.), p(0., 1.), p(0., 0.)], vec![[0, 1, 2]]).unwrap().build();
        let east = collider(vec![p(0., 0.), p(0., 1.), p(1., 0.)], vec![[0, 1, 2]]).unwrap().build();
        let west = west.shape().as_trimesh().unwrap();
        let east = east.shape().as_trimesh().unwrap();
        let registry = PageEdges::default();
        let mask = |m| registry.0.read().unwrap().meshes[&identity(m)].internal[0];
        registry.add(west, Some(4), "road", [-1, 0]);
        registry.add(east, Some(5), "road", [0, 0]);
        assert_eq!(mask(west), 0);
        registry.remove(east);
        registry.add(east, Some(4), "road", [0, 0]);
        assert_eq!(mask(west), 2);
        assert_eq!(mask(east), 1);
        registry.remove(east);
        assert_eq!(mask(west), 0);
    }
}
