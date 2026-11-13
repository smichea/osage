// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The tier chosen for the machine (D-075): the player chooses only the
//! frame rate they want; the game picks the tier — what techniques the
//! picture has — from the graphics adapter, and steps down on a machine
//! where a tier has failed.
//!
//! - An integrated adapter gets T2, a discrete one T3 (T4, ray tracing,
//!   only when asked for, until it is ready); none found, T1.
//! - Each start notes its tier (`osage/kart-reports/tier`). If the run
//!   before, at a tier chosen here, did not finish starting, panicked or
//!   reported renderer errors (D-073), that tier is marked failed on this
//!   machine (`failed`) and a lower one is used from then on.
//! - Ten seconds into the race the game looks at one frame: a picture of
//!   one colour (the grey screen of T3 on a Direct3D 12 RTX PC) is a tier
//!   that does not work here. It fails, is noted for the report, and the
//!   game starts again (D-090: with a suspect technique left out, as
//!   below).
//! - A tier that fails is not given up at once (D-090): the next start
//!   tries it again without one technique more, the likeliest culprits
//!   first — the traced light (T4), the sky as a light, then post-processing (temporal
//!   anti-aliasing, motion blur), ambient occlusion, shadows — and keeps
//!   the first set that works (`trimmed`); only with every suspect left
//!   out does it go a tier lower.

use std::path::PathBuf;

use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured},
};
use osage_render::{Technique, Tier};

use crate::hud::{Lights, RaceView};

fn folder() -> Option<PathBuf> {
    Some(crate::settings::folder()?.join("kart-reports"))
}

/// The graphics adapter the game will draw with, as Bevy will pick it:
/// the high-performance one, Direct3D 12 on Windows unless `WGPU_BACKEND`.
pub fn adapter() -> Option<wgpu::AdapterInfo> {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    if cfg!(windows) && std::env::var_os("WGPU_BACKEND").is_none() {
        descriptor.backends = wgpu::Backends::DX12;
    }
    let instance = wgpu::Instance::new(descriptor);
    let options = wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..default() };
    bevy::tasks::block_on(instance.request_adapter(&options)).ok().map(|adapter| adapter.get_info())
}

/// The tier an adapter of this type is given.
pub fn tier_for(device: Option<wgpu::DeviceType>) -> Tier {
    match device {
        Some(wgpu::DeviceType::DiscreteGpu) => Tier::T3,
        Some(wgpu::DeviceType::IntegratedGpu | wgpu::DeviceType::VirtualGpu | wgpu::DeviceType::Other) => Tier::T2,
        Some(wgpu::DeviceType::Cpu) | None => Tier::T1,
    }
}

/// The tier below.
fn lower(tier: Tier) -> Tier {
    match tier {
        Tier::T4 => Tier::T3,
        Tier::T3 => Tier::T2,
        Tier::T2 | Tier::T1 => Tier::T1,
    }
}

/// The techniques a tier tries without, one more after each failure,
/// likeliest culprit first (D-090).
const SUSPECTS: [Technique; 5] =
    [Technique::RayTracing, Technique::SkyLight, Technique::Post, Technique::AmbientOcclusion, Technique::Shadows];

/// What this machine leaves out of each tier, after failures there: lines
/// `T3 sky-light post`.
fn trimmed() -> Vec<(Tier, Vec<Technique>)> {
    let text = folder().and_then(|f| std::fs::read_to_string(f.join("trimmed")).ok()).unwrap_or_default();
    parse_trimmed(&text)
}

fn parse_trimmed(text: &str) -> Vec<(Tier, Vec<Technique>)> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let tier = words.next()?.parse().ok()?;
            Some((tier, words.filter_map(|w| w.parse().ok()).collect()))
        })
        .collect()
}

/// What `tier` is run without on this machine.
fn without(tier: Tier) -> Vec<Technique> {
    trimmed().into_iter().find(|(t, _)| *t == tier).map(|(_, w)| w).unwrap_or_default()
}

/// After a failure with `left_out` left out: the next suspect to leave
/// out, or none when every one is out already (then the tier fails).
fn next_suspect(left_out: &[Technique]) -> Option<Technique> {
    SUSPECTS.into_iter().find(|t| !left_out.contains(t))
}

/// A failure at `tier`: one technique more left out of it, or, with
/// every suspect out, the tier marked failed. `why` is noted.
pub fn step_down(tier: Tier, why: &str) {
    let left_out = without(tier);
    let Some(next) = next_suspect(&left_out) else {
        fail(tier, why);
        return;
    };
    let Some(folder) = folder() else { return };
    let _ = std::fs::create_dir_all(&folder);
    let mut lines: Vec<(Tier, Vec<Technique>)> = trimmed().into_iter().filter(|(t, _)| *t != tier).collect();
    let mut now = left_out;
    now.push(next);
    lines.push((tier, now.clone()));
    let text: String = lines
        .iter()
        .map(|(t, w)| format!("{t} {}\n", w.iter().map(|t| t.name()).collect::<Vec<_>>().join(" ")))
        .collect();
    let _ = std::fs::write(folder.join("trimmed"), text);
    warn!("graphics: {tier} {why}; next, without {}", now.iter().map(|t| t.name()).collect::<Vec<_>>().join(", "));
}

/// A tier marked failed before the search existed (D-090) is tried again,
/// once, as a failure that starts the search: without the first suspect.
fn reopen_old_failures() {
    let Some(folder) = folder() else { return };
    if folder.join("searched").exists() {
        return;
    }
    let old = failed();
    let _ = std::fs::create_dir_all(&folder);
    let _ = std::fs::write(folder.join("searched"), "D-090\n");
    if old.is_empty() || !trimmed().is_empty() {
        return;
    }
    let _ = std::fs::remove_file(folder.join("failed"));
    for tier in old {
        step_down(tier, "failed before the search (D-090)");
    }
}

/// The tiers that failed on this machine.
fn failed() -> Vec<Tier> {
    let text = folder().and_then(|f| std::fs::read_to_string(f.join("failed")).ok()).unwrap_or_default();
    text.lines().filter_map(|line| line.split_whitespace().next()?.parse().ok()).collect()
}

/// Marks `tier` failed on this machine, with why.
pub fn fail(tier: Tier, why: &str) {
    let Some(folder) = folder() else { return };
    let _ = std::fs::create_dir_all(&folder);
    let mut text = std::fs::read_to_string(folder.join("failed")).unwrap_or_default();
    if !failed().contains(&tier) {
        text.push_str(&format!("{tier} {why}\n"));
        let _ = std::fs::write(folder.join("failed"), text);
    }
}

/// The tier to start with, chosen here, given whether the last run went
/// wrong (a report, D-073) — its tier, if chosen here, steps down (D-090)
/// — and the techniques this machine leaves out of it.
pub fn choose(last_went_wrong: bool) -> (Tier, Vec<Technique>, String) {
    reopen_old_failures();
    let previous: Option<Tier> =
        folder().and_then(|f| std::fs::read_to_string(f.join("tier")).ok()).and_then(|t| t.trim().parse().ok());
    if last_went_wrong && let Some(tier) = previous.filter(|tier| *tier > Tier::T2) {
        step_down(tier, "the run at this tier went wrong");
    }
    let adapter = adapter();
    let mut tier = tier_for(adapter.as_ref().map(|a| a.device_type));
    let failed = failed();
    while failed.contains(&tier) && tier > Tier::T1 {
        tier = lower(tier);
    }
    if let Some(folder) = folder() {
        let _ = std::fs::create_dir_all(&folder);
        let _ = std::fs::write(folder.join("tier"), format!("{tier}\n"));
    }
    let name = adapter.map_or_else(
        || "no adapter found".to_owned(),
        |a| format!("{} ({:?}, {:?})", a.name, a.device_type, a.backend),
    );
    (tier, without(tier), name)
}

/// Whether a picture is of one colour, near enough: RGBA8 pixels, sampled.
pub fn uniform(pixels: &[u8]) -> bool {
    let samples: Vec<f32> = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .step_by(97)
        .map(|p| 0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2]))
        .collect();
    if samples.len() < 16 {
        return false;
    }
    let mean = samples.iter().sum::<f32>() / samples.len() as f32;
    let spread = (samples.iter().map(|s| (s - mean).powi(2)).sum::<f32>() / samples.len() as f32).sqrt();
    spread < 2.0
}

/// The tier was chosen here (not by `--tier`): only then is it checked.
#[derive(Resource)]
pub struct Chosen;

pub struct GraphicsPlugin;

impl Plugin for GraphicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, look.run_if(resource_exists::<Chosen>));
    }
}

/// Ten seconds into the race, one frame looked at.
fn look(
    mut commands: Commands,
    time: Res<Time>,
    view: Res<RaceView>,
    mut since: Local<Option<f32>>,
    mut done: Local<bool>,
) {
    if *done || view.stage == Lights::Off {
        return;
    }
    let now = time.elapsed_secs();
    let start = *since.get_or_insert(now);
    if now - start < 10.0 {
        return;
    }
    *done = true;
    commands.spawn(Screenshot::primary_window()).observe(judge);
}

fn judge(captured: On<ScreenshotCaptured>, tier: Res<Tier>, mut exit: MessageWriter<AppExit>) {
    let Some(pixels) = captured.image.data.as_ref() else { return };
    if !uniform(pixels) || *tier <= Tier::T1 {
        return;
    }
    warn!("graphics: the picture is of one colour at {}: this tier does not work here", *tier);
    step_down(*tier, "the picture stayed of one colour");
    // For the report at the next start (D-073).
    if let Some(folder) = folder() {
        let _ = std::fs::write(folder.join("hung.txt"), format!("the picture stayed of one colour at {}\n", *tier));
        let _ = std::fs::write(folder.join("running"), "starting\n");
    }
    crate::report::keep_for_next();
    // Again, without one technique more, or a tier lower (D-090).
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).args(std::env::args().skip(1)).spawn();
    }
    exit.write(AppExit::Success);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapters_get_their_tier_and_failed_tiers_step_down() {
        assert_eq!(tier_for(Some(wgpu::DeviceType::DiscreteGpu)), Tier::T3);
        assert_eq!(tier_for(Some(wgpu::DeviceType::IntegratedGpu)), Tier::T2);
        assert_eq!(tier_for(None), Tier::T1);
        assert_eq!(lower(Tier::T3), Tier::T2);
        assert_eq!(lower(Tier::T1), Tier::T1);
    }

    #[test]
    fn a_failing_tier_drops_its_suspects_one_by_one_before_it_fails() {
        let mut left_out = Vec::new();
        let mut order = Vec::new();
        while let Some(next) = next_suspect(&left_out) {
            left_out.push(next);
            order.push(next);
        }
        assert_eq!(order, SUSPECTS.to_vec(), "the likeliest culprit first, each once");
        let read = parse_trimmed("T3 sky-light post\nT2\nnonsense line\n");
        assert_eq!(read, vec![(Tier::T3, vec![Technique::SkyLight, Technique::Post]), (Tier::T2, Vec::new())]);
    }

    #[test]
    fn a_grey_picture_is_uniform_a_scene_is_not() {
        let grey: Vec<u8> = (0..4 * 200 * 100).map(|i| if i % 4 == 3 { 255 } else { 120 }).collect();
        assert!(uniform(&grey));
        let scene: Vec<u8> = (0..4 * 200 * 100).map(|i| ((i / 4 * 37) % 255) as u8).collect();
        assert!(!uniform(&scene));
    }
}
