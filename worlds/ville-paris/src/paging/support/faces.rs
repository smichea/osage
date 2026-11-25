// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{Guard, Index};
use crate::paging::tiles::faces::Mapping;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct Selection {
    start: u32,
    #[serde(default)]
    exceptions: Vec<(usize, usize, Vec<[u32; 2]>)>,
}

impl Selection {
    pub(super) fn apply(self, guard: &mut Guard, index: &Index, pages: &[usize]) -> Result<()> {
        ensure!(
            self.exceptions.windows(2).all(|r| (r[0].0, r[0].1) < (r[1].0, r[1].1)),
            "duplicate or unordered certified face selections"
        );
        let mut exceptions = self.exceptions.into_iter().peekable();
        for (physical, &i) in pages.iter().enumerate() {
            let tile = &index.tiles[i];
            let Some(layer) = &tile.physics else {
                continue;
            };
            let page = &mut guard.catalogue[i];
            let mut counts = vec![0usize; layer.ids.len()];
            for (&slot, shape) in page.slots.iter().zip(&page.shapes) {
                counts[slot] = shape.triangles.len();
            }
            for omitted in &guard.residual[i] {
                let slot = omitted.shape.context("certified face ordering requires per-slot residuals")?;
                counts[slot] = counts[slot].checked_add(omitted.faces).context("collision piece size overflow")?;
            }
            for (slot, full) in counts.into_iter().enumerate() {
                let mut order = layer.face_order(slot, full)?;
                let Ok(j) = page.slots.binary_search(&slot) else {
                    continue;
                };
                let shape = &mut page.shapes[j];
                let count = u32::try_from(shape.triangles.len())?;
                let full = u32::try_from(full)?;
                if count < full {
                    let chosen = if exceptions.peek().is_some_and(|e| (e.0, e.1) == (physical, slot)) {
                        let (_, _, runs) = exceptions.next().expect("checked certified face selection");
                        Mapping::runs(&runs, count, full)?
                    } else {
                        Mapping::span(self.start, count, full)?
                    };
                    order.subset = Some(chosen);
                }
                shape.faces = order;
            }
        }
        ensure!(exceptions.next().is_none(), "unused certified face selection");
        Ok(())
    }
}
