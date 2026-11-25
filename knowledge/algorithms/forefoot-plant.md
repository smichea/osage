---
type: Algorithm
title: Planting the forefoot during an animated stride
description: Holds a low forefoot at a horizontal contact point using hip and knee rotation, then releases it as the shoe rises.
resource: https://github.com/smichea/osage/blob/main/crates/osage-humans/src/stance.rs
licence: MIT OR Apache-2.0
status: draft
tags: [animation, characters, locomotion]
sources:
  - id: replay
    title: Asset-backed pedestrian stride replay
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/examples/pedestrian_stride.rs
generated: { by: "OpenAI/Codex", at: "2026-11-24T22:02:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

Matching clip cadence to ground speed prevents most forward foot sliding. Near toe-off, the backward velocity of the animated forefoot can change while the foot is still low enough to appear planted.

# Idea

Remember the world-space horizontal position where each forefoot entered stance. Correct the animated ankle position with a two-link leg solve. As the foot lifts, blend out the correction.

# Method

Read the four animated joints from thigh to forefoot after animation evaluation. The model origin supplies the level ground height. At unit scale the walking forefoot enters stance at a height of 35 mm and the jogging forefoot at 22.5 mm. Transform each correction into the upper joint parent's coordinates. The hip, requested ankle position and two segment lengths define a triangle. Keep the knee bend direction of the sampled pose, rotate the hip and knee and counter-rotate the ankle to preserve shoe orientation. Clamp the reach to the interval the two links can span. Limit the contact offset to 12 cm at unit model scale. Above the contact band, reduce the last correction smoothly to zero over 4 cm of lift.

A world can install `FootGround` with its own maximum step height and support query. Sample at the horizontally corrected forefoot and use the animated ground-relative height to place the shoe above that surface. Blend vertical correction out over eight centimetres above the stance band and preserve clearance against a higher surface during swing. For a lower target, project the hip-to-target vector onto the ground plane. Its horizontal distance and the two link lengths give the maximum vertical separation. Lower the visual armature by the largest shortfall across the legs. All limbs then solve relative to that armature. A descending target can lower the body immediately. The armature recovers at up to one unit-scale metre per second. D-224 records the choice.

A configured ankle can carry finite rigid sole samples. For each sample, transform it with the animated ankle, add the horizontal planting offset and query the surface at that position. Use the largest required lift together with the central target. When the central target has no support, a supported sole point can still request upward clearance (D-226).

A rig can use a [sampled gait cycle](/algorithms/cyclic-foot-contact.md) to plan its footprint before the world-space contact correction.

# Invariants

On a level plane, an unplanned foot changes only joint rotations. A planned low footprint may also lower the visual rig to reach its horizontal target. With sampled terrain it also translates the visual armature when a lower support requires more reach. It preserves clip time, actor trajectory and segment lengths. Restore the armature translation and the three source rotations before the next animation evaluation even if the world has removed the component. Each foot has its own contact point and release weight.

# Cost

Each opted-in actor stores one contact record per configured limb. Each frame resolves the actor and limb parents through the transform hierarchy and runs one analytical solve per corrected limb, plus any optional skinned-sole refinement.

# Failure modes

The height sampler handles stationary steps. Shoes keep their animated orientation on slopes. Moving supports need contact points in support space. An instantaneous floor discontinuity can produce a visible one-frame lift. A change of band, an unsupported animation blend, a frame displacement beyond one quarter of model scale in metres, or a vertical displacement over one millimetre at unit scale on the fallback plane resets the contacts. A missing or out-of-range support releases that foot. At full leg extension a requested ankle position can be unreachable. A straight chain uses a bend direction from the upper joint frame. Equal links folded onto the hip retain the current upper-link direction. Zero-length segments disable the correction.

Blended skin vertices can move relative to the ankle. Rigid sole points bound only their sampled positions, and a requested correction can exceed the available leg reach.

[Skinned sole clearance](/algorithms/skinned-sole-clearance.md) follows blended joints and can query the overlap of whole sole and terrain triangles (D-227).

# Alternatives

Setting playback from displayed horizontal travel is sufficient for a crowd at a distance. Nearby actors can additionally opt in to foot planting.

# Oracle

Ville-Paris replays the same root trajectory with unit cadence, matched cadence and planted feet, exposes both forefeet after transform propagation, and checks that removing the component returns the actor to the matched pose. The solver tests check reachable and unreachable targets, both segment lengths and shoe orientation. Use the replay’s `--curb` option to measure ascent and descent over a 15 cm pavement, and `--phase` to vary the leading foot.[^replay]

The four-legged replay loads the [model declaration](https://github.com/smichea/osage/blob/main/worlds/ville-paris/examples/fixtures/four-foot.gltf) and drives a diagonal trot across a 12 cm raised strip. It records both actors and every pad before and after crossing. [Replay](https://github.com/smichea/osage/blob/main/worlds/ville-paris/examples/quadruped_stride.rs)

For the authored fox, build `cargo build -p osage-ville-paris --example model_stride` and run `node tools/animation/fox-stride.mjs 0.25` from the workspace root with the asset manifest’s Fox.glb and workspace npm dependencies installed. Repeat with phases 0, 0.5 and 0.75. The runner samples the original Walk and Run clips at 60 Hz and applies the native solver in Bevy, then skins every selected paw vertex. Five rigid sites per ankle are used for correction. Measurement includes all drawn vertices with majority paw influence, including those blended across joints. Skin-to-ground clearance is measured at each vertex's X/Z. A separate check on the pedestrian uses `pedestrian_stride --curb --soles`. [Fox replay](https://github.com/smichea/osage/blob/main/tools/animation/fox-stride.mjs)

# Reuse

Attach `PlantFeet` to a [human](/modules/osage-humans.md) with an upright rig. Supply `FootGround` for stationary terrain of varying height. The world still owns the movement policy and the requested gait.

[^replay]: [Pedestrian stride replay](https://github.com/smichea/osage/blob/main/worlds/ville-paris/examples/pedestrian_stride.rs).
