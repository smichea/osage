---
type: Algorithm
title: Planning district collision reads
description: Schedules detailed physical surfaces along a moving capsule's path using estimated service time.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/planning.rs
licence: MIT OR Apache-2.0
status: draft
tags: [paris, streaming, physics]
sources:
  - id: replay
    title: Bir-Hakeim descent fixtures
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/fixtures/descent.json
  - id: queue
    title: District streaming queue and admission
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/reader.rs
generated: { by: "OpenAI/Codex", at: "2026-11-23T16:52:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A falling capsule reaches 55 m/s. A page first requested 64 m from the actor has roughly 1.2 seconds to arrive. Increasing that radius in every direction can also load surfaces on neighbouring streets. A separate camera can be hundreds of metres away from any physical threat to the actor.

# Idea

Ask whether the actor can touch a collision page by the time a read could complete. Request nearby pages as usual. Also request a more distant page when the actor can reach it before the predicted read completes, including a small time margin. Recompute from the simulated body on each tick.

# Method

Start at the Rapier body's centre with its current linear velocity. Horizontal velocity is constant during one forecast. Height follows gravity until the configured terminal speed, then falls linearly.[^replay] Immediately after a supporting contact with vertical speed below 0.1 m/s, project the measured velocity without gravity until the next physics step supplies a fresh contact. Enlarge each physical page's bounds by the capsule radius plus the half-length of its centre segment. This sphere encloses the capsule as its orientation changes. Intersect the X and Z slabs to obtain one interval, then check height on each side of the trajectory's apex. Sixteen bisections locate a vertical crossing on its monotone interval. A page hit here means that its bounds could meet the capsule. The detailed contact solver remains responsible for actual faces. [Capsule state](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/solver.rs) · [Interval calculation](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/planning.rs)

With a configured link, estimate service as the greater of the remaining physical wire backlog and the base request latency plus the upper jitter allowance, followed by the file's transfer time at the configured bit rate. Transfer takes at least one tick. On unshaped paced disk or HTTP reads, start with the configured estimate. After a verified read, update the service estimate to its elapsed tick count if that is larger. For a faster sample, move one eighth of the difference toward the new observation, rounding up. A still-pending read's age and the current page's estimated transfer time are further lower bounds. The request horizon adds one eighth of estimated service, rounded up, and two ticks for admission and publication. During an unannounced interruption, the physical safety guard continues to protect the actor. [Queue, service measurements and admission](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/reader.rs)

The first physical query builds one bounding-volume hierarchy over the physical payloads. Its leaves store the directory tile IDs and the world-space bounds of the physical surfaces, including geometry that extends outside its tile. For a trajectory query, the largest encoded physical payload gives an upper bound on the service horizon. A missed parent box excludes its entire subtree. At each surviving leaf the scheduler uses that page’s own byte length to compute the exact deadline and first possible entry. Proximity and eviction queries use the distance to the nearest point on each box.

# Invariants

Geometry needed by the next step has highest priority. A projected intersection ranks ahead of ordinary proximity, ordered by time to entry relative to the page deadline. In trajectory mode, at most three ordinary physical reads occupy slots, leaving the fourth for an immediate blocker. Existing transfers keep their place on the wire.[^queue] Only physical actors contribute to this channel. Kinematic observers can opt into the same page reservations as the simulated capsule. [Shared physical reservations](/algorithms/physical-page-reservations.md) On a stop or turn, obsolete queued requests are cancelled. A completed transfer for a page outside the new path and radius cannot install that page. Full geometry, certified support and clearance checks retain their original source ownership. [Support guard](/algorithms/district-paging.md)

# Cost

The hierarchy needs one leaf per physical page and space linear in the page count. Per observer the query visits the overlapping branches and their immediate rejected neighbours. When every page overlaps the path it can visit the whole tree. The reader also processes outstanding requests and previous residents so a turn or a departure still cancels or retires the correct pages.

On a 4,745-tile Bir-Hakeim district (4,257 physical pages), the 15-part rig replay took 3.752 s of process CPU with exhaustive selection and 2.602 s with indexed selection. Seven alternating pairs on one logical core of an i5-1335U gave a median paired reduction of 29.7%. The left foot still reached the rail on tick 55. On the 323-tile district the paired reductions were 4.0% for the short rig replay and 5.7% for the full capsule descent. These times include startup, simulation and writing the trace. The comparison checked the complete receipts after excluding the two elapsed-time counters for model decoding and publication.

# Failure modes

An actor starting 66 ticks from impact cannot make an 85-tick physical read arrive in time. The certified surface or the existing movement hold covers that interval. Sudden turns, reversals, external forces and newly degraded network service can invalidate a forecast. Page boxes can encompass empty space. A body projecting along a large box can request detail that it never touches. The margin is an engineering choice, and a measured service time from one small file may understate the next file's latency.

# Alternatives

Use `--physics-plan radius` to isolate the old distance scheduler. Supplying `--physics-radius M` alone also selects that control. An explicit `--physics-plan trajectory --physics-radius M` changes the proximity radius while retaining the prediction.

For an isolated upright capsule on streamed static terrain, a known residual triangle can also matter through Rapier's speculative contact range before the swept physical volume intersects it. The district reserves such a page in advance. When a bounding box alone would hold the actor, a native preview and the upright geometry can prove that the missing face cannot contribute. A newly acquired support must persist into a second native step before the preview is accepted. Uncertified motion retains the hold. [Capsule and speculative clearance](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/streamed/clearance.rs) · [Measured trajectories](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-capsule-residual-clearance.py).

# Oracle

The reader test compares the indexed prediction map with an exhaustive calculation over 561 tiles, including unequal payload sizes, empty tiles, a page spanning cells and different pending read times. It compares each candidate’s entry tick, service estimate and horizon and checks inclusion at the radius boundary.

At Pont de Bir-Hakeim, changing the rate-independent latency to 1.4 seconds at 20 Mbit/s advances the rail request from tick 737 to 709. It completes at 794 and the capsule contacts owner 147 at 807. The Grenelle roof request moves from 719 to 691, completes at 776, and contacts owner 1408 at 789. Both pages previously arrived 15 ticks late. Each now has 13 ticks in hand. At two seconds both pages have 18 ticks in hand. At 200 and 600 ms the original leads and the completed byte counts match. On the short road approach, both modes request at tick zero. At 1.4 and 2 seconds the certified surface supplies the initial contact.

The 1.4-second rail run completes one 3,993-byte physical read in both modes. The roof run completes 11,358 bytes in trajectory mode and 11,663 with the radius. At two seconds the trajectory completes the rail's 3,993 bytes before the end of the replay, while the radius control completes none. On a separate vertical reversal, the actor rises from 100.88 m to 132.68 m, peaks at tick 154 and lands on the roof at 425. The predicted page is ready at 406, compared with 436 using the radius, with 11,663 completed bytes in both runs. Moving the rail camera to another street changes visual reads from zero to nine while preserving the full physical event history. [Replay, budgets and camera controls](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-physics-plan.py)

At just 25 kbit/s and 200 ms latency, the 3,993-byte rail page needs 89 ticks of service. The trajectory requests it at 704 and it arrives with 14 ticks to spare. The radius requests it at 737, giving 19 ticks of delay after contact. Two separate loopback HTTP runs use the server's actual response time without an injected client-side link profile. A prompt road response takes three simulation ticks and a response delayed by 300 ms takes 21. The physical lane records both values as its new service estimate. Both runs complete four reads totalling 14,974 bytes and meet the same road surface at tick 66.

# Reuse

Run `python worlds/ville-paris/check-physics-plan.py --http` with the release executable. A pre-cooked district can be supplied with `--pages`. The oracle writes each request trace, the complete body path, the two scheduler outputs and a summary. Reader tests separately stop an eastbound approach, reverse to the west, and insert an immediate blocker into a saturated queue. [Reader tests](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/reader.rs)

[^replay]: [Bir-Hakeim descent fixtures](https://github.com/smichea/osage/blob/main/worlds/ville-paris/fixtures/descent.json).
[^queue]: [District streaming queue and admission](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/reader.rs).
