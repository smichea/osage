// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! How the client shows radiance: through Blender's own AgX, the view
//! transform Cycles' pictures are seen through, so that the street looks as
//! Blender shows it (research 10). Bevy's AgX is Sobotka's first one, not
//! Blender's: under the same light the two differ by a fifth in the
//! mid-tones. Blender's is a table (`tools/bistro/agx-lut.py`) read where
//! Bevy reads its Blender Filmic table, over the same domain; without the
//! table, Bevy's AgX.

use std::path::{Path, PathBuf};

use bevy::{
    asset::RenderAssetUsages,
    core_pipeline::tonemapping::{Tonemapping, TonemappingLuts},
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

/// The table, under the asset manifest's content.
pub const TABLE: &str = "blender/agx-64.bin";
/// Its side: entries a band.
const SIDE: u32 = 64;

/// The tonemapping to ask osage-render for: Blender's AgX if its table is
/// there, else Bevy's.
pub fn tonemapping(root: &Path) -> Tonemapping {
    if root.join(TABLE).exists() {
        Tonemapping::BlenderFilmic
    } else {
        warn!("paris: no {TABLE} (tools/bistro/agx-lut.py): Bevy's AgX, not Blender's");
        Tonemapping::AgX
    }
}

/// Puts Blender's AgX table where Bevy's Blender Filmic table was.
pub struct LookPlugin {
    pub table: PathBuf,
}

#[derive(Resource)]
struct Table(PathBuf);

impl Plugin for LookPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Table(self.table.clone())).add_systems(Startup, swap);
    }
}

fn swap(table: Res<Table>, luts: Option<Res<TonemappingLuts>>, mut images: ResMut<Assets<Image>>) {
    let Some(luts) = luts else { return };
    let Ok(bytes) = std::fs::read(&table.0) else { return };
    let side = SIDE as usize;
    if bytes.len() != side * side * side * 8 {
        warn!("paris: {} is not a table of {SIDE}^3 half floats", table.0.display());
        return;
    }
    let size = Extent3d { width: SIDE, height: SIDE, depth_or_array_layers: SIDE };
    let mut image =
        Image::new(size, TextureDimension::D3, bytes, TextureFormat::Rgba16Float, RenderAssetUsages::default());
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        label: Some("Blender AgX".to_owned()),
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        address_mode_w: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    match images.insert(&luts.blender_filmic, image) {
        Ok(()) => info!("paris: shown through Blender's AgX"),
        Err(error) => warn!("paris: Blender's AgX table not put in place: {error}"),
    }
}
