# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Oracles for source ownership, delayed supports, handoff and empty space."""

import json
import math
import struct
import subprocess
import sys
import zlib
from collections import Counter
from pathlib import Path

from paging_support_storage import unpack_support
from paging_coarse import unpack_coarse
from paging_collision_codec import read_collision

HERE = Path(__file__).resolve().parent


def require(ok, why):
    if not ok:
        raise RuntimeError(why)


def glb(raw):
    size = struct.unpack_from("<I", raw, 12)[0]
    return json.loads(raw[20 : 20 + size]), raw[20 + size :]


def visual_owners(raw):
    if raw[:4] in (b"OVC1", b"OVC2", b"OVCU"):
        raw = unpack_coarse(raw)
    elif raw[:4] == b"OVP1":
        raw = zlib.decompress(raw[8:])
        size = struct.unpack_from("<I", raw)[0]
        order = json.loads(raw[4 : 4 + size])
        local = visual_owners(raw[4 + size :]) if len(raw) > 4 + size else []
        return [local[v] if isinstance(v, int) else v[1] for v in order]
    doc, _ = glb(raw)
    return [
        doc["nodes"][i]["extras"]["osage_source_node"]
        for i in doc["scenes"][doc["scene"]]["nodes"]
    ]


def upwards(f):
    a, b, c = f
    u = [y - x for x, y in zip(a, b)]
    v = [y - x for x, y in zip(a, c)]
    n = (
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    )
    return n[1] > math.hypot(*n) * 0.5


def audit(pages, idx):
    parent_owners = visual_owners((pages / idx["parent"]["file"]).read_bytes())
    catalogue = json.loads(
        unpack_support((pages / idx["support"]["file"]).read_bytes())
    )
    by_page = iter(catalogue["pages"])
    count = 0
    owners = set()
    for t in idx["tiles"]:
        if "physics" not in t:
            continue
        full = read_collision(pages, t["physics"])
        proxy = next(by_page)
        visual = set(visual_owners((pages / t["visual"]["file"]).read_bytes()))
        coarse = set(
            parent_owners[t["parent_first"] : t["parent_first"] + t["parent_count"]]
        )
        for s, (owner, vertices, tris) in zip(full, proxy, strict=True):
            require(
                owner == s["source_node"] and owner in visual and owner in coarse,
                "full, support and rendering owners diverge",
            )
            source_faces = [
                tuple(tuple(s["vertices"][i]) for i in tri) for tri in s["triangles"]
            ]
            proxy_faces = [tuple(tuple(vertices[i]) for i in tri) for tri in tris]
            require(
                Counter(proxy_faces) == Counter(f for f in source_faces if upwards(f)),
                "support changed a full triangle",
            )
            count += len(tris)
            owners.add(owner)
    require(next(by_page, None) is None, "extra proxy page")
    return {
        "source_nodes": len(idx["source_parents"]),
        "nodes_with_parent": sum(p is not None for p in idx["source_parents"]),
        "distinct_physical_owners": len(owners),
        "matched_upward_triangles": count,
        "full_detail_coarse_and_proxy_owners_match": True,
        "proxy_bytes": idx["support"]["bytes"],
        "proxy_decoded_bytes": idx["support"]["decoded_bytes"],
    }


def route(id, start, ticks, support, source):
    # The camera never serves as a collision predictor.
    return {
        "id": id,
        "start_m": start,
        "velocity_m_s": [0, 0, 0],
        "ticks": ticks,
        "expected_support": support,
        "camera": [
            {**source["camera"][0], "tick": 0},
            {**source["camera"][-1], "tick": ticks},
        ],
    }


def qualify_guard(run, replay, out, refs, phys, visual, slow):
    extended = []
    for source in replay["routes"]:
        end = max(source["ticks"], refs[source["id"]]["tick"] + 100)
        extended.append(
            {
                **source,
                "ticks": end,
                "camera": [
                    *source["camera"][:-1],
                    source["camera"][-1],
                    {**source["camera"][-1], "tick": end},
                ],
            }
            if end > source["ticks"]
            else source
        )
    extra = [
        route("between-decks", [0, 12, 0], 160, "road", replay["routes"][1]),
        route("above-both", [0, 23, 0], 200, "rail", replay["routes"][1]),
        route("open-river", [44, 23, 40], 180, "rail", replay["routes"][1]),
    ]
    p = out / "delayed-support.json"
    p.write_text(json.dumps({**replay, "routes": [*extended, *extra]}, indent=2) + "\n")
    result = {}
    for r in [*extended, *extra[:2]]:
        name = r["id"]
        ref = refs.get(name)
        if not ref:
            ref = run(
                "guard-reference-" + name, name, extra=["--replay", p, "--all-resident"]
            )["first_contact"]
        cell = ref["tile"]
        hold = ref["tick"] + 24
        options = ["--replay", p, "--hold", "physics", "--hold-ticks", hold]
        good = run("guard-late-" + name, name, phys, visual, slow, cell, options)
        bad = run(
            "unguarded-late-" + name,
            name,
            phys,
            visual,
            slow,
            cell,
            [*options, "--late-collision", "unprotected"],
        )
        first = good["first_contact"]
        state = good["support"]
        full = state["first_full_contact"]
        handoff = next(
            e["tick"]
            for e in state["events"]
            if e["state"] == "full_handoff" and e["tile"] == cell
        )
        require(
            first
            and first["geometry"] == "certified_support"
            and first["support"] == r["expected_support"]
            and first["source_node"] == ref["source_node"],
            "wrong fallback owner " + name,
        )
        require(
            first["tick"] == ref["tick"]
            and math.dist(first["point"], ref["point"]) < 0.01,
            "proxy changed first landing " + name,
        )
        require(
            good["at_contact"]["visual_has_same_source"]
            and not good["at_contact"]["landing_tile_has_physics"],
            "missing cross-representation identity " + name,
        )
        require(
            full
            and full["source_node"] == ref["source_node"]
            and full["tick"] >= hold
            and abs(full["point"][1] - ref["point"][1]) < 0.03,
            "full collision did not take over original landing " + name,
        )
        require(
            state["stats"]["full_handoffs"]
            and state["minimum_foot_y_after_contact_m"] > ref["point"][1] - 0.04,
            "support dropped during handoff " + name,
        )
        no = bad["first_contact"]
        require(
            not no
            or no["tick"] > ref["tick"]
            or no["source_node"] != ref["source_node"],
            "unprotected control unexpectedly met deadline " + name,
        )
        result[name] = {
            "held_until_tick": hold,
            "reference_tick": ref["tick"],
            "first_proxy_contact": first,
            "first_full_contact": full,
            "full_installed_tick": handoff,
            "minimum_foot_y_after_contact_m": state["minimum_foot_y_after_contact_m"],
            "source_path": good["at_contact"]["source_path"],
            "visual_has_same_source": True,
            "peak_active_proxy_pages": state["stats"]["peak_active_pages"],
            "peak_active_proxy_triangles": state["stats"]["peak_active_triangles"],
            "unprotected_first_contact": no,
            "unprotected_final_foot_m": bad["final_foot_m"],
        }
    protected, unprotected = [
        run(
            "guard-void-" + mode,
            "open-river",
            phys,
            visual,
            10_000,
            [0, 0],
            ["--replay", p, "--late-collision", mode],
        )
        for mode in ("proxy", "unprotected")
    ]
    require(
        not protected["first_contact"]
        and protected["final_foot_m"] == unprotected["final_foot_m"],
        "fallback created ground in the open Seine",
    )
    result["open-river"] = {
        "first_contact": None,
        "final_foot_m": protected["final_foot_m"],
        "matches_unprotected": True,
        "peak_active_proxy_pages": protected["support"]["stats"]["peak_active_pages"],
    }
    natural = {}
    for name, ref in refs.items():
        run_id = "guard-2s-" + name
        tested = run(run_id, name, phys, visual, 2000, ref["tile"], ["--replay", p])
        first = tested["first_contact"]
        later = tested["support"]["first_full_contact"]
        require(
            first
            and first["geometry"] == "certified_support"
            and first["source_node"] == ref["source_node"]
            and first["tick"] == ref["tick"]
            and later
            and later["tick"] > first["tick"],
            "ordinary slow read did not recover the landing on " + name,
        )
        natural[name] = {
            "run": run_id,
            "first_support_tick": first["tick"],
            "full_contact_tick": later["tick"],
            "source_node": first["source_node"],
        }
    result["unforced_2000_ms_reads"] = natural
    return result


def qualify_tree(run, src, replay, out):
    tree = out / "tree-probe"
    tree.mkdir(exist_ok=True)
    doc, tail = glb(src.read_bytes())
    prototype = next(n for n in doc["nodes"] if n.get("name") == "support/rail")
    beam = {
        "name": "support/beam",
        "mesh": prototype["mesh"],
        "scale": [192, 2, 12],
        "translation": [0, 7, 0],
    }
    doc["nodes"] = [
        {
            "name": "assembly",
            "children": [1, 3],
            "matrix": [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 8, 1, 0, 1],
        },
        {"name": "deck", "children": [2], "translation": [0, 2, 0]},
        beam,
        {"name": "deck", "children": [4], "translation": [0, 5, 20]},
        beam,
    ]
    doc["scenes"] = [{"nodes": [0]}]
    doc["scene"] = 0
    desc = json.dumps(doc, separators=(",", ":")).encode()
    desc += b" " * (-len(desc) % 4)
    original = tree / "assembly.glb"
    original.write_bytes(
        struct.pack(
            "<4sIIII", b"glTF", 2, 20 + len(desc) + len(tail), len(desc), 0x4E4F534A
        )
        + desc
        + tail
    )
    path = tree / "drop.json"
    path.write_text(
        json.dumps(
            {
                **replay,
                "routes": [
                    route("nested-a", [8, 20, 0], 180, "beam", replay["routes"][1]),
                    route("nested-b", [8, 20, 20], 180, "beam", replay["routes"][1]),
                ],
            },
            indent=2,
        )
        + "\n"
    )
    trials = []
    for edge in (64, 128):
        cells = tree / str(edge)
        subprocess.run(
            [
                sys.executable,
                str(HERE / "cook-pages.py"),
                str(original),
                str(cells),
                "--edge",
                str(edge),
            ],
            check=True,
        )
        idx = json.loads((cells / "index.json").read_bytes())
        check = audit(cells, idx)
        require(
            check["distinct_physical_owners"] == 2
            and idx["source_parents"] == [None, 0, 1, 0, 3]
            and len(idx["tiles"]) > 1,
            "tree ownership changed on tiling",
        )
        for name, ancestry, height in (
            ("nested-a", [0, 1, 2], 11),
            ("nested-b", [0, 3, 4], 14),
        ):
            full = run(
                f"tree-full-{edge}-{name}",
                name,
                extra=["--replay", path, "--pages", cells, "--all-resident"],
            )
            missing = run(
                f"tree-proxy-{edge}-{name}",
                name,
                64,
                48,
                600,
                full["first_contact"]["tile"],
                [
                    "--replay",
                    path,
                    "--pages",
                    cells,
                    "--hold",
                    "physics",
                    "--hold-ticks",
                    180,
                ],
            )
            for r in (full, missing):
                require(
                    r["at_contact"]["source_path"] == ancestry
                    and r["at_contact"]["visual_has_same_source"]
                    and r["first_contact"]["support"] == "beam"
                    and abs(r["first_contact"]["point"][1] - height) < 0.01,
                    "nested transformed mesh lost ancestry or height",
                )
            trials.append(
                {
                    "tile_edge_m": edge,
                    "tiles": len(idx["tiles"]),
                    "source_path": ancestry,
                    "full_tick": full["first_contact"]["tick"],
                    "proxy_tick": missing["first_contact"]["tick"],
                    "world_y_m": missing["first_contact"]["point"][1],
                }
            )
    return {
        "source_file": "tree-probe/assembly.glb",
        "source_nodes": 5,
        "trials": trials,
    }
