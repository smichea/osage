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

## Rendering techniques

* [Sky and sun](techniques/sky-and-sun.md) - A physically based atmosphere and sun whose cost is cut per tier, down to a flat sky with fog.
* [Light from the sky](techniques/sky-light.md) - How the sky and the ground light what the sun does not, chosen per tier because lighting from a cubemap costs 2 to 3 ms on an integrated GPU whatever its size.
* [Sun shadows](techniques/sun-shadows.md) - Cascaded shadow maps for the sun, with cascades, reach and resolution chosen per tier from measurements.
* [Ambient occlusion](techniques/ambient-occlusion.md) - Screen-space ambient occlusion from T3, kept out of T2 by its measured cost and by a defect on integrated graphics.
* [Post-processing and anti-aliasing](techniques/post-processing.md) - HDR, exposure, the world's look, anti-aliasing and bloom, chosen per tier so that the lower tiers need no depth prepass.
* [Resolution and upscaling](techniques/resolution-and-upscaling.md) - The scene is drawn into a picture whose size the tier decides, then stretched to the window under a sharp interface.

## Algorithms

* [Exact client-side prediction](algorithms/exact-client-prediction.md) - A client predicts its own vehicle with no visible correction, because the server applies exactly the steps the client sent and never makes one up.
* [Arcade kart physics](algorithms/arcade-kart-physics.md) - A small kart model with drifting and two mini-turbo stages, made of a forward speed, a sideways slide and a heading.
* [Timedemo bench](algorithms/timedemo-bench.md) - A benchmark that draws the same frames on every machine by stepping a deterministic simulation once per frame and drawing off screen.
* [Land around a track](algorithms/land-around-a-track.md) - Hills generated from noise that stay flat along a circuit and fade out at the edge of the modelled land, with a knoll and a lake placed in the room the circuit leaves inside its loop.
* [Sculpted scenery](algorithms/sculpted-scenery.md) - Static scenery built from simple shapes, painted through vertex colours and merged into one mesh per material and patch of land, so that thousands of objects cost a few dozen draws.
* [Particle pool](algorithms/particle-pool.md) - A fixed pool of camera-facing squares for sparks, smoke and flames, where a particle fades by shrinking so that each kind is drawn in one batch.

## Oracles

* [The render bench](oracles/render-bench.md) - Proves that a fidelity tier holds its frame-time budget on a machine, and records what was drawn.
* [Physics golden file](oracles/physics-golden.md) - Proves that the kart physics gives the same bits natively and in WebAssembly, which exact prediction relies on.

## Elsewhere in the repository

Decisions, design notes and research surveys are in `design/` and `research/` at the root of the repository; they join this bundle in a later version of the profile.
