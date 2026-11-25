---
type: Algorithm
title: Hearing an impact around city obstacles
description: Time and mix a contact wave that reaches the camera through one or two openings.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/impact/diffraction.rs
licence: MIT OR Apache-2.0
status: draft
tags: [paris, sound, geometry]
sources:
  - id: model
    title: Common noise assessment methods, diffraction term in equation 2.5.21
    resource: https://www.legislation.gov.uk/eudr/2015/996/pdfs/eudr_20150996_adopted_en.pdf
  - id: synth
    title: Material impact voice
    resource: https://github.com/smichea/osage/blob/main/crates/osage-sound-voices/src/contact.rs
generated: { by: "OpenAI/Codex", at: "2026-11-24T07:09:38Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A pedestrian strikes a surface behind a wall. An observer can hear the hit through an opening several metres to the side. Two openings in successive walls can require two changes of direction.

# Idea

Find a visible broken line from the source through exposed mesh edges to the moving listener. Its full length determines the instant of reception. Its last edge supplies the apparent direction. The direct sound through a material and the sound around an edge can arrive separately.

# Method

Start with the acoustic bindings crossed by the direct wave. For each triangle of an obstructing binding, project nearby triangles onto its sides using the object's bounding volume hierarchy. Remove spans shared with a neighbour on the other side, including seams between distinct nodes and partial overlaps. Cache the surviving boundaries with their owner paths. The companion Bir-Hakeim scene supplies the corners of its authored panels. [Mesh boundary extraction](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/acoustic_edges.rs)

On one finite edge the minimizing point divides the axial interval between the two projections in proportion to the perpendicular distances of source and receiver. Clamp that point to the segment. If either leg crosses another blocking surface, try edges from that surface in the order the wave encounters them. For a pair of edges, eliminate the point on the second edge with the one-edge solution. Minimize the resulting convex function on the first edge using its derivative and both endpoints. Recompute all three distances from the optimized points. [Path and reception solver](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/impact/arrival.rs)

For each piece of the camera trajectory solve for the first instant when the wave has travelled the length of the broken line. The speed is 343 m/s. A moving listener can cross the whole wave between camera keys. For a faster-than-sound observer, search the minimum distance residual within the interval before finding the first root. A stationary eye needs a single distance evaluation for the flight time.

Use the excess over the direct distance at reception to compute three insertion losses. At wavelength λ, the energy divisor is 3 + 40 C δ / λ. The factor C is one at a single edge. For two bends separated by more than 0.3 m use C = (1 + k) / (1/3 + k) with k = (5λ/e)² and e the distance between the bends. This uses the diffraction term for a thin screen. The renderer omits environmental mapping corrections, ground interaction and regulatory limits on the combined loss. It uses the city's sound speed. [^model]

Multiply the remaining energy fraction at each band by the material transmission on every leg. Only energy removed from the direct wave can feed the extra path. Select the candidate with the largest total received energy across the three bands. Use the free-field distance to the receiver to normalize pressure. Start a copy of the same seeded material voice at the computed sample and apply pressure filters and the ear model to it. [^synth]

# Invariants

A transparent path adds no voice. Changing a triangle diagonal or combining the coplanar panels of an opening under one mesh leaves its audible path intact. A sealed triangulated enclosure has no open sheet boundary. A certified convex shell can still supply exterior wedges for a source and a listener outside it. Materials on each leg keep their own response. Samples before the additional arrival match direct-only playback. The receipt identifies every bend in order from the contact, including its full node path and coordinates.

# Cost

Boundary extraction is lazy for each binding. A single impact checks the edges of the crossed surfaces. A pair search expands crossings found on the two legs and tests each distinct ordered pair once. Its worst-case number of pairs is quadratic in the number of relevant exposed segments. Camera motion can require multiple path evaluations. The mixer adds at most one indirect voice per contact.

# Failure modes

The sheet extractor handles open boundaries. Closed convex components have a separate exterior-wedge test. Concave solids without a suitable decomposition can still cast an unmodelled shadow. [Exterior wedges](/algorithms/exterior-acoustic-edges.md) Three successive turns, many reflected arrivals, and interference between competing indirect paths need more propagation work. A bent ray can miss a reachable opening if no candidate leg discovers its surface. The three frequency bands and estimated insertion losses need validation against acoustic measurements.

# Alternatives

The general sound engine uses the four edges of each occluding quadrilateral and blends the transmitted and single-edge responses near a shadow boundary. The city searches exposed triangle edges for an opening, or certified exterior wedges on a solid. It can schedule the transmitted impact and a one- or two-bend arrival at different samples. Both use the sound speed from the shared engine. [Sound ways](/algorithms/sound-ways.md) · [Timed city contacts](/algorithms/city-contact-sound.md)

Room and portal graphs can precompute possible connections across an extensive interior. A wave solver can model fine interference at substantially higher cost.

# Oracle

Record the same solved hit behind aligned and displaced openings, under several mesh bindings and triangulations, through a transmission panel and inside a sealed box. Compare optimized points with numerical or unfolded geometric controls. Turn the listener, move the camera during flight, check the stereo samples against direct-only playback, then repeat with two openings. [Executable oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-impact-diffraction.py)

# Reuse

Run `python3 worlds/ville-paris/check-impact-diffraction.py --bin target/release/ville-paris`. In an impact report, `diffracted_path.bends` lists the interaction points in order. `first_path_frame` is the earliest path with nonzero modelled energy. Filtering and 16-bit quantization can move the first nonzero PCM sample later. Use `--direct-only` to hear the direct component on its own.

[^model]: [European Commission, common noise assessment methods](https://www.legislation.gov.uk/eudr/2015/996/pdfs/eudr_20150996_adopted_en.pdf).
[^synth]: [Modal impact synthesis](https://github.com/smichea/osage/blob/main/crates/osage-sound-voices/src/contact.rs).
