# 08 — Recent kart racers: what to take for the kart world

Date: **2026-11-10** · Survey from public sources; items marked *unverified* could not be confirmed. No game asset, ROM or decompilation source was opened.

The kart world ([D-031](../design/decisions.md#d-031--the-kart-world-shared-physics-exact-prediction-a-world-that-never-makes-up-a-step)) is an original game. Game mechanics are ideas and may be adopted; names, characters, tracks, music and item designs of other publishers may not.

## Commercial kart racers, 2025–2026

| Game | Facts | Praised / criticised | Worth adopting |
|---|---|---|---|
| **Mario Kart World** (2025-06-05) | 24 players. Knockout Tour cuts the last four at each checkpoint. Free Roam, charge jump, rail grinding, wall riding. | Knockout Tour is the standout; Free Roam was called empty; forced connecting routes online caused a backlash, partly reverted. Items go by position, not distance; since 1.7.0 (2026-06-30) everyone gets mid-pack items for the first 20 s. | An elimination rally over point-to-point routes; a charge jump as a skill move without items; keep lap circuits first-class. |
| **Sonic Racing: CrossWorlds** (2025-09-25) | 12 players, crossplay. The leader picks the world of lap 2. Passive gadgets in loadout slots. One rival per Grand Prix. Rings raise top speed. | Handling, loadout depth and rivals praised; unskippable warps and grind criticised. The developers cut items judged "too stressful". | A portal to another world mid-race; loadouts; a rival bot; pickups that raise speed. |
| **Kirby Air Riders** (2025-11-20) | 16 online. City Trial: roam to collect upgrades, then a final event. | Chaos praised; the finale called anticlimactic. | A timed open-world preparation phase that feeds a race. |
| **Stampede: Racing Royale** | 60 players, 20 cut per round; closure announced in 2025-03. | The concept was liked; the live service failed. | Large-field elimination, with bots filling the grid. |
| Others | The Karters 2 (Early Access 2025-07), Garfield Kart 2 (2025-09-10, thin lobbies), Disney Speedstorm (free-to-play), Mythic Kart Maker (Early Access 2026-05, track editor; *single source*). | | Lobby and party features; a track editor. |

Whether Mario Kart World's online play is peer-to-peer or on dedicated servers is *unverified*.

## Open-source projects

| Project | Licence · activity | Technically interesting | Reuse in osage |
|---|---|---|---|
| **SuperTuxKart** | Code GPL-3.0-or-later; assets mostly CC BY-SA 4.0, mixed per file. 1.5 released 2025-10-20. | Server state with client prediction, rewind and replay, the error smoothed visually. Item collection is decided by the server and sent as deltas against a confirmed state. AI karts join as network clients. | **Ideas only** (GPL code cannot enter the MIT/Apache kart world; assets are not CC0). |
| **Dr. Robotnik's Ring Racers** | GPL-2.0-or-later; v2.4 released 2025-11-02. Contains SEGA IP. | Detailed anti-frustration rules (below). The team says it "plays best with 8 or fewer". | **Ideas only**; never assets. |
| **Kenney Starter-Kit-Racing** | Code MIT, assets CC0. Godot 4. | Arcade vehicle controls, tile-based tracks. | Code (to port from GDScript) and assets. |
| **pmndrs/racing-game** | MIT, CC0 assets. three.js. | Dust, skid marks and trails; HUD. | Code for the browser client. |
| **bevy_garage** | MIT OR Apache-2.0. Bevy + rapier3d. | WebAssembly build, virtual joystick, cameras. | Code for the native client. |

**CC0 asset packs** (not downloaded; to be listed in the [asset manifest](../assets/manifest.toml) if used):
- Kenney Car Kit — https://kenney.nl/assets/car-kit (kart racers added in v3.0)
- Kenney Racing Kit — https://www.kenney.nl/assets/racing-kit (track pieces)
- Kenney Toy Car Kit — https://kenney.nl/assets/toy-car-kit
- Quaternius Cars Pack — https://quaternius.com/packs/cars.html

## Feel and technique

- **Netcode.** Glenn Fiedler's networked-physics articles (which SuperTuxKart's blog cites); the Rocket League physics talk (GDC 2018); SuperTuxKart's networking notes. osage's exact prediction (D-031) is the same family: authoritative server, client replay, visual smoothing.
- **Catch-up rules** (Ring Racers 2.4), as data rather than hidden rubber-banding:
  - no item boxes before the first checkpoint;
  - fewer attack-only items when far from the others;
  - weaker odds in bigger lobbies;
  - a shorter stun when far from the pack;
  - a small speed assist far behind.
- **Item tables.** By distance to the leader (Mario Kart 8) or by position (Mario Kart World).
- **Track widths.** SuperTuxKart's guidelines: 8 (narrow), 12 (typical), 18 (wide) units; the kart world's road is 14 m.
- **Gaps.** No good public source was found on kart cameras or on drift-model implementation.

## Recommendations for kart v2

| # | Feature | Best shown by | Why it suits osage | Difficulty | The world decides / the engine provides |
|---|---|---|---|---|---|
| 1 | **Items** | CrossWorlds, Ring Racers | The biggest gap of the current game | medium-high | item list and odds / scheduled events, server random rolls, spawned entities |
| 2 | **Catch-up rules as data** | Ring Racers 2.4, Mario Kart World 1.7.0 | Tunable per world, visible to players | low | tables / rank and distance-to-leader queries |
| 3 | **Ghosts and verified time trials** | follows from determinism | A replay is an input log; the server can re-simulate and attest it — an oracle for records | low | leaderboard rules / replay and verification |
| 4 | **Knockout rally** | Mario Kart World, Stampede | Uses the open world; bots fill the field | medium | elimination rules / checkpoint graph, spectating |
| 5 | **Slipstream and speed pickups** | Ring Racers, CrossWorlds | Deterministic, low-stress catch-up | low-medium | values / proximity queries |
| 6 | **Charge jump and tricks**, rails later | Mario Kart World | Skill without items; a good test for AI agents | medium (rails: high) | boost values / spline rails, surface tags |
| 7 | **Rival bot, agent rivals** | CrossWorlds | Bots and MCP agents already exist | low-medium | personalities / targeting hooks |
| 8 | **Free-roam hub with events** | Kirby Air Riders; Mario Kart World as a caution | The open-world goal; gates could bind to real devices | high | missions / concurrent race instances, interest management |
| 9 | **24+ players** | Mario Kart World | Engine showcase | high | field size / delta snapshots, interest management |
| 10 | **Loadouts and a track editor** | CrossWorlds, Mythic Kart Maker | Community content | medium-high | gadget list / validated modifiers, track format |

**Items with exact prediction — sketch.**
- Item state lives in the shared deterministic simulation; "use item" is one more bit of the controls.
- **Predicted locally:** what depends only on the player's own state — using a boost, raising a shield, dropping a hazard (its entity id derived from player and tick).
- **Decided by the world:** the item roll (the client predicts the pickup and starts a roulette whose one or two seconds hide the round trip), contention for a box, hits on other players, global effects, eliminations.
- Every world effect is scheduled for a tick a little ahead, so that clients apply it on the same tick with no visible rollback.
- AI agents get `use_item` in their intent, and the held item and the threats in what they observe.

## Red lines

- No name, character, track, music, item name or item design of Nintendo, SEGA or another publisher. Items are invented.
- No code from, or informed by, decompilations and their ports (the Mario Kart 64 decompilation, SpaghettiKart) — [CONTRIBUTING, "Clean sources"](../CONTRIBUTING.md#clean-sources).
- No fan remake built on a publisher's IP, even labelled MIT.
- No GPL code in the MIT/Apache kart world.
- To watch: a Nintendo patent application (JP2025-82801) reported to cover building courses from zones and connecting routes; status *unverified*. Take legal advice before shipping features 4 or 8.

## Sources

- Mario Kart World: https://en.wikipedia.org/wiki/Mario_Kart_World · https://kotaku.com/mario-kart-world-items-tweak-power-ups-new-routes-knockout-2000711989 · https://www.gamesradar.com/games/racing/mario-kart-world-charge-jump
- CrossWorlds: https://en.wikipedia.org/wiki/Sonic_Racing:_CrossWorlds
- Kirby Air Riders: https://www.gematsu.com/2025/08/kirby-air-riders-launches-november-20
- Stampede: https://traxion.gg/free-to-play-kart-racer-stampede-racing-royale-is-shutting-down-this-summer/
- SuperTuxKart: https://supertuxkart.net/Licensing · https://github.com/supertuxkart/stk-code/blob/master/NETWORKING.md · https://blog.supertuxkart.net · https://supertuxkart.net/Making_Tracks:_Notes
- Ring Racers: https://github.com/KartKrewDev/RingRacers · https://www.kartkrew.org/news/2025-11-02_dr-robotniks-ring-racers-version-24.html
- Starter kits: https://github.com/KenneyNL/Starter-Kit-Racing · https://github.com/pmndrs/racing-game · https://github.com/alexichepura/bevy_garage
- Technique: https://gafferongames.com/post/networked_physics_2004 · https://www.gdcvault.com/play/1024972/ · https://gamedeveloper.com/design/rubber-banding-as-a-design-requirement · https://www.mariowiki.com/Mario_Kart_8_item_probability_distributions
- Patent report: https://automaton-media.com/en/news/nintendo-submits-patent-seemingly-related-to-mario-kart-world-race-course-generation/
