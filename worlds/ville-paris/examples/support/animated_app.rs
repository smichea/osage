// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{path::Path, time::Duration};

use anyhow::Result;
use bevy::{
    animation::AnimationPlugin,
    app::Plugins,
    gltf::GltfPlugin,
    image::{CompressedImageFormats, ImageLoader},
    mesh::MeshPlugin,
    prelude::*,
    time::TimeUpdateStrategy,
    world_serialization::WorldSerializationPlugin,
};

pub(super) fn animated_app<P: Plugins<M>, M>(plugins: P) -> Result<App> {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets").canonicalize()?;
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        AssetPlugin { file_path: assets.to_string_lossy().into_owned(), ..default() },
        WorldSerializationPlugin,
        ImagePlugin::default(),
        MeshPlugin,
        GltfPlugin::default(),
        AnimationPlugin,
    ))
    .add_plugins(plugins)
    .register_asset_loader(ImageLoader::new(CompressedImageFormats::NONE))
    .init_asset::<StandardMaterial>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    app.finish();
    app.cleanup();

    Ok(app)
}
