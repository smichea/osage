---
type: Algorithm
title: Sharing district collision pages between observers
description: Holds one physical page for its current observers and rejects a late completion after its slot is reused.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/observers.rs
licence: MIT OR Apache-2.0
status: draft
tags: [paris, streaming, physics, networking]
sources:
  - id: source
    title: Verified immutable input and shared tickets
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/source.rs
  - id: replay
    title: Two-observer replay on disk and HTTP
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-physics-observers.py
generated: { by: "OpenAI/Codex", at: "2026-11-23T17:05:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

Two actors can approach the same piece of a street. One can leave before the physical geometry arrives. A different page can then occupy the slot originally reserved for that actor's cancelled demand.

# Idea

Share the immutable page according to the current set of observers. Keep observer identity, page reservation and transport request distinct. The original model node remains the owner of each physical surface.

# Method

The existing transport already coalesces live tickets for the same digest-named file, uses four workers with an eight-entry channel, and verifies cached HTTP bodies before use. The existing physical scheduler deduplicates its queue by immutable tile index. The new reservation records which actors still need that tile and which lifetime of a reusable slot can accept its result.[^source]

The primary observer is the simulated Rapier capsule. The optional observer file supplies kinematic actors' time-stamped feet, centres, velocities and support states. The reader interpolates neighbouring samples and computes physical interest from all active actors. An observer slot can serve successive, nonoverlapping tracks. Each occupant gets a new generation. The visual camera continues to use its own selector.

A physical page owns one reservation while any observer needs it or retains it within the eviction margin. The scheduler unions the observers' immediate and predicted needs and admits one read of that page. Each observer queries the physical hierarchy out to the eviction margin. The reader records the closest observer for each nearby page and merges the predicted and explicitly required page IDs. One page can therefore serve several limbs without separate reads. [Physical page search](/algorithms/collision-read-planning.md) A departing observer changes the audience of an in-flight read. When the final observer leaves, the page releases the reservation and its slot becomes available. Reassigning a slot increments its generation.

An in-flight read retains the page ID and the reservation token it received at admission. A paced physical read continues to occupy one transfer admission until its ticket completes, including when the destination slot has already been reused. The workers verify the incoming file digest before marking their ticket ready. The main thread compares page, slot and generation with the current reservation, validates the physical shapes and owners of a matching response, and installs its colliders. A stale completion cannot install a collider or clear the replacement's reservation. A valid response can still populate the immutable HTTP disk cache.[^source]

# Invariants

Retiring one observer cannot evict a still-needed collider. Only the primary actor's clearance check can hold its motion. A second observer's urgent physical needs participate in the same channel budget. The normal trajectory scheduler reserves the fourth admission for a next-step blocker. Storage identity stays in the content digest. The identity of a roof or road stays in the source tree. [Priority and budgets](/algorithms/collision-read-planning.md) · [Physical geometry](/algorithms/district-paging.md)

# Cost

In the earlier split-road district, an 8 m fixed radius on the road approach means that replaying a second copy of the primary actor's motion needs one 3,993-byte physical read, just like the single-observer run. In the cancellation replay the street, abandoned roof page and replacement span use 3,993, 4,314 and 4,895 bytes. The cold disk and HTTP sources each deliver the three physical files once, totalling 13,202 bytes. The reader installs the street and the span, totalling 8,888 bytes. Two reservation slots coexist with three outstanding physical reads. Resident and installed byte counts measure the encoded pages. Rapier additionally allocates its shapes and acceleration structures. The next HTTP process can verify all three files from its local cache with zero HTTP content body bytes.

Across 15 interleaved trials on Linux CPU 2, the median paired increase relative to the single-actor path was 6.77 ms for primary-only reservation tracking and 10.14 ms with the duplicated trajectory, over 135 ticks. That measures total native process CPU time including startup, observer-file parsing, event collection and receipt encoding.

# Failure modes

Dropping the last observer cannot undo I/O that has already started. Such bytes count against the transport budget even when their destination is stale. If a page is abandoned and then requested again before its earlier completion, the new reservation waits for the old read to retire and can start a fresh lookup on the following tick. A verified HTTP cache entry can supply that lookup. The optional observers follow supplied kinematic state. These observers inherit the primary capsule's conservative prediction extent and fall-speed limit. Their own dynamics and collisions between actors require their respective simulators.

# Alternatives

A fixed immutable tile index can identify a pending read throughout a single-actor replay. Once a bounded reservation slot can represent different pages over time, the slot needs a generation or another unique lifetime identifier to distinguish those uses.

# Oracle

The benchmark places the simulated actor and a moving observer on the same road page. Both request it at tick 0. The observer leaves for a second page at tick 8 while the road is still in flight. At tick 20 another observer generation reuses its slot and requests a third page. The third page becomes ready before the abandoned second response arrives. The old response carries the original generation and cannot install. A local server enforces the ordering on HTTP. Named pipes enforce it independently on disk. The benchmark checks every physical digest, payload length and number of actual deliveries.[^replay]

The primary actor reaches the same road face at tick 66 in every run. Its 135 sampled states match the solo replay exactly. Duplicating the original trajectory also preserves all four physical reads, their admission and publication ticks, and their 14,974 bytes with the normal predictive scheduler on a 200 ms simulated link. In the split-road district the full geometry and support directory identify node 146 in both cells. The current cooker keeps the road together. The oracle then uses a 4 m radius to exclude an island underneath the road and places the replacement observer over the western quay. Each current cold run, on disk or HTTP, verifies 181 physical bytes and installs 91. Both layouts reproduce the same 135 primary states as their respective solo replay. A new HTTP process revalidates the same index and consumes the cached bodies. A second cold HTTP run recycles the same page and reservation slot. The roof's first observer leaves at tick 20, and the next one returns to the roof at tick 40. The old response still has the first generation. The reader discards it and verifies the digest from local cache for the new generation on the next lookup. Both lookups succeed, with one 4,314-byte body from the server and one collider installation. A Rust test also exercises both generations directly.

# Reuse

Pass `--physics-observers FILE.json` on the ordinary `ville-paris --paging` route. Version 1 uses a `tracks` array. Each entry supplies a positive `slot`, inclusive `start`, exclusive `end`, and ordered `keys` covering the interval's endpoints. Each key supplies `tick`, `foot_m`, `center_m`, `velocity_m_s` and optionally `supported`. Reservation events and per-digest verified byte counts appear under `physics_observers` in the receipt. Run `python worlds/ville-paris/check-physics-observers.py` to reproduce the disk, HTTP and cache checks. The optional `--cpu N` also times the whole native process on a single Linux CPU over 15 interleaved samples for each observer mode.

[^source]: [Verified immutable input and shared tickets](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/source.rs).
[^replay]: [Two-observer replay on disk and HTTP](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-physics-observers.py).
