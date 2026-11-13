// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Ground that varies, and does not repeat:
//!
//! - **variation:** a standard material whose colour is multiplied, in
//!   world space, by clumps a metre or two across and by drier and lusher
//!   patches tens of metres across, so that a tiled grain no longer reads as
//!   a carpet and the ground keeps texture where the grain has blurred to
//!   its mean;
//! - **no tile** ([`Ground::with_grain`]): the grain — the surface's colour
//!   map, given to this material instead of the standard one — is sampled
//!   by hexagonal cells, each shifted and turned by a hash of where it is,
//!   blended across their edges ([`hex_tiles`]): no period anywhere;
//! - **a paved path** ([`Ground::paved`]): on a mesh whose texture
//!   coordinates are metres across and along a road, the road's history
//!   drawn from the distance — paving seams, repaired patches, cracks,
//!   stains — a deterministic function of where, the same on every client.
//!
//! Octaves smaller than a few pixels fade out, so the far ground does not
//! shimmer. A world chooses the strengths, sizes and colours; the engine
//! provides the material. Every tier: a few noise lookups a pixel, and
//! three samples of the grain where it is not tiled.

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
    /// The grain: x 1 to sample it without a tile (hexagonal cells), 0 as
    /// a plain repeat; y its repeat in metres of texture coordinates; z
    /// how far a cell may turn (1: any angle); w how sharp the blend
    /// between cells is.
    #[uniform(103)]
    pub tiling: Vec4,
    /// A paved path's history: x strength (0 none), y the length of a
    /// stretch in metres, each with its own patches and cracks; z the
    /// road's width in metres; w the colour of fresh asphalt in a patch,
    /// as a multiplier.
    #[uniform(104)]
    pub path: Vec4,
    /// The surface's colour map, multiplied in place of the standard
    /// material's (which should then have none). White when there is none.
    #[texture(105)]
    #[sampler(106)]
    pub grain: Option<Handle<Image>>,
    /// Rock on steep ground (D-081): x its strength (0 none), y the
    /// repeat of its grain in metres, z the strength of its strata, w how
    /// far apart they lie, m. The grain is projected along the three axes,
    /// as a cliff has no texture coordinates of its own.
    #[uniform(107)]
    pub rock: Vec4,
    #[texture(108)]
    #[sampler(109)]
    pub rock_grain: Option<Handle<Image>>,
}

impl Ground {
    /// A meadow in summer: clumps of 1.6 m, patches of 22 m, dry ones
    /// straw-coloured, lush ones a deeper green.
    pub fn meadow() -> Self {
        Self {
            variation: Vec4::new(0.38, 0.8, 1.6, 22.0),
            dry: Vec4::new(1.35, 1.12, 0.72, 1.0),
            lush: Vec4::new(0.78, 0.92, 0.8, 1.0),
            ..Self::none()
        }
    }

    /// Asphalt: faint clumps, wide patches a little greyer or darker.
    pub fn asphalt() -> Self {
        Self {
            variation: Vec4::new(0.12, 0.35, 2.5, 30.0),
            dry: Vec4::new(1.12, 1.1, 1.08, 1.0),
            lush: Vec4::new(0.9, 0.9, 0.92, 1.0),
            ..Self::none()
        }
    }

    /// The same ground with no variation: the standard material's look.
    pub fn none() -> Self {
        Self {
            variation: Vec4::new(0.0, 0.0, 1.0, 1.0),
            dry: Vec4::ONE,
            lush: Vec4::ONE,
            tiling: Vec4::new(0.0, 1.0, 1.0, 6.0),
            path: Vec4::new(0.0, 40.0, 12.0, 0.8),
            grain: None,
            rock: Vec4::new(0.0, 6.0, 0.0, 2.0),
            rock_grain: None,
        }
    }

    /// Takes the grain `colour`, repeated every `tile` metres, sampled
    /// without a tile when `untiled`. Its standard material should then
    /// carry no colour map ([`untextured`]).
    pub fn with_grain(self, colour: Handle<Image>, tile: f32, untiled: bool) -> Self {
        Self { tiling: Vec4::new(if untiled { 1.0 } else { 0.0 }, tile, 1.0, 6.0), grain: Some(colour), ..self }
    }

    /// Rock where the ground is steep: the grain `colour` (a brightness
    /// map at the same mean as the surface's grain), repeated every `tile`
    /// metres, with strata of `strata` strength every `spacing` metres of
    /// height.
    pub fn with_rock(self, colour: Handle<Image>, tile: f32, strata: f32, spacing: f32) -> Self {
        Self { rock: Vec4::new(1.0, tile, strata, spacing), rock_grain: Some(colour), ..self }
    }

    /// Draws a road's history on a mesh whose texture coordinates are
    /// metres across (0 at the left edge) and along: stretches of
    /// `stretch` metres on a road `width` wide.
    pub fn paved(self, strength: f32, stretch: f32, width: f32) -> Self {
        Self { path: Vec4::new(strength, stretch, width, self.path.w), ..self }
    }
}

/// `material` without its colour map, for a [`Ground`] that takes it as
/// its grain; and the map it had.
pub fn untextured(material: StandardMaterial) -> (StandardMaterial, Option<Handle<Image>>) {
    let colour = material.base_color_texture.clone();
    (StandardMaterial { base_color_texture: None, ..material }, colour)
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

/// Hexagonal cells per repeat of the grain, across.
pub const HEX_CELLS: f32 = 1.6;
/// A cell weighing less than this, where a pixel lies, is not sampled.
pub const SKIP: f32 = 0.03;

/// The shader's hash, in 0..1 (`ground.wesl`'s `hash`, the same steps).
pub fn hash(p: Vec2) -> f32 {
    let q = (p * Vec2::new(0.1031, 0.1030)).fract();
    let r = q + q.dot(q.yx() + 33.33);
    ((r.x + r.y) * r.x).fract()
}

/// Where the grain is sampled for texture coordinate `uv` (in repeats),
/// and with what weight, as `ground.wesl`'s `untiled` does: the three
/// hexagonal cells around it, each shifted by a hash of its centre and
/// turned by up to `turn` of a whole turn, weighted by nearness raised to
/// `sharp`. The weights add to 1. The CPU's copy of the shader, for tests
/// and for whoever needs to know what a pixel shows.
pub fn hex_tiles(uv: Vec2, turn: f32, sharp: f32) -> [(Vec2, f32); 3] {
    // A triangular lattice: its vertices are the hexagons' centres.
    let p = uv * HEX_CELLS;
    let skewed = Vec2::new(p.x - p.y * 0.577_350_3, p.y * 1.154_700_5);
    let (base, f) = (skewed.floor(), skewed.fract());
    let z = 1.0 - f.x - f.y;
    let (weights, corners) = if z > 0.0 {
        (Vec3::new(z, f.y, f.x), [base, base + Vec2::Y, base + Vec2::X])
    } else {
        (Vec3::new(-z, 1.0 - f.y, 1.0 - f.x), [base + Vec2::ONE, base + Vec2::X, base + Vec2::Y])
    };
    let weights = weights.powf(sharp);
    let weights = weights / (weights.x + weights.y + weights.z);
    // A cell that weighs almost nothing is not sampled.
    let weights = Vec3::select(weights.cmplt(Vec3::splat(SKIP)), Vec3::ZERO, weights);
    let total = weights.x + weights.y + weights.z;
    let mut out = [(Vec2::ZERO, 0.0); 3];
    for (k, corner) in corners.into_iter().enumerate() {
        let (angle, shift) = cell(corner, turn);
        let (s, c) = angle.sin_cos();
        let turned = Vec2::new(c * uv.x - s * uv.y, s * uv.x + c * uv.y);
        out[k] = (turned + shift, weights[k] / total);
    }
    out
}

/// A cell's turn (radians) and shift (in repeats), from its lattice corner.
fn cell(corner: Vec2, turn: f32) -> (f32, Vec2) {
    let angle = hash(corner + 0.5) * std::f32::consts::TAU * turn;
    let shift = Vec2::new(hash(corner + Vec2::new(17.3, 3.1)), hash(corner + Vec2::new(5.7, 29.9)));
    (angle, shift)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grain with a feature every repeat: what tiling shows.
    fn grain(uv: Vec2) -> f32 {
        let f = uv.fract();
        // A bright spot off-centre and a stripe: no symmetry to hide behind.
        let spot = (-((f - Vec2::new(0.3, 0.6)).length_squared() / 0.01)).exp();
        let stripe = (std::f32::consts::TAU * f.x).sin() * 0.3 + (std::f32::consts::TAU * 2.0 * f.y).cos() * 0.2;
        spot + stripe
    }

    fn untiled(uv: Vec2) -> f32 {
        hex_tiles(uv, 1.0, 6.0).iter().map(|(at, w)| grain(*at) * w).sum()
    }

    /// Correlation between a strip and itself `lag` further on, the means
    /// taken off.
    fn correlation(values: &[f32], lag: usize) -> f32 {
        let n = values.len() - lag;
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        let (mut product, mut a2, mut b2) = (0.0, 0.0, 0.0);
        for i in 0..n {
            let (a, b) = (values[i] - mean, values[i + lag] - mean);
            product += a * b;
            a2 += a * a;
            b2 += b * b;
        }
        product / (a2 * b2).sqrt()
    }

    #[test]
    fn the_weights_add_to_one_and_follow_the_nearest_cell() {
        for i in 0..500 {
            let uv = Vec2::new(i as f32 * 0.137, i as f32 * 0.071 - 3.0);
            let tiles = hex_tiles(uv, 1.0, 6.0);
            let total: f32 = tiles.iter().map(|(_, w)| w).sum();
            assert!((total - 1.0).abs() < 1e-4, "{total}");
            assert!(tiles.iter().all(|(_, w)| *w >= 0.0));
        }
        // Near a cell's centre, that cell alone shows.
        let at_centre = hex_tiles(Vec2::ZERO, 1.0, 6.0);
        assert!(at_centre.iter().any(|(_, w)| *w > 0.99), "{at_centre:?}");
    }

    #[test]
    fn a_strip_of_untiled_grain_has_no_period() {
        // 64 repeats of the grain along a road, 16 samples a repeat.
        let per = 16;
        let along = |f: &dyn Fn(Vec2) -> f32, lateral: f32| -> Vec<f32> {
            (0..64 * per).map(|i| f(Vec2::new(lateral, i as f32 / per as f32))).collect()
        };
        for lateral in [0.13, 0.5, 2.71] {
            let tiled = along(&grain, lateral);
            let untiled = along(&untiled, lateral);
            for repeats in 1..=4 {
                let lag = repeats * per;
                let (t, u) = (correlation(&tiled, lag), correlation(&untiled, lag));
                assert!(t > 0.99, "a tiled grain repeats: {t}");
                assert!(u.abs() < 0.35, "untiled, no period at {repeats} repeats: {u} (tiled {t})");
            }
            // And none at any lag of the next 30 repeats.
            let worst = (per..30 * per).map(|lag| correlation(&untiled, lag)).fold(f32::MIN, f32::max);
            assert!(worst < 0.5, "the strongest echo: {worst}");
        }
    }

    #[test]
    fn untiled_grain_keeps_its_contrast_and_mean() {
        let samples: Vec<(f32, f32)> = (0..4000)
            .map(|i| {
                let uv = Vec2::new((i % 63) as f32 * 0.173, (i / 63) as f32 * 0.191);
                (grain(uv), untiled(uv))
            })
            .collect();
        let stats = |pick: fn(&(f32, f32)) -> f32| {
            let mean = samples.iter().map(pick).sum::<f32>() / samples.len() as f32;
            let var = samples.iter().map(|s| (pick(s) - mean).powi(2)).sum::<f32>() / samples.len() as f32;
            (mean, var.sqrt())
        };
        let ((m0, s0), (m1, s1)) = (stats(|s| s.0), stats(|s| s.1));
        assert!((m0 - m1).abs() < 0.05, "mean {m0} {m1}");
        assert!(s1 > 0.8 * s0, "the blend washes out little: {s0} {s1}");
    }
}
