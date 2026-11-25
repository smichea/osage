---
type: Algorithm
title: Lossless storage of district collision meshes
description: Encodes world-space binary32 positions and ordered triangles in streamed collision pages while retaining each surface on its source node.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/paging_collision_codec.py
licence: MIT OR Apache-2.0
status: draft
tags: [paris, streaming, physics, binary-format]
sources:
  - id: reader
    title: Native OCP1 decoder
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/tiles/collision.rs
  - id: cooker
    title: District asset writer
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/cook-pages.py
  - id: audit
    title: Ordered binary geometry and package audit
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-collision-codec.py
  - id: tests
    title: Edge cases and invalid packets
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/test_paging_collision_codec.py
generated: { by: "OpenAI/Codex", at: "2026-11-23T21:10:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A single road slab in the Bir-Hakeim district takes 1,423 bytes as a JSON collision page. Its twelve triangles reference twenty-four positions, many of which repeat a corner. A physics solver can depend on index order, orientation and repeated vertices. The transfer format must reconstruct them all before building the collider.

# Idea

Store the exact world-space binary32 coordinates once per distinct corner when the source has a known topology. Otherwise keep the full indexed mesh. Each record's position in the directory determines its surface ID, source node and physical material. Two nodes can use the same name and geometry while keeping distinct owners.

# Method

A `.cpage` is an uncompressed little-endian byte stream. It begins with ASCII `OCP1` and the shape count as a 32-bit unsigned integer. One unaligned record follows for each directory slot. The first byte selects one of four layouts.

| Tag | Bytes after tag | Payload |
| --- | ---: | --- |
| 0 | 8 + 12V + 3TI | Two unsigned 32-bit counts V and T, V triples of binary32 coordinates, T ordered index triples. The unsigned index width I is one byte for V ≤ 256, two for V ≤ 65,536, four otherwise. |
| 1 | 96 | Coordinates of eight corners in their source order. |
| 2 | 40 | Two horizontal rectangular rings with independent extents. Ten binary32 values hold `a,b,y,c,d,e,f,Y,g,h`. |
| 3 | 24 | An axis-aligned box. Six binary32 values hold `a,b,y,c,d,Y`. Expand the second ring using `e=b, f=a, g=c, h=d`. |

The ordered ring corners are `(a,y,c), (a,y,d), (b,y,d), (b,y,c), (e,Y,g), (e,Y,h), (f,Y,h), (f,Y,g)`. For tags 1–3 the vertex-to-corner table is `0 1 2 3 / 4 5 6 7 / 3 2 5 4 / 2 1 6 5 / 1 0 7 6 / 0 3 4 7`. For each group starting at vertex k, expand `(k,k+1,k+2)` then `(k,k+2,k+3)`. Bit 7 may be set on tags 1–3 to exchange indices 1 and 2 of every triangle. The encoder tests all original position bits and the whole ordered index table before choosing the shortest matching form. Every other ordering takes tag 0. The page has no padding or trailing data.[^reader]

# Invariants

The tile directory carries the byte count and SHA-256 of the complete page. Slot order binds each shape to its surface and optional material. Directories with a source tree also supply the node for every slot. Original version 1 directories can lack owners. Compact forms restore all twenty-four vertex positions, including duplicated corners and signed zero. A shear or arbitrary orientation can use tag 1 when its corner mapping and topology survive. A changed corner order or face list still fits tag 0.

# Cost

On the authored Bir-Hakeim GLB at a 128 m grid, the 226 physical pages carry 441 surfaces, 10,584 positions and 5,292 triangles. Their combined files shrink from 566,789 to 19,777 bytes. Of 441 records, 434 use two independent rings and seven use the box. The entire district index and all 555 unique referenced files shrink from 1,856,580 to 1,309,380 bytes. The index, coarse visual parent and support catalogue together use 563,275 versus 563,087 bytes. The discarded bytes therefore largely affect the full physics resources requested during movement. The isolated road's physical page uses 33 bytes. Sizes were counted from the actual directory and SHA-verified bodies on Linux with an Intel Core i5-1335U on 23 November 2026. These measurements count file bytes independently of timing, GPU activity and HTTP framing.[^audit]

The 226 packed files and the selected bootstrap support catalogue together occupy 38,133 bytes. The mixed visual and physical index occupies another 200,398 bytes. On the 128 m district, paired descents onto the road, roof and rail preserve all body samples, contact ticks and page events with either encoding of the support catalogue. [Bootstrap compression and cache behaviour](/algorithms/district-paging.md)

# Failure modes

A large irregular mesh retains its complete binary32 coordinates. A body with a bad SHA, unknown tag, truncated record, excessive count, out-of-range index, nonfinite position, out-of-bounds position or trailing bytes fails verification. The solver still reconstructs a complete triangle mesh in memory. A collider with fewer than three distinct corners may be topologically degenerate even if its indices fit the storage grammar.[^reader]

# Alternatives

The historical JSON body remains readable. The cooker accepts `--physics-storage json` to export it for an older executable. Quantizing world coordinates would change their bit patterns and the contact geometry at narrow edges.[^cooker]

# Oracle

Cook the same GLB into two directories, once with the default and once with `--physics-storage json`. Run `python worlds/ville-paris/check-collision-codec.py --legacy JSON_DIR --packed OCP1_DIR --out audit.json`. It verifies every unique referenced file, ordered binary32 and index bits, identities, physical materials, unchanged visual channels, and the rebinding of the same support geometry to the new digests. Boundary cases cover all four forms, reflected winding, signed zero, subnormals, index widths, malformed counts and a legacy directory without an owner.[^audit][^tests]

# Reuse

Use the file suffix to select the geometry decoder. Expose one shared decoder to every off-line tool that reads physical pages. Normal drawing and acoustic asset decoders can use their existing resource formats. [District paging](/algorithms/district-paging.md)

[^reader]: [Native OCP1 decoder](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/tiles/collision.rs).
[^cooker]: [District asset writer](https://github.com/smichea/osage/blob/main/worlds/ville-paris/cook-pages.py).
[^audit]: [Ordered binary geometry and package audit](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-collision-codec.py).
[^tests]: [Edge cases and invalid packets](https://github.com/smichea/osage/blob/main/worlds/ville-paris/test_paging_collision_codec.py).
