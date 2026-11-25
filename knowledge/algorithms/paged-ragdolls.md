---
type: Algorithm
title: Keeping articulated bodies in contact with streamed terrain
description: Loads terrain for the reach of each limb while a single actor persists across district page changes.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/solver.rs
licence: MIT OR Apache-2.0
status: draft
tags: [paris, physics, collision, streaming, audio]
sources:
  - id: solver
    title: District actor, terrain and stable surface identities
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/solver.rs
  - id: planning
    title: Ballistic paging requests
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/planning.rs
  - id: attack
    title: Measurement of articulated impact voices
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/impact/physical.rs
generated: { by: "OpenAI/Codex", at: "2026-11-24T04:42:57Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A falling person's hand can reach a neighbouring tile before the pelvis or feet arrive. A terrain request based solely on the root can miss that hand. Replacing a temporary floor while the actor rests on it can also make one continuous contact appear to hit twice.

# Idea

Keep a single articulated simulation alive. Publish or remove terrain in that simulation. Use each limb's reach to choose pages and the solved poses to draw the person. Carry the authored surface identity into each physical and acoustic measurement.[^solver]

# Method

The city uses the resident actor's parent-first rig, masses, inertias, joint frames, limits and optional animation handoff. Each limb provides a planning trajectory with its own centre, velocity and enclosing radius. All trajectories share the actor's reservation identity. Their earliest demands for a tile share the same read. A second, one-tick check uses the actual capsule axis, linear motion, gravity and angular velocity to bound the next sweep. An unresolved region that meets one sweep can hold the simulation until the required geometry arrives.[^planning]

For a multi-body Rapier rig moving among static meshes, both loaders create the rig before installing the terrain. The creation order can change the solver’s internal contact pair ordering and the subsequent tumble even when all triangles have arrived. The solver identifies the surface at each substep. A mapping from the temporary collider handle to the pair of authored node and surface name stays valid across replacement or reallocation of terrain. The renderer places the parts using the resulting poses. For imported rigs, geometry attached to a bone follows its nearest physical ancestor. A positive measured reaction can excite the surface material. The acoustic receiver dates the reaction in audio frames and combines simultaneous reactions on the same surface before launching the voice.[^attack]

The support catalogue keeps the original shape slot and owner of every certified surface. When the full page arrives, the loader verifies all its data and compares ordered vertices and indices, owner and physical coefficients at that slot. An exact match retains the native collider, its active contacts and the solver's accumulated impulses. A shape with an additional wall receives a new collider before the old one is removed. Across two independently loaded supports, an identical replacement at tick 47 preserves each of the 15 solved poses through tick 78. The mixed-page regression sends a hand into an added wall and follows the page through eviction. [Page handoff oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-collider-handoff.py)

The cooker keeps a compact source collider together when it straddles visual tiles. A larger mesh can group original triangles by centroid. Each physical page covers the actual extent of its triangles, so a limb or moving support can need a page whose storage cell is far away. [Source topology oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-collision-topology.py).

For a repeated small solid whose 12 triangles form a closed box, the catalogue can encode the original order of every vertex and triangle once. Per placement it supplies a pattern index, owner slot and two exact corners. At startup the reader checks each reconstructed upward face and the multiplicities of all other triangle bounds against the ordinary safety catalogue. On full page delivery it compares source identity, material and the ordered float32 and index buffers. An exact match preserves the original native collider throughout the fall. This also preserves contact against a side or edge. [Persistent support oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-corner-support.py).

# Invariants

Terrain changes preserve the dynamic bodies and their joints. Each contact retains its limb and substep when it crosses the terrain adapter. Multiple limbs can reserve a single physical file for the same actor. A completed acoustic read can make an existing path audible only from the first receiver sample still writable.

A replay can also supply one freely moving cuboid. The district uses the resident body's mass, half extents, rotation and velocity. The cuboid reserves terrain for its own trajectory, including a one-tick bound on the rotated corners. When it participates, the resident registers initial terrain before the cuboid and actor colliders. The district establishes that contact order once at startup while keeping its native bodies, joints and static page handles. A contact on the cuboid carries its own surface identity. Its closing speed includes the motion of both bodies at the witnessed point. [Moving-support physics oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-paged-bonnet.py).

A district replay can add a level kinematic lift to the Rapier terrain world before the first page arrives. The platform is owned by the route and has the stable surface ID `lift`. Terrain installation and eviction manage the cell’s static collider handles. At startup the adapter matches the resident ordering of the active colliders and preserves the route ID across a native handle change. The grounding check on the lift uses the vertical speed of the actor relative to the solved platform. A passenger carried upward can therefore remain supported while both world-space velocities exceed the 0.1 m/s grounding threshold. Each admitted physics tick advances both passenger and platform. During a terrain hold, both stay at the same simulation time.

For the next tick the clearance check uses the minimum and maximum platform heights over the actual interval, including a reversal between endpoints, plus the platform half-extents. The two-tick proxy query uses the same rule with a longer interval. When a rider is on the top face or can reach it during the interval, the upper bound for the rider also includes the fastest upward displacement prescribed to the lift over its native substeps. This allows the guard to request geometry above the passenger’s head during acceleration. Long-range requests use a sphere enclosing the full stroke. This conservative forecast can retain extra terrain around a tall lift.

# Cost

Space for current poses, previous poses and sweep bounds grows linearly with the number of limbs. Predicting demands adds one trajectory query per limb. The solver cost depends on joint count and actual contacting pairs. Per-limb pose serialization is optional.

# Failure modes

The one-tick bounds extrapolate current motion. A new constraint impulse during the tick can redirect a distal limb beyond its predicted sweep. A conservative support proxy covers only its certified triangles. Clipping a wide road for a page changes the triangles under a cuboid. Adding side walls when a detailed road arrives also replaces the contact manifold of that road. Both can change the final tumble even when all initial contacts select the same feet and substep.

# Alternatives

A single capsule remains the default for a cheaper falling actor. A resident replay can load the entire terrain before starting the same articulated actor.

# Oracle

Run the same rig, poses and fixed timestep against the source object and cooked district. Compare every limb in free flight and identify the reacting limb, surface and substep on first contact. Include a platform just within the reach of a hand and a neighbouring route which misses it. Compare the render with and without an attached primitive while retaining its physical body. In the hand-over-edge case the hand reaches the next tile at 0.8375 s. A fall straight onto the platform reaches it with both feet at 0.641667 s at both 20 Hz and 60 Hz. The 36 resident and district comparisons plus the unprotected controls report initial poses, contact-point differences and pose drift after impact. [Physics oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-paged-ragdoll.py).

The Bir-Hakeim lift carries its passenger for 600 simulation steps at 60 Hz while a second physics observer triggers five detailed page installs and two evictions. All 601 pairs of body poses and all 1,200 contact witnesses agree with the source-scene Rapier run at native float32 precision. The maximum foot displacement in the platform frame is 0.152 mm. [Paged lift comparison](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-paged-lift.py)

In the windowed room the two feet produce one material attack. At 60 Hz and 90 Hz the first impact behind the offset window has the same stereo samples as the resident reference. With the same actor-first construction order, moving the ears beside the feet also gives identical stereo samples. The full two-second fall has twelve measured material attacks and 96,000 matching stereo frames. The five acoustic meshes total 560 bytes and each is read once even with an added 50 ms service delay. The 90 Hz control measures one additional load 133 audio frames after the initial reaction, across a simulation tick boundary. [Audio oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-paged-ragdoll-audio.py).

# Reuse

Use `--paging --actor human` for the city mannequin or `--actor-rig FILE.glb` for an authored rig. Add `--trace-body` to record each solved pose and contact. Add `--impact-wav FILE.wav` for stereo output. [District paging](district-paging.md) · [Material impact voice](paged-impact-sound.md).

[^solver]: [District terrain adapter](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/solver.rs).
[^planning]: [Trajectory planning](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/planning.rs).
[^attack]: [Physical attack measurement](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/impact/physical.rs).
