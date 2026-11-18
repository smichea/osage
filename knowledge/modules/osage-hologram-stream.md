---
type: Module
title: osage-hologram-stream
description: Holograms sent by a server as a viewer needs them — the grid of views cut into blocks, each a short AV1 video along the viewing angle; a packer, a decoder in Rust, and a small HTTP server and client.
resource: https://github.com/smichea/osage/tree/main/crates/osage-hologram-stream
licence: MIT OR Apache-2.0
status: draft
tags: [streaming, holograms, compression, av1]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-18T03:40:00Z" }
---

# Purpose

Send a hologram's views from the server that keeps them to the client that draws them, only those the viewer needs, compressed as a video along the viewing angle (D-134, research [09](https://github.com/smichea/osage/blob/main/research/09-holographic-textures.md)).

# Concepts

- **Series:** the pictures of a hologram — colour as seen (`view`), colour without light (`albedo`) and normals for a hologram lit where drawn, depth, and the outline (`mask`).
- **Block:** a few rows and columns of the grid of views (3 × 4 unless told), one short AV1 video per series that starts with a whole picture and decodes alone; its views go back and forth along the rows, each a step of angle from the last.
- **Need:** the blocks of the views a viewer can see a hologram from before long, answered by the server from where the viewer is, in the hologram's frame, and how fast it goes.

# Architecture

`hologram-pack HOLOGRAM OUT` reads a folder of `hologram-bake` and writes one IVF file per block and series, and `stream.json` (`Packed`: framing, grid, tiling, series, each file's size). Colours go through SVT-AV1 (10-bit 4:2:0, BT.709), their ground first filled with the object's own colours spread outwards (push-pull), so that halving the chroma bleeds nothing foreign into the edges; normals through libaom (10-bit 4:4:4 RGB); depth as 12-bit grey, lossy, its ground filled the same way; the outline apart, 8-bit grey at libaom's finest quantizer, so that no texel changes side. The packer decodes every block back and reports sizes, PSNR on the object, depth error and decoding time. `decode` turns a block back into 8-bit RGBA pictures of the kind `hologram-bake` writes, with rav1d (dav1d's Rust port, its assembly when built with nasm), a row at a time; `decode::depths` joins a depth block to its outline. `http::serve` sends `/h/PATH` files and answers `/need?QUERY` as the world says; `http::Client` asks over one kept-alive connection. `hologram-serve ROOT` serves packed holograms and answers a viewer's need with the blocks of every direction from the photographed sphere towards a place it can reach within the horizon (2 s).

# Interfaces

`Series`, `Block`, `Tiling` (`views`, `blocks`, `block_of`, `blocks_of`), `Grid` (`from_steps`, `direction`), `views_within`, `Packed`, `depth_to_grey`, `grey_to_depth`; `ivf::{frames, write}`; `decode::{block, depths, Picture}`; `encode::{block, pad, Quality}` (needs ffmpeg with libsvtav1 and libaom); `http::{serve, Client, Need}`; the programs `hologram-pack` and `hologram-serve`.

# Invariants

- A block's views go back and forth, each a step of angle from the last; the blocks cover the grid once. Tested.
- Depth survives its grey; nothing stays nothing. Tested.
- Smooth colours and normals come back within 38 dB; the decoder agrees with ffmpeg's (libdav1d) within 38 dB on the same file. Tested when ffmpeg is there.
- Files and needs come over one connection; a path out of the folder is refused. Tested.

# Knowledge

- Measured (research 09, tenth experiment): a forest plot's 102 MB of PNG become 20.8 MB of blocks; turned round, the camera was sent 51 of 105 views, 7.8 MB.
- Draws streamed holograms: [osage-hologram](/modules/osage-hologram.md)'s `stream`; lists what a viewer sees: [osage-cubes](/modules/osage-cubes.md).

# Reuse

Any program that sends many pictures of one thing seen from neighbouring directions. Permissive licence; rav1d is BSD-2-Clause; ffmpeg is needed by the packer only.

# Limits

Blocks are fixed tiles of the grid; a whole picture starts each, so small blocks compress less. Normals are costly (×2.7) and serve only holograms lit where drawn. Depth is lossy: its error is largest on the edges inside the object (between a needle and what is behind it). Decoding runs on the CPU; the views go to the GPU only once stacked.
