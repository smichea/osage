// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The scene of the bridge is an object of the engine's own format: what
//! sound adds sits beside what the format knows, and the engine's reader
//! (`osage-objects`) loads the file as it is.

use osage_objects::ObjectProfile;
use osage_paris_soundscape::scene::{BIR_HAKEIM, Scene};

#[test]
fn the_engine_s_reader_loads_the_scene_as_an_object_tree() {
    let gltf = gltf::Gltf::from_slice_without_validation(BIR_HAKEIM.as_bytes())
        .expect("the scene is the JSON of a glTF asset");
    let profile =
        ObjectProfile::read(&gltf.document).expect("a well-formed object").expect("an object, not a plain glTF");
    let scene = Scene::bundled();
    assert_eq!(profile.header.id, scene.id);
    assert_eq!(profile.nodes.len(), scene.tree.nodes.len());
    assert_eq!(profile.roots.len(), 1);
    // Every node has the same path in both readers.
    let mut theirs: Vec<Vec<String>> = (0..profile.nodes.len())
        .map(|i| profile.node_path(i).unwrap().into_iter().map(str::to_owned).collect())
        .collect();
    let mut ours: Vec<Vec<String>> =
        (0..scene.tree.nodes.len()).map(|i| scene.tree.path(i).into_iter().map(str::to_owned).collect()).collect();
    theirs.sort();
    ours.sort();
    assert_eq!(theirs, ours);
    // And the materials sound meets are the format's own.
    for name in ["asphalt", "steel-deck", "water", "stone"] {
        assert!(profile.header.acoustic_materials.contains_key(name), "{name}");
    }
    for name in ["asphalt", "steel-plate", "rubber-sole"] {
        assert!(profile.header.collision_materials.contains_key(name), "{name}");
    }
}
