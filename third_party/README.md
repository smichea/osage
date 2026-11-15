<!--
SPDX-FileCopyrightText: The osage contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Bevy crates carried with a patch

osage builds on the Bevy release pinned in the workspace (D-001). When a fix to
Bevy cannot wait for its next release, the crate is copied here from crates.io
at that exact version, patched, and swapped in by `[patch.crates-io]` in the
workspace's `Cargo.toml`. Each patch is marked `osage patch` in the code, is
offered upstream, and the copy is deleted when a Bevy release carries it.

| Crate | Version | Patch | Why |
|---|---|---|---|
| `bevy_asset` | 0.20.1 | `AssetChanged` compares each entity's asset with the few changed since the query last ran, instead of looking every entity up in the map of every asset ever changed (`src/asset_changed.rs`) | One mesh or material changed in a frame (a tyre mark, a flag, the lake's ripples) made five of Bevy's per-frame systems look up all 56 000 mesh entities of the kart's T3/T4 scene in a hash map: the processor time D-126 found |

The copies keep their licence, MIT OR Apache-2.0, with their own licence files.
