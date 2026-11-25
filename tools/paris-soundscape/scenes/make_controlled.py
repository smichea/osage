#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Writes the controlled scenes next to this file: one sound, one question.

    python3 tools/paris-soundscape/scenes/make_controlled.py

Each is a geometry any sound engine can play alike, so that two of them can
be set side by side by what they put in each ear (bench/quality.py does it
for OpenAL Soft). Metres, Y up; the listener is at the origin of the ground,
its ears 1.6 m up, looking along -Z with +X on its right.

  controlled-circle.json   a noise 10 m away going round the listener in 20 s,
                           from ahead and by the right
  controlled-passby.json   a 1 kHz tone at 72 km/h on a line 5 m ahead, level
                           with the listener at 5 s
  controlled-far.json      the same tone, steady, 300 m ahead
  controlled-clicks.json   a wheel knocking on a plate at 1 s, 34.3 m to the
                           right; another at 3 s, 137.2 m to the left
  controlled-open.json     a noise 20 m ahead, in the open
  controlled-screen.json   the same behind a wall 4 m high and 20 m wide
  controlled-floor.json    a noise 1 m above a hard floor, ears 2 m above it,
                           4 m apart
"""

import json
import pathlib

HERE = pathlib.Path(__file__).resolve().parent
EARS = 1.6


def scene(name, seconds, nodes, surfaces=None, ears=EARS):
    """A scene: `nodes` are the world's children, each (id, dict of the node)."""
    world = {"children": list(range(1, len(nodes) + 2)), "extras": {"osage": {"id": "world"}}}
    if surfaces:
        world["extras"]["osageSound"] = {"surfaces": surfaces}
    listed = [world, {"translation": [0, ears, 0], "extras": {"osage": {"id": "ears"}}}]
    for ident, node in nodes:
        node.setdefault("extras", {})["osage"] = {"id": ident}
        node["extras"] = {"osage": node["extras"].pop("osage"), **node["extras"]}
        listed.append(node)
    document = {
        "asset": {"version": "2.0", "generator": "osage paris-soundscape: scenes/make_controlled.py", "extras": {
            "osageObject": {
                "version": 1, "id": name,
                "collisionMaterials": {"ground": {"friction": 0.8, "restitution": 0.0},
                                       "plate": {"friction": 0.5, "restitution": 0.0},
                                       "wheel": {"friction": 1.0, "restitution": 0.0}},
                "acousticMaterials": {"hard": {"absorption": [0.0, 0.0, 0.0], "transmission": [0.0, 0.0, 0.0],
                                               "scattering": 0.0}},
            },
            "osageSound": {"version": 0, "contactSounds": {
                "ground": {"hardness": 0.5},
                "plate": {"params": {"tone": 400.0, "ring": 0.3, "shape": 1, "scuff": 0.0}, "hardness": 1.0},
                "wheel": {"hardness": 1.0}}},
        }},
        "scene": 0,
        "scenes": [{"nodes": [0], "extras": {"osageSound": {
            "seconds": seconds, "seed": 6, "loudnessLufs": -23.0,
            "listener": {"node": ["world", "ears"]},
            "reverb": {"secondsT60": [0.5, 0.4, 0.3], "gainDb": 0.0},
            "air": {"celsius": 20.0, "humidityPercent": 50.0}}}}],
        "nodes": listed,
    }
    path = HERE / f"{name}.json"
    path.write_text(json.dumps(document, indent=1) + "\n", encoding="utf-8")
    (HERE / f"{name}.json.license").write_text(
        "SPDX-FileCopyrightText: The osage contributors\nSPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions\n", encoding="utf-8")
    print(path.name)


def emitter(voice, at=None, **params):
    entry = {"id": "sound", "voice": voice, "maxDistanceM": 600, "reflections": True}
    if at:
        entry["positionM"] = at
    if params:
        entry["params"] = params
    return {"emitters": [entry]}


# Round the listener at ear height: a node at the listener's head that turns,
# and the noise 10 m ahead in it. A turn to the right is a negative one.
scene("controlled-circle", 20.0, [
    ("turntable", {"translation": [0, EARS, 0], "extras": {"osageSound": {
        "turning": {"axisM": [0, 1, 0], "atS": 0.0, "degreesPerSecond": -18.0},
        **emitter("noise", [0, 0, -10])}}}),
])

scene("controlled-passby", 10.0, [
    ("mover", {"translation": [0, EARS, -5], "extras": {"osageSound": {
        "motion": {"alongM": [1, 0, 0], "atS": 5.0, "speedKmh": 72.0},
        **emitter("tone", hz=1000.0)}}}),
])

scene("controlled-far", 6.0, [
    ("beacon", {"translation": [0, EARS, -300], "extras": {"osageSound": emitter("tone", hz=1000.0)}}),
])

ground = [
    {"id": "ground", "quadM": [[-400, 0, -50], [-400, 0, 50], [400, 0, 50], [400, 0, -50]], "collision": "ground"},
    {"id": "plate-right", "quadM": [[34.3, 0.01, -3], [34.3, 0.01, 3], [35.3, 0.01, 3], [35.3, 0.01, -3]],
     "collision": "plate", "strikes": True},
    {"id": "plate-left", "quadM": [[-138.2, 0.01, -3], [-138.2, 0.01, 3], [-137.2, 0.01, 3], [-137.2, 0.01, -3]],
     "collision": "plate", "strikes": True},
]
wheel = {"touches": [{"kind": "wheels", "material": "wheel", "pointsM": [[0, 0, 0]], "strength": 1.0}]}
scene("controlled-clicks", 5.0, [
    # 36 km/h: each is 10 m short of its plate a second before it reaches it.
    ("wheel-right", {"translation": [34.3, 0, 0], "extras": {"osageSound": {
        "motion": {"alongM": [1, 0, 0], "atS": 1.0, "speedKmh": 36.0}, **wheel}}}),
    ("wheel-left", {"translation": [-137.2, 0, 0], "extras": {"osageSound": {
        "motion": {"alongM": [-1, 0, 0], "atS": 3.0, "speedKmh": 36.0}, **wheel}}}),
], surfaces=ground)

scene("controlled-open", 6.0, [
    ("source", {"translation": [0, 1.0, -20], "extras": {"osageSound": emitter("noise")}}),
])
scene("controlled-screen", 6.0, [
    ("source", {"translation": [0, 1.0, -20], "extras": {"osageSound": emitter("noise")}}),
], surfaces=[{"id": "wall", "quadM": [[-10, 0, -10], [10, 0, -10], [10, 4, -10], [-10, 4, -10]], "acoustic": "hard"}])

scene("controlled-floor", 6.0, [
    ("source", {"translation": [0, 1.0, -4], "extras": {"osageSound": emitter("noise")}}),
], surfaces=[{"id": "floor", "quadM": [[-30, 0, -30], [-30, 0, 30], [30, 0, 30], [30, 0, -30]], "acoustic": "hard"}],
    ears=2.0)
