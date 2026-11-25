# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Independent continuous flight and native-solver checks for the quadruped lift fixture."""
import csv
import json
import math
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent


def first_paw_contact(route, replay, initial, paws):
    lift = route["lift"]
    def gap(t):
        center = list(lift["center_m"])
        center[1] += .5*lift["travel_m"]*(1-math.cos(2*math.pi*t/lift["period_s"]))
        best = math.inf
        for i, radius in paws:
            p = [a+b*t for a,b in zip(initial[i]["center"],route["velocity_m_s"],strict=True)]
            p[1] -= .5*replay["gravity_m_s2"]*t*t
            excess = [abs(a-b)-extent for a,b,extent in zip(p,center,lift["half_extents_m"],strict=True)]
            best = min(best,math.hypot(*(max(e,0) for e in excess))+min(max(excess),0)-radius)
        return best
    # The fixture starts at rest vertically. All expected crossings persist longer
    # than one millisecond. Bracket the first one, then bisect the signed distance.
    duration = route["ticks"]/replay["tick_hz"]
    for i in range(1,round(duration*1000)+1):
        hi = i/1000
        if gap(hi)>0:
            continue
        lo = (i-1)/1000
        for _ in range(45):
            mid = (lo+hi)/2
            if gap(mid)>0:
                lo=mid
            else:
                hi=mid
        return hi
    return None


def first_manifolds(directory):
    result = {}
    meta = json.loads((directory/"dynamics.json").read_bytes())["run"]
    substeps = meta["substeps_per_tick"]
    outcomes = {event["id"]:event for event in meta["outcomes"]}
    surface = meta["surfaces"].index("lift")
    with (directory/"dynamics.csv").open() as data:
        for row in csv.DictReader(data):
            route = row["route"]
            if route in result:
                continue
            contacts = [c["substep"] for c in json.loads(row["contacts_json"])
                        if c["surface"]==surface and c["separation"]<=0]
            if contacts:
                result[route] = ((int(row["tick"])-1)*substeps+min(contacts))/(meta["tick_hz"]*substeps)
    return result,outcomes


def check(binary,pages,out,replay,rig):
    routes = [dict(r,lift_contact_hz=1920) for r in replay["routes"]]
    path = out/"routes-refined.json"
    path.write_text(json.dumps(dict(replay,routes=routes)))
    expected = {}
    for r in routes:
        trace = json.loads((out/(r["id"]+".json")).read_bytes())
        paws = [(i,p["radius_m"]) for i,p in enumerate(trace["actor"]["parts"]) if p["name"].endswith("_paw")]
        expected[r["id"]] = first_paw_contact(r,replay,trace["body_before_step"][0]["parts_after_step"],paws)
    measurements = {}
    for engine in ("rapier","jolt"):
        directory = out/(engine+"-refined")
        flags = [binary,"--scene",HERE/"fixtures/bir-hakeim.glb","--physics",engine,"--actor-rig",rig,"--replay",path]
        subprocess.run([*flags,"--bench",directory],stdout=subprocess.DEVNULL,check=True)
        subprocess.run([*flags,"--verify",directory],stdout=subprocess.DEVNULL,check=True)
        baseline,_ = first_manifolds(out/engine)
        _,outcomes = first_manifolds(directory)
        assert set(outcomes)==set(expected)
        for name,reference in expected.items():
            report = outcomes[name]["lift_approach"]
            assert report["interrupted_at_s"] is None
            readings = report["predicted_contacts"]
            seen = report["first_manifold"]
            if reference is None:
                assert not readings and seen is None and report["extra_intervals"]==0
                continue
            ids = {12,15} if name=="edge" else {6,9,12,15}
            assert {c["part"] for c in readings}==ids and seen["part"] in ids
            assert max(abs(c["seconds"]-reference) for c in readings)<1e-7
            assert all(c["normal"][1]>.99 and c["closing_m_s"]>0 for c in readings)
            assert abs(seen["end_s"]-seen["start_s"]-1/1920)<1e-12
            assert seen["separation_m"]<=0 and 0<report["extra_intervals"]<=35
            assert abs(seen["end_s"]-reference)<abs(baseline[name]-reference)
            measurements.setdefault(name,{"analytic_s":reference})[engine] = {
                "fixed_s":baseline[name],"refined_s":seen["end_s"],"extra_intervals":report["extra_intervals"]}
        if engine=="rapier":
            for name in expected:
                output = out/(name+"-refined-paged.json")
                args = [binary,"--paging","--no-visuals","--pages",pages,"--actor-rig",rig,"--replay",path,
                        "--route",name,"--out",output]
                subprocess.run(args,check=True)
                report = json.loads(output.read_bytes())["lift_approach"]
                assert report == outcomes[name]["lift_approach"]
    ordinary = out/"same-clock.json"
    ordinary.write_text(json.dumps(dict(replay,routes=[dict(routes[0],lift_contact_hz=240)])))
    output = out/"same-clock-paged.json"
    subprocess.run([binary,"--paging","--no-visuals","--trace-body","--support-footprint","0.001",
                    "--actor-rig",rig,"--pages",pages,"--replay",ordinary,"--route",routes[0]["id"],
                    "--out",output],check=True)
    same = json.loads(output.read_bytes())
    control = json.loads((out/(routes[0]["id"]+".json")).read_bytes())
    assert same["lift_approach"]["extra_intervals"] == 0
    assert same["body_before_step"] == control["body_before_step"]
    # One standalone capsule reaches the road before the lift beneath it.
    # The launch prediction remains auditable, while the road cancels its solver window.
    route = dict(routes[0],id="road-above-lift",start_m=[0,11,6],expected_support="road",
                 lift=dict(routes[0]["lift"],center_m=[0,1,6]))
    interrupted = out/"road-above-lift.json"
    interrupted.write_text(json.dumps(dict(replay,routes=[route])))
    for engine in ("rapier","jolt"):
        directory=out/(engine+"-road-above-lift")
        subprocess.run([binary,"--scene",HERE/"fixtures/bir-hakeim.glb","--actor","capsule",
                        "--replay",interrupted,"--physics",engine,"--bench",directory],stdout=subprocess.DEVNULL,check=True)
        outcome=json.loads((directory/"dynamics.json").read_bytes())["run"]["outcomes"][0]
        report=outcome["lift_approach"]
        assert report["first_manifold"] is None and outcome["target_contacted"]
        assert 0 < report["interrupted_at_s"] < report["predicted_contacts"][0]["seconds"]
        assert report["extra_intervals"]==0
    output=out/"road-above-lift-paged.json"
    subprocess.run([binary,"--paging","--no-visuals","--actor","capsule","--route",route["id"],
                    "--pages",pages,"--replay",interrupted,"--out",output],check=True)
    observed=json.loads(output.read_bytes())["lift_approach"]
    assert observed["interrupted_at_s"] is not None and observed["first_manifold"] is None
    (out/"lift-timing.json").write_text(json.dumps(measurements,indent=2))
