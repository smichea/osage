#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare the native capsule's finite sweep, delayed terrain and conservative fallback."""

import argparse
import hashlib
import json
import math
import resource
import runpy
import statistics
import tempfile
from pathlib import Path

from paging_checks import require

HERE = Path(__file__).resolve().parent
DIAGONAL = runpy.run_path(str(HERE / "check-residual-triangle-paths.py"))
FACE = DIAGONAL["FACES"]
AUDIO = FACE["AUDIO"]
PACKED = runpy.run_path(str(HERE / "check-packed-collision-reader.py"))


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def take(template, ident, distance, foot=5, velocity=(0, 0, 0), ticks=85):
    offset = distance / math.sqrt(2)
    return dict(
        template["routes"][0],
        id=ident,
        start_m=[-54 - offset, foot, 14 - offset],
        velocity_m_s=list(velocity),
        ticks=ticks,
        expected_support="ramp",
        camera=[
            {"tick": t, "eye": [-55, 6, 18], "aim": [-54, 3, 14]} for t in (0, ticks)
        ],
    )


def isolated_scene(out):
    source, replay = AUDIO["SAMPLE"](out)
    base, chunk = FACE["GLB"]["read_glb"](source)
    scene = out / "single-face.glb"
    face = [(-56, 3, 16), (-52, 3, 12), (-52, 3, 16)]
    AUDIO["EDGE"]["write_geometry"](base, chunk, scene, [face], [0.5] * 3, True, False)
    tree, chunk = FACE["GLB"]["read_glb"](scene)
    node = tree["nodes"][-1]["extras"]["osage"]
    node["collision"] = [
        {"id": "ramp", "material": "surface-7", "mesh": node["acoustic"][0]["mesh"]}
    ]
    FACE["GLB"]["write_glb"](scene, tree, chunk)
    pages = out / "single-face-pages"
    AUDIO["COOK"](scene, pages, 32, coarse_storage="ovc1")
    return pages, AUDIO["read"](replay)


def attempt(binary, pages, replay, route, mode, out, actor=None, delay=55):
    receipt = out / f"{route}-{mode}.json"
    flags = (
        ["--all-resident"]
        if mode == "resident"
        else [
            "--watch",
            "-2,0",
            "--hold",
            "physics",
            "--hold-ticks",
            str(delay),
            "--delay-ms",
            "0",
        ]
    )
    if actor:
        flags += ["--actor", actor]
    FACE["command"](
        binary,
        "--paging",
        "--pages",
        pages,
        "--replay",
        replay,
        "--route",
        route,
        "--trace-body",
        "--out",
        receipt,
        *flags,
    )
    result = AUDIO["read"](receipt)
    body = result["body_before_step"]
    held = [s["tick"] for s in body if s["tick"] and not s["step"]]
    counters = result["support"]["stats"]
    require(
        len(held) == counters["held_ticks"],
        "guard counters differ from the solver clock",
    )
    hit = result["first_contact"]
    trace = [[b["center_m"], b["velocity_m_s"], b["step"]] for b in body]
    components = [
        [
            b.get("parts_after_step"),
            b.get("boxes_after_step"),
            b.get("contacts_after_step"),
        ]
        for b in body
    ]
    return {
        "held": len(held),
        "held_interval": [held[0], held[-1]] if held else [],
        "previews": counters.get("capsule_sweep_previews", 0),
        "cleared": counters.get("capsule_sweep_clear_ticks", 0),
        "first": None
        if not hit
        else [hit["tick"], hit.get("source_triangle"), hit["support"]],
        "first_solver_step": None
        if not hit
        else sum(b["step"] for b in body if b["tick"] <= hit["tick"]),
        "end": result["final_foot_m"],
        "min_foot_y_m": min(b["foot_m"][1] for b in body),
        "trace_sha256": hashlib.sha256(json.dumps(trace).encode()).hexdigest(),
        "components_sha256": hashlib.sha256(
            json.dumps(components).encode()
        ).hexdigest(),
        "bytes_read": result["storage"]["cold_file_bytes"],
    }


def measure(binary, out, district, template, isolated, plain):
    out.mkdir(exist_ok=True)
    distances = [
        -0.7,
        -0.1,
        0.0,
        0.1,
        0.3,
        0.394,
        0.401,
        0.45,
        0.7,
        1.2,
        0.2799,
        0.28,
        0.2805,
        0.282,
        0.285,
    ]
    setups = [
        (take(template, f"vertical-{i}", d), district, 55, None, 9.81)
        for i, d in enumerate(distances)
    ]
    setups.append(
        (
            take(template, "oblique-coupled", 0.3, velocity=(0.15, 0, -0.15)),
            district,
            55,
            None,
            9.81,
        )
    )
    setups.append((take(template, "human", 0.3), district, 55, "human", 9.81))
    bonnet = take(template, "box", 0.3)
    bonnet["bonnet"] = dict(
        AUDIO["read"](HERE / "fixtures/bonnet.json")["routes"][0]["bonnet"],
        center_m=[1000, 2, 1000],
    )
    setups.append((bonnet, district, 55, None, 9.81))
    setups.append((take(template, "unknown-shape", 0.3), plain, 55, None, 9.81))
    for label, d, v in [
        ("oblique-free", 0.33, (0.15, -0.30, -0.15)),
        ("oblique-near-speculation", 0.30, (0.15, -0.30, -0.15)),
        ("oblique-tangent", 0.28, (0.15, -0.30, -0.15)),
        ("oblique-crossing", 0.30, (0.55, -0.30, 0.55)),
        ("oblique-within-speculation", 0.30, (0.15, -12.0, -0.15)),
    ]:
        setups.append(
            (
                take(template, label, d, foot=3.11, velocity=v, ticks=95),
                isolated,
                75,
                None,
                0.01,
            )
        )
    for d in (0.3, 0.394):
        setups.append(
            (take(template, f"coarse-{d}", d, ticks=50), district, 28, None, 9.81)
        )
    result = {}
    for route, pages, delay, actor, gravity in setups:
        name = route["id"]
        replay = out / f"{name}-route.json"
        replay.write_text(
            json.dumps(
                dict(
                    template,
                    tick_hz=20 if name.startswith("coarse-") else 60,
                    gravity_m_s2=gravity,
                    routes=[route],
                )
            )
        )
        result[name] = {
            mode: attempt(
                binary, pages, replay, name, mode, out, actor=actor, delay=delay
            )
            for mode in ["resident", "delayed"]
        }
        a, b = [
            AUDIO["read"](out / f"{name}-{mode}.json")["body_before_step"]
            for mode in ("resident", "delayed")
        ]
        result[name]["max_center_delta_m"] = max(
            math.dist(p["center_m"], q["center_m"]) for p, q in zip(a, b, strict=True)
        )
        result[name]["max_velocity_delta_m_s"] = max(
            math.dist(p["velocity_m_s"], q["velocity_m_s"])
            for p, q in zip(a, b, strict=True)
        )
        result[name]["first_state_difference"] = next(
            (
                p["tick"]
                for p, q in zip(a, b, strict=True)
                if (p["center_m"], p["velocity_m_s"])
                != (q["center_m"], q["velocity_m_s"])
            ),
            None,
        )
        stepped_a = [p for p in a if p["step"]]
        stepped_b = [p for p in b if p["step"]]
        result[name]["solver_aligned_max_center_delta_m"] = max(
            math.dist(p["center_m"], q["center_m"])
            for p, q in zip(stepped_a, stepped_b, strict=False)
        )
    return result


def qualify(current, previous):
    for name, row in current.items():
        resident, delayed = row["resident"], row["delayed"]
        old = previous[name]["delayed"]
        require(
            resident["trace_sha256"] == previous[name]["resident"]["trace_sha256"],
            f"{name}: the all-resident trajectory changed",
        )
        require(
            delayed["bytes_read"] == old["bytes_read"],
            f"{name}: changed the district-byte total",
        )
        if not delayed["held"]:
            require(
                row["max_center_delta_m"]
                <= (0.0001 if name.startswith("coarse-") else 0.00005)
                and row["max_velocity_delta_m_s"] < 1e-6,
                f"{name}: changed an unstalled body",
            )
        if name.startswith("vertical-"):
            i = int(name.split("-")[1])
            protected = i in {0, 1, 2, 3, 10, 11, 12}
            require(
                delayed["held"] == (17 if protected else 0),
                f"{name}: incorrect protection {delayed}",
            )
            require(
                delayed["first"] is not None
                and delayed["first"][0] == 39 + delayed["held"],
                f"{name}: timing",
            )
            if i in {4, 5, 13, 14}:
                require(
                    old["held"] == 17 and delayed["cleared"] == 17,
                    f"{name}: missing improvement",
                )
        elif name == "oblique-free":
            require(
                delayed["held"] == 0
                and delayed["cleared"] > 0
                and old["held"] > 0
                and not delayed["first"]
                and row["first_state_difference"] is None,
                f"{name}: did not pass the isolated side with all resident states",
            )
        elif name in {"human", "box", "unknown-shape"}:
            require(
                delayed["held"] == old["held"]
                and delayed["trace_sha256"] == old["trace_sha256"]
                and delayed["components_sha256"] == old["components_sha256"]
                and delayed["previews"] == 0,
                f"{name}: changed unsupported route",
            )
        elif name.startswith("coarse-"):
            require(
                delayed["held"] <= old["held"]
                and delayed["first"][1:] == resident["first"][1:],
                "unexpected coarse-clock support",
            )
            require(
                delayed["min_foot_y_m"] > 2.999,
                "adopted a partial support that loses its next contact",
            )
            if name == "coarse-0.394":
                require(
                    delayed["held"] == old["held"]
                    and delayed["cleared"] == 0
                    and row["solver_aligned_max_center_delta_m"] == 0.0,
                    "adopted the unstable 20 Hz manifold before full geometry arrived",
                )
        else:
            require(
                delayed["held"] > 0,
                f"{name}: failed to hold a tangent, crossing or unbounded contact",
            )
            if name == "oblique-coupled":
                require(
                    delayed["previews"] > delayed["cleared"],
                    "coupled lateral impulse did not trigger fallback",
                )
            if name == "oblique-within-speculation":
                require(
                    delayed["held_interval"][0] == resident["first"][0] == 2
                    and row["first_state_difference"] == 3
                    and delayed["cleared"] == 1,
                    "fast speculative reaction was passed without the missing triangle",
                )
            if name == "oblique-crossing":
                require(
                    delayed["held_interval"][0] <= resident["first"][0]
                    and delayed["first_solver_step"] == resident["first_solver_step"]
                    and old["first_solver_step"] > resident["first_solver_step"],
                    "the true crossing was not protected before its speculative contact",
                )
            if name == "oblique-near-speculation":
                require(
                    0 < delayed["held"] < old["held"]
                    and delayed["first"] is None
                    and resident["first"] is None,
                    "unresolved near pass lost its conservative wait",
                )


def timing(binary, out, pages, template, ticks=5000):
    # Both offsets fall onto the same available horizontal face. Each takes the full
    # number of solver steps. Only the closer one needs to certify missing geometry.
    routes = [
        take(template, "disputed", 0.3, ticks=ticks),
        take(template, "wide", 0.45, ticks=ticks),
    ]
    replay = out / "timing-routes.json"
    replay.write_text(json.dumps(dict(template, routes=routes)))

    def cpu():
        usage = resource.getrusage(resource.RUSAGE_CHILDREN)
        return usage.ru_utime + usage.ru_stime

    samples = []
    for trial in range(8):
        sample = {}
        for route in routes if trial % 2 == 0 else routes[::-1]:
            label = route["id"]
            receipt = out / f"timing-{label}.json"
            start = cpu()
            FACE["command"](
                binary,
                "--paging",
                "--pages",
                pages,
                "--route",
                label,
                "--replay",
                replay,
                "--watch",
                "-2,0",
                "--hold",
                "physics",
                "--hold-ticks",
                str(ticks + 2),
                "--delay-ms",
                "0",
                "--out",
                receipt,
            )
            elapsed = cpu() - start
            row = AUDIO["read"](receipt)
            stats = row["support"]["stats"]
            require(
                row["first_contact"]["tick"] == 39 and stats["held_ticks"] == 0,
                "timing changed physical work",
            )
            sample[label] = {
                "process_cpu_s": elapsed,
                "confirmed_ticks": stats.get("capsule_sweep_clear_ticks", 0),
            }
        if trial:
            samples.append(sample)
    costs = [
        (s["disputed"]["process_cpu_s"] - s["wide"]["process_cpu_s"])
        * 1e6
        / s["disputed"]["confirmed_ticks"]
        for s in samples
    ]
    return {
        "ticks": ticks,
        "pairs": samples,
        "incremental_cpu_us_per_disputed_tick_median": statistics.median(costs),
        "incremental_cpu_us_per_disputed_tick_range": [min(costs), max(costs)],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("bin", "reference", "out"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument(
        "--timing",
        action="store_true",
        help="measure seven paired whole-process CPU runs",
    )
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    inputs = out / "inputs"
    inputs.mkdir(exist_ok=True)
    pages, _, _ = DIAGONAL["setup"](inputs)
    single = out / "single-triangle"
    single.mkdir(exist_ok=True)
    isolated, template = isolated_scene(single)
    with tempfile.TemporaryDirectory(dir=inputs) as temp:
        plain = Path(temp)
        PACKED["CORNER"]["variant"](
            pages, plain, lambda doc: doc.pop("residual_triangle_corners")
        )
        indexes = [digest(p / "index.json") for p in (pages, isolated, plain)]
        current = measure(
            args.bin.resolve(), out / "current", pages, template, isolated, plain
        )
        previous = measure(
            args.reference.resolve(),
            out / "reference",
            pages,
            template,
            isolated,
            plain,
        )
    summary = {
        "bin_sha256": digest(args.bin),
        "reference_sha256": digest(args.reference),
        "page_indexes_sha256": indexes,
        "current": current,
        "reference": previous,
    }
    if args.timing:
        summary["timing"] = timing(args.bin.resolve(), out, pages, template)
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    qualify(current, previous)
    print(
        f"{len(current)} capsule and fallback routes, resident and delayed on both releases"
    )


if __name__ == "__main__":
    main()
