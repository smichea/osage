// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Certify closed acoustic shells only in the neighbourhood of a crossed surface.
use super::{AcousticEdge, AcousticGeometry, AcousticIndex, Crossing, edges, solids::Solids};
use osage_dynamics::StaticMesh;
use rapier3d::parry::bounding_volume::BoundingVolume;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex, OnceLock},
};

pub(super) struct Regions {
    slots: Vec<OnceLock<Arc<Solids>>>,
    discovery: Mutex<()>,
}

#[derive(Default, Serialize)]
pub(super) struct Storage {
    regions: usize,
    surfaces: usize,
    triangles: usize,
    certified_shells: usize,
    retained_vertices: usize,
}

impl Regions {
    pub(super) fn new(count: usize) -> Self {
        Self { slots: (0..count).map(|_| OnceLock::new()).collect(), discovery: Default::default() }
    }

    pub(super) fn for_binding(
        &self,
        index: &AcousticIndex,
        object: &AcousticGeometry,
        colliders: &[StaticMesh],
        binding: usize,
    ) -> Option<&Solids> {
        let slot = &self.slots[binding];
        if let Some(known) = slot.get() {
            return Some(known);
        }
        if !edges::blocks(object.acoustics[binding].transmission) {
            return None;
        }
        // Overlapping requests must publish one topology for the complete group.
        let _guard = self.discovery.lock().expect("acoustic region discovery");
        if let Some(known) = slot.get() {
            return Some(known);
        }
        let mut members = BTreeSet::from([binding]);
        let mut pending = vec![binding];
        while let Some(i) = pending.pop() {
            let surface = &object.acoustics[i];
            let Some(bounds) = surface.spatial.bounds(surface.mesh(colliders)) else { continue };
            // Two triangles sharing even one point have intersecting closed bounds.
            // Following the transitive closure also finds the far wall of a split room.
            for leaf in index.tree.leaves(|node| bounds.intersects(&node.aabb())) {
                let (next, placement) = &index.surfaces[leaf as usize];
                let next = *next;
                if edges::blocks(object.acoustics[next].transmission)
                    && placement.bounds.get().copied().flatten().is_some_and(|b| bounds.intersects(&b))
                    && members.insert(next)
                {
                    pending.push(next);
                }
            }
        }
        let topology = Arc::new(Solids::new(object, colliders, &members));
        for i in members {
            assert!(self.slots[i].set(Arc::clone(&topology)).is_ok());
        }
        slot.get().map(AsRef::as_ref)
    }

    pub(super) fn edges(
        &self,
        index: &AcousticIndex,
        object: &AcousticGeometry,
        colliders: &[StaticMesh],
        crossings: &[Crossing],
    ) -> Vec<AcousticEdge> {
        let by_id = index.binding_ids(&object.acoustics);
        let mut hit = BTreeSet::new();
        let mut selected = BTreeMap::new();
        for crossing in crossings {
            let Some(&binding) = by_id.get(&crossing.id) else { continue };
            if !edges::blocks(crossing.transmission) || object.acoustics[binding].owner_path != crossing.owner_path {
                continue;
            }
            if let Some(topology) = self.for_binding(index, object, colliders, binding)
                && let Some(shells) = topology.by_binding.get(&binding)
            {
                hit.insert(binding);
                for &shell in crossing.shells.as_ref().unwrap_or(shells) {
                    selected.insert(shell, topology);
                }
            }
        }
        let mut result = Vec::new();
        for (shell, topology) in selected {
            topology.append_edges(shell, &object.acoustics, &hit, &mut result);
        }
        result
    }

    pub(super) fn storage(&self) -> Storage {
        let mut stats = Storage::default();
        for (i, slot) in self.slots.iter().enumerate() {
            if let Some(topology) = slot.get() {
                stats.surfaces += 1;
                if topology.first_binding == i {
                    stats.regions += 1;
                    stats.triangles += topology.triangle_count;
                    let (shells, vertices) = topology.retained_geometry();
                    stats.certified_shells += shells;
                    stats.retained_vertices += vertices;
                }
            }
        }
        stats
    }
}
