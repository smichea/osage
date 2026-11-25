// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Rebuild the exact ordered collider for repeated meshes whose vertices lie at eight box corners.
use super::{Bounds, Guard, Index, Residual, Shape};
use anyhow::{Context, Result, ensure};
use bevy::math::Vec3;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Pattern(Vec<u8>, Vec<[u32; 3]>);
#[derive(Deserialize)]
struct Placement([i32; 2], usize, usize, Bounds);
#[derive(Deserialize)]
pub(super) struct Meshes {
    patterns: Vec<Pattern>,
    placements: Vec<Placement>,
}

impl Meshes {
    pub(super) fn expand(self, guard: &mut Guard, index: &Index, pages: &[usize]) -> Result<()> {
        let tiles: BTreeMap<_, _> = pages.iter().map(|&i| ([index.tiles[i].x, index.tiles[i].z], i)).collect();
        for Pattern(codes, faces) in &self.patterns {
            ensure!(
                (8..=36).contains(&codes.len())
                    && codes.iter().all(|&n| n < 8)
                    && (0..8).all(|n| codes.contains(&n))
                    && faces.len() == 12
                    && faces.iter().flatten().all(|&i| (i as usize) < codes.len()),
                "invalid collision corner pattern"
            );
        }
        for Placement(cell, slot, pattern, bounds) in self.placements {
            let &i = tiles.get(&cell).context("unknown corner mesh tile")?;
            let layer = index.tiles[i].physics.as_ref().context("missing corner mesh layer")?;
            ensure!(
                (0..3).all(|a| bounds[0][a] < bounds[1][a]
                    && bounds[0][a] >= layer.bounds[0][a]
                    && bounds[1][a] <= layer.bounds[1][a]),
                "corner mesh outside collision layer"
            );
            let Pattern(codes, faces) = self.patterns.get(pattern).context("unknown collision corner pattern")?;
            let page = &mut guard.catalogue[i];
            let j = page.slots.binary_search(&slot).ok().context("corner mesh lacks certified upper faces")?;
            ensure!(!page.complete.contains(&j), "duplicate certified corner mesh");
            let vertices: Vec<[f32; 3]> =
                codes.iter().map(|&n| std::array::from_fn(|a| bounds[usize::from(n >> a & 1)][a])).collect();
            let shape = &mut page.shapes[j];
            let old_faces = shape.triangles.len();
            let remaining = &mut guard.residual[i];
            let before = remaining.len();
            verify_faces(shape, &vertices, faces, remaining, slot)?;
            shape.verify_face_order(&vertices, faces)?;
            shape.faces.subset = None;
            remaining.retain(|r| r.shape != Some(slot));
            guard.stats.residual_boxes -= before - remaining.len();
            guard.stats.catalogue_triangles += faces.len() - old_faces;
            guard.stats.complete_corner_meshes += 1;
            shape.vertices = vertices;
            shape.triangles.clone_from(faces);
            page.complete.push(j);
        }
        Ok(())
    }
}

fn verify_faces(
    certified: &Shape,
    vertices: &[[f32; 3]],
    faces: &[[u32; 3]],
    residual: &[Residual],
    slot: usize,
) -> Result<()> {
    let mut upper = certified.triangles.iter();
    let mut omitted = BTreeMap::new();
    let key = |b: Bounds| b.map(|p| p.map(f32::to_bits));
    for r in residual.iter().filter(|r| r.shape == Some(slot)) {
        *omitted.entry(key(r.bounds)).or_insert(0usize) += r.faces;
    }
    for face in faces {
        let points = face.map(|v| vertices[v as usize]);
        let [a, b, c] = points.map(Vec3::from_array);
        let normal = (b - a).cross(c - a);
        ensure!(normal.is_finite() && normal.length_squared() > 0., "degenerate certified corner triangle");
        if normal.y > normal.length() * 0.5 {
            let saved = upper.next().context("extra upper triangle in corner pattern")?;
            ensure!(
                saved.map(|v| certified.vertices[v as usize]) == points,
                "certified upper face differs from corner pattern"
            );
        } else {
            let bounds = [a.min(b).min(c).to_array(), a.max(b).max(c).to_array()];
            let count = omitted.get_mut(&key(bounds)).context("unexpected residual triangle in corner pattern")?;
            *count = count.checked_sub(1).context("extra residual triangle in corner pattern")?;
        }
    }
    ensure!(upper.next().is_none() && omitted.values().all(|&n| n == 0), "incomplete certified corner mesh");
    Ok(())
}
