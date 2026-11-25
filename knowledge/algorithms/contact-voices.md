---
type: Algorithm
title: Contact voices
description: Which contacts — steps, wheels on joints, blows — get one of a few voices — only one that reaches the ears within its reach by a real way, and only from a voice heard less than it would be, judged by how loud it arrives by its loudest way and by what the voice still makes — with each contact's strength and grain drawn from whose contact it is, so that who else is in the scene changes nothing.
resource: https://github.com/smichea/osage/blob/main/crates/osage-sound/src/engine.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [sound, contacts, voice-allocation, determinism]
sources:
  - id: own
    resource: "osage's own design and the review of the sound prototype, 2026-11-23 and 2026-11-24"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-24T09:00:00Z" }
---

# Problem

A crowd walking on a bridge makes hundreds of contacts a second; a device can afford a handful of contact voices (4 to 16). Taking the first contacts that come, or the nearest as the crow flies, fills the pool with steps no one hears — behind a wall, or far — and silences the one step that is heard. And a contact's sound must not depend on the order of the scene's lists or on who else walks: taking people out of a scene must not change the steps of those who stay.

# Idea

A contact is worth how loud it arrives at the ears, by the loudest of the ways its voice would be carried by. A voice is worth how loud its contact arrived, times how much of its loudest sound it still makes. A contact takes the least-worth voice only if it is worth more[^own]. The expensive estimate (finding ways) is made only when the cheap one (straight-line level) says a voice could be taken at all. Every random draw is seeded by the toucher and the count of its own contacts.

# Method

A **contact** (a `Blow`) carries the surface struck, the point, the toucher, a strength (0…1), when in the block it happens, and a seed.

**Making contacts.** For each touch, in an order fixed by node path: feet advance a distance; each time it passes the stride, the next foot comes down at its point, with a strength varied by ±18 % from the toucher's own noise. Wheels look under each wheel point once every `every` blocks (staggered by the toucher's name) and knock when they come onto a different surface that `strikes`, harder the faster (full at 50 km/h, never under 15 %). The contact's seed is `hash(toucher's seed, "contact {n}")`, `n` counting that toucher's contacts.

**Giving a voice**, for each contact, with `gain = gain(struck matter) · gain(striker matter)`:

1. Leave out a contact that solo or mute excludes, one farther than the contact reach (150 m), or one whose gain is nil.
2. `most = gain · strength / max(distance, 0.3 m)`: as loud as it could arrive with nothing in the way.
3. For the struck thing, and for the striker if it rings by itself (a clog, a hammer's head), find the least-worth voice of the pool: a free voice (past its 2.5 s) is worth 0; a busy one `worth · last/top`, where `worth` is how loud its contact arrived, `top` the largest sample its voice has made and `last` the largest of its last block. Ties go to the voice that frees soonest.
4. If that voice is worth `most` or more, stop: nothing in the way can make the contact louder.
5. Otherwise compute `loud = gain · strength · arriving(point)`, where `arriving` is the loudest of the straight way (or through, or round an edge) and, at qualities that follow reflections, every mirrored way, each counted only if no longer than the reach, as `max(band)/max(length, 0.3)`. Once per contact.
6. If `loud` is nil (no way to the ears within reach), take the voice only if it is free, with a worth of 0. The contact rings out in its line, and a listener who comes round what hid it hears what it still makes. Its worth becomes its arrival level once it is heard. If `loud` is no more than the voice's worth, stop.
7. Take the voice: a new impact voice seeded `hash(contact seed, "voice {which}")`, with the struck matter's parameters and the hardness of the softer of the two; placed at the contact point on the struck surface's node; `worth = loud`; its first block rendered at once with its ways found in the same block. The blocks its ways are found in afterwards are told by that seed, not by which voice of the pool it took.

A voice of the pool is free again 2.5 s after it was struck. Until then its voice is made whether or not its ways reach the ears.

# Invariants

Tests of `crates/osage-sound/tests/physics.rs`:

- Steps too far to be heard, behind a wall, or heard less take no voice from a step that is heard (at two qualities).
- A contact struck in a closed box is heard once the box's door slides away while it still rings, as it would have been heard with no box (tested).
- A step is the same, sample for sample, whoever else walks: alone, among people too far to be heard, among people muted.
- A contact left out by solo or mute takes no voice from one that is heard.
- The scene's file in another order renders to the same samples.

# Cost

Step 2 is a distance; step 5 is the way search of one point, and is skipped for most contacts of a crowd. On the Pont de Bir-Hakeim, looking for a contact's ways before giving it a voice costs nothing that shows in the bench.

# Failure modes

- A contact that does not reach the ears when it is made takes only a free voice. When every voice is busy it is not heard, even if the listener comes round what hid it while it would still ring.
- A voice's worth is its arrival level when it was struck; if the listener moves, the comparison is with a stale level until the contact frees.

# Alternatives

Priority by distance only (most engines) is cheaper and fills the pool with hidden sources. Virtual voices that keep running silently and are swapped by audibility each block are fairer but cost a voice's state per contact.

# Oracle

The tests named above; [soundscape verify](/oracles/soundscape-verify.md) for the bridge.

# Reuse

Any engine with a small voice pool and a propagation model: judge by arrival level along real ways, decide cheaply first, and seed each event by its owner and its own count.

[^own]: osage design
