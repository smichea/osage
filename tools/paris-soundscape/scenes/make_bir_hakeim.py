#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Writes bir-hakeim.json next to this file: the small made-up tree of the
Pont de Bir-Hakeim that the soundscape prototype hears.

    python3 tools/paris-soundscape/scenes/make_bir_hakeim.py

Python standard library only. The JSON is what the program reads and may be
edited by hand; this script is how it was made, and makes it again to the
same bytes. Metres, Y up, +X from Passy to Grenelle, -Z upstream: the frame
of worlds/ville-paris/fixtures/make_sector.py, whose deck heights (road at
8 m, viaduct at 16 m) and widths it keeps. The file is the JSON of a glTF 2.0
asset without meshes: its osageObject header and each node's osage.id are the
object format's (design/08-object-format.md); what sound adds is in sibling
osageSound keys.
"""

import json
import pathlib

HERE = pathlib.Path(__file__).resolve().parent

# The bridge: length and widths of the city fixture; the split of the road
# deck (promenade under the viaduct, two carriageways, two pavements) from
# the bridge's published cross-section.
HALF_LENGTH = 118.58
ROAD_Y, VIADUCT_Y = 8.0, 16.0
DECK_HALF_WIDTH, VIADUCT_HALF_WIDTH = 12.35, 3.8
PAVEMENT_Z = -11.35  # the middle of the upstream pavement
LANES = {  # the middle of each lane, and the way its traffic goes along X
    "lane-to-passy-kerb": (-8.85, -1),
    "lane-to-passy-middle": (-5.85, -1),
    "lane-to-grenelle-middle": (5.85, 1),
    "lane-to-grenelle-kerb": (8.85, 1),
}

# The listener: walks from Passy towards Grenelle on the upstream pavement.
WALK_FROM_X, WALK_KMH, EAR_HEIGHT = -58.6, 5.04, 1.6
SECONDS = 45.0


def walker_x(t):
    return WALK_FROM_X + WALK_KMH / 3.6 * t


# The metro: an MP 89 CC towards Passy on the upstream track. It leaves
# Bir-Hakeim station, runs at 50 km/h over the listener, brakes for Passy.
# TRAIN_ABEAM_S is the instant its middle is level with the listener: move it
# to move the whole passage. Speeds are given at instants counted from then.
TRAIN_ABEAM_S = 25.85
TRAIN_SPEED_KMH = [[-25.85, 0.0], [-11.85, 50.4], [8.25, 50.4], [22.25, 0.0]]
TRAIN_GAIN_DB = 8.0
CARS, CAR_LENGTH, BOGIE_FROM_MIDDLE, AXLE_FROM_BOGIE = 5, 15.0, 5.0, 1.0
MOTOR_CARS = (1, 3, 5)  # assumed: three motor cars of five
TRACK_Z, RUNWAY_ABOVE_DECK = -1.5, 0.2

# How loud the city's bed is in each ear: under everything, a floor.
CITY_GAIN_DB = -20.0

# Road vehicles: (name, lane, the instant it is level with the listener,
# km/h then, km/h gained over a minute, voice, voice parameters). The near
# carriageway comes towards the walker, the far one overtakes. A first group
# passes before the train is heard and a second after it has gone, so that
# its coming and its going are in the clear. Nobody holds a speed exactly:
# two engines a few turns a minute apart would beat like a clock.
VEHICLES = [
    ("car-a", "lane-to-passy-kerb", 2.6, 31.0, 4.0, "car", {"size": 1.0}),
    ("car-b", "lane-to-passy-middle", 4.4, 28.0, -3.0, "car", {"engine": 0.0, "size": 1.1}),
    ("car-c", "lane-to-passy-kerb", 6.9, 33.0, 2.0, "car", {"size": 1.4, "rpm": 1500.0}),
    ("car-d", "lane-to-grenelle-middle", 11.5, 36.0, -4.0, "car", {"size": 0.9, "rpm": 2000.0}),
    ("car-e", "lane-to-passy-kerb", 37.0, 30.0, 3.0, "car", {"size": 1.0, "tone": 0.3}),
    ("car-f", "lane-to-grenelle-kerb", 38.8, 34.0, -2.0, "car", {"size": 1.2}),
    ("scooter", "lane-to-grenelle-middle", 40.8, 42.0, 5.0, "scooter", {}),
    ("car-g", "lane-to-passy-middle", 42.8, 29.0, -3.0, "car", {"engine": 0.0, "size": 0.9}),
]
# A 50 cc two-stroke is a good deal louder than a car at these speeds.
VEHICLE_GAIN_DB = {"car": 0.0, "scooter": 7.0}

# The Seine: water against the piers and the tip of the island under the
# bridge's upstream side, and open water further out. (name, x, z, gain dB,
# load, voice parameters) Louder than a slow river is beside traffic: the
# brief asks that it stay audible, and a real one is lost under every car.
# Listened to (issue #20), it was too much: 4 dB down since, and lapping as
# seldom as the voice's own default (once every 4 to 16 s on an ordinary day).
WATER = [
    ("pier-1", -87.0, -13.2, -2.0, 0.6, {"size": 0.7, "laps": 0.1}),
    ("pier-2", -42.0, -13.2, -2.0, 0.6, {"size": 0.7, "laps": 0.1}),
    ("island-tip", 0.0, -12.9, 0.0, 0.7, {"size": 0.9, "laps": 0.1}),
    ("open-water-1", -70.0, -28.0, 0.0, 0.4, {"size": 0.6, "bed": 0.6, "laps": 0.05}),
    ("open-water-2", -36.0, -34.0, 0.0, 0.4, {"size": 0.6, "bed": 0.6, "laps": 0.05}),
    ("open-water-3", -4.0, -30.0, 0.0, 0.4, {"size": 0.6, "bed": 0.6, "laps": 0.05}),
    ("open-water-4", 28.0, -40.0, 0.0, 0.4, {"size": 0.6, "bed": 0.6, "laps": 0.05}),
]

# A steel plate over an expansion joint of the road deck, at pier 2: the
# walker steps on it, the near carriageway's wheels knock on it.
PLATE_X, PLATE_HALF = -42.0, 0.75
# The viaduct's expansion joint at the middle pylon: every axle knocks on it.
JOINT_X = 0.0
# Spans of the steel viaduct, each rumbling under the bogies on it.
SPANS = [-96.0 + 24.0 * k for k in range(9)]

nodes = []


def node(ident, parent=None, translation=None, rotation=None, sound=None):
    """Adds a node under `parent` (an index) and returns its index."""
    entry = {}
    if translation:
        entry["translation"] = [round(v, 4) for v in translation]
    if rotation:
        entry["rotation"] = rotation
    entry["extras"] = {"osage": {"id": ident}}
    if sound:
        entry["extras"]["osageSound"] = sound
    nodes.append(entry)
    index = len(nodes) - 1
    if parent is not None:
        nodes[parent].setdefault("children", []).append(index)
    return index


def flat(y, x0, x1, z0, z1):
    return [[x0, y, z0], [x0, y, z1], [x1, y, z1], [x1, y, z0]]


def emitter(ident, voice, position=None, gain=0.0, params=None, **more):
    entry = {"id": ident, "voice": voice}
    if position:
        entry["positionM"] = position
    if gain:
        entry["gainDb"] = gain
    if params:
        entry["params"] = params
    entry.update(more)
    return entry


bridge = node("bridge")

# The road deck: its top is the road and the pavements.
road = node("road-deck", bridge, [0, ROAD_Y, 0], sound={"surfaces": [
    {"id": "roadway", "quadM": flat(0, -HALF_LENGTH, HALF_LENGTH, -DECK_HALF_WIDTH, DECK_HALF_WIDTH),
     "acoustic": "asphalt", "collision": "asphalt"},
    {"id": "underside", "quadM": flat(-1.6, -HALF_LENGTH, HALF_LENGTH, -DECK_HALF_WIDTH, DECK_HALF_WIDTH),
     "acoustic": "steel-deck", "reflects": False},
    {"id": "joint-plate", "quadM": flat(0.005, PLATE_X - PLATE_HALF, PLATE_X + PLATE_HALF, -DECK_HALF_WIDTH, -4.35),
     "collision": "steel-plate", "strikes": True},
]})
pavement = node("pavement-upstream", road, [0, 0, PAVEMENT_Z])
# A node looks along its own -Z: turned a quarter so that it looks along +X.
walker = node("walker", pavement, [WALK_FROM_X, 0, 0], [0, -0.7071067811865476, 0, 0.7071067811865476], sound={
    "motion": {"alongM": [1, 0, 0], "atS": 0, "speedKmh": WALK_KMH},
    "touches": [{"kind": "steps", "material": "rubber-sole", "strideM": 0.75,
                 "feetM": [[-0.09, 0, 0], [0.09, 0, 0]], "strength": 0.6}],
})
node("head", walker, [0, EAR_HEIGHT, 0])

lanes = {name: node(name, road, [0, 0, z]) for name, (z, _) in LANES.items()}
for name, lane, abeam, kmh, gained, voice, params in VEHICLES:
    way = LANES[lane][1]
    node(name, lanes[lane], [walker_x(abeam), 0, 0], sound={
        # Seconds from the instant it is level with the listener, and km/h.
        "motion": {"alongM": [way, 0, 0], "atS": abeam,
                   "speedKmh": [[-45.0, kmh - 0.75 * gained], [45.0, kmh + 0.75 * gained]]},
        "emitters": [emitter("engine-and-tyres", voice, [0, 0.35, 0], VEHICLE_GAIN_DB[voice], params)],
        "touches": [{"kind": "wheels", "material": "rubber-tyre",
                     "pointsM": [[1.35, 0, 0], [-1.35, 0, 0]], "strength": 0.8}],
    })

# The viaduct: its top carries the tracks, its underside is a ceiling over
# the promenade.
viaduct = node("upper-deck", bridge, [0, VIADUCT_Y, 0], sound={"surfaces": [
    {"id": "top", "quadM": flat(0, -HALF_LENGTH, HALF_LENGTH, -VIADUCT_HALF_WIDTH, VIADUCT_HALF_WIDTH),
     "acoustic": "steel-deck", "reflects": False},
    {"id": "underside", "quadM": flat(-1.5, -HALF_LENGTH, HALF_LENGTH, -VIADUCT_HALF_WIDTH, VIADUCT_HALF_WIDTH),
     "acoustic": "steel-deck"},
    {"id": "runway-joint", "quadM": flat(RUNWAY_ABOVE_DECK + 0.005, JOINT_X - 0.3, JOINT_X + 0.3, -3.6, 3.6),
     "collision": "steel-runway-joint", "strikes": True},
]})
TRAIN = ["bridge", "upper-deck", "track-to-passy", "train"]
for k, x in enumerate(SPANS):
    node(f"span-{k + 1}", viaduct, [x, -1.5, 0], sound={"emitters": [emitter(
        "rumble", "metro-span", gain=TRAIN_GAIN_DB,
        loadedBy={"under": TRAIN, "voice": "metro-bogie", "radiusM": 18.0, "full": 2.4})]})
track = node("track-to-passy", viaduct, [0, RUNWAY_ABOVE_DECK, TRACK_Z])
# From afar the train is two emitters; nearer than this, each bogie and each
# motor car is heard for itself.
train = node("train", track, [walker_x(TRAIN_ABEAM_S), 0, 0], sound={
    "motion": {"alongM": [-1, 0, 0], "atS": TRAIN_ABEAM_S, "speedKmh": TRAIN_SPEED_KMH},
    "proxyBeyondM": 160.0,
    "emitters": [
        emitter("rolling", "metro-bogie", [0, 0.3, 0], TRAIN_GAIN_DB + 10.0),
        emitter("traction", "metro-motor", [0, 0.6, 0], TRAIN_GAIN_DB + 4.8),
    ],
})
for c in range(1, CARS + 1):
    sound = None
    if c in MOTOR_CARS:
        sound = {"emitters": [emitter("traction", "metro-motor", [0, 0.6, 0], TRAIN_GAIN_DB)]}
    car = node(f"car-{c}", train, [(c - (CARS + 1) / 2) * CAR_LENGTH, 0, 0], sound=sound)
    # Numbered along the train: a node's name is its own in the whole object.
    for b, x in enumerate((-BOGIE_FROM_MIDDLE, BOGIE_FROM_MIDDLE), start=2 * c - 1):
        node(f"bogie-{b}", car, [x, 0, 0], sound={
            "emitters": [emitter("tyres", "metro-bogie", [0, 0.3, 0], TRAIN_GAIN_DB)],
            "touches": [{"kind": "wheels", "material": "rubber-tyre-metro",
                         "pointsM": [[-AXLE_FROM_BOGIE, 0, 0], [AXLE_FROM_BOGIE, 0, 0]], "strength": 0.9}],
        })

# The Seine.
river = node("river", bridge, [0, -0.05, 0], sound={"surfaces": [
    {"id": "water-upstream", "quadM": flat(0, -HALF_LENGTH, HALF_LENGTH, -300.0, -DECK_HALF_WIDTH - 0.05),
     "acoustic": "water", "occludes": False},
]})
for name, x, z, gain, load, params in WATER:
    node(name, river, [x, 0, z], sound={"emitters": [emitter(
        "wash", "water", [0, 0.05, 0], gain, params, load=load, reflections=False, maxDistanceM=250.0)]})

# The city far off: one bed in each ear.
node("city", bridge, sound={"emitters": [
    emitter("bed-left", "city-bed", gain=CITY_GAIN_DB, type="global", pan=-1.0),
    emitter("bed-right", "city-bed", gain=CITY_GAIN_DB, type="global", pan=1.0),
]})

document = {
    "asset": {
        "version": "2.0",
        "generator": "osage paris-soundscape: scenes/make_bir_hakeim.py",
        "copyright": "SPDX-FileCopyrightText: The osage contributors. SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions",
        "extras": {
            "osageObject": {
                "version": 1,
                "id": "bir-hakeim-sound-test",
                "collisionMaterials": {
                    "asphalt": {"friction": 0.9, "restitution": 0.0},
                    "steel-plate": {"friction": 0.5, "restitution": 0.05},
                    "steel-runway-joint": {"friction": 0.6, "restitution": 0.0},
                    "rubber-sole": {"friction": 0.9, "restitution": 0.0},
                    "rubber-tyre": {"friction": 1.0, "restitution": 0.0},
                    "rubber-tyre-metro": {"friction": 1.0, "restitution": 0.0},
                },
                # Fractions of the energy at 250, 1 000 and 4 000 Hz. Made-up
                # test values in the range textbooks give for such surfaces.
                "acousticMaterials": {
                    "asphalt": {"absorption": [0.02, 0.03, 0.05], "transmission": [0.0, 0.0, 0.0], "scattering": 0.1},
                    "steel-deck": {"absorption": [0.08, 0.04, 0.03], "transmission": [0.02, 0.003, 0.0005],
                                   "scattering": 0.35},
                    "water": {"absorption": [0.008, 0.015, 0.025], "transmission": [0.0, 0.0, 0.0], "scattering": 0.05},
                    "stone": {"absorption": [0.02, 0.03, 0.05], "transmission": [0.0, 0.0, 0.0], "scattering": 0.2},
                },
            },
            "osageSound": {
                "version": 0,
                # How each collision material sounds when touched; of two
                # things touching, the softer sets how long the push lasts.
                "contactSounds": {
                    "asphalt": {"params": {"tone": 110.0, "ring": 0.045, "shape": 0, "scuff": 0.45}, "hardness": 0.6,
                                "gainDb": -8.0},
                    "steel-plate": {"params": {"tone": 190.0, "ring": 0.7, "shape": 1, "scuff": 0.08}, "hardness": 0.9,
                                    "gainDb": -2.0},
                    "steel-runway-joint": {"voice": "metro-joint", "hardness": 0.8, "gainDb": 6.0},
                    "rubber-sole": {"hardness": 0.3},
                    "rubber-tyre": {"hardness": 0.12, "gainDb": 8.0},
                    "rubber-tyre-metro": {"hardness": 0.12, "gainDb": 0.0},
                },
            },
        },
    },
    "scene": 0,
    "scenes": [{
        "name": "Pont de Bir-Hakeim, upstream pavement, 45 seconds",
        "nodes": [bridge],
        "extras": {"osageSound": {
            "seconds": SECONDS,
            "seed": 6,
            "loudnessLufs": -20.0,
            "listener": {"node": ["bridge", "road-deck", "pavement-upstream", "walker", "head"]},
            "reverb": {"secondsT60": [0.7, 0.55, 0.35], "gainDb": 0.0},
            "air": {"celsius": 12.0, "humidityPercent": 75.0},
        }},
    }],
    "nodes": nodes,
}


def written(value, indent=0):
    """JSON with one node, emitter or surface a line where it fits."""
    pad = "  " * indent
    short = json.dumps(value, ensure_ascii=False)
    if not isinstance(value, (dict, list)) or len(short) + len(pad) <= 118:
        return short
    if isinstance(value, dict):
        inner = [f'{pad}  {json.dumps(key)}: {written(item, indent + 1)}' for key, item in value.items()]
        return "{\n" + ",\n".join(inner) + f"\n{pad}}}"
    inner = [f"{pad}  {written(item, indent + 1)}" for item in value]
    return "[\n" + ",\n".join(inner) + f"\n{pad}]"


if __name__ == "__main__":
    path = HERE / "bir-hakeim.json"
    path.write_text(written(document) + "\n", encoding="utf-8")
    emitters = sum(len(n["extras"].get("osageSound", {}).get("emitters", [])) for n in nodes)
    print(f"{path.name}: {len(nodes)} nodes, {emitters} emitters")
