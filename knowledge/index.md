---
okf_version: "0.2"
---

# osage knowledge

What osage knows about its own modules: their architecture, the algorithms and techniques they use, and the checks that prove their claims. Written to be read without the code, and reused in other engines. Format: [Open Knowledge Format v0.2](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md) with the [osage engineering profile](profile.md).

## Start here

* [osage engineering profile](profile.md) - The types, fields and sections every card follows, and how the bundle is checked and federated.
* [Change log](log.md) - What changed in this bundle.

## Modules

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
* [osage-region-link](modules/region-link.md) - The message contract between the replication gateway and a region server, the same in one process and across machines.

## Rendering techniques

* [Sky and sun](techniques/sky-and-sun.md) - A physically based atmosphere and sun whose cost is cut per tier, down to a flat sky with fog.
* [A photographed sky](techniques/panorama.md) - An unclipped HDR photograph of the sky drawn as the sky, lighting the scene with its sun taken out, and calibrated by its own sun so that the sky has its true brightness.
* [Ground that varies](techniques/ground-variation.md) - A ground texture that never repeats — its grain sampled by hexagonal cells shifted and turned by where they are, multiplied by clumps and patches, and on a road by its history along its length (seams, patches, cracks, stains); octaves smaller than a few pixels fade out.
* [Light from the sky](techniques/sky-light.md) - How the sky and the ground light what the sun does not, chosen per tier because lighting from a cubemap costs 2 to 3 ms on an integrated GPU whatever its size.
* [Sun shadows](techniques/sun-shadows.md) - Cascaded shadow maps for the sun, with cascades, reach and resolution chosen per tier from measurements.
* [Ambient occlusion](techniques/ambient-occlusion.md) - Screen-space ambient occlusion from T3, kept out of T2 by its measured cost and by a defect on integrated graphics.
* [Post-processing and anti-aliasing](techniques/post-processing.md) - HDR, exposure, the world's look, anti-aliasing and bloom, chosen per tier so that the lower tiers need no depth prepass.
* [Resolution and upscaling](techniques/resolution-and-upscaling.md) - The scene is drawn into a picture whose size the tier decides, then stretched to the window under a sharp interface.
* [Holding a frame rate](techniques/frame-pacing.md) - Given the frame rate a person wants, the picture climbs down a ladder of cheaper pictures until frames come at that rate, and back up when there is room.

## Algorithms

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

## Oracles

* [The render bench](oracles/render-bench.md) - Proves that a fidelity tier holds its frame-time budget on a machine, and records what was drawn.
* [Physics golden file](oracles/physics-golden.md) - Proves that the kart physics gives the same bits natively and in WebAssembly, which exact prediction relies on.

## Elsewhere in the repository

Decisions, design notes and research surveys are in `design/` and `research/` at the root of the repository; they join this bundle in a later version of the profile.
