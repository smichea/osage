# Knowledge Update Log

## 2026-11-25

* **Creation**: Added the oracle [sound against another engine](/oracles/sound-against-reference.md) (issue #19, D-243). On the controlled scenes osage is nearer what the geometry asks than reference A in every scene both played (research 11). The blind listening test is written down and its kit made; it has not run.

* [Walk-grid routing](/algorithms/walk-grid-routing.md) retries at half-cell spacing on connected floor when a bend cannot be reached through cell centres. Two opposing pedestrians complete a 50 cm bend (D-241).

* [Pedestrian encounters](/algorithms/pedestrian-encounters.md) find a supported waiting place when opposing patrols meet in single-file traffic. Straight, bent and no-bay streets exercise the full trajectory (D-240).

* A [blocked pedestrian](/algorithms/pedestrian-encounters.md) can step aside and try a retreat before resuming its route. An offset-destination plaza checks all 64 return trips (D-239).

* [Pedestrian encounters](/algorithms/pedestrian-encounters.md) protect committed steps before predicting undecided steps. A shared passing side helps 64 crossing walkers reach both ends of a crowded plaza (D-238).

* [Pedestrian encounters](/algorithms/pedestrian-encounters.md) bound relative travel on both axes to find which neighbours can affect a steering decision. The replay compares those distances with a full scan (D-237).

* [Walk-grid routing](/algorithms/walk-grid-routing.md) now checks terrain intervals in crossing order and stops at the first blocked interval. A grazing test covers cells exactly one walker radius wide (D-236).

* [Walk-grid routing](/algorithms/walk-grid-routing.md) now measures axial and diagonal steps separately when choosing between passages. A server replay checks the length travelled on both legs (D-235).

* [Walk-grid routing](/algorithms/walk-grid-routing.md) takes two endpoints from the street header. The Bistro replay follows the detour in both directions (D-234).

* The [Paris world](/modules/paris-world.md) reserves space for each arriving walker and waits when its entrance is full. Two SDK replicas check simultaneous arrivals and a late observer (D-233).

* Paris now simulates two [autonomous passers](/algorithms/pedestrian-encounters.md) on crossing street routes, with one-second encounter prediction (D-232).

* The [Paris client](/modules/paris-native.md) selects pedestrian foot correction using the camera projection (D-231). A 24-actor replay checks the live clip clocks and both forefeet through a camera turn.

* [Reversing a loop crossfade](/algorithms/reversible-loop-crossfade.md) preserves live playback state. The native fox replay isolates joint displacement at the command boundary (D-230).

* [Human gait contacts](/modules/osage-humans.md) use the live player phase and the loaded body in the Paris client. The replay checks the shoes during idle, walk and jog transitions (D-229).

* [Cyclic foot contact](/algorithms/cyclic-foot-contact.md) measures the footprint over a whole low interval. The fox replay follows motion of both the pad and the closest material point (D-228).

* [Skinned sole clearance](/algorithms/skinned-sole-clearance.md) measures the skin and terrain across the contact area (D-227). The authored fox replay measures crossings between the visible triangle vertices.

* [Foot planting](/algorithms/forefoot-plant.md) checks rigid sole points at the curb edge (D-226). The fox replay measures both authored gaits using deformed paw vertices, with four initial phases.

## 2026-11-25

* **Creation**: Added the algorithm [measured ears](/algorithms/measured-ears.md) to [osage-sound](/modules/osage-sound.md) (issue #18, D-242): SADIE II's KEMAR through 54 virtual loudspeakers, each way keeping its measured time between the ears.

## 2026-11-24

* **Creation**: Added the algorithm [sound faces from meshes](/algorithms/sound-faces.md) to [osage-sound](/modules/osage-sound.md) (issue #15): faces from an object's meshes through [osage-objects](/modules/osage-objects.md), sheets, a tree of boxes over the faces, and contacts as [osage-dynamics](/modules/osage-dynamics.md) reports them.
* Articulated stops, first lift contact, load transfer, the support footprint and the closest capsule-to-box witness are documented in the algorithm cards. The [loaded landing](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/loaded_landing.rs) exercises the two engine crates together.
* Added [osage-objects](/modules/osage-objects.md) for the glTF object tree and [osage-dynamics](/modules/osage-dynamics.md) for fixed-step contacts and articulated bodies.
* **Creation**: Added the algorithm [far sound field](/algorithms/far-sound-field.md) to [osage-sound](/modules/osage-sound.md) (issue #17): far sound as an ambisonic field per cube, its cost, what it loses against ways, and its bit rate in Opus.
* **Creation**: Added the module [osage-sound-web](/modules/osage-sound-web.md), the engine in a browser's AudioWorklet (issue #16). **Update**: [osage-sound](/modules/osage-sound.md) plays live, natively on a thread of its own and as a Bevy audio source, its nodes placed by a game.
* An [object](/modules/osage-objects.md) can declare its limb contacts in the model tree. [Foot planting](/algorithms/forefoot-plant.md) follows each limb’s own attachment. A four-legged trot crosses a raised strip (D-225).

* [Forefoot planting](/algorithms/forefoot-plant.md) samples a stationary surface independently for each leg. A curb replay measures both shoe clearances under the same root motion (D-224).

* [Forefoot planting](/algorithms/forefoot-plant.md) corrects the support leg after the gait is sampled. Ville-Paris replays both feet through travel, a hold and resumption (D-223).

* The [Paris client](/modules/paris-native.md) matches replicated pedestrians to the calibrated walking and jogging speeds of [osage-humans](/modules/osage-humans.md) using travel after interpolation (D-222).

* A change of pace on a playing [human animation](/modules/osage-humans.md) preserves the stride phase (D-221).

* [Articulated swing stops](/algorithms/articulated-swing-envelope.md) refine simultaneous twist and tilt under measured contact (D-220). The dynamics crate includes the sixteen-part frictionless landing regression.

* [Free rigid pair impacts](/algorithms/free-pair-impact.md) and [lift approaches](/algorithms/lift-contact-clock.md) share a convexity test that resolves endpoint witnesses before partitioning a capsule spine at box faces (D-219).

* The [loaded joint replay](/algorithms/articulated-swing-envelope.md) pairs a sliding impact with a frictionless control and records angular error, penetration and signed vertical speed relative to the platform.

* The [articulated swing envelope](/algorithms/articulated-swing-envelope.md) aligns the oblique projection normals with the transverse ellipse after contact and keeps them as the actor leaves a support. The loaded replay covers the shifted landing and ledge departure (D-218).

* The [articulated swing envelope](/algorithms/articulated-swing-envelope.md) gives a joint more impulse passes when a stop remains exceeded under measured external contact. The small-pad landing checks the resulting peak (D-217).

* The [articulated swing envelope](/algorithms/articulated-swing-envelope.md) bounds lateral projections during each Rapier solve. A quadruped with unequal tilt widths exercises oblique contacts and a certified near miss (D-216).

* [Lift contact response](/algorithms/lift-contact-response.md) refreshes Rapier witnesses near finite platform edges. A certified near miss checks that the platform imparts no momentum (D-215).

* The [articulated swing envelope](/algorithms/articulated-swing-envelope.md) requests extra native impulse passes when a stop exceeds its error band. Two-paw and one-paw landings check the result at the same timestep (D-214).

* The [articulated swing envelope](/algorithms/articulated-swing-envelope.md) transports the Rapier twist axis with the moving limbs. A passive quadruped tests the loaded stop through contact and subsequent free fall (D-213).

* [Articulated swing envelope](/algorithms/articulated-swing-envelope.md) adds an explicit diagonal tilt boundary and a 300-pose replay (D-212).

* [Joint frame conversion](/algorithms/articulated-joint-frames.md) supports a body-to-joint matrix with a Z hinge or twist axis in XML and loose binary fragments (D-211).

* [Load transfer on a moving lift](/algorithms/lift-contact-response.md) uses rig momentum and material-point velocity to refine one tick after impact (D-210).

* [First contact on a moving lift](/algorithms/lift-contact-clock.md) has a continuous prediction and a measured native contact window in both resident and paged replays (D-209).

* [Articulated support](/algorithms/articulated-support.md) projects body momentum against the current material points on rigid supports. The [object importer](/modules/osage-objects.md) can preserve a distinct sphere contact radius (D-207 and D-208).

* [Articulated support](/algorithms/articulated-support.md) measures the extent of upward contacts and the signed margin around the projected centre of mass (D-206).

* [Physical segment queries](/algorithms/scene-ray-coverage.md) can include articulated bodies at their solver poses. Occupied spans retain the physical parent, and imported body slots identify the original bone and material (D-205).

* [Physical segment queries](/algorithms/scene-ray-coverage.md) now report both sides of a moving lift or bonnet at its current solver pose, including clipped distances along a face (D-204).

* [District launch updates](/algorithms/district-http.md) use the cached decoded package as a compression dictionary. A three-release test measures HTTP transfers, checks the first GPU image and queries a raised quay (D-203).

* The [district launch package](/algorithms/district-http.md) can deliver the first rendered image from one immutable file, including a known spawn region. GPU frames, successive launches and an authoring-only version change qualify its cache trade-off (D-202).

* The [visual district directory](/algorithms/district-http.md) retains every visual tile address while requesting collision data by region. Three falls check the first GPU frame and the publication of the first detailed tile (D-201).

* A [regional collision file](/algorithms/district-http.md) can carry the original detail pages. The cooker compares sizes and the native solver can install a verified page without a second HTTP request (D-200).

* A [regional physics pin](/algorithms/district-http.md) resolves detail addresses from the same immutable files as collision support. Four native HTTP trajectories qualify the local and district-wide readers (D-199).

* The [headless district pin](/algorithms/district-http.md) can carry SHA-256 addresses and spatial bounds in columns. The regional replay compares both encodings against the full directory (D-198).

* A delayed [regional collision read](/algorithms/district-http.md) now allows the replay clock to keep running. Two HTTP falls check uninterrupted road motion and an actor held above missing landing geometry (D-197).

* Ville-Paris reads [collision support by region](/algorithms/district-http.md) from the live actor trajectory. A physics-only district pin preserves the source tree and acoustic bindings (D-196).

* Ville-Paris can start from an immutable, compressed [district directory](/algorithms/district-http.md). The HTTP replay checks byte counts, cache repair and an offline restart on the same geometry (D-195).

* A [headless Ville-Paris replay](/algorithms/district-http.md) can stream the district’s collision data independently of its graphics. The HTTP and disk controls check resident queries and three delayed landings (D-194).

* [The district support catalogue](/algorithms/district-paging.md) can use Zstandard or zlib. The cooker chooses by file length, and the reader checks the same collision data in either representation (D-193).

* [District collision support](/algorithms/district-paging.md) now uses the physical page index to select missing faces and retain nearby support. Doorway replays check three publication times against the resident trajectory (D-192).

* [Collision read planning](/algorithms/collision-read-planning.md) now queries physical page bounds for each actor and checks deadlines against an exhaustive search. The comparison includes a 4,745-tile district and a 15-part pedestrian (D-191).

* [Supported-box impact](/algorithms/supported-box-impact.md) now follows the normal momentum of the full rig during compression. The [city impact oracle](/oracles/pedestrian-impact.md) checks foot penetration on offset landings and the subsequent rest (D-190).

* [Reusing contact geometry on a sleeping rig](/algorithms/resting-contact-witnesses.md) reduces the cost of reporting a settled articulated actor and tests waking it after its supporting page is replaced (D-189).

* [First impact on a supported box](/algorithms/supported-box-impact.md) splits an approaching joint tree’s collision step. The [pedestrian oracle](/oracles/pedestrian-impact.md) now measures the native contact geometry and the speed of its surfaces.

* [Settling a hanging ragdoll branch](/algorithms/hanging-ragdoll-branches.md) uses the mass and inertia of a supported limb and checks its equilibrium over a deck edge (D-187).

* **Creation**: Added the modules [osage-sound](/modules/osage-sound.md), [osage-sound-voices](/modules/osage-sound-voices.md) and [osage-sound-analysis](/modules/osage-sound-analysis.md) (D-180, issue #14); the algorithms [sound ways](/algorithms/sound-ways.md), [sound delay lines](/algorithms/sound-delay-lines.md), [contact voices](/algorithms/contact-voices.md), [feedback delay reverberation](/algorithms/feedback-delay-reverb.md) and [loudness mastering](/algorithms/loudness-mastering.md); and the oracle [soundscape verify](/oracles/soundscape-verify.md).

* The reference reader in [sound delay lines](/algorithms/sound-delay-lines.md) shares eight-sample interpolation weights between listeners. [Fractional-delay reconstruction](/algorithms/moving-sound-delay.md) gives the coefficients and the moving-tone regression (D-186).

* [Ville-Paris](/modules/ville-paris.md) renders its impacts with the workspace sound and voice crates. The bridge tool supplies its bundled sound scene.

* [Pedestrian impact on a movable bonnet](/oracles/pedestrian-impact.md) measures relative wrist motion in flight and on a supported branch, including the time to lasting rest.

* [Pedestrian impact on a movable bonnet](/oracles/pedestrian-impact.md) measures deep limb overlap across native substeps and checks how the joint tree settles after impact.

* [First impact of a freely translating pair](/algorithms/free-pair-impact.md) now reports an inward collision at time zero when the moving capsule starts exactly on the box.

* [Streamed terrain contacts](/algorithms/paged-ragdolls.md) now retain the vertical lift and its standing passenger across physical page loads and evictions in [Ville-Paris](/modules/ville-paris.md).

* [Ville-Paris](/modules/ville-paris.md) measures how closely a standing capsule and its contact point follow a moving lift through one full cycle in both [rigid-body solvers](/modules/osage-dynamics.md).

## 2026-11-23

* [Streamed terrain contacts](/algorithms/paged-ragdolls.md) can reconstruct the complete original topology of repeated box-shaped supports before their physical pages arrive.

* [Streamed terrain contacts](/algorithms/paged-ragdolls.md) retain complete source triangles across visual tile edges.

* [District impact sound](/algorithms/paged-impact-sound.md) now measures relative point velocity on a moving support and reserves its acoustic path before contact.

* **Update**: [District impact sound](/algorithms/paged-impact-sound.md) reserves independent receiver paths for simultaneous foot contacts and assigns the material voices in authored surface order.

* **Update**: [Articulated terrain contacts](/algorithms/paged-ragdolls.md) now use the same actor-first creation order in static Rapier scenes.

* **Creation**: [Articulated bodies on streamed terrain](/algorithms/paged-ragdolls.md) covers limb-based requests, solved poses and contact identity in the district.

* [Acoustic mesh selection](/algorithms/paged-acoustic-rays.md) keeps a transformed vertex on the boundary of its district query, including when an older directory rounded the bound inward.

* [Selecting acoustic meshes for a ray](/algorithms/paged-acoustic-rays.md) reuses verified district geometry across a series of finite segments.

* [Loading impact sound geometry](/algorithms/paged-impact-sound.md) connects the district solver and streamed object surfaces to the existing material voices.

* [Sound around a closed convex object](/algorithms/exterior-acoustic-edges.md) distinguishes an exterior path along a solid from an impact inside a sealed room.

* [Hearing an impact around city obstacles](/algorithms/diffracted-city-contacts.md) connects the exposed borders of city meshes to delayed directional impact sound.

* [Activating a ragdoll from animation](/algorithms/animated-ragdoll.md) carries walking, running and landing poses into the passive [rigid dynamics](/modules/osage-dynamics.md).

* [Direct sound through city surfaces](/algorithms/direct-sound-transmission.md) follows a wave through the bridge deck or a panel in an imported object.
* [Sound from city contacts](/algorithms/city-contact-sound.md) follows the falling body into the material voice and checks the stereo signal at the camera in speaker and headphone modes.

* [District paging](/algorithms/district-paging.md) gives omitted collision faces conservative bounds and tests passage through a doorway while its full page is delayed.

* [District content over HTTP](/algorithms/district-http.md) records the network reader, shared disk cache and late-page replay.

* [Mipmaps for masked surfaces](/algorithms/cutout-mipmaps.md) records the alpha test, mip construction and measurement used by Ville-Paris.

* [Projected mesh error](/algorithms/projected-mesh-error.md) relates the visual detail of a part to the current camera and viewport.

* The [object profile](/modules/osage-objects.md) keeps node identity through drawing, contact and sound. The [ballistic query](/algorithms/ballistic-impact.md) measures the first physical instant on a fixed mesh.

* [District paging](/algorithms/district-paging.md) qualifies visual and collision request distances during the three approaches to Bir-Hakeim.

* [Ville-Paris](/modules/ville-paris.md) runs the same [pedestrian impact](/oracles/pedestrian-impact.md) in both physics solvers with a rig read from the [object format](/modules/osage-objects.md).

## 2026-11-22
* [osage-objects](/modules/osage-objects.md) defines a static object and its three geometry channels. [Ville-Paris](/modules/ville-paris.md) imports them and compares intersections and rigid contact on an original slab.

* [osage-dynamics](/modules/osage-dynamics.md) supplies two native solvers to [Ville-Paris](/modules/ville-paris.md). Replays include a 15-part human, bridge edges and a movable bonnet.
* [Ville-Paris](/modules/ville-paris.md) adds a portable city fixture, three repeatable landings and separate CPU and rendering measurements.
* **Update**: the Paris walk released as the kart is (D-143): [osage-update](/modules/osage-update.md) gains `folder`, the policy `kart-update` and the new `paris-update` share; [osage-paris-native](/modules/paris-native.md) gains its launchers and its content zip.
* **Update**: the light ahead made ready (D-142): [osage-paris-native](/modules/paris-native.md) makes the probes' texture arrays once, lends their layers and writes the light of the cubes the walker goes to before it gets there, its light threads fetching what presses most; [osage-world-paris](/modules/paris-world.md) tells what is on a walker's way from what is beside it; a film's measure is paced as a window's and counts every frame.

## 2026-11-19
* **Update**: probes as tiles, decoded where the walker looks (D-141): [osage-hologram-stream](/modules/osage-hologram-stream.md) packs each probe inside a cube as tiles that decode alone; [osage-world-paris](/modules/paris-world.md) sends the halves first, then the probes whole; [osage-paris-native](/modules/paris-native.md) draws the half at once and writes the tiles over it from the middle of the view outwards.
* **Update**: lighter probes (D-140): [osage-hologram-stream](/modules/osage-hologram-stream.md) packs at quality 22 and at half the size; [osage-world-paris](/modules/paris-world.md) sends a runner the halves; [osage-paris-native](/modules/paris-native.md) draws the best size it has.
* **Update**: the light sent along the walker's way (D-139): [osage-world-paris](/modules/paris-world.md) sends the centres' probes of the cubes on its way and beside it, its own cube whole only when it stands, from its velocity; [osage-paris-native](/modules/paris-native.md) asks the same walking alone.
* **Update**: a smoother walk: [osage-paris-native](/modules/paris-native.md) writes each probe into its layer of the cube's arrays from the render world, a band a frame, and decodes at a lower priority; [osage-hologram-stream](/modules/osage-hologram-stream.md) packs a light's radiance as it converts it (`Planes::rgb_as`).
* **Update**: the traced block under a dome of the light coming in (D-138): [osage-paris-native](/modules/paris-native.md) cuts the street to the walker's block for the rays and shines the centre probe's light round it, its floor at 0.88 of Cycles from 0.71; [ray-traced light](/techniques/ray-traced-light.md) traces a thing drawn with another mesh anew.
* **Update**: [osage-paris-native](/modules/paris-native.md) sets out from `--at`, facing `--heading`, for a walk too; [ray-traced light](/techniques/ray-traced-light.md) draws with Solari's reference path tracer under `OSAGE_PATHTRACE=1`.

## 2026-11-18
* **Update**: walking with the probes inside the cubes (D-137): [osage-world-paris](/modules/paris-world.md) sends the centres' probes first; [osage-paris-native](/modules/paris-native.md) asks the server on a thread of its own and keeps the block drawn until the next one's light has come; [osage-hologram-stream](/modules/osage-hologram-stream.md) decodes a light's picture on several threads.
* **Update**: the light coming into the walker's block is taken by five probes inside its cube (D-137): [osage-world-paris](/modules/paris-world.md) sends a cube's probes where they are baked, else its block's tiles; [osage-paris-native](/modules/paris-native.md) draws from them by default.
* **Update**: [osage-paris-native](/modules/paris-native.md) fills the holes no probe sees, draws its faces after the opaque pass (traced light's denoiser leaves them) and tries probes inside the walker's cube (`--cube-probes`): 28.4 dB against Cycles, a hundredfold fewer holes than the block's surface tiles.
* **Update**: [osage-paris-native](/modules/paris-native.md) marches exactly in the probes (the last step halved, hidden points set aside, probes weighed by how like the eye's their look is), shows radiance through Blender's own AgX and leaves SSAO out: 22.4 dB against Cycles from 19.6, every region's brightness within 6 %; [osage-hologram-stream](/modules/osage-hologram-stream.md) keeps distance nearly exact.
* **Creation**: Added the Paris world, [osage-world-paris](/modules/paris-world.md) and its client [osage-paris-native](/modules/paris-native.md): a walk in Bistro, the 9 cubes around the walker drawn, the light coming into them streamed (D-136); [osage-hologram-stream](/modules/osage-hologram-stream.md) carries tiles of light (radiance in high dynamic range, distance).
* **Creation**: Added the modules [osage-cubes](/modules/osage-cubes.md) — nested cubes around a viewer, each level centred on its cube, coarse holograms carved by finer neighbourhoods — and [osage-hologram-stream](/modules/osage-hologram-stream.md) — a hologram's views in blocks of AV1 video along the angle, sent as the viewer needs them (D-134).
* **Update**: [osage-hologram](/modules/osage-hologram.md) draws streamed holograms (`stream::StreamPlugin`), their window filled as views come.

## 2026-11-14
* **Update**: [osage-update](/modules/osage-update.md): `kart-update` takes the programs' zip of its own system, Windows or a Mac with Apple silicon, and runs a Mac's `.command` launchers again with `exec` (D-127).

## 2026-11-13
* **Update**: hills, bridges and tunnels (D-077): [osage-kart-physics](/modules/kart-physics.md) and [arcade kart physics](/algorithms/arcade-kart-physics.md) carry a road's height; [osage-kart-tracks](/modules/kart-tracks.md) has Eagle Ridge and Red Gorge; [osage-kart-land](/modules/kart-land.md) lays gorges, ridges, walls; [osage-kart-native](/modules/kart-native.md) draws bridges and tunnels; [osage-kart-cup](/modules/kart-cup.md) has High Roads.
* **Update**: [exact client prediction](/algorithms/exact-client-prediction.md) knocks into the other karts, stepped on with the controls their states now carry (D-076); [osage-kart-native](/modules/kart-native.md) draws them at the player's moment.
* **Update**: [osage-kart-native](/modules/kart-native.md) chooses its tier for the machine and steps down where one fails (D-075); 30 frames a second by default (D-074).
* **Creation**: Added the module [osage-report](/modules/osage-report.md): crash reports found at the next start and filed as GitHub issues through a relay on Cloud Run, with the player's consent (D-073); [osage-kart-native](/modules/kart-native.md) asks and sends.
* **Update**: [osage-kart-items](/modules/kart-items.md) releases things through one function a client shares (D-071); oil makes tyres oily ([osage-kart-physics](/modules/kart-physics.md)'s `slick`, D-072).
* **Update**: [osage-kart-sound](/modules/kart-sound.md) plays its engine loops as grains; [osage-kart-native](/modules/kart-native.md) hears the karts from the player's kart with distance, panning and Doppler (D-070), shows a frame-rate meter and keeps a quality setting.
* **Update**: [a photographed sky](/techniques/panorama.md) can be calibrated by its brightness overhead, for an overcast sky with no sun (D-069); [osage-kart-native](/modules/kart-native.md) races the rain under one.
* **Update**: rain (D-068): [osage-kart-physics](/modules/kart-physics.md) and [arcade kart physics](/algorithms/arcade-kart-physics.md) have a wet road; [osage-kart-tracks](/modules/kart-tracks.md) a chance of rain; [osage-kart-native](/modules/kart-native.md) draws the rain.
* **Update**: [osage-kart-native](/modules/kart-native.md) lays marks on the track as karts drive (D-067).
* **Update**: [osage-kart-native](/modules/kart-native.md) has a photo mode (D-066): the picture frozen, a free camera, zoom and depth of field.
* **Update**: battles (D-065): [osage-kart-tracks](/modules/kart-tracks.md) has arenas, the Yard among them; [osage-kart-items](/modules/kart-items.md) has a battle's paddock; [osage-kart-cup](/modules/kart-cup.md)'s events gain `battle:ARENA`; [osage-kart-native](/modules/kart-native.md) shows lives.
* **Creation**: Added the module [osage-kart-profile](/modules/kart-profile.md): the player's profile on their computer, paints won in cups (D-064); [osage-kart-native](/modules/kart-native.md) keeps it and paints with P; the world carries `Choice.paint`.
* **Update**: [osage-kart-native](/modules/kart-native.md) presents the race (D-063): a flyover of the circuit on the grid, the player's finish again in slow motion.

## 2026-11-12
* **Creation**: Added the module [osage-kart-ghost](/modules/kart-ghost.md): time trials against the ghosts of the best runs, kept as the steps of each tick and driven again by the world (D-062); [osage-kart-records](/modules/kart-records.md) keeps the best trials; [osage-kart-cup](/modules/kart-cup.md)'s events gain `trial:ID`; [osage-kart-items](/modules/kart-items.md) has an empty paddock; [osage-kart-native](/modules/kart-native.md) draws ghosts seen through.
* **Update**: each track its place (D-061): [osage-kart-land](/modules/kart-land.md) can fall away to a sea past cliffs; [osage-kart-roadside](/modules/kart-roadside.md) grows more or fewer trees by the track's scenery; [osage-kart-native](/modules/kart-native.md) draws the sea, a lighthouse, a harbour town.
* **Creation**: Added the module [osage-voice](/modules/osage-voice.md): the players talk to one another through a relay beside the world, push to talk or an open microphone, each voice mutable (D-060); [osage-kart-native](/modules/kart-native.md) joins the conference, V to talk; `kart-server` starts the relay.
* **Creation**: Added the module [osage-music](/modules/osage-music.md): files, internet radio and its directory, a host's playlist shared on the network (D-059); [osage-kart-native](/modules/kart-native.md) plays the game's music, the player's, the host's or radio, B to skip; `kart-server --music`.
* **Creation**: Added the module [osage-kart-cup](/modules/kart-cup.md): cups of four races with points, standings and a podium; the host chooses the event, the others vote (D-057); [osage-kart-native](/modules/kart-native.md) shows the cup between races.
* **Creation**: Added the module [osage-kart-tracks](/modules/kart-tracks.md): tracks are text files, checked and built (D-056); four tracks; [osage-kart-native](/modules/kart-native.md)'s bench takes `--track` and the scene is built again when the world's track changes; [osage-kart-records](/modules/kart-records.md) keeps records per track.
* **Creation**: Added the module [osage-kart-roster](/modules/kart-roster.md): eight drivers in three classes and four kart bodies, each pair a build within a narrow band (D-055); [osage-kart-physics](/modules/kart-physics.md) and [Arcade kart physics](/algorithms/arcade-kart-physics.md) carry a build and share bumps by mass; [osage-kart-racecraft](/modules/kart-racecraft.md) drives each build at its limits; [osage-kart-native](/modules/kart-native.md) chooses between races.
* **Update**: [Ground that varies](/techniques/ground-variation.md) samples a grain without a tile (hexagonal cells) and draws a road's history; [osage-kart-native](/modules/kart-native.md) untiles its road from T2 and grass and gravel from T3; [osage-kart-roadside](/modules/kart-roadside.md)'s species follow the place (D-054).
* **Creation**: Added the module [osage-kart-records](/modules/kart-records.md): the kart world keeps best laps and races across restarts (D-053); [osage-kart-native](/modules/kart-native.md) shows them between races.
* **Update**: [osage-kart-items](/modules/kart-items.md) has ten items drawn by place and gap to the leader, some held behind as a shield, a drone, a bumper and a pit-crew push (D-052); [osage-kart-sound](/modules/kart-sound.md) gains cues and a drone's buzz; [osage-kart-native](/modules/kart-native.md) draws and plays them; [osage-kart-racecraft](/modules/kart-racecraft.md)'s bots steer round what lies on the road.
* **Update**: the ramps become crests (D-051): [Arcade kart physics](/algorithms/arcade-kart-physics.md)'s flight is ballistic over a road that rises and falls; [osage-kart-physics](/modules/kart-physics.md) has `Crest` and `track_lift`; [osage-kart-land](/modules/kart-land.md)'s land and frames follow the road over a crest; [osage-kart-native](/modules/kart-native.md) draws road, kerbs, verges, barriers and fences over it.
* **Update**: [Arcade kart physics](/algorithms/arcade-kart-physics.md) and [osage-kart-physics](/modules/kart-physics.md): three levels of mini-turbo, ramps and tricks, starts judged against the lights, slipstreams, and a table of the values that make the feel (D-050); [osage-kart-racecraft](/modules/kart-racecraft.md)'s bold bots trick off ramps; [osage-kart-native](/modules/kart-native.md) draws ramps, flight, tricks, level-coloured sparks, wakes and stalls.
* **Update**: [Holding a frame rate](/techniques/frame-pacing.md) can also trade the light from the sky for a constant light, the atmosphere for a sky of one colour and a haze, and leave out detail textures; the sun's shadows come back before Bevy works out its cascades, which it otherwise did not survive.
* **Update**: [osage-kart-native](/modules/kart-native.md): the host starts each race from the grid, a player alone can end one (D-049).
* **Update**: [osage-kart-items](/modules/kart-items.md) gains a smoke canister (coloured smoke, `Racer.smoke`); [osage-kart-native](/modules/kart-native.md) shows on every kart the item it holds.
* **Update**: [osage-kart-physics](/modules/kart-physics.md) gains a rocket (D-048); [osage-kart-items](/modules/kart-items.md) a rocket item and a 2.5 s nitro; [osage-kart-native](/modules/kart-native.md) glass crates with question marks, broken at once with confetti, rockets and boost flames and smoke.
* **Creation**: Added the module [osage-update](/modules/osage-update.md): the kart's Windows folder updates itself from GitHub releases before each game (D-047).
* **Creation**: Added the technique [holding a frame rate](/techniques/frame-pacing.md) to [osage-render](/modules/osage-render.md); [osage-kart-native](/modules/kart-native.md) gains settings (Escape) with the frame rate wanted.
* **Update**: [osage-kart-items](/modules/kart-items.md) spares the owner of oil and a cone for 6 s; the native client draws things as far ahead as its prediction runs, tosses cones and spreads oil; the cockpit camera no longer lags behind the helmet; [osage-kart-sound](/modules/kart-sound.md)'s engine loops are steadier.
* **Update**: [osage-kart-sound](/modules/kart-sound.md) plays CC0 recordings when the content has them (engine loops following the revs, squeal, gravel, knocks; the crowd in [osage-kart-native](/modules/kart-native.md)'s grandstand) and synthesises the rest; it computes its slow controls once every 32 samples, which stopped most of the audio callback's underruns.
* **Creation**: Added the module [osage-kart-items](/modules/kart-items.md): paddock objects — crates, a nitro, a tyre to throw, a cone and oil — in the world's races (D-046); [osage-kart-native](/modules/kart-native.md) draws them and shows the item in hand.
* **Creation**: Added the module [osage-kart-sound](/modules/kart-sound.md): the karts of [osage-kart-native](/modules/kart-native.md) are heard, their sound synthesised from their state.
* **Creation**: Added the module [osage-kart-racecraft](/modules/kart-racecraft.md): the world's bots race with temperaments; the [timedemo bench](/algorithms/timedemo-bench.md) races them too and becomes `kart-lap/3` ([render bench](/oracles/render-bench.md)).
* **Creation**: Added the algorithm [capsule contacts with impulses](/algorithms/capsule-contacts.md) to [osage-kart-physics](/modules/kart-physics.md): karts knock, spin and scrape the barriers with momentum (D-045); each wheel feels road, kerb or grass ([arcade kart physics](/algorithms/arcade-kart-physics.md), [physics golden file](/oracles/physics-golden.md)).
* **Update**: [osage-kart-model](/modules/kart-model.md)'s drivers bend at the waist: a spine bone carries the trunk, the shoulders and the head; [osage-kart-native](/modules/kart-native.md) throws the trunk out of bends and forward under braking, on a spring.

## 2026-11-11
* **Update**: [Blended solids](/algorithms/blended-solids.md) can be skinned; [osage-kart-model](/modules/kart-model.md)'s drivers steer, their arms following the wheel; its bodywork is moulded.
* **Update**: [A photographed sky](/techniques/panorama.md): a world may have one photograph per time of day; the kart world gains a late afternoon ([osage-kart-native](/modules/kart-native.md), `--sky evening`).
* **Update**: [osage-kart-model](/modules/kart-model.md) covers its karts in sponsors' stickers, abstract marks drawn by the program.
* **Update**: [osage-kart-crowd](/modules/kart-crowd.md) stands people up: onlookers along the fences and marshals at the bends of [osage-kart-native](/modules/kart-native.md).
* **Update**: [osage-props](/modules/osage-props.md) can light grass as the ground; [osage-kart-roadside](/modules/kart-roadside.md)'s grass no longer reads as dark leaves on the meadow.
* **Creation**: Added the algorithm [tree cards](/algorithms/tree-cards.md) to [osage-props](/modules/osage-props.md); [osage-kart-roadside](/modules/kart-roadside.md) grows woods of a few thousand trees with it.
* **Creation**: Added the module [osage-kart-crowd](/modules/kart-crowd.md): the grandstand's spectators are sculpted figures in three poses.
* **Update**: [Sculpted scenery](/algorithms/sculpted-scenery.md) gains texture coordinates in metres, projected from the side each face faces most; the photographed kart circuit lays a concrete grain over what is built.
* **Creation**: Added the algorithm [blended solids](/algorithms/blended-solids.md) to [osage-procedural](/modules/osage-procedural.md); [osage-kart-model](/modules/kart-model.md) sculpts its driver with it.
* **Creation**: Added the technique [ground that varies](/techniques/ground-variation.md) to [osage-render](/modules/osage-render.md); the kart circuit's meadows, verges and aprons use it.
* **Update**: [osage-kart-model](/modules/kart-model.md): a livery names the suit's own colour; two-tone helmets, a neck brace, a black rear bumper with its number plate.
* **Creation**: Added the algorithm [aggregate thinning](/algorithms/aggregate-thinning.md): [osage-props](/modules/osage-props.md) thins foliage instead of clustering it, and can draw blended materials opaque; [osage-kart-roadside](/modules/kart-roadside.md) plants firs and broadleaf trees.
* **Creation**: Added the technique [a photographed sky](/techniques/panorama.md) and the modules [osage-content](/modules/osage-content.md), [osage-procedural](/modules/osage-procedural.md), [osage-props](/modules/osage-props.md), [osage-kart-land](/modules/kart-land.md), [osage-kart-roadside](/modules/kart-roadside.md) and [osage-kart-model](/modules/kart-model.md): the kart client split into components (D-035) on the way to a photographed circuit.
* **Update**: [osage-props](/modules/osage-props.md) lays carpets — grass around the camera, moved with it — and can tint tiny models; [osage-kart-roadside](/modules/kart-roadside.md) lays one along the circuit.
* **Update**: [Post-processing](/techniques/post-processing.md): a world's look can name a lens — vignette and colour fringes — from T3.
* **Update**: [osage-props](/modules/osage-props.md) makes levels of detail by vertex clustering for dense models seen far.
* **Update**: [Sculpted scenery](/algorithms/sculpted-scenery.md) and [land around a track](/algorithms/land-around-a-track.md) now live in their own crates; [the timedemo bench](/algorithms/timedemo-bench.md) waits for the scenery to load and, without a window, for the GPU.

## 2026-11-10
* **Creation**: Added the module [osage-region-link](/modules/region-link.md), the messages between the gateway and a region (D-037).
* **Update**: [The render bench](/oracles/render-bench.md) runs tiers T3 and T4 on a cloud GPU for the changes merged into `main`, at most once an hour.
* **Update**: [The render bench](/oracles/render-bench.md) is now `kart-lap/2`, whose karts overtake ([timedemo bench](/algorithms/timedemo-bench.md)); its receipt records whether the camera followed the race.
* **Creation**: Added the technique [light from the sky](/techniques/sky-light.md), split from [sky and sun](/techniques/sky-and-sun.md), and the algorithm [sculpted scenery](/algorithms/sculpted-scenery.md).
* **Update**: [Land around a track](/algorithms/land-around-a-track.md) gains a knoll, a lake, painted ground and a finer grid near the circuit; [osage-kart-native](/modules/kart-native.md) gains its landmarks.
* **Creation**: Established the bundle and the [osage engineering profile](/profile.md), version 0.1.
* **Creation**: Added the modules [osage-render](/modules/osage-render.md), [osage-kart-physics](/modules/kart-physics.md) and [osage-kart-native](/modules/kart-native.md), their five rendering techniques, five algorithms and two oracles.
