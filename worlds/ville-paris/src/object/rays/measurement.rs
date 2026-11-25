// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::mesh::Source;
use super::{AcousticMesh, Index, Sector, StaticMesh};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::BTreeSet,
    env,
    ffi::{c_int, c_long},
    fs,
    hint::black_box,
    sync::Arc,
};

#[repr(C)]
struct Time {
    sec: c_long,
    ns: c_long,
}
#[repr(C)]
#[derive(Default)]
struct MallInfo {
    arena: usize,
    ordblks: usize,
    smblks: usize,
    hblks: usize,
    hblkhd: usize,
    usmblks: usize,
    fsmblks: usize,
    uordblks: usize,
    fordblks: usize,
    keepcost: usize,
}

unsafe extern "C" {
    fn clock_gettime(clk: c_int, target: *mut Time) -> c_int;
    fn mallinfo2() -> MallInfo;
}

fn cpu() -> u64 {
    let mut t = Time { sec: 0, ns: 0 };
    // SAFETY: CLOCK_THREAD_CPUTIME_ID on Linux LP64 accepts a writable pair of long integers.
    assert_eq!(unsafe { clock_gettime(3, &mut t) }, 0);
    t.sec as u64 * 1_000_000_000 + t.ns as u64
}
fn heap() -> usize {
    // SAFETY: GNU libc returns the allocation statistics by value and takes no pointers.
    let usage = unsafe { mallinfo2() };
    usage.uordblks + usage.hblkhd
}
fn elapsed<T>(f: impl FnOnce() -> T) -> (T, u64) {
    let before = cpu();
    let value = f();
    (value, cpu() - before)
}

#[derive(Deserialize)]
struct Probe {
    kind: String,
    start: [f32; 3],
    end: [f32; 3],
}

fn storage(mesh: &StaticMesh) -> usize {
    mesh.vertices.capacity() * size_of::<[f32; 3]>() + mesh.triangles.capacity() * size_of::<[u32; 3]>()
}

#[test]
#[ignore = "measured by check-object-ray-scaling.py in fresh pinned processes"]
fn measure_instanced_rays() {
    let data = fs::read(env::var_os("OSAGE_OBJECT_RAY_SCENE").unwrap()).unwrap();
    let input = fs::read(env::var_os("OSAGE_OBJECT_RAY_PATHS").unwrap()).unwrap();
    let probes: Vec<Probe> = serde_json::from_slice(&input).unwrap();
    let rounds: usize = env::var("OSAGE_OBJECT_RAY_ROUNDS").unwrap().parse().unwrap();
    let mut times = Vec::new();
    let mut metrics = None;
    for round in 0..rounds {
        let before_scene = heap();
        let (scene, load) = elapsed(|| Sector::read(black_box(&data)).unwrap());
        let after_scene = heap();
        let ctx = scene.object.as_ref().unwrap();
        let (index, build) = elapsed(|| Index::new(&scene, ctx).unwrap());
        let after_index = heap();
        let (mut base, profile) = elapsed(|| crate::object::inspect(&scene, &[], &[]).unwrap());
        let mut all = Vec::with_capacity(probes.len());
        let mut query = serde_json::Map::new();
        for group in probes.chunk_by(|a, b| a.kind == b.kind) {
            let (_, ns) = elapsed(|| {
                for p in group {
                    all.push(black_box(index.query(black_box(p.start), black_box(p.end)).unwrap()));
                }
            });
            query.insert(group[0].kind.clone(), json!(ns));
        }
        let ((), assembly) = elapsed(|| {
            base["ray"] = all.last().unwrap().clone();
            if all.len() > 1 {
                base["rays"] = json!(all);
            }
        });
        let (pretty, serialization) = elapsed(|| serde_json::to_vec_pretty(black_box(&base)).unwrap());
        let (mut standalone, mut acoustic_query) = (None, serde_json::Map::new());
        if round == 0 {
            let before = heap();
            let (acoustic, ns) = elapsed(|| ctx.sound.prepare_acoustics(&scene.colliders));
            let after = heap();
            let (leaves, tree, refs) = acoustic.measured_storage();
            standalone = Some(
                json!({"leaves":leaves,"tree_heap_bytes":tree,"triangle_reference_bytes":refs,"heap_increment":after.saturating_sub(before),"build_cpu_ns":ns}),
            );
        }
        // For subsequent rounds this warms the *separate* sound path outside the inspect timers above.
        ctx.sound.prepare_acoustics(&scene.colliders);
        let mut acoustic_counts = Vec::new();
        for group in probes.chunk_by(|a, b| a.kind == b.kind) {
            let (_, ns) = elapsed(|| {
                for p in group {
                    let (hits, tested) = ctx.sound.acoustic_hits(
                        &scene.colliders,
                        black_box(p.start),
                        black_box(p.end),
                        0.,
                        [None, None],
                    );
                    acoustic_counts.push((hits.len(), tested));
                }
            });
            acoustic_query.insert(group[0].kind.clone(), json!(ns));
        }
        if round == 0 {
            let mut owned = BTreeSet::new();
            let mut owned_bytes = 0;
            for s in &ctx.sound.acoustics {
                if let AcousticMesh::Owned(m) = &s.mesh
                    && owned.insert(Arc::as_ptr(m))
                {
                    owned_bytes += storage(m);
                }
            }
            let source_bytes: usize =
                scene.meshes.iter().flatten().map(|p| p.positions.capacity() * 12 + p.indices.capacity() * 4).sum();
            let (mut details, mut counts) = (Vec::new(), Vec::new());
            for (row, acoustic) in all.iter().zip(acoustic_counts) {
                for kind in ["visual", "collision", "acoustic"] {
                    details.push(row[kind].as_array().unwrap().len());
                }
                counts.push(json!({"tested":row["testedTriangles"],"binding_hits":details,"sound_hits":acoustic.0,"sound_tested":acoustic.1}));
                details.clear();
            }
            let physics_bytes: usize = scene.colliders.iter().map(storage).sum();
            let local_leaves = index.meshes.iter().map(|m| u64::from(m.tree.leaf_count())).sum::<u64>();
            let local_heap = index.meshes.iter().map(|m| m.tree.heap_memory_size()).sum::<usize>();
            metrics = Some(json!({"scene_bytes":data.len(),"object_nodes":ctx.profile.nodes.len(),
                "visual_meshes":scene.meshes.len(),"visual_instances":scene.instances.len(),"visual_position_and_index_capacity_bytes":source_bytes,
                "placed_visual_triangles":scene.triangles,"physical_world_meshes":scene.colliders.len(),"physical_position_and_index_capacity_bytes":physics_bytes,
                "owned_acoustic_world_meshes":owned.len(),"acoustic_position_and_index_capacity_bytes":owned_bytes,
                "profile_bindings":index.bindings.len(),"geometry_groups":index.placements.len(),"bvh_leaves":u64::from(index.tree.leaf_count())+local_leaves,
                "outer_tree_leaves":index.tree.leaf_count(),"local_tree_leaves":local_leaves,"local_trees":index.meshes.len(),
                "world_fallback_trees":index.meshes.iter().filter(|m|matches!(m.source, Source::World(_))).count(),
                "outer_tree_heap_bytes":index.tree.heap_memory_size(),"local_tree_heap_bytes":local_heap,
                "bvh_heap_bytes":index.tree.heap_memory_size()+local_heap,"sector_heap_increment":after_scene.saturating_sub(before_scene),
                "index_heap_increment":after_index.saturating_sub(after_scene),"standalone_acoustic":standalone,"pruning":counts,
                "pretty_bytes":pretty.len(),"pretty_sha256":crate::bench::sha(&pretty)}));
        }
        times.push(json!({"load_and_hierarchy_cpu_ns":load,"build_cpu_ns":build,"profile_json_value_cpu_ns":profile,
            "query_and_hit_value_cpu_ns":query,"assemble_json_cpu_ns":assembly,"pretty_print_cpu_ns":serialization,"standalone_acoustic_query_cpu_ns":acoustic_query}));
    }
    println!("OSAGE_OBJECT_RAY_TIMING {}", json!({"metrics":metrics,"rounds":times}));
}
