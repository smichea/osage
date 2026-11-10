// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! osage rendering plugins (design/03): one plugin per technique, each
//! configured for the fidelity tier the app runs at. An app adds
//! [`OsageRenderPlugin`], marks its main 3D camera [`OsageCamera`] and its
//! sun [`Sun`]; the techniques do the rest.
//!
//! | Module | Workstream | Tiers |
//! |---|---|---|
//! | [`sky`] | R1 sky and sun | T1 flat sky and fog; T2+ physical atmosphere |
//! | [`sky_light`] | R1 light from the sky | T1–T2 a constant ambient light; T3+ the atmosphere, every frame (a clear day computed once, without an atmosphere) |
//! | [`shadows`] | R2 shadows | all: cascades per tier; contact shadows T3+ |
//! | [`ambient_occlusion`] | R3 ambient occlusion | T3+ |
//! | [`post`] | R10 post-processing and anti-aliasing | all: HDR, tonemapping, exposure; FXAA T1–T2, bloom T2+; TAA and motion blur T3+ |
//! | [`resolution`] | R11 resolution and upscaling | all: a picture no taller than the tier allows, stretched to the window; T2 draws 85 % |
//!
//! Each tier's set is what its budget allows on the reference hardware,
//! measured with the bench (`bench/README.md`); each module says what it
//! measured.

pub mod ambient_occlusion;
pub mod post;
pub mod resolution;
pub mod shadows;
pub mod sky;
pub mod sky_light;

use std::{fmt, str::FromStr};

use bevy::{core_pipeline::tonemapping::Tonemapping, prelude::*};

/// Fidelity tiers (research/00): the same world at every tier, each with
/// its own techniques and budget. T0 (headless) renders nothing.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    /// Old integrated GPUs, GLES3 / WebGL2.
    T1,
    /// Current integrated GPUs, WebGPU.
    T2,
    /// Discrete GPUs without ray tracing.
    T3,
    /// GPUs with ray tracing.
    T4,
}

impl Tier {
    /// The frame-time budget of this tier at its bench resolution,
    /// milliseconds (design/03 §3).
    pub fn budget_ms(self) -> f32 {
        match self {
            Tier::T1 | Tier::T4 => 1000.0 / 30.0,
            Tier::T2 | Tier::T3 => 1000.0 / 60.0,
        }
    }

    /// The resolution this tier is benchmarked at (design/03 §3).
    pub fn bench_resolution(self) -> (u32, u32) {
        match self {
            Tier::T1 => (1280, 720),
            Tier::T2 => (1920, 1080),
            Tier::T3 | Tier::T4 => (2560, 1440),
        }
    }
}

impl FromStr for Tier {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "T1" => Ok(Self::T1),
            "T2" => Ok(Self::T2),
            "T3" => Ok(Self::T3),
            "T4" => Ok(Self::T4),
            other => Err(format!("unknown tier {other}: T1, T2, T3 or T4")),
        }
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// How a world wants its picture to look. The engine gives the means; the
/// look is the world's: a world inserts its own before adding
/// [`OsageRenderPlugin`], and the post-processing technique applies it.
#[derive(Resource, Clone, Debug)]
pub struct Look {
    pub tonemapping: Tonemapping,
    /// 1 leaves colours as rendered; above, more vivid.
    pub saturation: f32,
    /// Stops added to the exposure of a sunny day; above 0, brighter.
    pub exposure: f32,
    /// The colour of the land, which tints the light that comes back from
    /// below ([`sky_light`]).
    pub ground: Color,
}

impl Default for Look {
    fn default() -> Self {
        Self { tonemapping: Tonemapping::AgX, saturation: 1.0, exposure: 0.0, ground: Color::srgb(0.42, 0.44, 0.36) }
    }
}

/// The main 3D camera, which the techniques configure.
#[derive(Component, Default)]
pub struct OsageCamera;

/// The sun: a directional light the techniques configure.
#[derive(Component, Default)]
pub struct Sun;

/// A rendering technique: one module, one plugin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Technique {
    Sky,
    /// The sky and the ground as lights ([`sky_light`]); without it, a
    /// constant ambient light.
    SkyLight,
    Shadows,
    AmbientOcclusion,
    Post,
    /// Drawing under the output size ([`resolution`]); without it, the
    /// picture has the output's size, up to the tier's height.
    Upscaling,
}

impl Technique {
    pub const ALL: [Technique; 6] = [
        Technique::Sky,
        Technique::SkyLight,
        Technique::Shadows,
        Technique::AmbientOcclusion,
        Technique::Post,
        Technique::Upscaling,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Technique::Sky => "sky",
            Technique::SkyLight => "sky-light",
            Technique::Shadows => "shadows",
            Technique::AmbientOcclusion => "ambient-occlusion",
            Technique::Post => "post",
            Technique::Upscaling => "upscaling",
        }
    }
}

/// The techniques left out of this run.
#[derive(Resource, Clone, Default)]
pub struct LeftOut(pub Vec<Technique>);

impl FromStr for Technique {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let names = || Technique::ALL.map(Technique::name).join(", ");
        Technique::ALL.into_iter().find(|t| t.name() == s).ok_or_else(|| format!("unknown technique {s}: {}", names()))
    }
}

/// The rendering techniques at `tier`, except those in `without` — left out
/// to measure what each one costs, or to find which one causes a defect.
pub struct OsageRenderPlugin {
    pub tier: Tier,
    pub without: Vec<Technique>,
}

impl Plugin for OsageRenderPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.tier)
            .insert_resource(LeftOut(self.without.clone()))
            .init_resource::<Look>()
            .insert_resource(sky_light::constant_ambient())
            .add_plugins(resolution::ResolutionPlugin);
        for technique in Technique::ALL.into_iter().filter(|t| !self.without.contains(t)) {
            match technique {
                Technique::Sky => app.add_plugins(sky::SkyPlugin),
                Technique::SkyLight => app.add_plugins(sky_light::SkyLightPlugin),
                Technique::Shadows => app.add_plugins(shadows::ShadowsPlugin),
                Technique::AmbientOcclusion => app.add_plugins(ambient_occlusion::AmbientOcclusionPlugin),
                Technique::Post => app.add_plugins(post::PostPlugin),
                // Always there: leaving it out only sets its scale to one.
                Technique::Upscaling => app,
            };
        }
    }
}
