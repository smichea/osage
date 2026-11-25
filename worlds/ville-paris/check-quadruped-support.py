#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Check measured support against raw contacts on the same three passive quadruped routes."""
import argparse
import csv
import json
import math
from pathlib import Path
import lift_approach_checks
import lift_response_checks
import runpy
import struct
import subprocess
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
SNAPSHOT = runpy.run_path(str(HERE / "check-moving-surface-rays.py"))["snapshot"]


def f32(x):
    return struct.unpack("f", struct.pack("f", x))[0]


def orientation(a, b, c):
    return (b[0]-a[0])*(c[1]-a[1]) - (b[1]-a[1])*(c[0]-a[0])


def boundary(points):
    # Gift wrapping walks boundary edges directly from the leftmost point.
    points = sorted(set(points))
    if len(points) < 3:
        return points
    hull = [points[0]]
    while True:
        a = hull[-1]
        b = next(q for q in points if q != a)
        for c in points:
            turn = orientation(a, b, c)
            if turn < 0 or (turn == 0 and math.dist(a,c) > math.dist(a,b)):
                b = c
        if b == hull[0]:
            return hull
        hull.append(b)


def verify(snapshot, masses, poses, contacts):
    centre = [sum(m*f32(p["center"][axis]) for m,p in zip(masses,poses,strict=True))/sum(masses)
              for axis in range(3)]
    assert math.dist(centre, snapshot["center_of_mass_m"]) < 1e-9
    velocity = [sum(m*f32(p["linear_velocity"][axis]) for m,p in zip(masses,poses,strict=True))/sum(masses)
                for axis in range(3)]
    assert math.dist(velocity,snapshot["center_velocity_m_s"]) < 1e-9
    projection = snapshot["projected"]
    assert projection["horizon_s"] == .1
    predicted = [centre[k]+.1*velocity[k] for k in [0,2]]
    assert math.dist(predicted,projection["center_xz_m"]) < 1e-9
    # All our decks translate vertically. Every horizontal material coordinate stays fixed.
    assert projection["hull_xz_m"] == snapshot["hull_xz_m"]
    assert snapshot["substep"] == 4 and abs(snapshot["max_separation_m"]-.001) < 1e-9
    witnesses = [c for c in contacts if c["substep"] == 4 and c["normal"][1] > .5
                 and f32(c["separation"]) <= f32(.001)]
    assert len(witnesses) == snapshot["witness_count"]
    assert sorted({c["part"] for c in witnesses}) == snapshot["supporting_parts"]
    hull = boundary([(f32(c["point"][0]), f32(c["point"][2])) for c in witnesses])
    assert hull == list(map(tuple, snapshot["hull_xz_m"]))
    if not hull:
        assert snapshot["margin_m"] is None and projection["margin_m"] is None and snapshot["area_m2"] == 0
        return
    edges = list(zip(hull,hull[1:]+hull[:1]))
    area = abs(sum(a[0]*b[1]-a[1]*b[0] for a,b in edges))/2
    assert abs(area-snapshot["area_m2"]) < 1e-9
    assert abs(signed_margin((centre[0],centre[2]),hull,edges)-snapshot["margin_m"]) < 1e-8
    assert abs(signed_margin(predicted,hull,edges)-projection["margin_m"]) < 1e-8


def signed_margin(p,hull,edges):
    distances = []
    for a,b in edges:
        dx,dz = b[0]-a[0],b[1]-a[1]
        t = 0 if b == a else ((p[0]-a[0])*dx+(p[1]-a[1])*dz)/(dx*dx+dz*dz)
        t = max(0,min(1,t))
        distances.append(math.dist(p,(a[0]+dx*t,a[1]+dz*t)))
    inside = len(hull)>2 and all(orientation(a,b,p)>=0 for a,b in edges)
    return (1 if inside else -1)*min(distances)


def quadruped_replay():
    replay = json.loads((HERE/"fixtures/lift.json").read_bytes())
    template = replay["routes"][0]
    replay["routes"] = []
    for name,z,width,y in [("four",8,1,80.8),("edge",7.65,.16,80.15),("release",12,.16,80.8)]:
        lift = dict(template["lift"],center_m=[32,80,z],half_extents_m=[1,.125,width],travel_m=.5,period_s=4)
        camera = [dict(tick=t,eye=[35,83,12],aim=[32,81,8]) for t in [0,120]]
        replay["routes"].append(dict(template,id=name,start_m=[32,y,8],ticks=120,lift=lift,camera=camera))
    for speed in (0, 1, 2, 3, 4):
        lift = dict(replay["routes"][0]["lift"], half_extents_m=[.45,.125,1])
        replay["routes"].append(dict(replay["routes"][0], id=f"slide-{speed}", start_m=[32,80.14,8],
                                     lift=lift, velocity_m_s=[speed,0,0]))
    return replay


def run(binary,pages,out):
    out.mkdir(parents=True, exist_ok=True)
    tools = ROOT / "tools/object-convert"
    xml,rig = out/"quadruped.yft.xml",out/"quadruped.glb"
    subprocess.run([sys.executable,tools/"fixtures/make_quadruped.py",xml],check=True)
    subprocess.run([sys.executable,tools/"convert.py","--id","quadruped","--fragment",xml,"-o",rig],check=True)
    replay = quadruped_replay()
    routefile = out/"routes.json"
    routefile.write_text(json.dumps(replay))
    flags = ["--actor-rig",rig]
    observed = {}
    for route in replay["routes"]:
        name = route["id"]
        def trace(label, options, pairs=()):
            return SNAPSHOT(binary,pages,routefile,name,120,flags+options,out/(label+".json"),pairs)
        data = trace(name,["--support-footprint","0.001"])
        control = trace(name+"-control",[])
        resident = trace(name+"-resident",["--support-footprint","0.001","--all-resident"])
        masses = [p["mass_kg"] for p in data["actor"]["parts"]]
        assert abs(sum(masses)-21)<1e-5 and len(masses)==16
        poses = data["body_before_step"]
        assert [row["tick"] for row in poses] == list(range(route["ticks"]+1))
        for row,ref,all_loaded in zip(poses,control["body_before_step"],resident["body_before_step"],strict=True):
            foot = row["support_footprint"]
            verify(foot,masses,row["parts_after_step"],row["contacts_after_step"])
            assert foot == all_loaded["support_footprint"]
            assert {k:v for k,v in row.items() if k!="support_footprint"} == ref
        tick30 = poses[30]["support_footprint"]
        if name.startswith("slide-"):
            def first_outside(projected):
                for row in poses:
                    foot = row["support_footprint"]
                    margin = (foot["projected"] if projected else foot)["margin_m"]
                    if margin is not None and margin < 0:
                        return row["tick"]
                return None
            forecast, actual = first_outside(True), first_outside(False)
            last = max(row["tick"] for row in poses if row["support_footprint"]["witness_count"])
            speed = int(name.split("-")[1])
            if speed == 0:
                assert forecast is None and actual is None and last == 120
            elif speed == 1:
                assert forecast == actual and last < 120
            else:
                assert forecast < actual < last < 120
            observed[name] = dict(projected_outside=forecast, observed_outside=actual, last_witness=last)
        else:
            wanted = {"four":[6,9,12,15],"edge":[12,15],"release":[]}[name]
            assert tick30["supporting_parts"] == wanted
            margin = tick30["margin_m"]
            if name == "four":
                assert margin > .14
            elif name == "edge":
                assert margin < -.55
            else:
                assert margin is None
            observed[name] = {"tick30_margin_m":margin,"tick30_area_m2":tick30["area_m2"]}
        assert data["support_footprint"] == poses[-1]["support_footprint"]
        final_only = out/(name+"-final.json")
        subprocess.run([binary,"--paging","--no-visuals","--pages",pages,"--replay",routefile,
                        "--route",name,"--out",final_only,*flags,"--support-footprint","0.001"],check=True)
        assert json.loads(final_only.read_bytes())["support_footprint"] == data["support_footprint"]
        if name == "four":
            pairs = []
            for i in wanted:
                x,y,z = poses[-1]["parts_after_step"][i]["center"]
                pairs.append(([x-.13,y,z],[x+.13,y,z]))
            query = trace("four-rays",["--collision-ray-actor"],pairs)
            for i,probe in zip(wanted,query["collision_probes"],strict=True):
                span = next(s for s in probe["actor_spans"] if s["part"]==i)
                lo,hi = span["distance_range_m"]
                assert abs(hi-lo-.13)<2e-5
    for engine in ["rapier","jolt"]:
        directory = out/engine
        subprocess.run([binary,"--scene",HERE/"fixtures/bir-hakeim.glb","--physics",engine,
                        "--actor-rig",rig,"--replay",routefile,"--bench",directory],check=True)
        receipt = json.loads((directory/"dynamics.json").read_bytes())
        expected_routes = {r["id"] for r in replay["routes"]}
        assert set(receipt["support_footprints"]) == expected_routes
        final_routes = set()
        with (directory/"dynamics.csv").open() as csvfile:
            for row in csv.DictReader(csvfile):
                if int(row["tick"]) != 120:
                    continue
                assert row["route"] not in final_routes
                final_routes.add(row["route"])
                body = [{"center":[f32(float(row[k])) for k in ["cx","cy","cz"]],
                         "linear_velocity":[f32(float(row[k])) for k in ["vx","vy","vz"]]},*json.loads(row["parts_json"])]
                metric = receipt["support_footprints"][row["route"]]
                verify(metric,masses,body,json.loads(row["contacts_json"]))
                if row["route"] == "four":
                    assert metric["margin_m"]>0 and metric["supporting_parts"]==[6,9,12,15]
                if row["route"] == "release":
                    assert metric["margin_m"] is None
        assert final_routes == expected_routes
    lift_approach_checks.check(binary,pages,out,replay,rig)
    lift_response_checks.check(binary, pages, out, replay, rig)
    print(json.dumps(observed, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin",type=Path,default=ROOT/"target/release/ville-paris")
    parser.add_argument("--pages",type=Path,required=True)
    parser.add_argument("--out",type=Path,required=True)
    args = parser.parse_args()
    run(args.bin.resolve(),args.pages,args.out)
