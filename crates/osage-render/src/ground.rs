// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Ground that varies: a standard material whose colour is multiplied, in
//! world space, by clumps a metre or two across and by drier and lusher
//! patches tens of metres across. A tiled grain — grass, earth — repeats
//! every few metres; under this variation it no longer reads as a carpet,
//! and the ground keeps texture at distances where the grain has blurred
//! to its mean. Octaves smaller than a few pixels fade out, so the far
//! ground does not shimmer.
//!
//! A world chooses the strengths, sizes and colours ([`Ground`]); the
//! engine provides the material. Every tier: a few noise lookups a pixel.

use bevy::{
    asset::embedded_asset,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};

/// The standard material with [`Ground`] variation.
pub type GroundMaterial = ExtendedMaterial<StandardMaterial, Ground>;

/// How a ground varies.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct Ground {
    /// Strength of the clumps and of the patches (0 to 1), the size of a
    /// clump and of a patch in metres.
    #[uniform(100)]
    pub variation: Vec4,
    /// The colour multiplier of a dry patch (yellower, paler).
    #[uniform(101)]
    pub dry: Vec4,
    /// The colour multiplier of a lush patch (darker, greener).
    #[uniform(102)]
    pub lush: Vec4,
}

impl Ground {
    /// A meadow in summer: clumps of 1.6 m, patches of 22 m, dry ones
    /// straw-coloured, lush ones a deeper green.
    pub fn meadow() -> Self {
        Self {
            variation: Vec4::new(0.38, 0.8, 1.6, 22.0),
            dry: Vec4::new(1.35, 1.12, 0.72, 1.0),
            lush: Vec4::new(0.78, 0.92, 0.8, 1.0),
        }
    }

    /// The same ground with no variation: the standard material's look.
    pub fn none() -> Self {
        Self { variation: Vec4::new(0.0, 0.0, 1.0, 1.0), dry: Vec4::ONE, lush: Vec4::ONE }
    }
}

impl MaterialExtension for Ground {
    fn fragment_shader() -> ShaderRef {
        "embedded://osage_render/ground.wesl".into()
    }
}

/// Registers [`GroundMaterial`].
pub struct GroundPlugin;

impl Plugin for GroundPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "ground.wesl");
        app.add_plugins(MaterialPlugin::<GroundMaterial>::default());
    }
}
