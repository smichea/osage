---
okf_version: "0.2"
---

# osage knowledge

What osage knows about its own modules: their architecture, the algorithms and techniques they use, and the checks that prove their claims. Written to be read without the code, and reused in other engines. Format: [Open Knowledge Format v0.2](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md) with the [osage engineering profile](profile.md).

## Start here

* [osage engineering profile](profile.md) - The types, fields and sections every card follows, and how the bundle is checked and federated.
* [Change log](log.md) - What changed in this bundle.

## Modules

* [osage-objects](modules/osage-objects.md) - Visual, physical and acoustic geometry in a glTF object tree.
* [osage-dynamics](modules/osage-dynamics.md) - Fixed-step rigid bodies, articulated actors and contact reports with Rapier and Jolt.

* [osage-render](modules/osage-render.md) - Rendering techniques as independent Bevy plugins, each configured for the fidelity tier the application runs at.
* [osage-kart-physics](modules/kart-physics.md) - The kart world's arcade physics, one deterministic Rust crate run natively by the server and as WebAssembly by clients.
* [osage-kart-client](modules/kart-client.md) - The browser client of the kart world: three.js, the kart predicted by the shared physics in WebAssembly, the paddock's things and the rain drawn.
* [osage-kart-native](modules/kart-native.md) - The native client of the kart world, built on Bevy, and the testbed of the rendering programme with its benchmark.
* [osage-content](modules/osage-content.md) - Content from the asset manifest made ready to draw — textures with full mipmaps and filtering, photographs of the sky decoded to linear light.
* [osage-update](modules/osage-update.md) - Keeps a program folder up to date from a project's GitHub releases — the newest release of a tag prefix, downloads checked against GitHub's digests, files replaced in place; with kart-update, the kart's Windows launcher step.
* [osage-music](modules/osage-music.md) - Music for a world's players: playlists of files, internet radio and its directory (opt-in), a host's playlist shared on the local network and heard at the same point by all; decoded on a thread into a feed the audio output never waits on.
* [osage-report](modules/osage-report.md) - Crash reports for osage programs: a run's log, panic and start watched in a folder; at the next start, a report of a panic, a start that never finished or renderer errors, cleaned of what identifies the player, sent where the program says once the player agrees — to a relay that files GitHub issues without duplicates.
* [osage-voice](modules/osage-voice.md) - Players talking to one another as in a conference call: speech in ADPCM frames over UDP to a relay beside the world, each voice queued and mixed, any of them muted; the microphone opened only when the game asks.
* [osage-procedural](modules/osage-procedural.md) - Procedural geometry — shapes sculpted, painted and merged per material and patch of land — and a repeatable generator, so that what is generated is the same on every machine.
* [osage-hologram](modules/osage-hologram.md) - Holographic textures (issue #7) — an object's appearance from every viewing angle, baked, and compressed as a video along the viewing direction.
* [osage-hologram-stream](modules/osage-hologram-stream.md) - Holograms sent by a server as a viewer needs them — the grid of views cut into blocks, each a short AV1 video along the viewing angle; a packer, a decoder in Rust, and a small HTTP server and client.
* [osage-cubes](modules/osage-cubes.md) - Nested cubes of 81, 27, 9 and 3 m around a viewer — its own 3 m cube in 3D, every other a hologram, each level centred on the cube the viewer is in — and which views of those holograms it needs before long.
* [osage-world-paris](modules/paris-world.md) - The Paris world — a walk on foot in Amazon Lumberyard's Bistro — its walk grid and walkers, the tiles of light coming into each walker's block, and the server that sends them as each walker's place and speed ask.
* [osage-paris-native](modules/paris-native.md) - The native client of the Paris world — a walk on foot in Bistro, seen from the walker's eyes — drawing the 9 cubes of 3 m around the walker and, beyond them, the light coming into its block, streamed and decoded as it walks.
* [Ville-Paris](modules/ville-paris.md) - An independent city experiment with a portable scene, fixed-tick descents and measured landings on both levels of Pont de Bir-Hakeim.
* [osage-humans](modules/osage-humans.md) - People from rigged models of the asset manifest, dressed by the joints that move each part of the body, animated from a library on the same skeleton.
* [osage-props](modules/osage-props.md) - Models of the asset manifest placed many times, drawn in batches, each set seen up to its own distance.
* [osage-kart-land](modules/kart-land.md) - The land around the kart circuit as geometry — flat where karts drive, hills beyond, a knoll and a lake inside the loop — for whoever draws, dresses or plants it.
* [osage-kart-roadside](modules/kart-roadside.md) - What stands along the kart circuit when the asset manifest's models are there — tyre walls, grass, wild plants, rocks and pines — placed the same way on every machine.
* [osage-kart-model](modules/kart-model.md) - A racing kart and its driver modelled to scale from simple shapes, in a team's colours, each finish — varnish, satin, rubber, steel, alloy, chrome, cloth, visor — its own material.
* [osage-kart-crowd](modules/kart-crowd.md) - Spectators sitting in a grandstand, sculpted as whole figures in a few poses and shared, for a world to dress in its own clothes.
* [osage-kart-cup](modules/kart-cup.md) - Cups of kart races: points by finishing place, standings between races with their tie-breaks, the podium; the world races a cup or one track, the host choosing and the others voting.
* [osage-kart-ghost](modules/kart-ghost.md) - A kart's run kept to race against — the state it started from and the steps applied in each tick, driven again by the deterministic physics to the same bit — in a plain text file per track, laps and driver; the kart world's time trials are built on it.
* [osage-kart-profile](modules/kart-profile.md) - A kart player's profile kept on their own computer — the driver, kart and paint raced with last, points won in cups, cups raced, won and on the podium — and the special paints those points unlock, cosmetic only.
* [osage-kart-items](modules/kart-items.md) - Paddock objects for the kart world — ten items drawn by place and gap to the leader, some held behind the kart as a shield — and how AI drivers use them; deterministic, run by the world.
* [osage-kart-records](modules/kart-records.md) - A kart world's records — best laps, best races and best time trials per track, one per driver name, the fastest ten — kept by the world in a plain text file that survives a restart, published to clients and agents as kart.records/1.
* [osage-kart-roster](modules/kart-roster.md) - The kart game's drivers (eight, light, medium or heavy) and kart bodies (four), each with the build it gives — top speed, acceleration, handling, grip and weight as shares of the standard kart's — balanced so that every pair laps within a narrow band.
* [osage-kart-tracks](modules/kart-tracks.md) - The kart game's tracks as data: a plain text format (centre line, road and verge widths, crests, start, sky, scenery), checked for crossings, bends too tight and crests in bends, and built into the physics' Track; four tracks come with the game.
* [osage-kart-racecraft](modules/kart-racecraft.md) - How the kart world's AI drivers race — a temperament each, braking for bends, a racing line, overtaking or waiting, defending, steering round things on the road, mistakes and grudges after a knock — deterministic, for the server's bots and the bench.
* [osage-kart-sound](modules/kart-sound.md) - A kart's sound synthesised from its state — a two-stroke engine following the revs and the throttle, tyres squealing as they slide, kerbs drumming, grass, wind, knocks — sample by sample, deterministic.
* [osage-sound](modules/osage-sound.md) - Sound placed in an object tree and carried to a listener's two ears — emitters, surfaces and touches on the tree's nodes, the ways each sound takes, a delay line per emitter so that distance and Doppler need no rule, ears without a measured head, a reverberation tail — at four qualities a device can afford, rendered offline today.
* [osage-sound-voices](modules/osage-sound-voices.md) - The voices osage-sound places in a scene, each synthesised sample by sample from the state of what makes it — a river, cars, a scooter, a rubber-tyred metro and its viaduct, footsteps, the city far off, contacts — with no recording, deterministic.
* [osage-sound-analysis](modules/osage-sound-analysis.md) - What a stereo recording tells of where its sounds are, measured the same way whatever made it — true peak, loudness by BS.1770-4, octaves, level and time differences between the ears, a tone's Doppler shift, a passage.
* [osage-sound-web](modules/osage-sound-web.md) - osage-sound in a browser: the engine as WebAssembly with a small C interface, played in an AudioWorklet whose render quantum is its block, each block made when the output needs it with every pose told until then.
* [osage-region-link](modules/region-link.md) - The message contract between the replication gateway and a region server, the same in one process and across machines.

## Rendering techniques

* [Sky and sun](techniques/sky-and-sun.md) - A physically based atmosphere and sun whose cost is cut per tier, down to a flat sky with fog.
* [A photographed sky](techniques/panorama.md) - An unclipped HDR photograph of the sky drawn as the sky, lighting the scene with its sun taken out, and calibrated by its own sun so that the sky has its true brightness.
* [Ground that varies](techniques/ground-variation.md) - A ground texture that never repeats — its grain sampled by hexagonal cells shifted and turned by where they are, multiplied by clumps and patches, and on a road by its history along its length (seams, patches, cracks, stains); octaves smaller than a few pixels fade out.
* [Light from the sky](techniques/sky-light.md) - How the sky and the ground light what the sun does not, chosen per tier because lighting from a cubemap costs 2 to 3 ms on an integrated GPU whatever its size.
* [Ray-traced light](techniques/ray-traced-light.md) - At T4, direct and indirect light traced by Bevy Solari on hardware with ray queries, meshes doubled for the rays, hidden ones left out.
* [Traced-light denoising](techniques/traced-light-denoising.md) - At T4, the traced light's grain filtered by an edge-avoiding à-trous wavelet over the light divided by surface colour.
* [Sun shadows](techniques/sun-shadows.md) - Cascaded shadow maps for the sun, with cascades, reach and resolution chosen per tier from measurements.
* [Ambient occlusion](techniques/ambient-occlusion.md) - Screen-space ambient occlusion from T3, kept out of T2 by its measured cost and by a defect on integrated graphics.
* [Post-processing and anti-aliasing](techniques/post-processing.md) - HDR, exposure, the world's look, anti-aliasing and bloom, chosen per tier so that the lower tiers need no depth prepass.
* [Resolution and upscaling](techniques/resolution-and-upscaling.md) - The scene is drawn into a picture whose size the tier decides, then stretched to the window under a sharp interface.
* [Holding a frame rate](techniques/frame-pacing.md) - Given the frame rate a person wants, the picture climbs down a ladder of cheaper pictures until frames come at that rate, and back up when there is room.

## Algorithms

* [Articulated bodies on streamed terrain](algorithms/paged-ragdolls.md) - Reserves terrain for limbs and preserves their solved contacts and poses across page changes.
* [Loading impact sound geometry](algorithms/paged-impact-sound.md) - Predicts the landing and preserves the receiver clock when acoustic meshes arrive late.

* [Mipmaps for masked surfaces](algorithms/cutout-mipmaps.md) - Keeps the occupied fraction of a texture close to its full-resolution value at smaller levels.
* [Visible mesh selection by projected error](algorithms/projected-mesh-error.md) - Chooses a mesh for each placed part with a geometric error bound in pixels.
* [First impact of an articulated body on a supported box](algorithms/supported-box-impact.md) - Predicts the incoming contact with a supported box and refines the solve while the joint tree transfers its load.
* [First impact of a freely translating rigid pair](algorithms/free-pair-impact.md) - Advances a translating capsule or joint tree to its first contact with a free box.
* [Reusing contact geometry on a sleeping rig](algorithms/resting-contact-witnesses.md) - Preserves published witnesses while Rapier advances sleeping bodies.
* [Settling a hanging ragdoll branch](algorithms/hanging-ragdoll-branches.md) - Follows a supported joint tree to damp the relative rotation of a freely hanging limb.
* [Predicting encounters between walking pedestrians](algorithms/pedestrian-encounters.md) - Predicts when walkers meet and chooses an avoidance step on the Paris street.
* [Planting the forefoot during an animated stride](algorithms/forefoot-plant.md) - Holds a low forefoot in place on level ground as the actor advances.
* [Retaining clip time through a reversed crossfade](algorithms/reversible-loop-crossfade.md) - Reuses a live loop clock when the gait request changes again (D-230).
* [Choosing a footprint over an entire stance interval](algorithms/cyclic-foot-contact.md) - Plans a contact from a sampled gait loop and its nominal root travel.
* [Ground clearance of a skinned sole](algorithms/skinned-sole-clearance.md) - Follows foot skin weights and checks overlapping terrain triangles (D-227).
* [Activating a ragdoll from animation](algorithms/animated-ragdoll.md) - Starts each rigid body with its sampled pose and a velocity consistent with its joint anchors.
* [Attributing solved contact impulses](algorithms/contact-impulses.md) - Identifies the actor part and surface for a measured reaction and retains ambiguous contact balances.
* [First contact on a ballistic arc](algorithms/ballistic-impact.md) - Predicts the instant and owner of a capsule contact on fixed geometry.

* [Requesting district LODs from a moving camera](algorithms/paged-lod-requests.md) - Reuses cell visibility and requests the levels of individual source owners.
* [Publishing district LODs when their data becomes available](algorithms/publishing-paged-lods.md) - Updates affected source owners as the reader verifies and releases models and textures.
* [Keeping render entities when neighbouring objects change detail](algorithms/stable-lod-placements.md) - Preserves unaffected placements and measures GPU geometry during movement.
* [Updating the district camera index on page publication](algorithms/resident-visibility-index.md) - Repairs the render lookup only for cells whose geometry changes.
* [Conservative camera visibility during a district descent](algorithms/district-camera-visibility.md) - Filters the main camera at street level and tracks the GPU work saved.
* [Sharing district collision pages between observers](algorithms/physical-page-reservations.md) - Protects current readers when a physical reservation is reused.
* [Sound around a closed convex object](algorithms/exterior-acoustic-edges.md) - Checks both incident surfaces where a sound path follows an exterior corner.
* [Hearing an impact around city obstacles](algorithms/diffracted-city-contacts.md) - Follows the sound through one or two openings to a moving listener.
* [Selecting and releasing acoustic meshes for district rays](algorithms/paged-acoustic-rays.md) - Loads acoustic geometry only for candidate bindings and shares it across ordered queries.
* [Building acoustic triangle indexes on first use](algorithms/acoustic-surface-index.md) - Avoids preparing distant triangles until a resident sound path reaches their surface.
* [Preparing closed acoustic solids near a sound path](algorithms/acoustic-solid-regions.md) - Finds the neighbouring walls needed to certify a crossed obstacle.
* [Acoustic paths through authored rooms](algorithms/acoustic-room-portals.md) - Finds a feasible polyline through named apertures and bounds its geometric error.
* [Direct sound through city surfaces](algorithms/direct-sound-transmission.md) - Connects acoustic geometry, material energy fractions and the actual received waveform.
* [Reading a moving sound at a fractional delay](algorithms/moving-sound-delay.md) - Measures Doppler pitch and interpolation images in the rendered wave.
* [Sound from city contacts](algorithms/city-contact-sound.md) - Times an impact at a moving listener and shares the same samples between live and recorded playback.
* [Lossless storage of the first district scene](algorithms/coarse-district-storage.md) - Reduces coarse geometry transfer while preserving every byte of the initial GLB.
* [Lossless storage of district collision meshes](algorithms/packed-collision-pages.md) - Preserves ordered positions and triangles with compact corner and indexed records.
* [Continuous approach to a moving lift](algorithms/lift-contact-clock.md) - Forecasts the first rig contacts and refines native sampling around their arrival.
* [Articulated joint frame conversion](algorithms/articulated-joint-frames.md) - Resolves endpoint orientation, signed hinge motion and principal swing ranges in a portable frame.
* [Articulated swing envelope](algorithms/articulated-swing-envelope.md) - Bounds diagonal swing and tests the response against a free joint.
* [Load transfer at first contact with a lift](algorithms/lift-contact-response.md) - Samples the loading of a rig for one tick after the first measured impact.
* [Articulated support footprint](algorithms/articulated-support.md) - Measures where the centre of mass projects relative to upward contacts.
* [Finite segment intersections and missing physical coverage](algorithms/scene-ray-coverage.md) - Reports source faces across object channels and identifies unobserved static terrain.
* [Sharing triangle indexes across object placements](algorithms/instanced-object-rays.md) - Preserves exact world-space crossings and source owners while reusing a model’s triangle tree.
* [Sharing fixed collision shapes away from a moving body](algorithms/native-static-instances.md) - Preserves source contacts while reusing distant native triangle meshes.
* [Planning district collision reads](algorithms/collision-read-planning.md) - Anticipates physical page arrival along the actor trajectory.
* [District content over HTTP](algorithms/district-http.md) - Streams verified files on demand and reuses them across visits.
* [District paging](algorithms/district-paging.md) - Loads urban geometry by distance and verifies support during a descent.

* [Exact client-side prediction](algorithms/exact-client-prediction.md) - A client predicts its own vehicle with no visible correction, because the server applies exactly the steps the client sent and never makes one up.
* [Arcade kart physics](algorithms/arcade-kart-physics.md) - A small kart model with drifting, three mini-turbo levels, crests and tricks, starts and slipstreams, made of a forward speed, a sideways slide, a heading and a height.
* [Capsule contacts with impulses](algorithms/capsule-contacts.md) - Two bodies in the plane, each a capsule with a mass and a moment of inertia, touch — they are pushed apart and exchange one impulse at the contact point, with restitution and friction, so that a knock pushes, slows and spins each by where it lands.
* [Timedemo bench](algorithms/timedemo-bench.md) - A benchmark that draws the same frames on every machine by stepping a deterministic simulation once per frame and drawing off screen.
* [Land around a track](algorithms/land-around-a-track.md) - Hills generated from noise that stay flat along a circuit and fade out at the edge of the modelled land, with a knoll and a lake placed in the room the circuit leaves inside its loop.
* [Sculpted scenery](algorithms/sculpted-scenery.md) - Static scenery built from simple shapes, painted through vertex colours and merged into one mesh per material and patch of land, so that thousands of objects cost a few dozen draws.
* [Aggregate thinning](algorithms/aggregate-thinning.md) - A coarser level of detail for foliage — needles, leaves, blades — made by keeping a random share of its small pieces and growing each kept one so that the crown covers as much as before.
* [Blended solids](algorithms/blended-solids.md) - Organic shapes — a figure, cloth over limbs — made of round cones, ellipsoids and boxes blended by a signed distance field and meshed by surface nets, painted by the piece nearest each point or by paint volumes.
* [Tree cards](algorithms/tree-cards.md) - Woods of thousands of trees for a few thousand triangles — each tree drawn as crossed cards showing pictures of the real model, rendered on the processor at load from its own geometry and textures.
* [Particle pool](algorithms/particle-pool.md) - A fixed pool of camera-facing squares for sparks, smoke and flames, where a particle fades by shrinking so that each kind is drawn in one batch.
* [Far sound field](algorithms/far-sound-field.md) - What is far from a cube heard as an ambisonic field made once at its centre and decoded for each listener through virtual loudspeakers turned with the head, in two bands: the far world costs a listener the same for 32 emitters or 1 024.
* [Sound faces from meshes](algorithms/sound-faces.md) - The faces sound meets from an object's acoustic and collision meshes: triangles paired into quads, faces of one plane that share a side made one sheet (no edge, a reflection that crosses, one scatter), a tree of boxes over them: a surface sounds and costs the same however finely its mesh is cut.
* [Sound ways](algorithms/sound-ways.md) - The ways a sound takes among flat faces — straight, through, round the loudest edge (Maekawa from the Fresnel number, continued into the lit side), mirrored from an image source — chosen by what the ways are, so that nothing steps behind an edge and the faces' order changes nothing.
* [Sound delay lines](algorithms/sound-delay-lines.md) - Each emitter's voice in a ring of its own, each way reading it as far back as it is long, so that delay, Doppler and an echo's own shift need no rule; a moving emitter's trail; ways found in staggered blocks, gliding between, ranked by loudness under a budget.
* [Contact voices](algorithms/contact-voices.md) - Which contacts get one of a few voices — only one that reaches the ears by a real way, and only from a voice heard less — with each contact's strength and grain drawn from whose contact it is.
* [Feedback delay reverberation](algorithms/feedback-delay-reverb.md) - Eight prime-length lines mixed by a Householder matrix, each keeping per turn what a decay at 250 Hz, 1 kHz and 4 kHz asks of its three bands, solved for gently parted bands and capped at twice the longest decay.
* [Loudness mastering](algorithms/loudness-mastering.md) - A render made into a file without falsifying it: faded ends, one gain to a BS.1770 loudness capped by a true-peak ceiling, triangular dither, 16 bits.

## Oracles

* [The render bench](oracles/render-bench.md) - Proves that a fidelity tier holds its frame-time budget on a machine, and records what was drawn.
* [Pedestrian impact](oracles/pedestrian-impact.md) - Measures contact time, penetration and rebound on a moving or stationary bonnet.
* [Physics golden file](oracles/physics-golden.md) - Proves that the kart physics gives the same bits natively and in WebAssembly, which exact prediction relies on.
* [Soundscape verify](oracles/soundscape-verify.md) - Proves that a render of a scene is reproducible, does not clip, is as loud as asked, keeps a side source on its side and swaps it when the head turns, and hears the train come, pass on the right and go.

## Elsewhere in the repository

Decisions, design notes and research surveys are in `design/` and `research/` at the root of the repository; they join this bundle in a later version of the profile.
