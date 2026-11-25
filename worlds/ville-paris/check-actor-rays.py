#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Verify occupied segments against the posed actor, including articulated and imported bodies."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import runpy
import struct
import subprocess
import sys

from paging_checks import require

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
support = runpy.run_path(str(HERE / "check-moving-surface-rays.py"))
basis, f32, snapshot = (support[k] for k in ("basis", "f32", "snapshot"))


def dot(a,b):
    return sum(x*y for x,y in zip(a,b))


def frame(pose):
    axes = basis(pose["rotation_xyzw"])
    center = pose["center"]
    def world(p):
        return [f32(center[i]+sum(axis[i]*v for axis,v in zip(axes,p))) for i in range(3)]
    def local(p):
        return [dot([x-y for x,y in zip(p,center)], axis) for axis in axes]
    return world, local


def witness(a, b, h, r):
    # Minimise distance to the centre segment, then bisect each zero on its side
    # of the minimum. This reference uses no sphere or cylinder intersection formulas.
    delta = [y-x for x,y in zip(a,b)]
    def clearance(t):
        x,y,z = (x+t*v for x,v in zip(a,delta))
        return math.hypot(x,max(abs(y)-h,0),z)-r
    left, right = 0.,1.
    for _ in range(65):
        u, v = (2*left+right)/3,(left+2*right)/3
        if clearance(u) <= clearance(v):
            right = v
        else:
            left = u
    middle=(left+right)/2
    if clearance(middle)>1e-11:
        return None
    def boundary(outside):
        if clearance(outside)<=0:
            return outside
        inside=middle
        for _ in range(53):
            t=(outside+inside)/2
            if clearance(t)>0:
                outside=t
            else:
                inside=t
        return inside
    return [boundary(0.),boundary(1.)]


def run(binary,pages,out):
    out.mkdir(parents=True,exist_ok=True)
    rig=out/"pedestrian.glb"
    subprocess.run([sys.executable,ROOT/"tools/object-convert/convert.py","--id","pedestrian","--fragment",
                    ROOT/"tools/object-convert/fixtures/pedestrian.yft.xml","-o",rig],check=True,stdout=subprocess.DEVNULL)
    glb=runpy.run_path(str(HERE/"check-impact-transmission.py"))
    doc,chunk=glb["read_glb"](rig)
    profile=doc["asset"]["extras"]["osageObject"]
    bodies=profile["articulation"]["bodies"]
    forearm=next(b for b in bodies if b["name"]=="lowerarm_l")
    forearm["material"]="forearm-lining"
    profile["collisionMaterials"][forearm["material"]]={"friction":0.375,"restitution":0.125}
    glb["write_glb"](rig,doc,chunk)
    source_nodes={n["extras"]["osage"]["id"]:i for i,n in enumerate(doc["nodes"])}
    lift=json.loads((HERE/"fixtures/lift.json").read_bytes())
    lift["routes"][0]["start_m"]=[0,100,0]
    isolated=out/"upright.json"
    isolated.write_text(json.dumps(lift))
    bonnet=HERE/"fixtures/bonnet.json"
    selected=[("capsule",isolated,"standing-lift",0,[])]
    for actor,flag in [("human",["--actor","human"]),("imported",["--actor-rig",rig])]:
        for tick in (0,24,36,90):
            selected.append((actor,bonnet,"moving",tick,flag))
    samples=[]
    all_tests=body_checks=mixed=overlaps=0
    candidates=available=0
    max_error=0.
    for actor,replay,route,tick,flags in selected:
        label=f"{actor}-{tick}"
        original=snapshot(binary,pages,replay,route,tick,flags,out/(label+"-control.json"))
        poses=original["body_before_step"][-1]["parts_after_step"]
        parts=original["actor"]["parts"]
        frames=[frame(p) for p in poses]
        tests=[]
        for i,(part,(world,_)) in enumerate(zip(parts,frames,strict=True)):
            h,r=map(f32,[part["half_segment_m"],part["radius_m"]])
            for name,a,b in [
                ("cross",[-2*r,0,0],[2*r,0,0]),("reverse",[2*r,0,0],[-2*r,0,0]),
                ("axial",[0,-h-2*r,0],[0,h+2*r,0]),("oblique",[-2*r,-h,0],[2*r,h,0]),
                ("cap",[-2*r,h+r*.5,0],[2*r,h+r*.5,0]),("inside",[0,0,0],[0,r*.4,0]),
                ("from-inside",[0,0,0],[3*r,0,0]),("clear",[-2*r,h+2*r,0],[2*r,h+2*r,0])]:
                tests.append((i,name,world(a),world(b)))
        foot=poses[-1]["center"]
        tests.append((len(parts)-1,"terrain",[f32(foot[0]),f32(foot[1]+2),f32(foot[2])],
                      [f32(foot[0]),-5,f32(foot[2])]))
        if actor=="capsule":
            c=poses[0]["center"]
            r=f32(parts[0]["radius_m"])
            next_r=struct.unpack("<f",struct.pack("<I",struct.unpack("<I",struct.pack("<f",r))[0]+1))[0]
            tests.extend((0,name,a,b) for name,a,b in [
                ("end",[2*r,c[1],0],[r,c[1],0]),("start",[r,c[1],0],[2*r,c[1],0]),
                ("tangent",[r,c[1],-2],[r,c[1],2]),("side",[r,c[1]-2,0],[r,c[1]+2,0]),
                ("side-inside",[r,c[1]-.125,0],[r,c[1]+.125,0]),
                ("ulp-miss",[next_r,c[1],-2],[next_r,c[1],2]),
                ("long",[-1e22,c[1],0],[1e22,c[1],0])])
        if actor!="capsule" and tick==0:
            arm=next(i for i,p in enumerate(parts) if p["name"]=="upperarm_l")
            _,y,z=poses[arm]["center"]
            for title,left,right in [("arm-span",-10,10),("far-arm",-1e22,1e22),("reverse-far-arm",1e22,-1e22)]:
                tests.append((arm,title,[left,y,z],[right,y,z]))
        tests.append((0,"remote",[5000,2000,5000],[5000,1999,5000]))
        pairs=[(a,b) for _,_,a,b in tests]
        trace=snapshot(binary,pages,replay,route,tick,[*flags,"--collision-ray-actor"],out/(label+".json"),pairs)
        if actor=="imported":
            expect=[dict(source_node=source_nodes[b["bone"]],bone=b["bone"],material=b["material"]) for b in bodies]
            require(trace["actor"]["bindings"]==expect and trace["actor"]["rig_sha256"]==hashlib.sha256(rig.read_bytes()).hexdigest(),
                    "body binding no longer addresses the original node and material")
        world_only=snapshot(binary,pages,replay,route,tick,flags,out/(label+"-world.json"),pairs)
        for key in ("body_before_step","physics","physics_events","visual_events","first_contact"):
            require(trace[key]==original[key]==world_only[key],f"{label}: query changed {key}")
        reference_arm=None
        for (i,name,_,_),probe,control in zip(tests,trace["collision_probes"],world_only["collision_probes"],strict=True):
            all_tests+=1
            candidates+=probe["stats"]["actor_parts_tested"]
            available+=len(parts)
            if name=="remote":
                require(probe["stats"]["actor_parts_tested"]==0,"clear ray reached actor primitives")
            start,end=probe["from"],probe["to"]
            length=math.dist(start,end)
            spans=probe["actor_spans"]
            rows={s["part"]:s for s in spans}
            require(len(rows)==len(spans) and [s["distance_range_m"][0] for s in spans]==sorted(s["distance_range_m"][0] for s in spans),"span ordering")
            surfaces=[h for h in probe["intersections"] if "actor" not in h]
            require(surfaces==control["intersections"] and probe["coverage"]==control["coverage"]
                    and probe["box_face_overlaps"]==control["box_face_overlaps"] and "actor_spans" not in control,"world query changed")
            if name in ("arm-span","far-arm","reverse-far-arm"):
                ordered=[(hit["actor"]["part"],hit["point_m"]) for hit in probe["intersections"] if "actor" in hit]
                if name=="arm-span":
                    require(len(set(j for j,_ in ordered))>=2, "missing multi-body reference")
                    reference_arm=ordered
                else:
                    expected=reference_arm if name=="far-arm" else reference_arm[::-1]
                    require(len(ordered)==len(expected) and all(j==k and math.dist(a,b)<1e-8 for (j,a),(k,b) in zip(ordered,expected)),
                            "distant origin reordered the bodies")
                    require(all(abs(math.dist(*s["points_m"])-s["length_m"])<1e-10 for s in spans),"lost distant span length")
                    continue
            mixed+=bool(spans and surfaces)
            overlaps+=len(spans)>1
            for j,((_,local),part) in enumerate(zip(frames,parts)):
                h,r=map(f32,[part["half_segment_m"],part["radius_m"]])
                a,b=local(start),local(end)
                span=rows.get(j)
                hits=[x for x in probe["intersections"] if x.get("actor",{}).get("part")==j]
                body_checks+=1
                if j==i and name!="long":
                    axes=basis(poses[j]["rotation_xyzw"])
                    samples.append(dict(label=label,case=name,part=j,half=h,radius=r,from_local=a,to_local=b,
                                        points=[local(hit["point_m"]) for hit in hits],
                                        normals=[[dot(hit["actor"]["normal"],axis) for axis in axes] for hit in hits]))
                if name in ("side","side-inside","tangent","end","start","ulp-miss","long"):
                    n={"side":2,"side-inside":2,"tangent":1,"end":1,"start":1,"ulp-miss":0,"long":2}[name]
                    require(len(hits)==n and bool(span)==bool(n), f"{name}: {hits}")
                    if name=="long":
                        require(span["length_m"]==2*r and [x["point_m"][0] for x in hits]==[-r,r],"distant ends lost the two boundaries")
                    if span:
                        require(span["surface_only"]==(name!="long"), f"{name}: contact continuity")
                    continue
                expect=witness(a,b,h,r)
                require((expect is None)==(span is None),f"{label}/{i}/{name}, body {j}: expected {expect}, got {span}")
                if span:
                    error=max(abs(t*length-v) for t,v in zip(expect,span["distance_range_m"]))
                    max_error=max(max_error,error)
                    require(abs(span["length_m"]-math.dist(*span["points_m"]))<1e-9 and error<1e-7 and span["name"]==part["name"] and span["parent"]==part["parent"],"incorrect body interval")
                    for hit in hits:
                        if actor=="imported":
                            require(hit["coefficients"]==profile["collisionMaterials"][bodies[j]["material"]],
                                    "wrong authored solver material")
                        normal=hit["actor"]["normal"]
                        point=local(hit["point_m"])
                        axis_y=max(-h,min(h,point[1]))
                        off=[point[0],point[1]-axis_y,point[2]]
                        axes=basis(poses[j]["rotation_xyzw"])
                        normal_local=[dot(normal,axis) for axis in axes]
                        require(abs(math.hypot(*off)-r)<1e-8 and dot(normal_local,off)/r>1-1e-9,"incorrect surface normal")
                    expected_hits=sum(0<t<1 for t in expect)
                    require(len(hits)==expected_hits,"missing or additional boundary")
    ordinary=snapshot(binary,pages,HERE/"fixtures/descent.json","rail",0,["--collision-ray-actor"],
                      out/"unarticulated.json",[([0,601,0],[0,600,0])])
    require(ordinary["actor"]["model"]=="capsule" and len(ordinary["actor"]["parts"])==1, "unbound capsule proxy")
    require(0<candidates<available, "spatial index did not filter the actor")
    require(overlaps and mixed,"no overlapping bodies or simultaneous support")
    (out/"samples.json").write_text(json.dumps(samples))
    result=dict(actor_primitive_tests=candidates,exhaustive_actor_tests=available,snapshots=len(selected),segments=all_tests,part_checks=body_checks,max_boundary_error_m=max_error,
                multiple_parts=overlaps,actor_and_world=mixed,rig_sha256=hashlib.sha256(rig.read_bytes()).hexdigest())
    (out/"summary.json").write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps(result))


if __name__=="__main__":
    ap=argparse.ArgumentParser(description=__doc__)
    for k in ("bin","pages","out"):
        ap.add_argument("--"+k,required=True,type=Path)
    args=ap.parse_args()
    run(args.bin.resolve(),args.pages.resolve(),args.out.resolve())
