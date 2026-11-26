---
type: Algorithm
title: Sound shared among threads
description: An engine's emitters shared among a pool of threads kept from one block to the next — the ways found by whichever thread is free, the voices and the ways carried to the ears by fixed groups of emitters into mixes of their own, added in the groups' order — so that one, two or four threads give the same samples to the bit.
resource: https://github.com/smichea/osage/blob/main/crates/osage-sound/src/pool.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [sound, threads, determinism, real-time]
sources:
  - id: own
    resource: "osage's own design, issue #23 and D-246, 2026-11-26"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-26T07:00:00Z" }
---

# Problem

[osage-sound](/modules/osage-sound.md) makes a block of 128 samples every 2.7 ms. On the bridge at reference quality, one core spends most of that time on it. Its emitters are independent and could share the work among cores. Two constraints make this harder:

- **Reproducible output.** A render must stay reproducible to the bit, since its digests and its tests compare bytes. Floating-point sums depend on their order, so the number of threads must not change the order in which anything is added.
- **Thread start-up.** Starting threads each block costs too much: 235 µs for three, a tenth of a block.

# Idea

A pool of threads waits between blocks, and a block's work is lent to it in two parts[^own]:

- **Finding the ways.** Each emitter's ways are found by itself, so whichever thread is free takes the next emitter. Imbalance does not matter, since what an emitter finds depends on it alone.
- **The voices and the ways carried to the ears.** These are summed, so they are grouped: fixed groups of emitters, each into a mix of its own, added afterwards in the groups' order. The order of every sum is set by the scene, never by the threads.

# Method

**The pool** (`osage_sound::pool::Pool`) holds `threads − 1` threads parked on a condition variable. `run(task)` does five things:

1. It publishes the task, with its lifetime erased.
2. It wakes the threads; each calls `task(index)`.
3. It calls `task(0)` on its own thread.
4. It waits until every thread has reported back.
5. It panics if any of them panicked.

The erasure is sound because `run` returns only once no thread holds the task. This is what `std::thread::scope` does, without starting threads. A round costs some 25 µs under load. A pool of one runs the task inline. In WebAssembly the engine always has one thread.

**A block**, in order:

1. **One thread.** It places the tree and the faces, makes the contacts, and works out each emitter's speed, load, cut, detail and highest frequency. These read one another (a span is loaded by the bogies near it), so they stay on one thread. It lists the emitters whose ways are due.
2. **Shared: the ways.** Every due emitter is in a slot behind its own lock. Each thread takes the next slot by an atomic counter and finds that emitter's ways with a scratch of its own. Nothing an emitter finds depends on another's.
3. **One thread.** It ranks the ways under the budget, keeps or leaves out ties together, and chooses which hidden emitters feed the reverberation.
4. **Shared: the voices and the ways carried.** The emitters are cut into groups of 8, in their order, each with a mix: both ears, the reverberation's eight lines, the far part's channels, the virtual loudspeakers' feeds. Each thread takes the next group. For each emitter it makes the voice into its line, then carries each way into the group's mix.
5. **One thread.** It adds the mixes in the groups' order, then runs the virtual loudspeakers' responses and the reverberation.

The tally's times for the voices and the ways are summed over threads: they are processor time, not the clock's.

# Invariants

- One, two and four threads give the same samples to the bit (tested):
  - on the bridge at reference quality for headphones, at medium for loudspeakers, and at high with a far field;
  - with measured ears and twenty emitters in three groups.
- The bridge with four threads gives the README's digests and passes `verify`.
- Every thread of the pool does its part, the caller waits for all, and a panic on another thread panics the caller while the pool goes on (tested).

# Cost

On the bridge at reference quality, headphones, on 2026-11-26 under a load of 9:

| Threads | On the clock | All processors |
|---:|---:|---:|
| 1 | 824 ms a second of sound | 814 |
| 2 | 511 | 851 |
| 4 | 334 | 899 |

The sums take about a tenth more processor time with four threads: waking the threads, the groups' mixes, cache traffic. `paris-soundscape bench --part bridge` prints these.

# Failure modes

- What stays on one thread does not shrink: the ranking, the reverberation, the loudspeakers' responses (a fixed cost with measured ears).
- With fewer emitters than groups have room for, there is nothing to share in the second part.
- Each group's mix holds a set of the virtual loudspeakers' feeds: 110 kB each with measured ears.

# Alternatives

- **A general work-stealing library** would do the same with less code of our own. It is a new dependency, which needs the maintainer's word to fetch.
- **Threads per block** are too slow to start.
- **Mixing per thread rather than per group** would make the bytes depend on the number of threads.

# Oracle

`the_bridge_is_the_same_samples_whatever_the_number_of_threads` (`tools/paris-soundscape/tests/scene.rs`), `through_measured_ears_the_samples_are_the_same_whatever_the_number_of_threads` (`crates/osage-sound/tests/hrir.rs`), the pool's own tests, and `paris-soundscape verify --threads 4`.

# Reuse

Any audio engine with independent voices: share what depends on one voice alone freely, and fix the order of every sum by groups that the content defines, not the threads.

[^own]: osage design
