# Knowledge Update Log

## 2026-11-11
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
