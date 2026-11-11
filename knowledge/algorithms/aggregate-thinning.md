---
type: Algorithm
title: Aggregate thinning
description: A coarser level of detail for foliage — needles, leaves, blades — made by keeping a random share of its small pieces and growing each kept one so that the crown covers as much as before.
resource: https://github.com/smichea/osage/blob/main/crates/osage-props/src/lod.rs
licence: MIT OR Apache-2.0
status: draft
tags: [level-of-detail, vegetation, geometry]
sources:
  - id: cook
    resource: https://graphics.pixar.com/library/StochasticSimplification/paper.pdf
    title: "Robert L. Cook, John Halstead, Maxwell Planck, David Ryu, Stochastic Simplification of Aggregate Detail, SIGGRAPH 2007"
  - id: own
    resource: "osage's own design and measurements, 2026-11-11"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-11T09:00:00Z" }
---

# Problem

A tree from a scan library is half a million triangles of needles or leaves. Seen from fifty metres it needs a fraction of them. Simplifying it like a solid — merging nearby vertices, collapsing edges — merges each needle into nothing: the crown goes bare and speckled, and the tree reads as a wire sculpture.

# Idea

A crown is an aggregate: many small pieces, none of which matters alone. Keep a random share `k` of the pieces and grow each kept one by `1/√k` about its own centre: the total area, hence how much of the sky the crown hides, stays the same in expectation[^cook].

# Method

1. **Find the pieces.** Weld the vertices that share a position (so that a texture seam does not split a leaf), then join the corners of every triangle with a union–find: each connected set of vertices is a piece.
2. **Sort them.** A piece whose box is at least 10 % of the mesh's size is large (a trunk, a bough) and kept as it is. The others are small. A mesh with fewer than 64 small pieces is not an aggregate: simplify it as a solid instead.
3. **Choose.** Keep a small piece when a hash of its number and a seed is below `k`. One seed for every level: the pieces a coarser level keeps are among those the finer one keeps, so that switching levels only removes pieces, it never moves the others.
4. **Grow.** Move each vertex of a kept small piece to `c + (p − c)/√k`, where `c` is the centre of the piece's box. Normals, texture coordinates and tangents stay as they were.
5. **Rebuild.** Keep the vertices of kept pieces, renumbered in order, and the triangles all of whose corners are kept.

In osage, each mesh of a scatter seen far gets two levels kept at 15 % and 3 %, drawn from 4 % and 15 % of the distance the scatter is seen at; a needle is thinner than a pixel a few metres off, so the full mesh is needed only close by[^own].

# Invariants

The share of triangles and the total area of the small pieces follow `k`; the mesh keeps its size; large pieces are untouched; the result is the same at every run. Tested.

# Cost

Linear in the mesh's size, once per mesh at load: well under a second for a 700 000-triangle tree in a release build. At draw time, less than the full mesh by the share kept. Measured at T3 on the Iris Xe at 2560×1440, 2026-11-11, the kart circuit's race view: about 127 ms per frame with vertex clustering, 165 ms with levels kept at 20 % and 4 % from 6 % and 25 % of the distance, 126 ms with the levels above ([the render bench](/oracles/render-bench.md)).

# Failure modes

- A tree whose needles are modelled joined to their twig has few small pieces and many large ones: the large ones are kept whole and the level stays heavy (a Poly Haven pine sapling goes from 142 000 triangles to 43 000 and 24 000, not to 3 % of it).
- Grown pieces become visible as such up close: long needles look like strands. The levels must start where a piece is about a pixel.
- Thinning keeps the crown's coverage, not its shade: Cook et al. also adjust the colour of the kept pieces, which this does not.

# Alternatives

Vertex clustering (the same module, for solid things); error-driven edge collapse, which keeps a silhouette but not a crown's transparency; impostors — a tree rendered beforehand from several sides into a texture and drawn as a card — far cheaper beyond a hundred metres, at the price of a texture per tree.

# Oracle

The tests of `lod.rs` (a share of the pieces and their area kept, a solid mesh refused, a large piece kept whole) and the render bench's screenshots.

# Reuse

Engine-independent: any mesh of many small pieces — grass, leaves, hair cards, gravel — in any renderer.

[^cook]: Cook et al. 2007
[^own]: osage design
