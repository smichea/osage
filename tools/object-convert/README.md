<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Static object converter

The converter accepts a static YDR drawable, its embedded or external YBN collision bounds, and optional acoustic geometry. It writes one glTF 2.0 binary object that the city experiment can load. The default glTF scene draws the high-detail visual mesh. Other detail levels, collision faces and acoustic surfaces have their own mesh references. [Drawable and collision interchange](https://docs.sollumz.org/features) · [Bound authoring guide](https://docs.fivem.net/docs/assets-manual/beginner-series/part-5/) · [glTF 2.0](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html)

Run from the repository root with Python 3.11 or later.

```sh
python3 tools/object-convert/convert.py --id slab \
  --drawable tools/object-convert/fixtures/slab.ydr.xml \
  --bounds tools/object-convert/fixtures/slab.ybn.xml \
  --acoustic-bounds tools/object-convert/fixtures/sound.ybn.xml \
  --materials tools/object-convert/fixtures/surfaces.toml \
  --report -o /tmp/slab.glb
```

On the original test slab, the visible surface is two metres square at a height of 1.25 m. The collision surface of the same width sits at 1 m. The sound surface is 1.5 m square at 0.75 m. Source coordinates in metres map to the Osage frame by `(x,y,z) → (x,z,−y)`. The latter has Y up. Source winding is preserved through that proper rotation. A reflected composite transform requires an additional winding reversal.

## Inputs

The XML reader accepts the CodeWalker interchange for drawables, standalone bounds and embedded bounds. A caller supplying `--bounds` selects those bounds in place of any collision embedded in the drawable. A drawable alone can describe an object purely for sight. A standalone bounds file can describe a physical object with no drawing.

For loose RSC7 binary inputs, pass the same switches with the `.ydr` and `.ybn` files and add `--resource-tool /path/to/executable`. The external program must support `resource dump --no-update-check --no-dds INPUT -o output.xml`. Conversion of the resulting XML uses the same code path. The Linux x86-64 bridge qualified on these fixtures is release 0.29.0, with SHA-256 `36a3d8efdb310d9dc80703f86db681d8fb38dcb9f00fb2a9fd49b0c7c8a73e28`. Its release record includes the available binaries and their published digests. [Upstream release](https://api.github.com/repositories/1007841939/releases/tags/v0.29.0)

## Geometry and materials

Drawables keep indexed triangles, named shader slots, normals, tangents, texture coordinates, byte vertex colours and the available high, medium, low and very low geometry. Missing normals are computed from the triangles. The import record keeps the source LOD distances. The generic PBR material is white, nonmetallic and fully rough. A portable appearance manifest can bind authored base colour images to these slots. Skinning and bone-attached models produce an error.

### Portable base colour images

`--appearance` reads a version 1 JSON manifest. Its `textures` object maps the name inside a texture-valued `DiffuseSampler` to an independently supplied PNG or JPEG. Paths resolve relative to the manifest. Names match without regard to case. The converter embeds each distinct encoded image once in the GLB’s binary chunk, even when multiple slots, names or files resolve to the same bytes. Each source geometry selects its own material slot at every detail level. Every primitive using a bound material must supply `TEXCOORD_0`. The default import preserves vertex colours, geometry and stored UVs. [Sollumz diffuse maps](https://docs.sollumz.org/2.6/tutorials/creating-props/setting-up-sollumz-shaders) · [glTF materials](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#materials) · [glTF images](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#images).

```json
{
  "version": 1,
  "textures": {"panel_stripes": "stripes.png"},
  "materials": {
    "0": {"shader": "panel_near", "alphaMode": "MASK", "alphaCutoff": 0.4, "doubleSided": true},
    "1": {"shader": "panel_far", "alphaMode": "MASK", "alphaCutoff": 0.6, "doubleSided": false}
  }
}
```

The optional `materials` object uses zero-based shader slots. Its optional `shader` field checks the source name or its stored 32-bit hash. Valid alpha modes are `OPAQUE`, `MASK` and `BLEND`. The defaults are opaque and single-sided. A mask defaults to a 0.5 cutoff. These portable settings describe the author’s choice. The import report records which properties were explicitly supplied, the resolved diffuse name and image digest, and the remaining shader parameters. A missing manifest entry leaves the slot without a base colour image. An ambiguous mapping, missing declared file or missing UV set stops conversion with the affected slot. The import report lists remaining program parameters, including additional samplers. Independently prepared mip levels can be supplied with the mapping below.

By default the glTF material multiplies the image by all four components of `COLOR_0`. A slot whose authored alpha has another meaning can request `"vertexColorAlpha": "one"`. The rendered `COLOR_0` then contains the original RGB with full alpha. The normalized four-byte attribute `_OSAGE_SOURCE_COLOR_0` preserves the original RGBA for every vertex. An already opaque layer shares its original accessor, and `COLOR_1` retains its own data. For an input without vertex colours, glTF supplies its ordinary white multiplier. The option also applies to untextured materials. [glTF vertex attributes](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#meshes-overview) · [glTF base colour](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#metallic-roughness-material)

Generate the independently authored two-level panel and read its materials with the city’s native inspector.

```sh
python tools/object-convert/fixtures/make_appearance.py /tmp/osage-panel
python tools/object-convert/convert.py --id panel \
  --drawable tools/object-convert/fixtures/textured.ydr.xml \
  --appearance /tmp/osage-panel/appearance.json --report -o /tmp/osage-panel/panel.glb
target/release/ville-paris --scene /tmp/osage-panel/panel.glb --inspect-textures
```

Add `--unit-vertex-alpha` to the fixture generator to exercise the optional mapping on both levels. The high-detail panel’s source alpha ranges from 0 to 96 on the byte scale. Its 0.4 mask discards the entire panel with source alpha and reveals the texture pattern with unit vertex alpha. Every medium-detail source alpha already equals 255.

### Optional GPU compression

The optional `basisuMips` object maps the same diffuse names to ordered arrays of lower-level PNGs. The ordinary base image from `textures` supplies level 0. Supply the complete chain down to 1 × 1 in top-left image order, with sRGB colour and straight, linear alpha. Each dimension halves with integer division and stops at 1. The base dimensions must be multiples of four for the Khronos extension. The converter checks every supplied size before encoding. This allows the author to choose how the mask coverage evolves at distance. [KHR_texture_basisu](https://github.com/KhronosGroup/glTF/tree/main/extensions/2.0/Khronos/KHR_texture_basisu)

```json
{
  "version": 1,
  "textures": {"panel_stripes": "stripes.png"},
  "basisuMips": {"panel_stripes": ["stripes-2.png", "stripes-1.png"]}
}
```

The GLB embeds the original fallback in `texture.source` and a KTX2 image in the optional `KHR_texture_basisu` texture extension. Materials with identical fallback bytes and identical supplied levels share the resulting texture. Different mip chains can share the same fallback image. A reader that supports the extension can transcode the UASTC blocks to a format supported on its adapter, including BC7, BC3, ASTC 4 × 4 or ETC2 RGBA. When loading a GLB, Ville-Paris transcodes 2D sRGB UASTC to BC7 on a Bevy worker if the active device supports filtered BC7 sampling and upload. Other devices read the PNG or JPEG on a worker. Demand-paged districts store the KTX2 and its core image as separate, content-addressed files. The district requests the supported variant once for all resident placements and levels that use it. A bad or unavailable KTX2 falls back to the verified PNG or JPEG. [KTX transcode targets](https://github.khronos.org/KTX-Software/ktxtools/ktx_transcode.html)

The optional external Khronos `ktx` tool encodes UASTC at quality 2 with Zstandard level 10 and one worker, then validates the KTX2 against the glTF extension. The import report records the executable version and SHA-256, the encoded size and digest, and the supplied level digests. The qualified x86-64 Linux package is KTX-Software 4.4.2. Khronos’ own code and the Basis Universal encoder use Apache-2.0. The package includes third-party components with their own notices, including Zstandard under BSD-3-Clause. [Upstream release and packages](https://github.com/KhronosGroup/KTX-Software/releases/tag/v4.4.2) · [KTX licensing](https://github.com/KhronosGroup/KTX-Software/blob/v4.4.2/LICENSE.md) · [Basis Universal license](https://github.com/BinomialLLC/basis_universal/blob/master/LICENSE) · [Zstandard license](https://github.com/facebook/zstd/blob/dev/LICENSE)

```sh
python tools/object-convert/fixtures/make_appearance.py /tmp/osage-panel --basisu --unit-vertex-alpha
python tools/object-convert/convert.py --id panel \
  --drawable tools/object-convert/fixtures/textured.ydr.xml \
  --appearance /tmp/osage-panel/appearance.json --ktx-tool /path/to/ktx --lod-error \
  --report -o /tmp/osage-panel/panel.glb
```

Reproduce the HTTP and GPU comparison with the generated panel on a BC7-capable adapter.

```sh
python worlds/ville-paris/check-paged-basisu.py --bin target/release/ville-paris \
  --source /tmp/osage-panel/panel.glb --out bench/out/ville-paris/paged-basisu-panel
```

The original 4 × 4 panel carries three supplied levels. A BC7 reader retrieves 16 bytes for each of its 4 × 4, 2 × 2 and 1 × 1 levels. The regression also transcodes to RGBA and verifies the independently authored colours of the two smaller images. Set `OSAGE_KTX_TOOL` to the executable when running the converter tests. Providing `OSAGE_CITY_BIN` as well enables the paired native GPU captures of this panel. For a native GLB run, `OSAGE_TEXTURE_TRACE=1` records the worker, selected bytes and GPU format. `OSAGE_DISABLE_BC=1` requests a device without BC formats to reproduce the same core-image path on a desktop GPU. The native runtime links the Basis Universal decoder in its executable. [Rust bindings and upstream licences](https://github.com/aclysma/basis-universal-rs#license)


### Collision and acoustic surfaces

For a geometry bound, the reader adds `GeometryCenter` to each source vertex and then applies each enclosing `CompositeTransform` from child to parent. Triangle and box polygons are grouped by source material slot. Axis-aligned box bounds and four-corner box polygons produce twelve triangles each, preserving their transformed flat faces. Curved analytic shapes produce a diagnostic. Source margins, filters, edge flags, shrunk-vertex variants and acceleration trees have different semantics from the exported faces. The destination physics engine rebuilds its own triangle acceleration data.

Each collision surface has a stable ID derived from the object ID, the child indices and the local material slot. Material type 7 uses mapping key `surface-7`, independent of its slot within a particular bound. The supplied material map sets friction and restitution. Missing values use the stated Osage assumptions of friction 0.5 and restitution 0. The import record identifies which values came from the map.

An author can select an independent mesh with `--acoustic-bounds` or share the physical faces with `--acoustic-proxy`. Omitting both leaves the acoustic surface list empty. Each known acoustic material has energy absorption and transmission fractions centred at 250, 1000 and 4000 Hz plus a single scattering fraction. Absorption and transmission together cannot exceed one per band. An empty material records unknown coefficients. The sample TOML contains synthetic values authored for the test.

The conversion report is in `asset.extras.osageImport`. It records input digests, the binary reader's version and digest when used, shader slots, geometry limits and the material mapping. The `--report` flag also prints it. Publication atomically replaces the output after conversion succeeds.

## Visual LOD deviation

Pass `--lod-error [TOLERANCE_M]` to bound the surface distance from every alternative visual mesh to the node's primary mesh. The optional tolerance defaults to 0.01 m. The converter reads the emitted float32 positions and triangle indices across every primitive. Each `visualLods` entry gains a `maxDeviationM` in the node's local metres. Omitting the flag preserves the original GLB. Positions alone determine this measure. Opacity, normals, textures and the projected silhouette can change independently.

The bound covers both directions. For a source triangle and one convex target face, the largest of three corner-to-face distances bounds the distance of every interior point to that face. A bounding-volume tree finds nearby faces. The source triangle's vertices and centre supply lower-bound samples. The converter bisects the longest edge of the source triangle with the greatest upper bound until the global uncertainty meets the tolerance. It allows for floating-point error and rounds the reported upper bound toward positive infinity in float32. The report records `lowerM`, `upperM`, triangle counts, subdivisions and `gapReached`. If 16,384 subdivisions or 48 levels are exhausted, it retains the conservative bound and records the limit in `stoppedAt`.

```sh
python3 tools/object-convert/convert.py --id slab \
  --drawable tools/object-convert/fixtures/slab.ydr.xml --lod-error 0.001 --report -o /tmp/slab.glb
```

For the authored slab, simplifying a 2 m square to the inscribed triangle moves the two rear corners by `2 / sqrt(5)` metres (about 0.894427 m). Tests also cover a tetrahedron whose two meshes share every vertex but differ by half an altitude in their interiors.

## Verification

```sh
python3 -m unittest discover -s tools/object-convert -v
OSAGE_RESOURCE_TOOL=/path/to/executable python3 -m unittest discover -s tools/object-convert -v
```

The second command also compiles the original XML fixtures into native resources, reads them back and checks the resulting positions, indices, roles and materials. A separate four-triangle bound has a vertex away from the precision grid. With the qualified binary reader and writer its largest point displacement on the five tested vertices was 0.0301 mm. The test accepts less than 0.1 mm on this fixture.

## Articulated fragments

The fragment reader accepts a pristine articulated YFT with rigid bone-bound visuals, one centred sphere or Y-axis capsule per physical link and one passive hinge or three-axis joint per non-root link. The original example represents a 75 kg pedestrian with a head, two arms, hands, legs and feet. Its 15 named bones carry 15 visual meshes and 15 physical bodies. Four hinges and ten three-axis joints connect the physical bodies. Limb names, the mass budget and selected landmarks follow the existing city mannequin to make traces comparable. The generator authors its rigid visual topology, analytic bounds, inertia tensors and two-sided joint frames. The source XML and its independent generator are in `fixtures/pedestrian.yft.xml` and `fixtures/make_pedestrian.py`. [Fragment authoring](https://docs.sollumz.org/2.6/documentation/fragments.yft) · [Interchange types](https://github.com/Sollumz/szio) · [Primitive bound exporter](https://github.com/Sollumz/Sollumz/blob/main/ybn/ybnexport.py)

```sh
python3 tools/object-convert/convert.py --id pedestrian \
  --fragment tools/object-convert/fixtures/pedestrian.yft.xml -o /tmp/pedestrian.glb
target/release/ville-paris --actor-rig /tmp/pedestrian.glb --physics rapier \
  --replay worlds/ville-paris/fixtures/bonnet.json --bench /tmp/bonnet-rapier
target/release/ville-paris --actor-rig /tmp/pedestrian.glb --physics jolt \
  --replay worlds/ville-paris/fixtures/bonnet.json --bench /tmp/bonnet-jolt
```

The default glTF scene carries the skeleton as ordinary nodes with local translation, rotation and scale. Each rigid drawable model attaches to its source bone. The city viewer transforms the authored vertices into each body's local frame and follows the simulated pose. The native bench writes the actor GLB's SHA-256, per-body parameters, transforms, velocities, surface contacts and joint-anchor gaps. The same CLI supports `--verify` for a deterministic replay and `--shot FILE.png --tick N` for a rendered frame.

`asset.extras.osageObject.articulation.bodies` lists parents before children. Each body refers to a stable bone ID and one collision material. By default, the converter emits `articulation.space = "bone"`. A body's `centerM` and `rotationXyzw` give the mass frame relative to that glTF bone. Its joint's `parentFrame` and `childFrame` each give an anchor in metres and an orientation relative to the corresponding body's mass frame. A translation or rotation of the assembly root therefore carries its visuals, collision shapes and joints together. The reader requires rigid right-handed transforms along the physical binding and coincident joint endpoints at rest. Run `ville-paris --scene FILE.glb --inspect-object` to see the assembled body and joint poses under `resolvedRigBodies`.

For the earlier representation, pass `--rig-space actor`. The converter then expresses body centres, orientations and one common joint frame in actor coordinates. Existing files that omit `articulation.space` retain this interpretation.

Mass is in kg and the three principal moments in kg·m² are expressed in the body's local axes. The source capsule's `Margin` is its physical radius. Subtracting it from `SphereRadius` gives the half-length of the straight segment. The reader checks those values against the axis-aligned bound. The spine capsule has a radius of 0.145 m and a straight half-segment of approximately 0.07551 m.

For articulated spheres the default reads `SphereRadius`. When a source uses `Margin` for the contact radius, pass `--sphere-radius margin` on either XML or binary input. An Osage contact sphere requires a positive radius. A missing or zero margin produces an error in this mode. An explicit `--sphere-radius bound` uses the declared extent. Both explicit choices record the bone, both input values and the chosen physical radius in the import report. Render geometry and capsule dimensions are independent.

The adapter reconstructs the parent and child joint frames independently using each body's attachment matrix and checks their rest anchor and orientation. The import report preserves both source frames and the four source limit parameters. The portable joint records three pairs of limits in radians. A hinge rotates around frame X. A three-axis joint assigns source twist to X and the two lean half-ranges to Y and Z. Rapier constrains those axes with a generic joint. Jolt uses native hinges and a six-degree-of-freedom joint with pyramid swing limits. The conversion record identifies unsupported soft control, torque, breakage, damping and collision-filter behaviour.

The default joint convention is `--joint-frames joint-to-body-x`. For a fragment storing body-to-joint rotation matrices with a Z hinge or twist axis, pass `--joint-frames body-to-joint-z`. This applies to both XML and packed YFT. The converter reads the columns of each stored matrix, expresses Z, X and Y as portable X, Y and Z, and preserves the body-local anchor. For a three-axis joint with source lean extents 5° and 20° and twist 45°, the portable principal rotation amplitudes become 45°, 20° and 5°. A signed hinge interval keeps both endpoints. The optional selection appears in the import report along with the original endpoint matrices. Matching these axial limits alone does not reproduce coupled diagonal swing. [Coordinate conventions](../../knowledge/algorithms/articulated-joint-frames.md)

Use `--swing-envelope angular-ellipse` to couple the positive symmetric transverse rotation ranges on eligible three-axis joints. Hinges and joints with a locked transverse axis keep their existing limits. [Swing envelope](../../knowledge/algorithms/articulated-swing-envelope.md)

The reader checks the 22 parent-index and joint-kind lookup entries and the inertia/mass vector for every link. It rejects conflicting redundant values, inconsistent anchors, offset shapes, weighted skinning and child-local drawables. The root drawable contains the authored rigid visual meshes in this profile. Collision materials accept the same explicit map as static inputs, with Osage defaults of 0.5 friction and zero restitution. The fixture has no authored acoustic surfaces.

### Loose binary round trip

The optional bridge uses the independently distributed CodeWalker.Core 1.0.3 assembly. The qualified Linux environment used Python.NET 3.2.1, .NET 10 CoreCLR and the SharpDX 4.2.0 and SharpDX.Mathematics 4.2.0 dependencies next to the assembly. Python.NET is optional when reading XML. Supply the dependencies locally and run these commands with the Python interpreter that has Python.NET installed. [CodeWalker.Core release](https://www.nuget.org/packages/CodeWalker.Core/1.0.3) · [Python.NET](https://pythonnet.github.io/pythonnet/python.html)

```sh
python tools/object-convert/fragment_bridge.py --assembly /path/to/CodeWalker.Core.dll \
  encode tools/object-convert/fixtures/pedestrian.yft.xml /tmp/pedestrian.yft
python tools/object-convert/convert.py --id pedestrian --fragment /tmp/pedestrian.yft \
  --fragment-core /path/to/CodeWalker.Core.dll --report -o /tmp/pedestrian.glb
OSAGE_FRAGMENT_CORE=/path/to/CodeWalker.Core.dll python -m unittest discover -s tools/object-convert -v
```

Set `OSAGE_CITY_BIN` to the native city executable to exercise both solvers and verify their two 240-tick traces during the same test suite.

The binary test encodes the original XML into a loose RSC7 version 162 binary, decodes it via the independent assembly, and compares every body, joint, visual index and vertex and the link lookup tables with the XML conversion. The authored XML SHA-256 is `d78aff68963594763145bce0170c1ed3e078057e489b44613c4cd02c61683a63`. The 5,113-byte binary in the qualification environment has SHA-256 `7ca6b17ca24283828377ddd17170277a4b2e8118eaadd4c3fc9827c53c124713`. The GLB records the actual input and decoder digests on every conversion.
