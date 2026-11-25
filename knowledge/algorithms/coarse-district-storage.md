---
type: Algorithm
title: Lossless storage of the first district scene
description: Transfers the coarse parent as exact glTF text columns and vertex streams, then restores its original GLB before the first frame.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/paging_coarse.py
licence: MIT OR Apache-2.0
status: draft
tags: [paris, rendering, streaming, gltf]
sources:
  - id: columns
    title: Lossless number and JSON columns
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/paging_coarse_columns.py
  - id: native
    title: Bounded native coarse decoder
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/coarse/mod.rs
  - id: audit
    title: Parent package and native CPU comparison
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-coarse-storage.py
  - id: frames
    title: Native first frames of all three district routes
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-bootstrap-frames.py
  - id: cases
    title: Attribute, material, image, fallback and integrity controls
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/test_paging_coarse.py
generated: { by: "OpenAI/Codex", at: "2026-11-23T21:39:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

Before detail arrives, a district displays a single coarse scene. The Bir-Hakeim parent holds 2,170 meshes and 3,711 placements. Repeated normals, triangle order and glTF metadata account for much of its 338,375-byte transfer.

# Idea

Factor the repeated order of local corners, binary buffer views and JSON records. Keep the exact encoded values of every vertex component, transformation, source-node reference and material. The decoder can pass the reconstructed GLB to the existing scene reader.

# Method

The four-byte `OVC2` signature precedes three little-endian unsigned 32-bit sizes for the intermediate representation, the unpadded JSON and the binary chunk. A zlib stream follows. It contains length-prefixed metadata and binary sections. Each can carry original bytes directly. The cooker compares the final OVC2 with the historical level-6 zlib GLB and writes the shorter resource under its SHA-256.[^native]

In metadata arrays, group rows by the exact text surrounding their numbers. Keep the order of the templates for each row. Each numeric column can hold a constant, a signed arithmetic series, deltas, byte planes or variable-width integers. Fractional numbers keep their exact decimal spelling, directly or by reference to a dictionary. Unknown material and extension fields can remain literal. A sequence of all root nodes in increasing order needs only its count.[^columns]

For binary data, partition the original byte stream into ordered spans, including all padding. A span can refer to a stored buffer view that occurs several times. For binary32 positions, number distinct points within each view by first appearance. Store this local index pattern once across the scene. Encode the distinct point sequence as three axis dictionaries of original four-byte words with index streams. The decoder restores every local repetition. Other attributes, including colours and UV coordinates, and embedded images can use direct bytes or a view dictionary. Overlaps between buffer views take the direct path.[^native]

# Invariants

The complete output retains both GLB chunks, their lengths and padding. The first scene uses the existing owner indices, placement order, winding, hierarchy and material inputs. The system verifies the digest of the encoded parent and all declared sizes. A JSON-only GLB keeps its original envelope. OVC1 remains readable, and an OVC1-only consumer can receive assets cooked with `--coarse-storage ovc1`.

# Cost

On the authored 128 m Bir-Hakeim district, OVC1 uses 338,375 bytes. Compressing its plain GLB at zlib level 9 uses 309,739 bytes including the historical header. Compressing the existing OVC1 file again with zlib level 9 uses 273,169 bytes before outer framing. OVC2 takes **40,432 bytes**, saving 297,943 bytes (88.05%) against OVC1. Both produce the same 3,761,460-byte GLB, including 1,381,304 bytes of padded JSON and 2,380,128 bytes of binary. The original carries 84,984 positions with 14,263 distinct triplets. Its 2,170 normal views form 37 distinct byte strings, and its index views form eleven. Measured on an Intel Core i5-1335U with the file size and geometry oracle on 23 November 2026.[^audit]

The release binary for the workspace tests decoded the same resident input bytes on CPU 0. Across 50 warm pairs in alternating order, thread CPU time for OVC1 and OVC2 was 12.066 and 9.620 ms at the median, and 14.389 and 12.881 ms at the 95th percentile. Across eleven fresh processes per decoder, median thread CPU time was 15.597 and 14.551 ms. Median process peak RSS was 40,244 and 41,448 KiB. Each run hashed the GLB outside the timed interval. These durations cover decompression and reconstruction before scene parsing or graphics upload.[^audit]

At tick zero on the Intel Iris Xe, a 1280 × 720 capture of the road and roof routes matched at every pixel. The aerial rail capture differed at 183 pixels across formats. Two captures with the same format on the same route differed at 110 pixels. All three routes received identical geometry and draw-owner counts.[^frames]

# Failure modes

The native decoder refuses a bad zlib trailer, surplus input, inconsistent lengths, arithmetic overflow, unknown modes and out-of-range dictionary references. The intermediate representation and reconstructed GLB are limited to 256 MiB each. The compact format limits an integer column to 8,388,608 elements. The writer preserves the historical representation when a compact column would exceed the limit, the file layout cannot be represented exactly, or it would take more bytes. Full scene decoding, mesh allocation and GPU upload still follow the transfer.[^native]

# Alternatives

Higher zlib levels can be deployed with the existing reader. Separating binary attributes alone cuts repeated normal and index data, but still transports the repetitive glTF text. Transferring a geometry approximation would require a separate error and first-frame contract.

# Oracle

Cook the same source into two directories using `--coarse-storage ovc1` and the default `auto`. Run `python worlds/ville-paris/check-coarse-storage.py --ovc1 OLD --ovc2 NEW --out receipt.json`. The tool checks that the directories differ only in the parent, verifies file lengths and digests, and compares the complete decoded GLBs. Its optional `--test-bin` path measures the native functions in a pinned Linux process. Unit cases exercise a mesh with two embedded images, two colour sets, a source RGBA channel, UV coordinates, an affine shear and an unknown application attribute.[^audit][^cases]

# Reuse

The hierarchy continues to own the objects. The compact parent is a lossless transfer representation. An ordinary GLB reader can open the reconstructed parent. [District paging](/algorithms/district-paging.md)

[^columns]: [Lossless number and JSON columns](https://github.com/smichea/osage/blob/main/worlds/ville-paris/paging_coarse_columns.py).
[^native]: [Bounded native coarse decoder](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/coarse/mod.rs).
[^audit]: [Parent package and native CPU comparison](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-coarse-storage.py).
[^frames]: [Native first frames of all three district routes](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-bootstrap-frames.py).
[^cases]: [Attribute, material, image, fallback and integrity controls](https://github.com/smichea/osage/blob/main/worlds/ville-paris/test_paging_coarse.py).
