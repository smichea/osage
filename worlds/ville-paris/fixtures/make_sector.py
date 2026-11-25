# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Rebuild the original, self-contained glTF next to this file. Units are metres."""
import json
import math
import struct
from pathlib import Path

# Palette in sRGB. glTF material factors are linear light.
palette = {
    "water": "538681", "quay": "ae9c86", "ground": "b0a191", "tarmac": "717675",
    "iron": "46706c", "edge": "7e9b8a", "limestone": "d9cbb6", "plaster": "c3ac91",
    "chalk": "e7d9c4", "zinc": "6a7982", "slate": "526575", "glass": "546b70",
    "trunk": "71674e", "leaf": "667b4d", "lightleaf": "879354", "lawn": "899b6f",
    "path": "cfbfa2", "tower": "776756", "mark": "e3d7b9", "brick": "996d57",
}
def linear(n):
    s = int(n, 16) / 255
    return s / 12.92 if s <= 0.04045 else ((s + 0.055) / 1.055) ** 2.4

materials = []
for name, rgb in palette.items():
    color = [round(linear(rgb[i:i + 2]), 6) for i in (0, 2, 4)]
    materials.append({"name": name, "pbrMetallicRoughness": {
        "baseColorFactor": color + [1], "metallicFactor": 0, "roughnessFactor": 0.91}})
meshes, nodes, views, accessors = [], [], [], []
blob = bytearray()

def accessor(values, fmt, shape, element, bounds=False):
    while len(blob) % 4:
        blob.append(0)
    offset = len(blob)
    for v in values:
        blob.extend(struct.pack("<" + fmt, *v))
    views.append({"buffer": 0, "byteOffset": offset, "byteLength": len(blob) - offset})
    acc = {"bufferView": len(views) - 1, "componentType": element, "count": len(values), "type": shape}
    if bounds:
        acc["min"], acc["max"] = list(map(min, zip(*values))), list(map(max, zip(*values)))
    accessors.append(acc)
    return len(accessors) - 1

def normal(a, b, c):
    u = [b[i] - a[i] for i in range(3)]
    v = [c[i] - a[i] for i in range(3)]
    n = [u[1]*v[2]-u[2]*v[1], u[2]*v[0]-u[0]*v[2], u[0]*v[1]-u[1]*v[0]]
    length = math.hypot(*n)
    return tuple(k / length for k in n)

# Rectangular base with a smaller rectangular top for stone, roofs and trees.
def frustum(x, y, z, w, h, d, top=1):
    corners = [(-1,-1), (-1,1), (1,1), (1,-1)]
    v = [(x + a*w/2, y-h/2, z+b*d/2) for a,b in corners]
    v += [(x + a*w*top/2, y+h/2, z+b*d*top/2) for a,b in corners]
    return [[v[i] for i in q] for q in ((3,2,1,0), (4,5,6,7), (0,1,5,4),
                                         (1,2,6,5), (2,3,7,6), (3,0,4,7))]

def geometry(faces):
    pos, norms, indices = [], [], []
    for face in faces:
        start = len(pos)
        pos.extend(face)
        norms.extend([normal(*face[:3])] * 4)
        indices.extend([(start+i,) for i in (0,1,2,0,2,3)])
    return {"attributes": {"POSITION": accessor(pos, "fff", "VEC3", 5126, True),
                           "NORMAL": accessor(norms, "fff", "VEC3", 5126)},
            "indices": accessor(indices, "H", "SCALAR", 5123)}

cube = geometry(frustum(0, 0, 0, 1, 1, 1))
hip = geometry(frustum(0, 0, 0, 1, 1, 1, 0.74))
def mesh(name, parts):
    meshes.append({"name": name, "primitives": [
        {**g, "material": list(palette).index(mat)} for mat, g in parts]})
    return len(meshes)-1

unit = {mat: mesh(mat, [(mat, cube)]) for mat in palette}
roofs = {mat: mesh(mat+"-hip", [(mat, hip)]) for mat in ("slate", "zinc")}
windows, ledges = [], []
for row in range(5):
    y = -0.38 + row*0.181
    for k in range(7):
        d = -0.417 + k*0.139
        for s in (-1, 1):
            # Thin boxes straddle the facade with an outward face on each side.
            windows += frustum(d,y,s*0.5,0.061,0.106,0.008)
    for k in range(4):
        d = -0.365 + k*0.243
        for s in (-1, 1):
            windows += frustum(s*0.5,y,d,0.008,0.106,0.108)
for y in (-0.28,0.27,0.487):
    ledges += frustum(0,y,0,1.018,0.022,1.025)
win, trim = geometry(windows), geometry(ledges)
facades = {mat: mesh(mat+"-facade", [(mat,cube),("glass",win),("chalk",trim)])
           for mat in ("limestone", "plaster", "chalk")}
crowns = {}
for mat in ("leaf", "lightleaf"):
    crowns[mat] = mesh(mat+"-crown", [(mat,geometry(
        frustum(0,-0.22,0,0.67,0.55,0.70,1.42) + frustum(0,0.255,0,0.95,0.4,1,0.48)))])

def node(m, pos, size, name=None, rot=None):
    n = {"mesh": m, "translation": [round(v,4) for v in pos], "scale": [round(v,4) for v in size]}
    if name:
        n["name"] = name
    if rot:
        n["rotation"] = [round(v,8) for v in rot]
    nodes.append(n)

def box(mat, pos, size, name=None):
    node(unit[mat], pos, size, name)

def beam(mat, a, b, width):
    dx,dy,dz = (b[i]-a[i] for i in range(3))
    length = math.hypot(dx,dy,dz)
    # Quaternion carrying the unit Y axis onto the segment.
    q = (dz/length, 0, -dx/length, 1 + dy/length)
    norm = math.hypot(*q)
    node(unit[mat], [(x+y)/2 for x,y in zip(a,b)], (width,length,width),
         rot=[c/norm for c in q] if norm else [1,0,0,0])

# X crosses the Seine from Passy to Grenelle. Negative Z points upstream.
box("water",(0,-0.55,0),(244,1,2300))
for side in (-1,1):
    x = side*(118.58+452)
    box("ground",(x,2.1,0),(904,4.2,2300),"support/bank-"+str(side))
    box("quay",(side*120,2.8,0),(5,5.6,2300),"support/quay-"+str(side))
    box("tarmac",(side*139,4.24,0),(14,0.08,2300))
    box("path",(side*127,4.26,0),(6,0.12,2300))
    # Open streets and their light pavements between the authored city blocks.
    for col in range(10):
        avenue = side*(217+82*col)
        box("path",(avenue,4.23,0),(19,0.06,2110))
        box("tarmac",(avenue,4.28,0),(12,0.04,2110))
    for row in range(-11,12):
        street = row*90+45
        box("path",(side*572,4.24,street),(866,0.08,18))
        box("tarmac",(side*572,4.31,street),(866,0.04,11))
    box("tarmac",(side*573,4.34,0),(886,0.1,15))
box("quay",(0,1.1,253),(17,2.2,526),"support/island")
box("lawn",(0,2.24,256),(14,0.08,520))
box("path",(0,2.31,256),(4,0.06,520))
# Published crossing length and approximate width. Heights are fixture parameters.
box("tarmac",(0,7.2,0),(237.16,1.6,24.7),"support/road")
box("iron",(0,15.25,0),(237.16,1.5,7.6),"support/rail")
for z in (-12.1,12.1):
    box("iron",(0,8.9,z),(237.16,0.18,0.15))
    box("edge",(0,7.1,z),(237.16,0.56,0.48))
    for x in range(-117,118,5):
        box("iron",(x,8.5,z),(0.11,0.9,0.11))
for x in (-87,-42,0,39,84):
    box("quay",(x,3,0),(5,6,24))
for x in range(-114,116,6):
    for z in (-3.1,3.1):
        box("iron",(x,11.3,z),(0.32,6.6,0.32))
        beam("iron",(x,13.0,z),(x+2.5,14.65,z),0.20)
        beam("iron",(x,13.0,z),(x-2.5,14.65,z),0.20)
for z in (-3.72,3.72,-1.0,1.0):
    box("edge",(0,16.14,z),(237.16,0.22,0.12))
for x in range(-114,118,9):
    for z in (-6.4,6.4):
        box("mark",(x,8.012,z),(3,0.024,0.10))

def building(x,z,w,d,h,style,ident):
    y=4.2
    node(facades[("limestone","plaster","chalk")[style%3]],(x,y+h/2,z),(w,h,d))
    top=("slate","zinc")[style%2]
    node(roofs[top],(x,y+h+1.8,z),(w+0.5,3.6,d+0.5),"support/"+ident)

# Original massing, with open street corridors and a reserved park by the tower.
for s in (-1,1):
    for col in range(10):
        for row in range(-11,12):
            if row == 0:
                continue
            x, z = s*(176+col*82), row*90
            if s==1 and x<270 and -720<z<-415:
                continue
            if s==1 and col==0 and row==-1:
                continue
            variant=(col*11 + (row+12)*7 + (s+1)*3)%13
            building(x,z,58+variant%6,51+variant%7,14+variant,
                     variant,f"roof-{s}-{col}-{row}")
building(188,-72,48,30,24.2,2,"roof-grenelle")
box("lawn",(194,4.28,-571),(129,0.16,281))
# The tower is a scale cue: an original lattice silhouette with three platforms.
tx,tz=184,-581
levels=[(4.4,60),(60,35),(119,18),(197,9),(280,3),(307,1.7)]
for (h,r),(h2,r2) in zip(levels, levels[1:]):
    for a,b in ((-1,-1),(-1,1),(1,-1),(1,1)):
        beam("tower",(tx+a*r,h,tz+b*r),(tx+a*r2,h2,tz+b*r2),max(1.6,r*0.093))
    for k in range(4):
        t, u = k/4, (k+1)/4
        y, yy = h+(h2-h)*t, h+(h2-h)*u
        w, ww = r+(r2-r)*t, r+(r2-r)*u
        if h == levels[0][0]:
            continue
        for s in (-1,1):
            for d in (-1,1):
                beam("tower",(tx+d*w,y,tz+s*w),(tx-d*ww,yy,tz+s*ww),max(0.58,r*0.021))
                beam("tower",(tx+s*w,y,tz+d*w),(tx+s*ww,yy,tz-d*ww),max(0.58,r*0.021))
for s in (-1,1):
    arch = [(tx+50*math.cos(i*math.pi/10), 12+36*math.sin(i*math.pi/10)) for i in range(11)]
    for (x,h),(xx,hh) in zip(arch,arch[1:]):
        beam("tower",(x,h,tz+s*46),(xx,hh,tz+s*46),2.4)
        beam("tower",(tx+s*46,h,tz+x-tx),(tx+s*46,hh,tz+xx-tx),2.4)
for h,r in ((60,38),(119,21),(280,5)):
    box("tower",(tx,h,tz),(2*r,3.5,2*r))
beam("tower",(tx,305,tz),(tx,334,tz),0.95)

def tree(x,ground,z,v):
    box("trunk",(x,ground+2.5,z),(0.52,5,0.55))
    node(crowns[("leaf","lightleaf")[v%2]],(x,ground+6.5,z),(5+v%3,6,5.4))
for z in range(-900,901,17):
    if abs(z)>16:
        for s in (-1,1):
            tree(s*126,4.2,z,abs(z+s)%7)
for z in range(28,514,14):
    tree((-1)**(z//14)*4.8,2.35,z,z//14)

doc = {"asset":{"version":"2.0","generator":"osage ville-paris",
                "copyright":"SPDX-FileCopyrightText: The osage contributors. SPDX-License-Identifier: MIT OR Apache-2.0"},
       "scene":0,"scenes":[{"name":"Bir-Hakeim pilot","nodes":list(range(len(nodes)))}],
       "nodes":nodes,"meshes":meshes,"materials":materials,"accessors":accessors,
       "bufferViews":views,"buffers":[{"byteLength":len(blob)}]}
encoded = json.dumps(doc, separators=(",", ":")).encode("utf-8")
encoded += b" " * (-len(encoded) % 4)
blob.extend(bytes(-len(blob) % 4))
length = 12 + 8 + len(encoded) + 8 + len(blob)
path = Path(__file__).with_name("bir-hakeim.glb")
with path.open("wb") as out:
    out.write(struct.pack("<4sII", b"glTF", 2, length))
    out.write(struct.pack("<I4s", len(encoded), b"JSON") + encoded)
    out.write(struct.pack("<I4s", len(blob), b"BIN"+bytes(1)) + blob)
