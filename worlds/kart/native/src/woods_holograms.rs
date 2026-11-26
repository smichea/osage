// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The circuit's own woods baked where they stand (D-133, issue #8): cut
//! into chunks, each chunk's trees photographed at full detail on its own
//! ground from the directions the track sees it from, and drawn as its
//! hologram in place of its tree cards.
//!
//! - **Cutting** (`OSAGE_WOODS_CHUNKS=DIR` while a circuit is built): the
//!   woods' trees are cut into squares of [`CHUNK`] m; each square with
//!   enough trees becomes a [`osage_hologram::Scene`] — its trees in its
//!   own frame (turned to face the nearest point of the track), the grid of
//!   directions from which the track, seen from a kart's height, sees it —
//!   written as `DIR/chunk-X-Z/scene.json`, for `hologram-bake`.
//! - **Drawing:** where a chunk's views are installed under
//!   `holograms/woods-TRACK/chunk-X-Z` (with their `hologram.json`), it is
//!   drawn as its hologram, and the cards of its trees are left out.

use std::path::Path;

use bevy::prelude::*;
use osage_hologram::{Framing, Scene, SceneModel};
use osage_kart_land::{Frame, Land};
use osage_props::{Use, Woods};

/// The window of views a chunk keeps on the GPU: columns, rows.
const WINDOW: (usize, usize) = (5, 3);
/// A chunk's side, m.
pub const CHUNK: f32 = 60.0;
/// The fewest trees worth a chunk.
const FEWEST: usize = 15;
/// How high a kart's camera looks from, m, and how far a chunk is seen.
const EYE: f32 = 1.5;
const SEEN: f32 = 450.0;
/// The grid's step, degrees, and a margin round the directions seen.
const STEP: f32 = 3.0;
const MARGIN: f32 = 2.0;

/// The real tree a hologram is baked from, for a model the woods' cards are
/// made from (D-133: a hologram's geometry costs nothing where it is drawn,
/// so it is the most detailed there is): the cards' model, the baked one,
/// and the scale the cards' model stands at on average, which the baked one
/// takes as its own size. Poly Haven's pines and firs of its pine forest,
/// whole, needles modelled: 15 to 20 m, 5 to 6 million faces a tree.
const REAL_TREES: [(&str, &str, f32); 2] = [
    ("polyhaven/pine_sapling_small/pine_sapling_small_1k.gltf", "polyhaven/pine_tree_01/pine_tree_01_2k.gltf", 6.5),
    ("polyhaven/fir_sapling_medium/fir_sapling_medium_1k.gltf", "polyhaven/fir_tree_01/fir_tree_01_2k.gltf", 1.08),
];

/// The model a chunk bakes in place of `path`, and how much smaller a place
/// scales it.
fn baked_as(path: &str) -> (&str, f32) {
    REAL_TREES.iter().find(|(cards, ..)| *cards == path).map_or((path, 1.0), |(_, real, usual)| (*real, *usual))
}

/// A chunk's name, from its square.
fn name(cell: IVec2) -> String {
    format!("chunk-{}-{}", cell.x, cell.y)
}

fn cell_of(at: Vec3) -> IVec2 {
    (Vec2::new(at.x, at.z) / CHUNK).floor().as_ivec2()
}

/// Writes each chunk of `woods` as a scene for `hologram-bake`, under `dir`.
pub fn write_chunks(dir: &Path, track: &str, woods: &[Woods], land: &Land, frames: &[Frame]) -> std::io::Result<usize> {
    let mut chunks: std::collections::BTreeMap<(i32, i32), Vec<(usize, Transform)>> = Default::default();
    for (kind, w) in woods.iter().enumerate() {
        for place in &w.places {
            let cell = cell_of(place.translation);
            chunks.entry((cell.x, cell.y)).or_default().push((kind, *place));
        }
    }
    let mut written = 0;
    let mut list = String::new();
    for ((x, z), trees) in chunks {
        if trees.len() < FEWEST {
            continue;
        }
        let cell = IVec2::new(x, z);
        let middle = (cell.as_vec2() + 0.5) * CHUNK;
        let ground = trees.iter().map(|(_, t)| t.translation.y).sum::<f32>() / trees.len() as f32;
        // Turned to face the nearest point of the track.
        let nearest = frames.iter().min_by(|a, b| a.at.distance(middle).total_cmp(&b.at.distance(middle)));
        let Some(nearest) = nearest else { continue };
        let facing = (nearest.at - middle).normalize_or(Vec2::Y);
        let turn = Quat::from_rotation_y(facing.x.atan2(facing.y));
        let frame = Transform::from_xyz(middle.x, ground, middle.y).with_rotation(turn);
        let inverse = frame.compute_affine().inverse();
        // The directions from which the track sees it, from a kart's height.
        let target = Vec3::new(middle.x, ground + 6.0, middle.y);
        let (mut az, mut el) = ((f32::MAX, f32::MIN), (f32::MAX, f32::MIN));
        for f in frames.iter().step_by(3) {
            let eye = Vec3::new(f.at.x, land.height(f.at) + EYE, f.at.y);
            if eye.distance(target) > SEEN {
                continue;
            }
            let d = (turn.inverse() * (eye - target)).normalize_or_zero();
            let (a, e) = (d.x.atan2(d.z).to_degrees(), d.y.clamp(-1.0, 1.0).asin().to_degrees());
            (az, el) = ((az.0.min(a), az.1.max(a)), (el.0.min(e), el.1.max(e)));
        }
        if az.0 > az.1 {
            continue;
        }
        let snap = |v: f32, up: bool| if up { (v / STEP).ceil() * STEP } else { (v / STEP).floor() * STEP };
        let (a0, a1) = (snap(az.0 - MARGIN, false).max(-177.0), snap(az.1 + MARGIN, true).min(177.0));
        let (e0, e1) = (snap(el.0 - MARGIN, false).max(-10.0), snap(el.1 + MARGIN, true).min(60.0));
        // A wide arc of azimuths with coarser steps: views stay few.
        let a_step = if a1 - a0 > 120.0 { STEP * 2.0 } else { STEP };
        let mut models: Vec<SceneModel> = woods
            .iter()
            .map(|w| SceneModel {
                path: baked_as(&w.model.path().map(|p| p.to_string()).unwrap_or_default()).0.to_owned(),
                variants: w.using == Use::Variants,
                places: Vec::new(),
            })
            .collect();
        let shrink: Vec<f32> =
            woods.iter().map(|w| baked_as(&w.model.path().map(|p| p.to_string()).unwrap_or_default()).1).collect();
        for (kind, place) in &trees {
            let local = inverse.transform_point3(place.translation);
            let rotation = turn.inverse() * place.rotation;
            models[*kind].places.push([
                local.x,
                local.y,
                local.z,
                rotation.x,
                rotation.y,
                rotation.z,
                rotation.w,
                place.scale.x / shrink[*kind],
            ]);
        }
        models.retain(|m| !m.places.is_empty() && !m.path.is_empty());
        let at = frame.translation;
        let r = frame.rotation;
        let scene = Scene { models, grid: [a0, a1, a_step, e0, e1, STEP], at: [at.x, at.y, at.z, r.x, r.y, r.z, r.w] };
        let folder = dir.join(name(cell));
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join("scene.json"), serde_json::to_string(&scene).unwrap_or_default())?;
        list.push_str(&format!("{} {} {}\n", name(cell), trees.len(), nearest.at.distance(middle).round()));
        written += 1;
    }
    std::fs::write(dir.join("chunks.txt"), list)?;
    info!(chunks = written, track, ?dir, "woods: chunks written for baking");
    Ok(written)
}

/// The chunks of `track` baked and installed under `root`: each one's
/// folder (under the manifest's content), its square and its framing.
pub fn baked(root: &Path, track: &str) -> Vec<(String, IVec2, Framing)> {
    let base = format!("holograms/woods-{track}");
    let Ok(entries) = std::fs::read_dir(root.join(&base)) else { return Vec::new() };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let folder = entry.file_name().to_string_lossy().into_owned();
        let Some(cell) = folder.strip_prefix("chunk-").and_then(parse_cell) else { continue };
        let framing = std::fs::read_to_string(entry.path().join(Framing::FILE))
            .ok()
            .and_then(|text| serde_json::from_str::<Framing>(&text).ok());
        let complete = entry.path().join("albedo-r00-c00.png").exists();
        if let (Some(framing), true) = (framing, complete) {
            found.push((format!("{base}/{folder}"), cell, framing));
        }
    }
    found.sort_by_key(|(folder, ..)| folder.clone());
    found
}

/// "X-Z" with X and Z signed integers.
fn parse_cell(text: &str) -> Option<IVec2> {
    let bytes = text.as_bytes();
    let split = (1..bytes.len()).find(|&i| bytes[i] == b'-' && bytes[i - 1].is_ascii_digit())?;
    Some(IVec2::new(text[..split].parse().ok()?, text[split + 1..].parse().ok()?))
}

/// Draws the baked chunks as holograms and leaves their trees out of the
/// woods' cards. How many chunks were drawn.
pub fn draw(commands: &mut Commands, root: &Path, track: &str, woods: &mut [Woods]) -> usize {
    let chunks = baked(root, track);
    if chunks.is_empty() {
        return 0;
    }
    let cells: std::collections::HashSet<IVec2> = chunks.iter().map(|(_, cell, _)| *cell).collect();
    for w in woods.iter_mut() {
        w.places.retain(|place| !cells.contains(&cell_of(place.translation)));
    }
    for (folder, _, framing) in &chunks {
        // Only a window of views on the GPU, slid as the camera moves.
        let hologram = osage_hologram::Hologram { window: Some(WINDOW), ..framing.hologram(folder) };
        commands.spawn((crate::scene::Part, hologram, framing.transform()));
    }
    info!(chunks = chunks.len(), track, "woods: chunks drawn as holograms");
    chunks.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_names_read_back_with_their_signs() {
        for cell in [IVec2::new(3, 4), IVec2::new(-3, 4), IVec2::new(3, -4), IVec2::new(-12, -7)] {
            let name = name(cell);
            assert_eq!(parse_cell(name.strip_prefix("chunk-").unwrap()), Some(cell), "{name}");
        }
    }

    #[test]
    fn saplings_are_baked_as_grown_trees() {
        let (real, usual) = baked_as("polyhaven/fir_sapling_medium/fir_sapling_medium_1k.gltf");
        assert!(real.contains("fir_tree_01") && usual > 0.0);
        assert_eq!(baked_as("polyhaven/rock_09/rock_09.gltf"), ("polyhaven/rock_09/rock_09.gltf", 1.0));
    }

    #[test]
    fn places_fall_in_their_squares() {
        assert_eq!(cell_of(Vec3::new(10.0, 3.0, 70.0)), IVec2::new(0, 1));
        assert_eq!(cell_of(Vec3::new(-0.5, 0.0, -61.0)), IVec2::new(-1, -2));
    }
}
