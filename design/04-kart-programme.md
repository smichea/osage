# 04 — Kart programme: arcade play of the best of the genre, a GTA 5–class look

Date: 2026-11-12 · Status: **under way** (progress in §5, updated 2026-11-14) · Rendering bar: [03-rendering](03-rendering.md)

The kart world plays well enough for friends on a network (0.2.0), but it is one track, one kart, one mode, and its picture is good materials on simple shapes. This programme takes it to two bars at once:

- **Play**: the depth of the best arcade kart racers — driving that rewards skill, items that keep a race open, a choice of drivers and tracks, cups and modes.
- **Look**: a picture in the class of GTA 5 at T3/T4 — detailed models, animated people, light that bounces and reflects, weather — while T1 still runs on Priya's Intel HD 630.

## 1. Rules for this programme

1. **Original, not a recreation.** The genre's mechanics are fair to use (drift boosts, items by position, cups); another game's names, characters, items, tracks, music or look are not. The kart world keeps its paddock theme: items are things found in a paddock, drivers are its own.
2. **Everything in [AGENTS.md](../AGENTS.md) holds**: clean sources, an SPDX header on every file, a manifest entry for every asset, a card in `knowledge/` with every module or technique, a decision entry for every choice of rule.
3. **Ask first** before downloading any asset (CC0 only, from Poly Haven, Freesound, ambientCG, Kenney, Quaternius or similar), and before any push, release, cloud VM or message. Cloud bench at most once an hour, always `--project=osage-bench`.
4. **Reusable components** ([D-035](decisions.md)): a mechanic is a crate under `worlds/kart/components/` or a technique in `osage-render`, not code in the client.
5. **The world decides, the client draws.** Rules (boosts, items, scoring) live in the authoritative world and the shared physics (Rust and its WebAssembly build); the native and browser clients only predict and draw them. The physics stays deterministic: golden files change only with a decision entry.
6. **Frame rate first.** Every visual feature has a rung or a tier in [frame pacing](../knowledge/techniques/frame-pacing.md); the bench runs before and after each rendering change.

## 2. Milestones

Each milestone ends with every oracle in AGENTS.md green, screenshots of what changed, and a playtest build that is released **only when the responsible person says so**.

### M1 — Driving that rewards skill

| Task | What | Oracle |
| --- | --- | --- |
| G1.1 Drift boost | Holding a drift charges three levels (sparks change colour); releasing gives a boost by level. | Physics unit tests per level; WASM and Rust agree; new golden file. |
| G1.2 Start boost | Throttle pressed in a short window before "go" boosts; too early stalls briefly. | World test on the countdown. |
| G1.3 Tricks | A button in the air gives a small boost on landing; the track gains a crest that karts flat out take off from (D-051: no ramps, a real track's relief). | Physics test; a crest on the paddock track. |
| G1.4 Slipstream | Close behind a kart for a moment: a short draft boost, with air lines drawn. | Physics test. |
| G1.5 Feel | Wall bounces, off-road slowdown and steering tuned; recorded in a card of tuning values. | Bots still finish in a time band (world test). |

### M2 — Items that keep a race open

| Task | What | Oracle |
| --- | --- | --- |
| G2.1 By position | Draw weights depend on rank and gap to the leader: the last get strong items, the first defensive ones. | Item tests over thousands of draws per rank. |
| G2.2 Held behind | An item (tyre, cone, oil) can be held behind the kart as a shield, then dropped. | Items tests; drawn on the kart. |
| G2.3 New paddock items | A drone that flies to the leader; a stack of three tyres; a bumper that knocks nearby karts aside; a short invulnerable "pit-crew push". Each has its own model, sound and effect. | Items tests; bots use each one. |
| G2.4 Bots use items | Bots pick when to throw, hold and dodge. | World test: bots fire items in a race. |

### M3 — Drivers, karts, tracks, cups

| Task | What | Oracle |
| --- | --- | --- |
| G3.1 Drivers and karts | Eight original drivers and four kart bodies with stats (weight, top speed, acceleration, handling, grip); a selection screen (keyboard, gamepad, browser). | Physics tests per stat; protocol carries the choice. |
| G3.2 Track format | A track is data: centre spline with elevation and banking, width, surfaces, jumps, item rows, scenery placement; built by the existing land and scenery algorithms. | Tests that each track closes, has a racing line bots can follow, and checkpoints in order. |
| G3.3 Four tracks | The paddock, a coastal road, a forest at dusk, a harbour town; each with its own landmark and sky. | Bots finish every track; bench per track. |
| G3.4 Cup | Four races, points, standings between races, a podium. Host chooses the cup; others vote or join. | World test of a whole cup. |

### M4 — Modes and presentation

| Task | What | Oracle |
| --- | --- | --- |
| G4.1 Time trial | Solo laps against a ghost of one's best (inputs recorded, replayed by the deterministic physics); ghosts saved per track. | Ghost replays to the same lap time. |
| G4.2 Battle | An arena; each kart has three hits; last one standing. | World test. |
| G4.3 Split screen | Two to four players on one PC with gamepads. | Runs at T1 with two views. |
| G4.4 Presentation | Track flyover before the start, countdown, a slow-motion finish replay, music (CC0) and menus throughout. | Screenshots; frame pacing holds. |
| G4.5 Leaderboards | Each world keeps its records: best lap and best race per track, per driver name, with date, kart and driver; shown on the results screen, in the menus and to AI agents (a component type). A board shared between worlds is a service a world operator runs, not the engine (mechanisms, not policy). | World test: a record beaten replaces the old one and survives a restart of the world. |
| G4.6 Profiles and progress | A player's name, colour, favourite driver and kart kept on their PC; points won in cups unlock kart colours and parts (cosmetic only: no paid content, no advantage). | Tests of the profile file and unlocks. |
| G4.7 Replays and photos | Any race saved as inputs and replayed by the deterministic physics; a photo mode on pause (free camera, depth of field, the picture at T4). | A replay ends with the same results; screenshots. |

### M5 — People and detail

| Task | What | Oracle |
| --- | --- | --- |
| V1.0 Nothing repeats | Ground textures sampled without a visible tile (hex-tile blending, offsets and turns by position); the road varied along its length by a deterministic function of the distance (wear and rubber on the racing line, patches, seams, cracks, stains); grass with species, flowers and bare soil by place; every tree's species, size, turn and tint drawn from its position. | Reference screenshots; an autocorrelation test that finds no period in a road or grass strip; the same picture on every client. |
| V1.1 Marks on the track | Skid marks and tyre rubber as decals that fade; dust and gravel when off track; debris after hits. | Bench budget at each tier. |
| V1.2 Drivers animated | A rigged driver per character: steering, leaning, looking back, hit reaction, celebration on the podium. | Screenshots; animation unit tests on poses. |
| V1.3 Crowd and marshals | The grandstand crowd animated (impostors far, meshes near), marshals waving flags. | Bench budget. |
| V1.4 Models | Detailed kart bodies, tyres and engines; grandstands, buildings, barriers; vegetation with levels of detail and wind. CC0 or made here; every asset in the manifest. | Manifest check; bench per tier. |

### M6 — Light and weather (the GTA 5 bar)

| Task | What | Oracle |
| --- | --- | --- |
| V2.1 Reflections | Screen-space reflections at T3; ray-traced reflections at T4 (the user's RTX desktop). | Reference images; bench. |
| V2.2 Global illumination | Light bouncing at T3 (probes or SSGI) and ray-traced at T4. | Reference images; bench. |
| V2.3 Volumetrics and clouds | Volumetric fog and sun shafts; clouds that move and shade the ground. | Bench. |
| V2.4 Time of day | The sun, sky and lights through a day; headlights and track lights at dusk. | Screenshots at four hours. |
| V2.5 Rain | Falling rain, a wet track that darkens and reflects, puddles, spray from wheels; less grip in the physics (a decision entry). | Physics test; bench. |
| V2.6 Scale | Levels of detail and impostors for everything far; streaming of tracks larger than memory. | Bench on the largest track at T1 and T4. |

## 3. Order and parallel work

M1 and M2 first: little work, and what players feel first. Two pieces of the look go early, beside M2, at the responsible person's request: V1.0 (nothing repeats) and a first ray-traced picture of the karts at T4 (Solari: reflections and global illumination on an RTX card such as Adrien's), which then matures through M6. The bar for the look is the [rendering programme](03-rendering.md)'s: GTA 6 class at T4. M3 then M4 for depth. M5 can start beside M3 (different crates). M6 last, on top of the models of M5. Within a milestone, tasks touching different crates can go to parallel agents.

## 4. Done

The programme is done when every task above has passed its oracle, the knowledge cards and decision log are up to date, and the responsible person has played a playtest build of each milestone with friends.

## 5. Progress (2026-11-14)

Each line points at the decisions that did the work; "partly" says what is missing.

| Milestone | State |
| --- | --- |
| **M1 Driving** | Done: three-level drift boosts, perfect starts and stalls, tricks over crests, slipstream (D-050, D-051); contacts with momentum (D-045); a trick's roll as a rigid body that bounces on landing (D-096). |
| **M2 Items** | Done: ten paddock items drawn by place and gap, some held behind, bots using them (D-046, D-052); thrown tyres and cones as bodies (D-079); oil takes grip (D-072); a player's own items drawn at once (D-071). |
| **M3 Drivers, karts, tracks, cups** | Done: eight drivers and four bodies (D-055); tracks as data (D-056); six tracks with their places — paddock, forest, coast, harbour town, canyon, mountain — hills, bridges and tunnels (D-061, D-077, D-080, D-081); cups (D-057). |
| **M4 Modes and presentation** | Done: time trials with ghosts (D-062); battles (D-065); two players on one screen (D-086 — two, not yet four); flyover and slow-motion finish (D-063); the whole race shown again as television would (D-082); records and a leaderboard page (D-053, D-091); profiles and paints won (D-064); photo mode (D-066); bot difficulty and engine classes (D-088, D-089). |
| **M5 People and detail** | Partly. Done: nothing repeats (D-054); marks on the track (D-067); drivers who steer, lean, look into bends, cheer, get dizzy (D-087), and flinch when knocked (D-111); a crowd that follows the race (D-103); marshals waving their flags at the karts (D-109); karts that look raced (D-098 to D-100); mown grass, scrub, grime, birds and animals (D-101 to D-104, D-106). Missing: crowd impostors far off, modelled buildings and barriers beyond what the procedural kit makes, wind in the vegetation. |
| **M6 Light and weather** | Partly. Done: ray-traced light and reflections at T4 with osage's denoiser (D-093 to D-097); rain (D-068, D-069); night under floodlights (D-085); a nearer haze (D-107); tree cards for the far woods; clouds' shadows drifting over the land (D-108). Missing: global illumination at T3, volumetric fog, clouds that move in the sky, a time of day that moves, streaming of tracks larger than memory. |

Releases so far: pre-releases up to kart 0.13.3, Windows builds updating themselves (D-047, D-083). Measured on the cloud bench's NVIDIA L4 at 2560×1440: T3 14.4 ms, T4 29.9 ms by day and 32.8 ms at night in the rain.
