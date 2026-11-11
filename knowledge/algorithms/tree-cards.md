---
type: Algorithm
title: Tree cards
description: Woods of thousands of trees for a few thousand triangles — each tree drawn as crossed cards showing pictures of the real model, rendered on the processor at load from its own geometry and textures.
resource: https://github.com/smichea/osage/blob/main/crates/osage-props/src/impostor.rs
licence: MIT OR Apache-2.0
status: draft
tags: [vegetation, impostors, level-of-detail, rasterisation]
sources:
  - id: castano
    resource: http://the-witness.net/news/2010/09/computing-alpha-mipmaps/
    title: "Ignacio Castaño, Computing alpha mipmaps, 2010"
  - id: own
    resource: "osage's own design and measurements, 2026-11-11"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-11T13:00:00Z" }
---

# Problem

A circuit in the countryside is ringed by woods. A modelled tree is hundreds of thousands of triangles; even thinned to its coarsest level it is thousands, and a few hundred of them fill the horizon with a sparse grove at a cost that matters. Seen from more than a hundred metres, a tree covers a few dozen pixels.

# Idea

Draw a tree that is always far as a picture of itself. Crossed cards — flat quads through the trunk, each turned to its own direction — are the oldest trick for it; here each card shows the real model seen from its side, so the woods match the trees near the track. The pictures are made at load from the model, so no artist draws them and any tree of the library can become woods.

# Method

1. **Triangles.** Gather the model's triangles (each variant on its own), placed as in the model, with their texture coordinates and their material: a colour factor, a colour texture readable by the processor, an alpha cut.
2. **Views.** For `k` in `0..3`, look from `(sin θ, 0, cos θ)` with `θ = kπ/3`, right `(cos θ, 0, −sin θ)`. Fit an orthographic square to the tree: its side the larger of its width along the right and its height, centred on its width, its foot at the bottom.
3. **Raster.** Draw every triangle into a square twice as fine as the picture: for each pixel centre inside it (barycentric weights all positive), interpolate depth and texture coordinates, sample the texture, drop the pixel if its alpha is under the cut, keep the nearest. Darken each pixel by up to 45 % with its depth into the crown: light reaches the inside of a tree less.
4. **Average** each 2 × 2 into one pixel: colour weighted by coverage, coverage as alpha. Soft edges.
5. **Atlas.** Views in columns, variants in rows. Fill transparent pixels with the mean colour, so that filtering never pulls black into the edges. Halve level by level; at each level scale alpha (by bisection) so that as many pixels pass the cut as at the top: a card does not thin out as it shrinks[^castano].
6. **Cards.** For each place, for each view, a quad through the trunk, facing the view's direction, as wide and tall as the view's square, its texture the view's cell; both faces, counter-clockwise from the side each faces. Normals lean up and out from the trunk, so that the card is lit like a rounded crown rather than a sheet. Every place of a set of woods goes into one mesh, alpha-tested at 0.5, casting no shadow.

In osage, the kart circuit's woods are groves of pines, firs and broadleaf trees from 110 m off the track to the edge of the modelled land: about one tree per 20 m², one per 3.6 m cell at most (at T2, where no modelled tree stands nearer, from 35 m): some 18 000 trees (8 200 pines, 7 300 firs, 2 700 broadleaf trees), 440 000 triangles[^own].

# Invariants

A view is fitted to the tree, its foot at the bottom, its trunk where its axis falls; the atlas keeps its coverage across its levels; each place gets two faces of a card per view, from its foot to the tree's top, scaled with it. Tested.

# Cost

At load, a fraction of a second per model in a release build (700 000 triangles, three views of 512² before averaging). At draw, 24 triangles per tree. Measured at T3 on the Iris Xe at 2560×1440, 2026-11-11: replacing 700 modelled trees of the far woods by some 18 000 cards took the race view from 128 to 115 ms and the wide view over the meadows from 113 to 98 ms ([the render bench](/oracles/render-bench.md)).

# Failure modes

- Up close, a card is a card: woods must stay far from every camera (here, from the track).
- Seen from above, cards thin to lines: fine from a kart, not from a helicopter. A view from above would be a fourth card, lying flat.
- The lighting is the card's, not the leaves': no self-shadowing beyond the baked darkening.
- A texture the processor cannot read (compressed, or kept only on the GPU) leaves the material's plain colour.

# Alternatives

Octahedral impostors (many views, chosen per pixel by the view direction; no visible crossing, a larger atlas and a custom shader); billboards turned to the camera (one card, rotating, needing a shader or a per-frame update); rendering the pictures on the GPU (faster, but a render pass to schedule at load).

# Oracle

The tests of `impostor.rs` and of [osage-kart-roadside](/modules/kart-roadside.md)'s woods; [the render bench](/oracles/render-bench.md) screenshots.

# Reuse

Engine-independent: any model of many leaves seen from afar — trees, bushes, reeds.

[^castano]: Castaño, alpha mipmaps
[^own]: osage design
