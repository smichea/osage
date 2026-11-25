// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Identify contacts on an authored collision surface, across its page pieces and certified subsets.
use super::{Index, Layer, Shape};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Deserialize)]
pub(in crate::paging) struct Piece(usize, u32, Vec<[u32; 2]>);

#[derive(Clone)]
pub(in crate::paging) struct Run {
    local: u32,
    source: u32,
}

/// Local ranges are contiguous, so only their start offsets need to be stored.
#[derive(Clone)]
pub(in crate::paging) enum Mapping {
    Span { first: u32, count: u32 },
    Runs { count: u32, starts: Arc<[Run]> },
}

impl Mapping {
    pub(in crate::paging) fn span(first: u32, count: u32, limit: u32) -> Result<Self> {
        ensure!(count > 0 && first.checked_add(count).is_some_and(|n| n <= limit), "source face range outside surface");
        Ok(Self::Span { first, count })
    }

    pub(in crate::paging) fn runs(input: &[[u32; 2]], expected: u32, limit: u32) -> Result<Self> {
        let count = length(input, limit)?;
        ensure!(count == expected, "source face range count differs from collision geometry");
        if let &[[first, n]] = input {
            return Self::span(first, n, limit);
        }
        let mut local = 0;
        let starts = input
            .iter()
            .map(|&[source, n]| {
                let span = Run { local, source };
                local += n;
                span
            })
            .collect();
        Ok(Self::Runs { count, starts })
    }

    fn get(&self, local: u32) -> Option<u32> {
        match self {
            Self::Span { first, count } => (local < *count).then(|| first + local),
            Self::Runs { count, starts } if local < *count => {
                let r = &starts[starts.partition_point(|r| r.local <= local) - 1];
                Some(r.source + (local - r.local))
            }
            _ => None,
        }
    }
}

fn length(runs: &[[u32; 2]], limit: u32) -> Result<u32> {
    ensure!(!runs.is_empty(), "empty source face ranges");
    let mut previous = None;
    let mut count = 0u32;
    for &[first, size] in runs {
        let end = first.checked_add(size).context("source face range overflow")?;
        ensure!(
            size > 0 && end <= limit && previous.is_none_or(|before| before < first),
            "source face ranges overlap, touch or exceed their surface"
        );
        count = count.checked_add(size).context("source face count overflow")?;
        previous = Some(end);
    }
    Ok(count)
}

#[derive(Clone, Default)]
pub(in crate::paging) struct FaceOrder {
    source: Option<Mapping>,
    /// The certified mesh can contain a strict subset of the full page piece.
    pub(in crate::paging) subset: Option<Mapping>,
}

impl FaceOrder {
    pub(in crate::paging) fn source_triangle(&self, local: u32) -> Option<u32> {
        self.source.as_ref()?.get(self.full_triangle(local)?)
    }

    fn full_triangle(&self, local: u32) -> Option<u32> {
        self.subset.as_ref().map_or(Some(local), |map| map.get(local))
    }
}

impl Layer {
    pub(in crate::paging) fn face_order(&self, slot: usize, count: usize) -> Result<FaceOrder> {
        if !self.ordered_faces || self.source_nodes.get(slot).is_none() {
            return Ok(FaceOrder::default());
        }
        let count = u32::try_from(count)?;
        let source = match self.source_face_runs.binary_search_by_key(&slot, |r| r.0) {
            Ok(i) => {
                let Piece(_, limit, runs) = &self.source_face_runs[i];
                Mapping::runs(runs, count, *limit)?
            }
            Err(_) => Mapping::span(0, count, count)?,
        };
        Ok(FaceOrder { source: Some(source), subset: None })
    }
}

impl Shape {
    /// Cross-check a certified native triangle's declared full ordinal when the content arrives.
    pub(in crate::paging) fn verify_face_order(&self, vertices: &[[f32; 3]], triangles: &[[u32; 3]]) -> Result<()> {
        if self.faces.source.is_none() {
            return Ok(());
        }
        for (i, face) in self.triangles.iter().enumerate() {
            let index = self.faces.full_triangle(u32::try_from(i)?).context("missing certified face ordinal")?;
            let full = triangles.get(usize::try_from(index)?).context("certified face ordinal outside full mesh")?;
            let matching = face.iter().zip(full).all(|(&a, &b)| {
                self.vertices
                    .get(a as usize)
                    .zip(vertices.get(b as usize))
                    .is_some_and(|(p, q)| p.map(f32::to_bits) == q.map(f32::to_bits))
            });
            ensure!(matching, "certified face ordinal differs from full collision geometry");
        }
        Ok(())
    }
}

pub(super) fn validate(index: &mut Index) -> Result<()> {
    let Some(version) = index.collision_face_order else {
        return Ok(());
    };
    ensure!(version == 1, "unknown collision face ordering");
    let mut whole = BTreeSet::new();
    let mut pieces = BTreeMap::new();
    for layer in index.tiles.iter_mut().filter_map(|t| t.physics.as_mut()) {
        layer.ordered_faces = true;
        ensure!(
            layer.source_face_runs.windows(2).all(|r| r[0].0 < r[1].0)
                && layer.source_face_runs.iter().all(|r| r.0 < layer.ids.len() && r.0 < layer.source_nodes.len()),
            "invalid source face slots"
        );
        for (slot, (&owner, id)) in layer.source_nodes.iter().zip(&layer.ids).enumerate() {
            let key = (owner, id.as_str());
            match layer.source_face_runs.binary_search_by_key(&slot, |r| r.0) {
                Ok(i) => {
                    let Piece(_, total, runs) = &layer.source_face_runs[i];
                    ensure!(length(runs, *total)? < *total && !whole.contains(&key), "invalid collision fragment");
                    let (limit, ranges) = pieces.entry(key).or_insert_with(|| (*total, Vec::new()));
                    ensure!(*total == *limit, "collision fragments disagree on source size");
                    ranges.extend_from_slice(runs);
                }
                Err(_) => ensure!(!pieces.contains_key(&key) && whole.insert(key), "ambiguous whole collision surface"),
            }
        }
    }
    for (limit, mut ranges) in pieces.into_values() {
        ranges.sort_unstable();
        let mut next = 0;
        for [first, count] in ranges {
            ensure!(first == next, "collision fragments overlap or leave missing source faces");
            next += count;
        }
        ensure!(next == limit, "incomplete source collision surface");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_noncontiguous_proxy_maps_through_noncontiguous_source_runs() {
        let order = FaceOrder {
            source: Some(Mapping::runs(&[[2, 2], [9, 3], [80, 1]], 6, 81).unwrap()),
            subset: Some(Mapping::runs(&[[1, 2], [5, 1]], 3, 6).unwrap()),
        };
        assert_eq!([0, 1, 2, 3].map(|n| order.source_triangle(n)), [Some(3), Some(9), Some(80), None]);
        assert_eq!(FaceOrder::default().source_triangle(0), None);
        let full = FaceOrder { subset: None, ..order };
        assert_eq!(full.source_triangle(4), Some(11));
        assert_eq!(full.source_triangle(6), None);
    }

    #[test]
    fn invalid_ranges_fail_without_expanding_their_length() {
        for bad in [vec![], vec![[1, 0]], vec![[0, 2], [1, 1]], vec![[0, 1], [1, 2]], vec![[5, 2]], vec![[u32::MAX, 2]]]
        {
            assert!(Mapping::runs(&bad, 3, 6).is_err());
        }
        assert!(Mapping::runs(&[[1, 3]], 2, 6).is_err());
        let last = Mapping::span(u32::MAX - 1, 1, u32::MAX).unwrap();
        assert_eq!(last.get(0), Some(u32::MAX - 1));
        assert_eq!(last.get(1), None);
    }
}
