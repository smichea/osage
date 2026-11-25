#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Check physical segment queries against the actual poses of a lift and a freely rotating bonnet."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import subprocess

from paging_checks import require

HERE = Path(__file__).resolve().parent


def f32(x):
    return struct.unpack("<f", struct.pack("<f", x))[0]


def basis(q):
    x, y, z, w = [v / math.hypot(*q) for v in q]
    return [[1-2*(y*y+z*z), 2*(x*y+z*w), 2*(x*z-y*w)],
            [2*(x*y-z*w), 1-2*(x*x+z*z), 2*(y*z+x*w)],
            [2*(x*z+y*w), 2*(y*z-x*w), 1-2*(x*x+y*y)]]


def snapshot(binary, pages, replay, route, tick, flags, output, pairs=()):
    args = [binary, "--paging", "--no-visuals", "--trace-body", "--pages", pages, "--replay", replay,
            "--route", route, "--tick", str(tick), "--out", output, *flags]
    for start, end in pairs:
        args.extend(["--collision-ray", ",".join(map(str, start)), ",".join(map(str, end))])
    subprocess.run(args, stdout=subprocess.DEVNULL, check=True)
    return json.loads(output.read_bytes())


def run(binary, pages, out):
    out.mkdir(parents=True, exist_ok=True)
    replay = json.loads((HERE / "fixtures/bonnet.json").read_bytes())
    tilted, = [r for r in replay["routes"] if r["id"] == "moving"]
    tilted["bonnet"]["rotation_xyzw"] = [0, 0, math.sin(0.29), math.cos(0.29)]
    tilted["bonnet"]["friction"] = 0.37
    bonnet = out / "tilted.json"
    bonnet.write_text(json.dumps(replay))
    examples = []
    maximum = 0
    incomplete = 0
    for body, replay, route, ticks in [("lift", HERE/"fixtures/lift.json", "standing-lift", (0,150,300)),
                                      ("bonnet", bonnet, "moving", (0,12,36))]:
        for mode, flags in [("resident", ["--all-resident"]), ("streamed", ["--physics-radius","1","--delay-ms","400"])]:
            for tick in ticks:
                label = f"{body}-{mode}-{tick}"
                before = snapshot(binary,pages,replay,route,tick,flags,out/(label+"-control.json"))
                pose = before[body]["final_pose"]
                center = pose["center"]
                axes = basis(pose["rotation_xyzw"])
                half = [f32(v) for v in before[body]["definition"]["half_extents_m"]]
                def world(p):
                    return [f32(center[i]+sum(p[j]*axes[j][i] for j in range(3))) for i in range(3)]
                def local(p):
                    return [sum((p[i]-center[i])*axis[i] for i in range(3)) for axis in axes]
                tests = []
                def add(name, start, end, faces, local_points):
                    tests.append((name,world(start),world(end),faces,[world(p) for p in local_points]))
                for i, axis in enumerate("xyz"):
                    high = [0,0,0]
                    high[i] = half[i]
                    low = [-v for v in high]
                    a, b = [2*v for v in high], [2*v for v in low]
                    add("through-"+axis,a,b,["+"+axis,"-"+axis],[high,low])
                    add("reverse-"+axis,b,a,["-"+axis,"+"+axis],[low,high])
                    add("inside-"+axis,[0,0,0],a,["+"+axis],[high])
                add("within",[0,0,0],[0,half[1]/2,0],[],[])
                add("clear",[2*half[0],half[1]*2,0],[2*half[0],-half[1]*2,0],[],[])
                if body == "lift":
                    y = half[1]
                    add("end-on-top",[0,2*y,0],[0,y,0],["+y"],[[0,y,0]])
                    add("start-on-top",[0,y,0],[0,2*y,0],["+y"],[[0,y,0]])
                    add("edge",[2*half[0],2*y,0],[0,0,0],["+x","+y"],[[half[0],y,0]]*2)
                    add("within-plane",[-.5,y,0],[.5,y,0],[],[])
                    add("across-plane",[-3,y,0],[3,y,0],["-x","+x"],[[-half[0],y,0],[half[0],y,0]])
                    add("along-two-planes",[half[0],y,-3],[half[0],y,3],["-z","+z"],
                        [[half[0],y,-half[2]],[half[0],y,half[2]]])
                    tests.append(("shaft-at-bottom",[32,24.5,8],[32,23.5,8],
                                  ["+y","-y"] if tick==0 else [],
                                  [[32,24+y,8],[32,24-y,8]] if tick==0 else []))
                # The ray continues below the moving surface into potentially unread terrain.
                span = 1024
                add("long", [0,span,0],[0,-span,0],["+y","-y"],[[0,half[1],0],[0,-half[1],0]])
                trace = snapshot(binary,pages,replay,route,tick,flags,out/(label+".json"),[(t[1],t[2]) for t in tests])
                require(all(trace[key]==before[key] for key in ("body_before_step","physics","physics_events",body)),
                        "observing the snapshot changed physics or streaming")
                for (case,start,end,faces,points),probe in zip(tests,trace["collision_probes"],strict=True):
                    require(probe["stats"]["resident_boxes"] == 1, "missing moving collider")
                    hits = [h for h in probe["intersections"] if h["id"]==body]
                    require([h["box_face"] for h in hits]==faces, f"{label}/{case}: {hits}")
                    for hit,point in zip(hits,points,strict=True):
                        error = math.dist(point,hit["point_m"])
                        maximum = max(maximum,error)
                        require(error < 0.00015 and abs(math.dist(start,hit["point_m"])-hit["distance_m"])<1e-7
                                and abs(hit["coefficients"]["friction"]-before[body]["definition"]["friction"])<1e-6
                                and all(k not in hit for k in ("local_triangle","source_triangle")), "invalid box hit")
                    spans = probe["box_face_overlaps"]
                    expected = {"within-plane": (["+y"],[0,1]), "across-plane": (["+y"],[1.5,4.5]),
                                "along-two-planes": (["+x","+y"],[1.75,4.25])}.get(case,([],[]))
                    require([s["box_face"] for s in spans] == expected[0], f"{label}/{case}: wrong coplanar faces")
                    for span in spans:
                        distances = [math.dist(start, span[k]) for k in ("from_m","to_m")]
                        require(span["id"] == body and distances == span["distance_range_m"] == expected[1],
                                "incorrect length on face")
                    if case=="long" and hits and not probe["coverage"]["complete"]:
                        incomplete += 1
                    examples.append(dict(label=label,case=case,half=half,from_local=local(start),to_local=local(end),
                                         osage_local_hits=[local(h["point_m"]) for h in hits],faces=faces,
                                         overlaps=[[local(s["from_m"]),local(s["to_m"])] for s in spans]))
    require(incomplete>0,"missing the overlap between observed support and incomplete terrain")
    (out/"segments.json").write_text(json.dumps(examples))
    fingerprints = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in (pages/"index.json", HERE/"fixtures/lift.json", bonnet)}
    summary = dict(input_sha256=fingerprints, snapshots=12,segments=len(examples),max_point_error_m=maximum,long_rays_with_unknown_terrain=incomplete)
    (out/"summary.json").write_text(json.dumps(summary,indent=2)+"\n")
    print(json.dumps(summary))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ("bin","pages","out"):
        parser.add_argument("--"+key,type=Path,required=True)
    args = parser.parse_args()
    run(args.bin.resolve(),args.pages.resolve(),args.out.resolve())
