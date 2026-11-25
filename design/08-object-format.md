# Object geometry, revision 1

<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

An Osage object is a self-contained glTF 2.0 binary file (GLB). A building can have detailed window frames for the eye, a smooth wall for contact and a third surface for sound passing through it. All three are located by the same object tree. Stable surface identifiers make two physics replays comparable even when a renderer chooses a different mesh. [Original object proposal](06-objects.md) · [glTF 2.0](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html) · [Reader](../crates/osage-objects/src/profile.rs)

## Bytes and coordinates

The first 12 bytes hold the glTF magic, version and total byte length. Each aligned chunk has a little-endian u32 byte length and type. The JSON chunk describes nodes, meshes, materials, accessors and buffer views. A single embedded BIN chunk holds all mesh data. Revision 1 uses indexed triangle lists, local metre coordinates and the standard right-handed glTF frame with Y up. Positions are f32 triples. Indices are unsigned integers addressed by their accessor type. Each dense accessor specifies its count and element format. The byte offset in the buffer is the view offset plus the accessor offset. A view can specify a byte stride for interleaved vertices. The native reader checks the declared bounds of views and accessors. [glTF buffers and accessors](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#buffers-and-buffer-views) · [City reader](../worlds/ville-paris/src/scene.rs)

The default scene selects the roots. Local transforms compose as parent × child on column vectors. Each shape and each visible mesh uses its owner's transform. A shared mesh may appear at multiple placements. An explicit acoustic proxy shares the physical mesh's binary data. A reflected transform reverses triangle winding for the physics surface. The city viewer can bake a reflected or sheared transform into the visible vertices and transform the normals with the inverse transpose. [Object binding](../worlds/ville-paris/src/object.rs)

A collision triangle has an oriented material side. When baking a node's local vertices into world space, use the determinant of the composed linear transform. For a negative determinant, exchange the second and third triangle indices to preserve the side described by the transformed local normal. The material and source node still belong to that placement. The district's support catalogue uses the resulting direction to distinguish a top face from its underside. [Winding and landing oracle](../worlds/ville-paris/check-mirrored-collision.py)

## Fields

| Location | Meaning |
| --- | --- |
| `asset.extras.osageObject` | Required header with `version: 1`, persistent string `id`, `collisionMaterials` and `acousticMaterials` dictionaries. |
| `node.extras.osage.id` | Unique node ID. Every reachable node in the default scene has one. Group nodes may have children without a mesh. The nodes must form a forest. |
| `node.mesh` | Primary visible mesh. Ordinary glTF viewers draw it by following the scene graph. |
| `node.extras.osage.visualLods[]` | Optional entries with `level` and a glTF `mesh` index. Valid alternative levels are `medium`, `low` and `veryLow`, each at most once per node. An absent requested level falls back to the primary mesh. |
| `node.extras.osage.subtreeProxy` | Optional per-channel thresholds under `visual`, `collision` and `acoustic`. At or beyond its threshold a group supplies the geometry for that whole branch. At finer levels its children supply the geometry for that channel. A plain node keeps its own geometry alongside its children. |
| `node.extras.osage.collision[]` | Each entry has an `id`, a glTF `mesh` index and a `material` key in `collisionMaterials`. Each surface can own multiple triangles. IDs are unique within the physical channel. |
| `node.extras.osage.acoustic[]` | Each entry has an `id`, `mesh`, `material` and `source`. An `authored` surface carries its own geometry. A `collision-proxy` explicitly references a physical mesh on that node. IDs are unique within the acoustic channel. |
| `node.extras.osage.acousticRoom` | A convex room defined by `boundsM: [min, max]` in node-local metres. Each bound has three coordinates. |
| `node.extras.osage.acousticPortal` | A shared opening with two distinct room node IDs in `rooms` and four ordered vertices in `cornersM`, in the portal node's local metres. Optional `closureFraction` defaults to zero. |

A collision material carries non-negative friction and restitution in [0,1]. For the two current solvers, contact friction uses the geometric mean and restitution uses the larger value. The capsule in the drop test has zero restitution. Acoustic material coefficients describe energy fractions at 250, 1000 and 4000 Hz in that order. Absorption and transmission are triples in [0,1], with a sum at most one in each band. The remaining energy is reflected. A scalar scattering coefficient in [0,1] represents the diffuse fraction of that reflected energy. An empty material `{}` records an unknown response. A probe can still locate its surface. A clip or impact sound is a distinct asset. [Material validation](../crates/osage-objects/src/profile.rs) · [Physics adapters](../crates/osage-dynamics/src/lib.rs)

The sound scene reader accepts emitters and contact definitions in sibling `osageSound` fields. The object profile uses the surface response above. Integrating the emission fields into that profile is proposed in the sound design. [Sound proposal, §2](09-sound.md#2-what-an-object-carries-for-sound)

## Articulated actors

An object may add `articulation.bodies` to the version 1 header. An actor uses the same metre and axis conventions as a static object. The root comes first. Each later body refers to an earlier parent. Each body has a unique name and a distinct bone node ID. The following fields describe the rest pose and mass distribution. [Schema and validation](../crates/osage-objects/src/articulation.rs)

| Fields | Meaning |
| --- | --- |
| `centerM`, `rotationXyzw` | Centre and unit quaternion of the principal frame relative to the named bone when `space` is `bone`, or in actor coordinates for legacy input. |
| `massKg`, `inertiaKgM2` | Positive mass and the three principal moments about that centre in body coordinates. |
| `shape`, `material` | A sphere with `radiusM`, or a capsule with `radiusM` and `halfSegmentM` along local Y. The material key resolves in the collision dictionary. |
| `joint.anchorM`, `joint.frameXyzw` | Shared rest anchor and constraint orientation in the legacy actor-coordinate encoding. |
| `joint.kind`, `joint.limitsRad` | A hinge or three-axis joint. Each of the three intervals gives lower and upper angular stops in radians. For a hinge only X has a free interval. |

The actor importer takes a fragment with rigidly bound visual parts, sphere or capsule collision shapes and a pristine physics hierarchy. It checks agreement of the parent and child rest frames. For the supported capsule interchange, the physical radius is the margin and the centre segment half-length is the enclosing sphere radius minus that margin. The two values have different meanings even for a vertical capsule with a tight bounding box. Three-axis stops become per-axis angular limits in the native solvers. The import receipt records omitted features such as active joint drives. A free-fall replay uses the complete GLB as its actor input. [Importer](../tools/object-convert/fragment.py) · [Impact oracle](../knowledge/oracles/pedestrian-impact.md)

The default fragment conversion writes `articulation.space = "bone"`. A rigid shape follows its named scene node. A joint has a `parentFrame` relative to the parent rigid body and a `childFrame` relative to the child rigid body. Each has `anchorM` and `rotationXyzw`. Composing both ends into actor coordinates must give the same joint within the reader tolerance. The older actor-coordinate representation omits `space` or uses `actor` and keeps a single joint frame. The converter can reproduce that encoding with `--rig-space actor`.

The converter can express either joint-to-body basis vectors or a body-to-joint matrix with twist about source Z in the portable joint frame. The import record identifies an explicit source convention and preserves its raw endpoint fields. [Joint frames](../knowledge/algorithms/articulated-joint-frames.md)

The optional joint field `swingEnvelope` defaults to `independent`. With `angular-ellipse`, positive symmetric Y and Z limits are the two half-widths in radians. The allowed tilt about unit transverse axis (0, y, z) is 1 / hypot(y/Y, z/Z). X retains its signed twist interval. [Solver interpretation](../knowledge/algorithms/articulated-swing-envelope.md)

## Runtime ownership

The canonical forest uses a parent-first list with root and immediate-child indices. A world transform is the product of local transforms on the path. The city's static viewer creates one Bevy entity per model node and attaches drawable primitives to their owner. Reflections and local shear retain the authored positions, transformed normals and triangle winding through a mesh-space adjustment. A ray through two placements of the same mesh gives a different `nodePath` for each placement. The same path appears on physical contact and acoustic query records. [Nested fixture and assertions](../tools/object-convert/test_hierarchy.py) · [Native binding](../worlds/ville-paris/src/object.rs)

Select a cut at load time with `--visual-lod`, `--collision-lod` and `--acoustic-lod`. Each defaults to `high`. A bridge can draw its deck as one coarse mesh while a footstep still collides with a specific span and an acoustic ray finds the span's own sound surface. When a coarse collider replaces that branch the reported owner is the deck itself. An acoustic proxy for a fine mesh remains queryable while the solver uses a coarser cut. The object test exercises these configurations independently.

## Reading and streaming

The district cooker composes the source forest once in parent-to-child order. The acoustic catalogue can declare `placed_bounds: true` when its boxes enclose the actual single-precision world vertices. A reader can use those bounds on the closed endpoints of a query. An older directory uses wider bounds or tests a placement directly when its transform cannot bound the rounding error. The mesh file remains in local coordinates and keeps its content identity across placements. [Method and boundary cases](../knowledge/algorithms/paged-acoustic-rays.md)

The current city reader opens the complete GLB, validates its metadata and reads its indexed meshes. It selects the requested visual level per node and sends those visual meshes to Bevy. The city preview uses positions, normals and the first colour and UV sets. Each visual primitive may bind a PNG or JPEG base-colour image on UV set zero. Its sampler, alpha mode, threshold and sidedness follow the glTF material. For masked surfaces, the native viewer adjusts lower mip levels to approximate the occupied fraction after sampling. It measures four bilinear samples per texel under linear minification, or one texel value under nearest minification. The object inspector reports these levels and their owners through `--inspect-textures`. [Mip selection and alpha test](../knowledge/algorithms/cutout-mipmaps.md). The converter can also preserve tangents and additional numbered colour and UV sets for other glTF readers. For fixed collision, it resolves each physical binding to world-space triangles and a material in the chosen solver. The acoustic query returns the nearest two-sided intersection per named acoustic surface for a finite segment, along with the material and its origin. Switching visible levels preserves the physical and acoustic bindings.

Changing the selected level in the current city reader uses data resident in memory. Chunk and accessor offsets locate mesh bytes within the single file. The district experiment builds a separate index of independently readable visual and physical tile files and selects them using the actor and camera positions. [District paging](../knowledge/algorithms/district-paging.md) The existing Bistro client streams tiles of lighting with a separate protocol. [City paging design](07-city-scale-quality.md) · [Bistro light stream](../knowledge/modules/osage-hologram-stream.md)

## Converting an object

The offline converter accepts a static drawable, external or embedded bounds, and optional independently authored sound geometry. The import notes inside `asset.extras.osageImport` identify the source hashes, material assumptions and source features omitted from the output. The resource tool can translate supported loose native binaries into its published XML interchange. Only the offline importer needs the tool. The baseline input identifiers in the examples belong to the original slab fixture. [Converter](../tools/object-convert/convert.py) · [Input examples](../tools/object-convert/fixtures) · [Authoring documentation](https://docs.fivem.net/docs/assets-manual/beginner-series/part-5/)

From the repository root, the following commands produce an object with three independently located surfaces and query all three, using a ray directed downwards from Y = 3 m.

```sh
python3 tools/object-convert/convert.py --id slab \
  --drawable tools/object-convert/fixtures/slab.ydr.xml \
  --bounds tools/object-convert/fixtures/slab.ybn.xml \
  --acoustic-bounds tools/object-convert/fixtures/sound.ybn.xml \
  --materials tools/object-convert/fixtures/surfaces.toml \
  -o bench/out/ville-paris/slab.glb
cargo run --release -p osage-ville-paris -- --scene bench/out/ville-paris/slab.glb \
  --inspect-object --ray 0,3,0 0,0,0
```

The expected distances are 1.75 m for the visible slab, 2.00 m for its collision and 2.25 m for the independent sound surface. With `--acoustic-proxy` the latter becomes 2.00 m. The same file can enter the normal city viewer or a headless rigid-body replay with a replay file naming its physical surface. The end-to-end check converts the inputs, queries high and low visual levels and runs both native solvers. Pass `--resource-tool /path/to/executable` to additionally build native input binaries and repeat the full comparison. [End-to-end check](../worlds/ville-paris/check-object.py)

## Choosing a visible mesh for a perspective view

A node can give an alternate visible mesh a finite, nonnegative `maxDeviationM`. It is a conservative bound in local metres on the distance from either triangle surface to the other. A measurement of vertices alone can miss a hole inside a triangle. The visible primary mesh is the reference surface. With a fixed level the existing selection still applies. With automatic detail a node can only switch to an alternate carrying a bound. Collision and acoustic bindings retain their owners and their own selected levels. [Object reader](../crates/osage-objects/src/profile.rs) · [Projection and selection](../crates/osage-objects/src/visual_detail.rs)

The ordinary object viewer uses the physical viewport height, vertical field of view and local-to-world transform to project the error in pixels. It can switch to a cheaper mesh at one pixel of estimated geometric deviation and keeps it until two pixels. These two thresholds are an Osage design choice. A surface bound says nothing about changes to colour, normals, transparency or lighting. All offered meshes are resident in this path. [Worked method and controls](../knowledge/algorithms/projected-mesh-error.md)

### Original triangles across physical page boundaries

The district cooker keeps the indexed topology of a physical surface with at most 64 triangles in one page. Its bounds may extend beyond the cell containing the middle of the object. A larger surface spanning several cells assigns each triangle once, using its centroid. Each group preserves source face order, references its used vertices in source order, and retains the same tree node, material and surface identifier. Candidate reads and certified support use the full geometric bounds. This also applies to the named physical surfaces of the original flat district format. Visual tile clipping is independent. [Cooker](../worlds/ville-paris/paging_channels.py) · [Ordered-face and distant-contact checks](../worlds/ville-paris/check-collision-topology.py)

The optional `corner_meshes` field in the support catalogue shares the vertex and triangle order of repeated closed boxes. Each placement identifies its tile, physical shape slot, pattern and two opposite corners in world space. The three low bits of each vertex code select the endpoint on the X, Y and Z axes respectively. Repeated codes retain separate vertices at the same position. The reader checks the ordered upper triangles and all omitted face bounds against the support catalogue before installing the full collider. When the physical page arrives, exact geometry, owner and material agreement allow the existing native contacts to survive. [Reader](../worlds/ville-paris/src/paging/support/corners.rs) · [Four-second trajectory and sound oracle](../worlds/ville-paris/check-corner-support.py)

A physical contact can report the zero-based `source_triangle` on a named surface in its original tree node. Triangle numbering follows the source mesh's primitives in order. Transforming a placement preserves that number, including when a reflection reverses the winding. In a district declaring `collision_face_order: 1`, a complete shape uses the source order directly. A split shape lists the source ordinal intervals for the triangles in its page. The certified support catalogue first maps its own triangle to the corresponding local face of the full shape. For an old district without an order declaration, the original triangle is unknown. [Cooked source data and runtime checks](../worlds/ville-paris/check-source-faces.py)

### Querying the three surfaces with a finite segment

Repeat `--ray X,Y,Z X,Y,Z` on the object inspector to query a batch in world metres. For a batch, `rays` follows request order and `ray` contains the final result. A single request still returns `ray`. Each hit binding retains its ID, tree path and nearest `distanceM`. Its `intersections` array contains every crossed source triangle sorted by distance then ordinal. Each record gives `source_triangle`, `distance_m` and `point_m`. Both endpoints count. Two faces with identical vertices retain separate ordinals. A coplanar segment has no isolated crossing. A collision-backed acoustic binding shares the geometric test and keeps its own acoustic material. [Resident inspector](../worlds/ville-paris/src/object/rays.rs)

In the district, repeat `--collision-ray X,Y,Z X,Y,Z` to inspect the static physical state after the specified tick. Each record in `collision_probes` gives installed intersections and `coverage`. When `coverage.complete` is false, the missing regions listed in `coverage.gaps` could contain additional intersections. Historical districts can have complete geometric coverage while an individual hit has no known source ordinal. These queries use the installed terrain and leave the existing streaming schedule intact. [District inspector](../worlds/ville-paris/src/paging/solver/rays.rs) · [Coverage catalogue](../worlds/ville-paris/src/paging/support/rays.rs)

### Routing through authored rooms

A room and its openings may live on different branches of the model. The importer composes the complete node transforms and checks that each finite convex quadrilateral lies on a wall of both rooms, with their interiors on opposite sides. A rigid, scaled or sheared placement transports the whole room. A closure fraction in [0,1] is the blocked fraction of the opening's area. Partial closure scales its corners about their mean by the square root of the remaining fraction. Full closure removes the connection. Solid-wall transmission and reflection are separate material properties.

Query the resident model with `--inspect-object --ray FROM TO` or the cooked district with `--paging --acoustic-ray FROM TO`. Positions use comma-separated world metres. When rooms are present, `roomPath` reports the source-to-listener room and portal IDs, world-space crossings including both endpoints, the length of that feasible path, a lower bound and whether the topological search completed. A point on a shared wall can belong to both closed room boxes and yields `ambiguous`. Other outcomes are `connected`, `outside` and `disconnected`. The current impact mixer uses its triangle-based direct and diffracted paths. [Routing algorithm](../knowledge/algorithms/acoustic-room-portals.md)

### Animated ground contacts

A version 1 object can declare an optional sibling of `osageObject` in `asset.extras`. The armature and contacts refer to persistent `node.extras.osage.id` values.

```json
"osageFootRig": {
  "armature": "frame",
  "contacts": ["fore-left/pad", "hind-right/pad", "hind-left/pad", "fore-right/pad"]
}
```

The armature is an object root. For each listed contact the reader walks three parent edges to find the ankle, lower joint and upper joint. These four nodes must be distinct from those of every other limb, and no limb may descend from a driven node of another. Limbs can share ancestors such as a shoulder girdle. The order of contacts follows the declaration, independent of glTF node array order. At runtime the world attaches the armature to the actor, samples its gait and supplies its ground query. The sample glTF and trot replay use this declaration. [Fixture](../worlds/ville-paris/examples/fixtures/four-foot.gltf) · [Replay](../worlds/ville-paris/examples/quadruped_stride.rs)

The optional `solePoints` map inside `osageFootRig` uses the contact IDs as keys. Each value is a nonempty list of finite XYZ positions in the local metre coordinates of that contact's ankle. For example, `"solePoints": {"fore-left/pad": [[0, -0.02, -0.04], [0, -0.02, 0.05]]}` supplies two points on opposite sides of a pad. An unknown contact is an error. Readers resolve the points to their ankle's slot once. A model with a single central contact can omit the map. [Object reader](../crates/osage-objects/src/foot_rig.rs) · [Ground contact](../knowledge/algorithms/forefoot-plant.md)

For a deforming foot, the four nodes of the contact chain can address ordinary glTF skin geometry. A reader uses POSITION, JOINTS_0, WEIGHTS_0 and the inverse bind matrices, then retains the source faces that lie within the selected foot region. D-227 uses this representation for terrain clearance. [Binding and surface query](../knowledge/algorithms/skinned-sole-clearance.md)

A runtime can sample the existing clip at each model-defined contact to build a per-gait horizontal placement table. The nominal root travel belongs to that gait. The mapped ankle consumes its own table at playback. D-228 derives this data from the existing animation, model tree and the world's motion calibration. [Cyclic foot contact](../knowledge/algorithms/cyclic-foot-contact.md)
