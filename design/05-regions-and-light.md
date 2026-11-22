# 05 — Regions and the light between them

Date: 2026-11-22 · Status: **proposal** — nothing of §2 to §5 is built or measured · From: the maintainer's questions while walking the Paris test world ([research 10](../research/10-incoming-light.md), [D-136](decisions.md) to D-142)

The Paris test world draws, for a walker, the 9 cubes of 3 m around it from the street's model, and all beyond from light baked ahead: probes inside the cubes, panoramas with how far each pixel lies. It works from one machine's point of view: a probe is baked by a renderer that sees the whole scene. A world held by several servers has no such renderer. This note says how the same picture could be made when each server knows only its own region.

## 1. What is measured, and what it leaves open

- A walker's block drawn from the five probes of its cube comes within 28.0 dB of Cycles; from one probe a cube — the centres of the block's nine cubes, each baked with its own block transparent and shared by the nine blocks it is in — 27.6 dB, blurred near the block, for a fifth of the bake.
- Drawing the street's own meshes for the eight blocks round the walker's, lit by the probes that saw each point, gives the same 28.0 dB without the march through the probes (33 ms a frame against 57), and with the meshes' own textures the near façades get their grain back: the probe then gives the light alone.
- All of it rests on probes that saw everything. That is the open point.

## 2. The principle

Two tools, by distance.

- **Near the player: probes.** Its block in 3D, the blocks round it as meshes lit by probes, the rest of its region from the probes of the cubes round it. As today.
- **Between regions: a light field on their borders.** For each point of a region's border and each direction, the light that leaves the region there. A region's server makes it from two things only: its own geometry, and the light fields its neighbours published on the borders it shares with them. No server sees another's geometry.

The second is the maintainer's first idea of a hologram — a picture holding a colour for each direction, at each point of a surface — put where it pays: it composes. The light leaving a region is a function of what is in it and what enters it.

## 3. Time: the light goes a region a turn

A server computes the light leaving its borders at turn *t* from its geometry and the light that entered at turn *t − 1*:

    out(X, t) = render(geometry(X), in(X, t − 1))
    in(X, t)  = the out(Y, t) of each neighbour Y, on the border shared with X

- A world that does not change converges in as many turns as it is regions across; each turn adds the light that came one region farther.
- A change far away (a lamp lit five regions off) is seen five turns late. Its part in the picture is small, and the delay is the world's own: light here has a speed, a region a turn, set by the servers' power and their links.
- A server that stops leaves its last light on its neighbours' borders: it fades from there on, region by region, as their turns go on without it.

## 4. Regions: fixed, and overlapping

- **Fixed on a grid**, of about 81 m (27 cubes): 3 m cubes, 9 m blocks and 81 m regions nest by threes. A region centred on each player would have borders of its own, and its light field would be nobody else's; fixed, a border's light field is the same for every player and every neighbour.
- **Overlapping by a cube or more.** In the band two regions share, both servers hold the geometry and step the physics, and agree on it. A player crossing is in both for the width of the band: no moment where one server hands everything to the other. The light fields sit on the borders of the regions' own parts, inside the band, so each server renders up to a border the other one covers.
- A player's device may hold the physics of its own surroundings; the band is then also where it settles with the servers on either side.

## 5. What a light field is made of

A light field with no depth needs both a fine grid of points and a fine grid of directions: for a façade 10 m off to stay sharp, a tenth of a degree, as the probes have; for the view to change rightly as the eye moves, a point every few centimetres. Together, millions of directions at thousands of points a square metre: too much.

The probes escape it by keeping how far each pixel lies: a few points, metres apart, stand for all those between, the reader finding along its own ray what each probe saw there. The same holds on a border:

- **Border probes with depth**: on a lattice over the border, a wide picture looking into the region (the fisheyes of D-136 are that), with the light and the distance of what it sees. The depth is not knowledge of the neighbour's geometry: it tells only what is seen from the border, as far as it is seen.
- **Reading one**: a server renders its region; a ray that leaves through a border is carried on into the neighbour's border probes, by the march the client already does on the block's faces.
- **The probes a player gets** are made the same way: the region's geometry, and its neighbours' border probes for whatever lies beyond.

Orders of size, from what is measured (a probe of 4096 × 2048 packed with its half: about 680 KiB; a fisheye of 2048 px: about 355 KiB):

| | Count | Size |
|---|---|---|
| One region's inner probes, a centre a cube, ground layer all walkable | 729 | ≈ 480 MiB |
| One region's border probes, four sides 81 m by 27 m, a lattice of 3 m | 972 | ≈ 340 MiB |
| The same, a lattice of 1.5 m | 3 888 | ≈ 1.3 GiB |

Most of a border is wall or sky, where a coarser lattice would do; the sizes are a ceiling, not an estimate of a real region.

## 6. What is not solved

1. **Cycles does not read a light field as light.** A region's bake needs its neighbours' border probes as the light beyond its borders, with their parallax. Either a shader that does the march inside Cycles, or the bake in our own renderer. First thing to try.
2. **The lattice on a border.** D-136's lesson was that probes on a surface, metres apart, miss what stands just behind a near thing (3.8 % of the picture, against 0.04 % with probes near the eye). On a border far from the player the angle is kinder; near a door it is not.
3. **Light both ways.** A bright region lights its neighbour's walls near the border, which lights it back: the turns of §3 do it, each adding a bounce across. How many turns a still scene takes to look settled is to be measured.
4. **What changes.** A region whose geometry changes bakes its border probes again; its neighbours bake theirs a turn later. A turn's cost sets the world's speed of light; a bake of minutes on a GPU is far from a turn a second.
5. **The overlap's physics** is its own subject: two servers stepping the same band and agreeing.

## 7. A first trial, in Bistro

Bistro is 60 m by 100 m: hardly more than one region of 81 m. For a trial, regions of 27 m (9 cubes), six to eight of them over the street, an overlap of one cube.

1. Bake each region's border probes with its geometry alone (what is beyond the border: nothing yet), then again with its neighbours' border probes as the light beyond — two turns.
2. Bake the inner probes of two neighbouring regions the same way; walk across their border.
3. Measure against the Cycles picture of the whole scene, from a camera in one region looking into the other: the picture, the part seen through the border, and the floor's brightness near the border after one turn and after two.

The centre probes of every cube of the street are being baked whole-scene (2026-11-22): they are the reference the trial's probes are held against.
