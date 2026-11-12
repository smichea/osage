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
* [osage-kart-native](modules/kart-native.md) - The native client of the kart world, built on Bevy, and the testbed of the rendering programme with its benchmark.
* [osage-content](modules/osage-content.md) - Content from the asset manifest made ready to draw — textures with full mipmaps and filtering, photographs of the sky decoded to linear light.
* [osage-procedural](modules/osage-procedural.md) - Procedural geometry — shapes sculpted, painted and merged per material and patch of land — and a repeatable generator, so that what is generated is the same on every machine.
* [osage-props](modules/osage-props.md) - Models of the asset manifest placed many times, drawn in batches, each set seen up to its own distance.
* [osage-kart-land](modules/kart-land.md) - The land around the kart circuit as geometry — flat where karts drive, hills beyond, a knoll and a lake inside the loop — for whoever draws, dresses or plants it.
* [osage-kart-roadside](modules/kart-roadside.md) - What stands along the kart circuit when the asset manifest's models are there — tyre walls, grass, wild plants, rocks and pines — placed the same way on every machine.
* [osage-kart-model](modules/kart-model.md) - A racing kart and its driver modelled to scale from simple shapes, in a team's colours, each finish — varnish, satin, rubber, steel, alloy, chrome, cloth, visor — its own material.
* [osage-kart-crowd](modules/kart-crowd.md) - Spectators sitting in a grandstand, sculpted as whole figures in a few poses and shared, for a world to dress in its own clothes.
* [osage-kart-items](modules/kart-items.md) - Paddock objects for the kart world — racks of crates that hand out a nitro canister, a tyre to throw, a cone or an oil patch to leave behind, drawn to help the back of the field — and when an AI driver uses them; deterministic, run by the world.
* [osage-kart-racecraft](modules/kart-racecraft.md) - How the kart world's AI drivers race — a temperament each, braking for bends, a racing line, overtaking or waiting, defending, mistakes and grudges after a knock — deterministic, for the server's bots and the bench.
* [osage-kart-sound](modules/kart-sound.md) - A kart's sound synthesised from its state — a two-stroke engine following the revs and the throttle, tyres squealing as they slide, kerbs drumming, grass, wind, knocks — sample by sample, deterministic.
* [osage-region-link](modules/region-link.md) - The message contract between the replication gateway and a region server, the same in one process and across machines.

## Rendering techniques

* [Sky and sun](techniques/sky-and-sun.md) - A physically based atmosphere and sun whose cost is cut per tier, down to a flat sky with fog.
* [A photographed sky](techniques/panorama.md) - An unclipped HDR photograph of the sky drawn as the sky, lighting the scene with its sun taken out, and calibrated by its own sun so that the sky has its true brightness.
* [Ground that varies](techniques/ground-variation.md) - A tiled ground texture multiplied in world space by clumps a metre or two across and by drier and lusher patches tens of metres across, so that grass never reads as a carpet; octaves smaller than a few pixels fade out.
* [Light from the sky](techniques/sky-light.md) - How the sky and the ground light what the sun does not, chosen per tier because lighting from a cubemap costs 2 to 3 ms on an integrated GPU whatever its size.
* [Sun shadows](techniques/sun-shadows.md) - Cascaded shadow maps for the sun, with cascades, reach and resolution chosen per tier from measurements.
* [Ambient occlusion](techniques/ambient-occlusion.md) - Screen-space ambient occlusion from T3, kept out of T2 by its measured cost and by a defect on integrated graphics.
* [Post-processing and anti-aliasing](techniques/post-processing.md) - HDR, exposure, the world's look, anti-aliasing and bloom, chosen per tier so that the lower tiers need no depth prepass.
* [Resolution and upscaling](techniques/resolution-and-upscaling.md) - The scene is drawn into a picture whose size the tier decides, then stretched to the window under a sharp interface.
* [Holding a frame rate](techniques/frame-pacing.md) - Given the frame rate a person wants, the picture climbs down a ladder of cheaper pictures until frames come at that rate, and back up when there is room.

## Algorithms

* [Exact client-side prediction](algorithms/exact-client-prediction.md) - A client predicts its own vehicle with no visible correction, because the server applies exactly the steps the client sent and never makes one up.
* [Arcade kart physics](algorithms/arcade-kart-physics.md) - A small kart model with drifting and two mini-turbo stages, made of a forward speed, a sideways slide and a heading.
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
