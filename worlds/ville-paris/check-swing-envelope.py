#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Replay mixed swing and twist from rest and measure the first reaction and subsequent orientation."""
import argparse
import csv
import json
import math
from pathlib import Path
import subprocess
import sys
import xml.etree.ElementTree as ET

from impact_geometry import basis, inverse, multiply, read_poses

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / "tools/object-convert"))
sys.path.insert(0, str(ROOT / "tools/object-convert/fixtures"))
from convert import convert
from make_pedestrian import write_fragment
from rig_math import rotate

TICKS = 12


def axis_angle(axis, degrees):
    a = math.radians(degrees) / 2
    return tuple(x * math.sin(a) for x in axis) + (math.cos(a),)


def angle(q):
    return math.degrees(2 * math.atan2(math.hypot(*q[:3]), abs(q[3])))


def excess(q):
    """Measure angular ellipse in the tangent plane of swing and signed X twist."""
    x, y, z, w = (v / math.hypot(*q) for v in q)
    twist = (math.degrees(2 * math.atan2(x, w)) + 180) % 360 - 180
    # The X column of the relative rotation is the swung twist direction.
    d = basis((x, y, z, w))[0]
    r = math.hypot(d[1], d[2])
    swing = math.degrees(math.atan2(r, d[0]))
    width = r / math.hypot(d[2] / 20, d[1] / 5) if r else 20
    return max(0., swing - width), max(0., abs(twist) - 45.)


def make(out):
    xml = out / "arm.yft.xml"
    write_fragment(xml, [("trunk", None, (0,.18,-.2), (0,.18,0), .035,3.,None),
                         ("muzzle", 0, (0,.18,0), (0,.35,.1), .03,1.,(20,5,45))],
                   "arm", ((-.1,-.3,0),(.1,.3,.5)), (0,0,.2), .6,.6)
    for label, options in (("independent", {}), ("ellipse", {"swing_envelope":"angular-ellipse"})):
        glb, _ = convert(object_id="arm", fragment=xml, rig_space="actor", **options)
        (out / (label + ".glb")).write_bytes(glb)
    doc = json.loads(glb[20:20+int.from_bytes(glb[12:16],"little")])
    # Same two rigid bodies and anchor. Wide limits keep every test pose free.
    wide = ET.parse(xml)
    limits = wide.find("Physics/LOD1/ArticulatedBody/Joints/Item/UnknownA0")
    for key in "xyz":
        limits.set(key, str(math.radians(175)))
    wide_path = out / "free.yft.xml"
    wide.write(wide_path)
    free, _ = convert(object_id="arm", fragment=wide_path, rig_space="actor")
    (out / "free.glb").write_bytes(free)
    bodies = doc["asset"]["extras"]["osageObject"]["articulation"]["bodies"]
    joint = bodies[1]["joint"]
    assert joint["swingEnvelope"] == "angular-ellipse"
    assert angle(joint["frameXyzw"]) < .0001
    pivot = joint["anchorM"]
    routes, expected = [], {}
    for az in (0,15,30,45,60,75,90,135,210,285):
        a = math.radians(az)
        for lean in (2,6,8,16,24,44):
            for twist in (-65,-35,0,35,65):
                name = f"{az}_{lean}_{twist}"
                q = multiply(axis_angle((0,math.cos(a),math.sin(a)),lean), axis_angle((1,0,0),twist))
                samples = []
                for i, b in enumerate(bodies):
                    pos, rot = b["centerM"], b["rotationXyzw"]
                    if i:
                        arm = rotate(basis(q), [v-p for v,p in zip(pos,pivot)])
                        pos = [p+v for p,v in zip(pivot,arm)]
                        rot = multiply(q,rot)
                    pose = dict(center_m=pos,rotation_xyzw=rot)
                    samples.append(dict(name=b["name"], current=pose, previous=pose))
                expected[name] = q
                routes.append(dict(id=name,ticks=TICKS,start_m=[32,80,8],velocity_m_s=[0,0,0],
                                   expected_support="road",ragdoll_activation=dict(sample_interval_s=1/60,parts=samples),
                                   camera=[dict(tick=t,eye=[35,83,12],aim=[32,81,8]) for t in (0,TICKS)]))
    replay = dict(version=1,tick_hz=60,gravity_m_s2=9.81,terminal_m_s=55,viewport=[640,480],
                  vertical_fov_deg=42,routes=routes)
    (out / "poses.json").write_text(json.dumps(replay))
    return expected


def evaluate(directory, expected, free):
    meta = json.loads((directory/"dynamics.json").read_bytes())["run"]
    binds = [b["rotation_xyzw"] for b in meta["parts"]]
    cases = {name: {"initial_excess_deg":excess(q), "peak_relative_rad_s":0.} for name,q in expected.items()}
    with (directory / "dynamics.csv").open() as f:
        for row in csv.DictReader(f):
            name, tick = row["route"], int(row["tick"])
            assert not json.loads(row["contacts_json"])
            poses = read_poses(row)
            p,c = [multiply(b["rotation_xyzw"],inverse(bind)) for b,bind in zip(poses,binds,strict=True)]
            speed = math.dist(*(b["angular_velocity"] for b in poses))
            cases[name]["peak_relative_rad_s"] = max(cases[name]["peak_relative_rad_s"], speed)
            q = multiply(inverse(p), c)
            if tick == 0:
                assert angle(multiply(q,inverse(expected[name]))) < .001
            if tick == 1:
                cases[name]["first_change_deg"] = angle(multiply(q,inverse(expected[name])))
                if free is None:
                    cases[name]["first_xyzw"] = q
                else:
                    cases[name]["response_deg"] = angle(multiply(q,inverse(free[name]["first_xyzw"])))
            if tick == TICKS:
                cases[name]["last_excess_deg"] = excess(q)
                cases[name]["last_relative_rad_s"] = speed
    false_positive = false_negative = 0
    for case in cases.values():
        # Subtract the common anchor correction by comparing orientations with the free trial.
        reacted = case.get("response_deg", 0.) > .002
        outside = max(case["initial_excess_deg"]) > .001
        false_positive += int(reacted and not outside)
        false_negative += int(not reacted and outside)
    return dict(inside_reacted=false_positive, outside_missed=false_negative,
                final_worst_deg=max(max(c["last_excess_deg"]) for c in cases.values()), cases=cases)


def run(binary, out):
    out.mkdir(parents=True, exist_ok=True)
    expected = make(out)
    results = {}
    for engine in ("rapier","jolt"):
        free = None
        for mode in ("free","independent","ellipse"):
            directory = out / (engine+"-"+mode)
            args = [binary,"--scene",HERE/"fixtures/bir-hakeim.glb","--actor-rig",out/(mode+".glb"),
                    "--replay",out/"poses.json","--physics",engine]
            for flag in ("--bench","--verify"):
                subprocess.run([*args,flag,directory],check=True,stdout=subprocess.DEVNULL)
            result = evaluate(directory, expected, free)
            if mode == "free":
                free = result["cases"]
            else:
                results[directory.name] = result
        marked = results[engine+"-ellipse"]
        independent = results[engine+"-independent"]
        assert len(marked["cases"]) == 300
        outside = sum(max(c["initial_excess_deg"]) > .001 for c in marked["cases"].values())
        assert outside == 237
        assert (marked["inside_reacted"], marked["outside_missed"]) == (0, 0), engine
        assert marked["final_worst_deg"] < .05, (engine, marked["final_worst_deg"])
        assert marked["final_worst_deg"] < independent["final_worst_deg"], engine
    (out/"measurements.json").write_text(json.dumps(results,indent=2))
    for label,r in results.items():
        print(label,{k:v for k,v in r.items() if k!="cases"})


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--bin",type=Path,default=ROOT/"target/release/ville-paris")
    p.add_argument("--out",type=Path,required=True)
    args = p.parse_args()
    run(args.bin.resolve(), args.out)
