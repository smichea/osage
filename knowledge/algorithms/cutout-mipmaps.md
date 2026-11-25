---
type: Algorithm
title: Mipmaps for masked surfaces
description: Estimates the opaque area of a sampled material as its texture becomes smaller.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/texture.rs
licence: MIT OR Apache-2.0
status: draft
tags: [rendering, textures, lod]
sources:
  - id: alpha-mips
    title: Computing Alpha Mipmaps
    resource: https://www.ludicon.com/castano/blog/articles/computing-alpha-mipmaps/
  - id: gltf
    title: glTF 2.0 Specification
    resource: https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html
generated: { by: "OpenAI/Codex", at: "2026-11-23T15:05:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

An image can describe a leaf by giving some texels an alpha value above a threshold. When a leaf is distant, a smaller mip level supplies its samples. Combining opaque and transparent neighbours can push most samples below the threshold and thin the leaf. Castaño describes measuring and correcting this loss per mip.[^alpha-mips]

# Idea

Estimate how much of the full image passes the alpha test under its declared sample filter. Use that fraction as the target for each smaller image. The reference threshold includes the placement material's base alpha. Each material can reuse the same decoded image with a different sampler or alpha threshold.

# Method

Decode PNG or JPEG to RGBA8. Keep the original top level. Build the smaller levels with the colour channels in linear light and alpha in linear units. For a masked material, find the first alpha byte whose normalized value times the base alpha meets the glTF cutoff. Measure the passing fraction at level zero.[^gltf]

For nearest sampling, collect one alpha byte per texel. For bilinear minification, take four samples per texel, offset by a quarter texel toward each corner. The four contributing texels have weights 9/16, 3/16, 3/16 and 1/16. Round the result to the nearest byte and respect both address modes at the edges. Use the same grid at every level. Build a 256-bin histogram of its samples. A descending scan finds the byte threshold whose passing count is closest to the original fraction. Integer cross multiplication compares these fractions without division. Ties favour the threshold nearest the material's original threshold. A monotone table maps values on either side of the selected boundary to their corresponding side of the material cutoff. Zero and 255 keep their values. With bilinear minification a correction can increase opacity. If the grid already supplies at least the target fraction and the best adjustment would decrease opacity, the original bytes survive. This protects thin parts of an object when its projected UV samples differ from the uniform grid. The GPU then uses ordinary texture sampling and the material's alpha test.

# Invariants

All RGB bytes and all top-level bytes stay identical to their values before the correction. Opaque and blended materials use the regular mip chain. A sampler with no mip selection uploads only the top level. The image key includes the encoded content digest, sampler and effective alpha threshold. Mesh placements sharing all three can share a GPU image.

# Cost

Building the mip chain and histogram takes time proportional to the total texel count. The remapping table has 256 entries. For a square power-of-two RGBA8 texture the complete chain occupies less than four thirds of the top level plus one texel. GPU work per visible fragment follows the ordinary material pipeline.

The PNG/JPEG fallback decodes sRGB8 into linear u16 values before averaging. For an even image whose halved coordinates are exactly representable in f32, the footprint contains four texels. The reader averages those four with the existing integer rounding and produces the first reduced linear level directly from the RGBA8 source. For a 1,024-square image, that first temporary pixel array occupies 2 MiB. Expanding the full source to linear u16 would require 8 MiB. Odd dimensions, narrow strips and very large coordinate ranges retain the image library’s footprint calculation. A 64 KiB table maps each linear u16 colour to the byte produced by Bevy’s inverse sRGB curve. Alpha still uses its linear conversion. The output allocation includes the exact size of the pyramid. [Fallback implementation](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/texture/mipmap.rs)

The native CPU oracle compares all 65,536 input colour codes and every output RGBA byte for 788 image sizes, including 1 × 513 and 1,025 × 1. On an i5-1335U, the 487,667-byte 1,024-square PNG took 105.4 ms of decoding-thread CPU time in the original implementation and 27.5 ms with the new one on the first call in a fresh process. Repeated calls took 89.0 ms and 19.5 ms respectively. For a 32-square 126-byte PNG the same medians were 366 and 266 µs on the first call, then 112 and 32 µs. A 1,004-byte 32-square JPEG took 513 and 406 µs, then 184 and 101 µs. Each method ran in 15 fresh Linux processes pinned to the same CPU. Both implementations received the encoded image in memory, decoded it afresh on every call, and returned identical byte arrays and metadata. The timer excludes input-file access, rendering and upload. Run `python worlds/ville-paris/check-mip-prepare.py --input LABEL IMAGE` to measure another file. [Reference algorithm and parity tests](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/texture/verification.rs) · [Paired timing](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-mip-prepare.py)

# Failure modes

Tied alpha values and few texels limit the fractions a level can represent. The four-point grid estimates bilinear occupancy. Interpolation of remapped bytes, samples at other offsets, and blending between mip levels can yield a different passing fraction. Variable vertex alpha, UV distribution, silhouette geometry and camera pose also affect visible area. The inspector reports texel-centre counts and passing counts on the sample grid, before and after correction. A view-dependent claim requires a render of the surface.

# Alternatives

Ordinary mipmaps preserve unadjusted alpha values. A stochastic alpha test can represent partial opacity over many pixels or frames and needs a separate temporal stability check.

# Oracle

Run `cargo test -p osage-ville-paris texture::` for byte parity with the scalar mip generator, all linear colour codes, and the alpha-coverage checks for thresholds, wrap, clamp and edge opacity. Run `ville-paris --scene object.glb --inspect-textures` for a report linking each textured primitive to its model nodes. The report includes the dimensions, digest of the uploaded RGBA and alpha bytes, and the passing counts before and after correction at each uploaded level.

# Reuse

Attach base colour and alpha to each visual primitive. Let physical and acoustic surfaces keep their independent material and owner bindings.

[^alpha-mips]: [Computing Alpha Mipmaps](https://www.ludicon.com/castano/blog/articles/computing-alpha-mipmaps/).
[^gltf]: [glTF 2.0 Specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html).
