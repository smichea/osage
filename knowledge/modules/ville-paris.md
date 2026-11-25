---
type: Module
title: Ville-Paris
description: An independent city experiment. A portable scene and three fixed-tick drops tie each picture to a camera pose and each landing to its drawn surface.
resource: https://github.com/smichea/osage/tree/main/worlds/ville-paris
licence: MIT OR Apache-2.0
status: draft
tags: [paris, world, replay, physics]
generated: { by: "OpenAI/Codex", at: "2026-11-24T07:11:38Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Purpose

Reproduce a descent onto either deck of Pont de Bir-Hakeim or onto a roof on the Grenelle side. A second implementation can take the same files and compare the impact tick, the camera and the picture.

# Concepts

One glTF 2.0 binary holds geometry, transforms, material colours and support names. A separate JSON file specifies initial foot position and velocity in metres and metres per second, gravity, terminal speed, camera keys, vertical field of view, resolution and expected surface ID. Positive Y points up. Positive X crosses the bridge from Passy towards Grenelle, negative Z points upstream.

# Architecture

The native client and the CPU runner decode the same GLB. A node named `support/ID` contributes a rectangular horizontal landing surface. The reader checks the two upper triangles, their four corners and their shared diagonal. For a hipped roof this recovers the smaller, flat summit. Bevy instances the meshes and draws them through [osage-render](/modules/osage-render.md). Beyond 200 m from a building’s bounding sphere, the window mesh keeps the outward face of each pane with its original colour, normal and position. The close mesh has six faces per pane. The window opening and roof geometry stay fixed.

In the point control, every simulation tick applies gravity to the vertical velocity, clamps the fall speed and integrates the foot. The solver sweeps the entire segment against the candidate surfaces and keeps the earliest crossing. Equal fractions use stable surface IDs. The foot stops at the interpolated point of impact. The viewer places a capsule with a 0.28 m radius and a 1.2 m straight section, centred 0.88 m above the foot. The directional sun points from (−480, 730, 350) towards the origin. The native client uses a 32 m horizontal grid to select nearby surfaces. The bench also runs an exhaustive scan.

The district accepts the human mannequin and imported rigid actors. Each limb can reserve a physical page and excite the shared material voice on contact. [Articulated district terrain](/algorithms/paged-ragdolls.md)

The rigid paths use the full triangles on each tagged node, including slopes and sides. A node named `collider/ID` can add arbitrary solid faces. Rapier or Jolt advances the actor and any dynamic box on the CPU. Bevy places each visible segment at its solved centre and orientation. The human has 15 parts joined at 14 anchors and a total mass of 75 kg. Joint landmarks come from the project's open Quaternius skeleton. The mass distribution, capsule dimensions and joint limits are authored inputs. [Dynamics module](/modules/osage-dynamics.md) · [Quaternius base characters](https://quaternius.itch.io/universal-base-characters)

The viewer selects visual levels with a geometric error bound when the object supplies one. The inspector’s repeatable `--detail-view EYE TARGET FOV_DEG HEIGHT_PX` samples this selection in sequence. [Projected mesh error](/algorithms/projected-mesh-error.md)

Each draw primitive carries its glTF alpha mode, cutoff and side setting. The complete scene and tiled views apply them to the material on that placement. A masked fragment draws when its combined vertex and base alpha reaches the cutoff. An opaque closed building box may hide its neighbour. A translucent placement or a mesh with holes keeps the neighbour eligible to draw. The viewer also accepts PNG and JPEG base colours on UV set zero. Its texture inspector reads the full set of model levels and reports the image passed to the GPU for each primitive. [Mipmaps for masked surfaces](/algorithms/cutout-mipmaps.md)

The offscreen test uses seven materials or face configurations and compares the final RGB across three render paths. Its inputs have solid base colours and optional vertex colours. [Vulkan test](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-alpha.py) · [glTF material specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html)

The rigid replay can time a window of public physics steps with `--time-physics-from TICK`. It warms the world up to the selected tick, then reports durations and contact counts for the remaining steps. [Timing the sleeping pedestrian](/oracles/pedestrian-impact.md)

The viewer follows the GLB object tree when placing static geometry. The headless inspector identifies the model path in each geometry channel. `--predict-impact` returns that path with the first instant of a geometric capsule contact. [Object profile](/modules/osage-objects.md) · [Ballistic impact](/algorithms/ballistic-impact.md)

# Interfaces

A route can request a finer native clock around the first predicted lift encounter. The resulting report records the predicted body slots, the first nonpositive native witness and the extra intervals spent. A second route field can spend additional solves as the joints take the load during the next tick. [Lift contact clock](../algorithms/lift-contact-clock.md) · [Load transfer](../algorithms/lift-contact-response.md)

In a paged replay, use `--support-footprint MAX_GAP_M` for the final footprint and add `--trace-body` to keep each tick. The report also projects the current material points and the mass centre 100 ms forward. The resident Rapier and Jolt receipts include final footprints at a 1 mm separation threshold. [Support footprint](../algorithms/articulated-support.md)

An object-profile GLB can replace the sector through the existing scene option. The viewer selects the primary or a requested visual mesh and gives each solver the declared physical surfaces and their materials. The object inspector can query the visual, physical and acoustic geometry along the same segment. [Object format and commands](/modules/osage-objects.md)

With a paged district, `--trace-body` records the centre immediately before each step and whether the solver advances. An unavailable wall or ceiling reports its surface ID and its path through the model tree. [Doorway and moving camera during a collision hold](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-clearance.py)

From the repository root, the following commands launch the window, capture a frame, or record and verify a run. The default window uses Rapier and an articulated human. The CPU command with the default flat routes and no engine argument selects the point control. A replay with a moving box selects Rapier automatically.

```sh
cargo run --release -p osage-ville-paris
cargo run --release -p osage-ville-paris -- --bench bench/out/ville-paris/local
cargo run --release -p osage-ville-paris -- --verify bench/out/ville-paris/local
node worlds/ville-paris/check-contact.mjs bench/out/ville-paris/local
cargo run --release -p osage-ville-paris -- --physics jolt --actor human --replay worlds/ville-paris/fixtures/bonnet.json --route moving
cargo run --release -p osage-ville-paris -- --physics rapier --actor human --replay worlds/ville-paris/fixtures/bonnet.json --bench bench/out/ville-paris/human
cargo run --release -p osage-ville-paris -- --physics rapier --actor human --replay worlds/ville-paris/fixtures/bonnet.json --verify bench/out/ville-paris/human
cargo run --release -p osage-ville-paris -- --route rail --render-bench bench/out/ville-paris/frames.json
cargo run --release -p osage-ville-paris -- --route road --tick 65 --shot bench/out/ville-paris/road.png
```

Pass `--full-detail` to either picture command to retain every window face at all distances. Compare two PNGs from the same tick with `python3 worlds/ville-paris/check-pictures.py FULL.png DISTANT.png`. The image check allows a mean absolute RGB difference of one code value out of 255 and at most 1% of pixels with any channel differing by more than eight.

Keys 1, 2 and 3 select the corresponding route in the current replay. The default routes are rail, road and roof. R restarts the current drop, Space pauses or resumes, and Escape exits. Pass `--scene FILE.glb --replay FILE.json` to supply the inputs explicitly. The default files are embedded so the window also runs away from the source tree.

Run `python3 worlds/ville-paris/check-paging.py --frames` to build independently readable pages, sweep request distances during the three descents and capture delayed and ready visual detail. The dedicated `ville-paris --paging` entry accepts a page directory or an HTTP(S) base URL, replay, latency, bandwidth and per-channel distances. An optional disk cache shares verified file contents between processes. [District content over HTTP](/algorithms/district-http.md) Its receipts include actual read bytes, installation and eviction ticks and the first supported landing. [Selection and controls](/algorithms/district-paging.md)

# Invariants

The point-control CPU receipt checks every tick, including the starting pose and the resting ticks. At display cadences of 30, 60 and 144 Hz the indexed path must reproduce the exhaustive trace byte for byte. Both bridge approaches finish within 2 cm of one another in plan and reach their named deck. The verifier reads both input digests and regenerates the complete trace before it accepts a run. A separate JavaScript check opens the GLB in Three.js, sums the velocities analytically in double precision and intersects each segment with the actual mesh triangles. It checks the surface and tick exactly and the contact point within 1 cm.

The rigid recorder writes `dynamics.csv` with one row per tick. It includes root pose, child poses, moving-box poses and reported contacts. Each contact identifies its collision substep. The JSON pairs each index with its stable part or surface ID. It reports which target was reached and the peak and final joint anchor gaps. The verifier reruns the same backend and checks the full trace. Per-step timing excludes trace encoding and world construction. Repeating a trace establishes reproducibility on that build.

# Knowledge

The lower deck carries the road, the upper deck the metro. The fixture uses a crossing length of 237.16 m. The author chose the deck heights of 8 and 16 m for the vertical overlap test. Buildings, streets, terrain and the tower are original simplified geometry. [City of Paris](https://www.paris.fr/pages/le-pont-bir-hakeim-retrouve-une-seconde-jeunesse-cet-ete-8169) · [Musée d’Orsay](https://anosgrandshommes.musee-orsay.fr/index.php/Detail/objects/33359)

The [fixture generator](https://github.com/smichea/osage/blob/main/worlds/ville-paris/fixtures/make_sector.py) rebuilds the binary with the Python standard library. The [replay](https://github.com/smichea/osage/blob/main/worlds/ville-paris/fixtures/descent.json) contains the full camera path.

The bridge-edge replay places the actor 10 cm either side of the metro deck boundary. The bonnet replay uses the same pedestrian drop twice, onto a 1,250 kg box initially at rest or moving at 2.8 m/s along the road. Gravity, friction and contact impulses act on the box. Both its linear and angular response are recorded. [Edge replay](https://github.com/smichea/osage/blob/main/worlds/ville-paris/fixtures/deck-edge.json) · [Bonnet replay](https://github.com/smichea/osage/blob/main/worlds/ville-paris/fixtures/bonnet.json)

The resident lift route starts a stationary capsule off-centre on a horizontal platform at Bir-Hakeim. It travels up 3 m in five seconds and returns in five more. Run `python3 -B worlds/ville-paris/check-lift.py bench/out/ville-paris/lift` with a release build. The oracle checks the platform pose against the prescribed motion, reconstructs the lowest capsule point from its rotation and tests its distance from the initial foot anchor in the platform frame. It independently checks the solver contact point, upward normal and surface identity at every tick. The output gives maximum errors and contact counts on each half of the trip. On the 60 Hz fixture, Jolt’s world contact points coincide with the starting platform pose for the step and Rapier’s coincide with the ending pose. The oracle reports discrepancies against both times and applies the 2 cm criterion at step end. To see the same capsule run `target/release/ville-paris --replay worlds/ville-paris/fixtures/lift.json --actor capsule --mute`. [Lift replay](https://github.com/smichea/osage/blob/main/worlds/ville-paris/fixtures/lift.json) · [Contact measurements](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-lift.py)

Run `python3 -B worlds/ville-paris/check-paged-lift.py` for the same ride in the Rapier district. It cooks the same GLB into 128 m cells and compares both a lone passenger and a run in which a second physical observer traverses the district. The checker resolves surface names in each trace, then compares every float32 pose and every contact with its multiplicity. It also checks the planner’s support flag after the first contact and pairs each real eviction with an earlier publication of the same physical cell. [Streamed terrain contacts](/algorithms/paged-ragdolls.md) · [Paged lift check](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-paged-lift.py)

Run `python3 worlds/ville-paris/check-bonnet.py bench/out/ville-paris/bonnet-trials` after building the release binary. The runner varies the landing position, height and incident velocity on a 10 × 10 grid. It runs each case on the resting and moving box with both solvers and verifies every trace in a fresh process. Its independent geometry check measures capsule depth where the lowest point lies over the finite top face and the solver reports a top-facing contact. It counts runs of three consecutive 60 Hz physics ticks deeper than 20 mm, the final anchor gap and the final swing or twist error of single-axis elbows and knees. It records each failing route ID.

The [pedestrian impact check](/oracles/pedestrian-impact.md) records time to contact in seconds, signed distances between the oriented limbs and the moving box, and whole-body lift-off. It uses an imported GLB identified in both receipts by its digest.

# Reuse

For a headless physics consumer, add `--no-visuals` to `--paging`. The district index still supplies the source tree. The client fetches the support catalogue and the physical pages for the same solver, skips visual resources, and can still report static collision intersections and source nodes. [Transfer sizes and replay controls](/algorithms/district-http.md)

A world release can supply the directory reference printed by `cook-pages.py --pin-index`. Pass it to the replay with `--index-pin SHA256:BYTES`. With `--content-cache`, restarting that release offline works after the first run has fetched all the files required by the route. [Snapshot and cache controls](/algorithms/district-http.md)

To offer a smaller download to returning clients, keep the previous full first-frame package in the cooker's output directory and add `--previous-first-frame PREVIOUS_PIN` to `--pin-first-frame`. Give clients the printed launch reference and a persistent `--content-cache`. The cooker always publishes the complete current package for new installs. [District content over HTTP](/algorithms/district-http.md)

Add `--collision-ray-actor` to a paged collision query to include the capsule proxy or an articulated character. `actor_spans` gives the occupied length per body at the current physics tick. The actor table links imported bodies back to their rig nodes and materials. [Actor and terrain segment queries](/algorithms/scene-ray-coverage.md)

A paged scene can query its moving lift or bonnet with `--collision-ray FROM TO`. The reader uses the current solver pose. Results identify the local box faces encountered, any distance spent along a face and the remaining gaps in terrain coverage. [Physical segment coverage](/algorithms/scene-ray-coverage.md)

For a cold launch, `cook-pages.py --pin-first-frame` packages the directory and coarse mesh. Add `--first-frame-cell 0 0` for the Bir-Hakeim road spawn. Pass the printed `--index-pin` value to the ordinary paging client. The same road package also opens either aerial route. [Launch file and cache measurements](/algorithms/district-http.md)

A rendered district can be cooked with `--pin-visual-district` and opened using the printed `--index-pin`. It needs the coarse parent for the first frame and can issue nearby visual detail requests while that frame is prepared. Protected spawns also require collision support where the actor starts close to geometry. [Wire format and reference scenes](/algorithms/district-http.md)

For a headless region that can carry its complete geometry economically, cook with `--pin-bundled-physics`. The receipt counts detail installations from an already verified region in `physics.bundled_installs`. The HTTP transfer is counted once for the entire region. [Bundled collision pages](/algorithms/district-http.md)

For on-demand collision addresses, cook with `--pin-regional-physics` and pass its pin to `--no-visuals --index-pin SHA256:BYTES`. The default address group spans four tile keys on each horizontal axis. For an index containing every page address, cook with `--support-region-tiles 2 --pin-physics` and pass the printed pin to the headless replay. The cooker selects the smaller of the JSON and column physics directories. The native solver loads collision support for the spawn and each subsequent swept volume. Its receipt lists regional reads and installations in `support.regions.events`. A late response holds a threatened actor while the wall clock and I/O continue. Use `--trace-body` to distinguish solver steps from held poses. [Regional support files](/algorithms/district-http.md)

The interactive replay plays physical impacts using the material voices from `osage-sound-voices`. Both the impact track and the stationary room renderer load the bridge scene with `paris_soundscape::bridge()`. The tool owns the authored bridge scene. Ville-Paris uses the engine’s listener model for speaker panning and headphone cues, and excites the struck surface’s resonator with the impulse recorded for an articulated contact. The softer of the sole and surface determines the hardness of the strike. [Sound engine](/modules/osage-sound.md) · [Contact synthesis](/algorithms/contact-voices.md)

The city replay schedules arrivals at its moving camera. The same signal can be rendered to a stereo WAV with a per-impact receipt. Intervening acoustic surfaces filter this wave using the material and placement in their object tree. A screened contact can reach the listener through one or two openings or around the outside of a convex obstacle. Each solid bend records both incident owners. [Exterior wedges](/algorithms/exterior-acoustic-edges.md) [Sound through openings](/algorithms/diffracted-city-contacts.md) [Direct transmission](/algorithms/direct-sound-transmission.md) [Sound from city contacts](/algorithms/city-contact-sound.md)

The render receipt comparer takes paired files with `python3 worlds/ville-paris/compare-frames.py --control FULL.json --trial DISTANT.json`. Offscreen captures compile graphics pipelines synchronously so the first saved image contains the scene. The comparer rejects different inputs, actor, solver, camera ticks, optics, resolution, adapter, driver or timing methods and checks the reported quantiles against raw samples.

The CPU receipt gives elapsed nanoseconds for one suite of three drops and the number of candidate surface checks. Each of 61 samples times eight suites after warm-up, alternating the order of the two methods. Grid construction is timed separately. The render receipt gives elapsed wall time between completed GPU frames, the adapter and driver, image size and the simulation tick of each sample. It draws every sixth tick and includes the endpoint.

# Limits

The ordinary viewer and fixed replay load the full scene at startup. The paging experiment can open individual tile files. The point control measures the first crossing of a tagged flat top. The rigid pilot supports passive joints and moving solids. The bonnet is a solid box with a flat top. Vehicle suspension, active balance and getting up need separate experiments. The frame timing includes CPU and GPU work with one explicit fence per frame. The scene is a local metric model and geographic acceptance requires a mapped survey. An external comparison requires an actual scene import and independent pose and image receipts.
