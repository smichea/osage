#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Writes bir-hakeim-recorded.json next to this file: the bridge of
bir-hakeim.json, with voices played from recordings (issue #22).

    python3 tools/paris-soundscape/scenes/make_recorded.py

- The scooter's engine is the kart's three CC0 loops of a small two-stroke
  (assets/freesound/kart/engine-*.ogg, made by tools/make-sounds.py), each
  at the speed its engine speed stands for: idle at rest, 7 000 turns a
  minute at 8.8 m/s, 9 900 at 12.5 m/s (the kart's own figures, its top
  speed taken as the scooter's). The synthesised scooter stays under it, 9
  dB down, for its tyres.
- A blow on the expansion joint's steel plate plays one of the kart's
  three knocks, over the synthesised ring of the plate, 6 dB down.

The recordings are read from the assets' folder (`--assets`, default
`assets`): fetched as tools/fetch-assets.py and the manifest say.
"""

import json
import pathlib

HERE = pathlib.Path(__file__).resolve().parent


def main():
    scene = json.loads((HERE / "bir-hakeim.json").read_text(encoding="utf-8"))
    sound = scene["asset"]["extras"]["osageSound"]
    sound["clips"] = {
        "engine-idle": {"uri": "freesound/kart/engine-idle.ogg", "loop": True},
        "engine-mid": {"uri": "freesound/kart/engine-mid.ogg", "loop": True},
        "engine-high": {"uri": "freesound/kart/engine-high.ogg", "loop": True},
        "knock-1": {"uri": "freesound/kart/knock-1.ogg"},
        "knock-2": {"uri": "freesound/kart/knock-2.ogg"},
        "knock-3": {"uri": "freesound/kart/knock-3.ogg"},
    }
    sound["contactSounds"]["steel-plate"]["recorded"] = {
        "clips": ["knock-1", "knock-2", "knock-3"],
        "layerGainDb": -6.0,
    }
    found = 0
    for node in scene["nodes"]:
        if node.get("extras", {}).get("osage", {}).get("id") != "scooter":
            continue
        for emitter in node["extras"]["osageSound"]["emitters"]:
            emitter["recorded"] = {
                "follows": "speed",
                "clips": [
                    {"clip": "engine-idle", "at": 0.0},
                    {"clip": "engine-mid", "at": 8.8},
                    {"clip": "engine-high", "at": 12.5},
                ],
                "layerGainDb": -9.0,
            }
            found += 1
    assert found == 1, "the bridge has one scooter"
    scene["asset"]["extras"]["osageObject"]["id"] = "bir-hakeim-recorded"
    out = HERE / "bir-hakeim-recorded.json"
    out.write_text(json.dumps(scene, indent=2) + "\n", encoding="utf-8")
    print(out.name)


if __name__ == "__main__":
    main()
