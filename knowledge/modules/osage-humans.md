---
type: Module
title: osage-humans
description: People from rigged models of the asset manifest, dressed by the joints that move each part of the body, animated from a library on the same skeleton.
resource: https://github.com/smichea/osage/tree/main/crates/osage-humans
licence: MIT OR Apache-2.0
status: draft
tags: [content, gltf, skinning, animation, characters]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-15T01:20:00Z" }
---

# Purpose

Put people in a world, animated and dressed, from open models instead of sculpting them: a body, its hair, a library of moves, all rigged to one skeleton (Quaternius's Universal Base Characters and Universal Animation Library, CC0, D-122).

# Concepts

- **Human:** an entity with a body (man or woman), a skin tone, a hairstyle and its colour, an outfit, the move it plays, its pace and where in the move it starts.
- **Skeleton:** what a human has once built: one entity per bone, named as in the model (`hand_r`, `Head`, …), to hold things in a hand or look at a bone.
- **Outfit:** the colours of a top, a bottom and shoes.
- **Move:** a clip of the library, by its name there (`Idle_Loop`, `Walk_Loop`, `Sitting_Idle_Loop`, `Idle_Torch_Loop`, …).
- **Installed:** whether the people's files are in the manifest's content; without them, a world draws its people otherwise.

# Architecture

A Bevy plugin. At start it loads both bodies, the light skins' textures, the hairstyles and the move library. Once all have loaded with their textures, it makes once: an animation graph holding every clip of the library; for each body, which part of the body each vertex belongs to; and for each body and tone, a dressed material.

**Dressing by joints.** Each part of a rigged body is moved by its own bones, so the joint that weighs most on a vertex says what the vertex wears: head, neck and hands are skin; spine, clavicles and arms a top; pelvis and legs the bottom; feet shoes. The body's triangles are drawn in their texture space, each in the part most of its corners belong to, and the parts spread four texels past the seams so that filtering brings no skin into the cloth. On the texture of each body and tone, the worn texels become a light grey keeping a little of the skin's shading (folds, the shadow under an arm); on the normal map, flat; on the roughness map, rough and without metal. The dressed textures are 1024² with their chain of smaller levels. An outfit is the body's mesh with its vertex colours set from the part each vertex wears (white on skin), shared by every human dressed alike: the material multiplies the grey fabric by them.

**Building a human.** Its skeleton is spawned from the model's nodes under the human, each bone a target of the moves by its path of names from the armature — the same paths as in the move library, which shares the skeleton, so its clips play on it unchanged. The body, its eyes and its eyebrows are skinned to the skeleton; the hair, rigged in a file of its own to a copy of the same skeleton, is skinned to this one by matching bone names. Eyebrows and hair take the hair's colour. The human then plays its move, from its phase, at its pace; a changed move crossfades over 0.35 s. Updating the pace of the current move adjusts its playback speed at its current clip time, including pausing at zero and resuming from the same pose.

# Interfaces

`HumansPlugin`, `Human { body, tone, hair, hair_colour, outfit, act, pace, phase }`, `Body::{Man, Woman}`, `Tone::{Light, Dark}`, `Hair::{Buzzed, BuzzedShort, Parted, Long, Buns}`, `Outfit { top, bottom, shoes }`, `Move(name)` and its constants, `Skeleton { armature, bones }`, `installed(root)`.

The standalone `FootPlantPlugin` accepts a `FootRig` bound to disjoint entity chains. A gait supplies `ContactHeight` and the actor opts in with `PlantFeet`. The human adapter identifies its two named legs and supplies the band for walking and jogging. Other models can bind contacts from the [object tree](/modules/osage-objects.md) and animate their own gait. Optional `SolePoints` on the ankle query ground under the authored foot area (D-226).

A model can bind [weighted sole geometry](/algorithms/skinned-sole-clearance.md) from the loaded skin hierarchy via `SoleSkin::from_actor` (D-227). The world may also supply terrain face queries.

A world can attach `PlannedHumanSteps` to a human. The adapter takes both contact paths from the built body, samples the installed Bevy clip along their full ancestry at 180 phases, and shares the resulting tables across instances with the same body and gait. The soles are bound from the rendered, clothed body. While exactly one supported clip plays, the live player supplies its seek time. During an idle/walk/jog crossfade the mixed pose keeps its ground checks. Other actions release the contacts. [Native adapter](https://github.com/smichea/osage/blob/main/crates/osage-humans/src/stance/human_cycle.rs)

# Invariants

- `Move::ground_speed_m_s` returns the forward speed of the unit-scale walk and jog at nominal cadence. The calibration uses the median backward forefoot velocity near the ground. A world can divide displayed speed by this value to set `Human.pace` (D-222).
- A human is built only once every model has loaded; models that fail to load are reported once and nothing is built, never waited for forever.
- Every bone's part of the body is decided by its name alone, the same at every run. Tested.
- A dressed texture's levels halve down to one texel. Tested.
- Humans dressed alike share one mesh; humans of the same body and tone share one material.

# Knowledge

- [Reversed loop crossfades](/algorithms/reversible-loop-crossfade.md) preserve the live clock of a fading walk, jog or idle when the world requests it again.

- [Cyclic foot contact](/algorithms/cyclic-foot-contact.md) plans a footprint across a low interval of a repeating clip.

- [Forefoot planting](/algorithms/forefoot-plant.md) is enabled per actor with `PlantFeet` for walking and jogging. Worlds can supply their collision surface through `FootGround` to support different heights under the two feet (D-223, D-224).

- Used by [kart-native](/modules/kart-native.md): the marshals at the bends, the mechanics in the pits.
- Content: `tools/quaternius.py` takes the files out of the publisher's two zips, downloaded by a person (assets/manifest.toml).

# Reuse

Any Bevy app with the same models installed under the manifest's content; the dressing by joints works for any rigged body whose bone names say what they move. Permissive licence.

# Limits

One cut of clothes: long sleeves and trousers; no garment of its own geometry (collars, skirts, caps). Boundaries between parts follow the mesh's triangles, so a cuff or a collar is as ragged as the mesh is coarse. Each human has its own skeleton (65 bones): fine for tens of people near the camera, not for a stand of thousands.
