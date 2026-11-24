// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The scene of the bridge is an object of the engine's own format: what
//! sound adds sits beside what the format knows, and the engine's reader
//! (`osage-objects`) loads the file as it is. Built as an object with
//! meshes — its four-cornered faces as triangles of the format's acoustic
//! and collision surfaces — it sounds the same.

use osage_objects::ObjectProfile;
use osage_sound::{
    ears::Listening,
    engine::{Options, Quality},
    objects::{from_gltf, with_meshes},
    render::render,
    scene::Scene,
};
use paris_soundscape::BIR_HAKEIM;

fn profile(text: &str) -> ObjectProfile {
    let gltf = gltf::Gltf::from_slice_without_validation(text.as_bytes()).expect("the JSON of a glTF asset");
    ObjectProfile::read(&gltf.document).expect("a well-formed object").expect("an object, not a plain glTF")
}

fn paths(count: usize, path: impl Fn(usize) -> Vec<String>) -> Vec<Vec<String>> {
    let mut all: Vec<Vec<String>> = (0..count).map(path).collect();
    all.sort();
    all
}

#[test]
fn the_engine_s_reader_loads_the_scene_as_an_object_tree() {
    let profile = profile(BIR_HAKEIM);
    let scene = paris_soundscape::bridge();
    assert_eq!(profile.header.id, scene.id);
    assert_eq!(profile.nodes.len(), scene.tree.nodes.len());
    assert_eq!(profile.roots.len(), 1);
    // Every node has the same path in both readers.
    let theirs = paths(profile.nodes.len(), |i| profile.node_path(i).unwrap().into_iter().map(str::to_owned).collect());
    let ours = paths(scene.tree.nodes.len(), |i| scene.tree.path(i).into_iter().map(str::to_owned).collect());
    assert_eq!(theirs, ours);
    // And the materials sound meets are the format's own.
    for name in ["asphalt", "steel-deck", "water", "stone"] {
        assert!(profile.header.acoustic_materials.contains_key(name), "{name}");
    }
    for name in ["asphalt", "steel-plate", "rubber-sole"] {
        assert!(profile.header.collision_materials.contains_key(name), "{name}");
    }
}

#[test]
fn the_bridge_built_with_meshes_sounds_as_its_hand_written_faces() {
    let object = with_meshes(BIR_HAKEIM).unwrap();
    // The format's reader takes it: every face a mesh of the node it is on.
    let read = profile(&object);
    let acoustic: usize = read.nodes.iter().map(|n| n.geometry.acoustic.len()).sum();
    let collision: usize = read.nodes.iter().map(|n| n.geometry.collision.len()).sum();
    assert_eq!((acoustic, collision), (5, 3));
    let meshed = from_gltf(object.as_bytes()).unwrap();
    let written = paris_soundscape::bridge();
    // The same sources, in the same places, and the same faces.
    assert_eq!(meshed.emitters, written.emitters);
    assert_eq!(meshed.touches, written.touches);
    assert_eq!(meshed.tree, written.tree);
    let faces = |scene: &Scene, acoustic: bool| {
        scene.surfaces.iter().filter(|s| if acoustic { s.acoustic.is_some() } else { s.collision.is_some() }).count()
    };
    assert_eq!((faces(&meshed, true), faces(&meshed, false)), (faces(&written, true), faces(&written, false)));
    // Heard alike: the corners of a mesh are 32-bit numbers, and nothing else differs.
    for quality in [Quality::LOW, Quality::HIGH] {
        let options = Options { quality, listening: Listening::Headphones, ..Options::default() };
        let (a, b) = (render(&written, &options, 20.0), render(&meshed, &options, 20.0));
        assert_eq!(a.tally.contacts, b.tally.contacts, "{quality:?}");
        for (one, other) in [(&a.left, &b.left), (&a.right, &b.right)] {
            let power = |s: &[f32]| s.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>();
            let differs: Vec<f32> = one.iter().zip(other.iter()).map(|(x, y)| x - y).collect();
            let db = 10.0 * (power(&differs) / power(one)).log10();
            eprintln!("{}: the two differ at {db:.1} dB", quality.name);
            assert!(db < -60.0, "{quality:?}: the two differ at {db:.1} dB");
        }
    }
}
