// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{CertifiedPage, Guard, Index, World};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};

impl Guard {
    pub(super) fn read_bundle(&mut self, idx: &Index, pages: &[usize], raw: &[u8]) -> Result<()> {
        let (expected, mut input) = raw
            .strip_prefix(b"ODR2")
            .and_then(|bytes| bytes.split_at_checked(32))
            .context("invalid complete collision region header")?;
        let mut binding = Sha256::new();
        let mut decoded = Vec::with_capacity(pages.len());
        let mut addresses = Vec::with_capacity(pages.len());
        for &i in pages {
            let t = &idx.tiles[i];
            let layer = t.physics.as_ref().context("region contains a nonphysical tile")?;
            ensure!(layer.collision_extension() == "cpage", "complete region requires packed collision pages");
            let (page, rest) =
                input.split_at_checked(layer.content.bytes).context("truncated regional collision page")?;
            input = rest;
            let digest: [u8; 32] = Sha256::digest(page).into();
            binding.update(t.x.to_le_bytes());
            binding.update(t.z.to_le_bytes());
            binding.update(crate::bench::hex(&digest));
            addresses.push(digest);
            decoded.push(World::read_shapes(page, layer)?);
        }
        ensure!(input.is_empty() && binding.finalize()[..] == *expected, "complete region directory mismatch");
        idx.resolve_region(pages, &addresses)?;
        for (&i, shapes) in pages.iter().zip(decoded) {
            self.stats.catalogue_triangles += shapes.iter().map(|s| s.triangles.len()).sum::<usize>();
            self.residual[i].clear();
            self.catalogue[i] =
                CertifiedPage { slots: (0..shapes.len()).collect(), complete: (0..shapes.len()).collect(), shapes };
        }
        Ok(())
    }
}
