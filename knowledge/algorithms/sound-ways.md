---
type: Algorithm
title: Sound ways
description: The ways a sound takes from an emitter to a head among flat faces — straight, through what lets sound through, round the edge by which it arrives loudest (Maekawa's screen loss read from the Fresnel number, continued into the lit side), mirrored from an image source — chosen by what the ways are, never by the order the faces are listed in, so that nothing steps as an emitter goes behind a face.
resource: https://github.com/smichea/osage/blob/main/crates/osage-sound/src/ways.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [sound, acoustics, diffraction, image-source, occlusion]
sources:
  - id: maekawa
    resource: "Z. Maekawa, Noise reduction by screens, Applied Acoustics 1 (1968)"
    title: Noise reduction by screens
  - id: arup
    resource: https://strutt.arup.com/help/Environmental_Noise/EnviroBarrierAtten.htm
    title: Barrier attenuation, the piecewise form of Maekawa's curve
  - id: allen-berkley
    resource: https://doi.org/10.1121/1.382599
    title: J. Allen and D. Berkley, Image method for efficiently simulating small-room acoustics, JASA 65(4), 1979
  - id: iso9613
    resource: "ISO 9613-1:1993 and ISO 9613-2:1996, Attenuation of sound during propagation outdoors"
    title: ISO 9613
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-24T09:00:00Z" }
---

# Problem

A sound in a city reaches a listener straight, through glass, over the edge of a deck that hides it, and off the road and the walls. A game needs each of these ways — its length (so its delay), the point it arrives from (so its direction) and what is left of it in a few bands — every few milliseconds, for dozens of emitters, cheaply, and with no jump when an emitter passes behind something: a jump is heard as a click and a lurch of level.

# Idea

Find at most one way that is not mirrored and one per reflecting face. The way that is not mirrored is the straight one, multiplied by what every face it crosses lets through, or — if it is louder — the way round the edge that lengthens it least, quieter and duller by how many half wavelengths longer it is[^maekawa]. The screen loss is continued into the lit side, so a straight way that only grazes an edge already loses a little, and just behind an edge the two ways are added: there is nothing to switch. Mirrored ways come from the emitter's image behind each face[^allen-berkley]. Every choice between ways is made from the ways themselves.

# Method

Faces are flat quadrilaterals, each with an acoustic material giving, at 250 Hz, 1 kHz and 4 kHz, the share of pressure it lets through, mirrors and scatters. Ways are shaped in four bands: those three and 9 kHz, which takes the 4 kHz share.

**Edge loss.** For a way longer than the straight one by `δ` metres, at frequency `f`, the Fresnel number is `N = 2δf/c` (c = 343 m/s). The loss in dB[^arup]:

    N < −0.25:        0
    −0.25 ≤ N < 0:    6 − 12·√(−N)
    0 ≤ N < 0.25:     6 + 12·√N
    0.25 ≤ N < 1:     8 + 8·√N
    N ≥ 1:            16 + 10·log10 N
    capped at 20 dB (one edge, ISO 9613-2)

A negative `δ` is a straight way passing that close to an edge without being hidden.

**The point of an edge.** For an edge from `a` to `b`, the shortest way from source to head over it is found by unfolding about the edge: with `z_s`, `z_h` the positions of source and head along the edge and `r_s`, `r_h` their distances from its line, the point is at `z = z_s + (z_h − z_s)·r_s/(r_s + r_h)`, clamped to the edge.

**The way that is not mirrored**, from source `s` to head `h`:

1. For each face that hides (`occludes`): if the segment `s–h` crosses it, multiply each band by what its material lets through. Otherwise take the nearest of its four edges, compute the grazing loss for `−(detour)`, and keep, per band, the smallest `passed + (1 − passed)·10^(−loss/20)` over all faces beside the way: a face has as much of an edge as it stops, and a pane that lets everything through has none. Multiply the straight way by that.
2. Three faces crossed or more: their shares are multiplied in the order the segment meets them (sorted by distance), so the product is the same to the last bit whatever the faces' order.
3. If no face was crossed, that is the way.
4. Otherwise, for each crossed face and each of its edges: the way round is `|s − p| + |p − h|`; its bands are `10^(−loss(δ, f)/20)·(1 − passed)`, times what stands on each of its two legs (each stopped 0.1 mm short of the edge, so the faces meeting there do not count). Keep the one that arrives loudest at 1 kHz (`bands[1]/length`); of two as loud the shorter; of two as long, the one whose point is first by x, then y, then z; then by its bands. Skip early an edge that is longer and stops no more than the one kept.
5. Of the way through and the way round, keep the louder at 1 kHz, and add the other with the weight `clamp(1 − 2·(L_round − L_straight)·f/c, 0, 1)` per band: at the edge itself the two are one sound; half a wavelength beyond, only the louder remains.
6. A way under 10⁻³ of its pressure in every band (−60 dB) is no way.

**A mirrored way**, by a reflecting face with unit normal `n`:

1. Source and head must be on the same side, each at least 1 mm off the plane. The image is `s − 2·h_s·n` (`h_s` the source's height above the plane); the mirror point `p` is where the line from the image to the head crosses the plane.
2. Fade the reflection over the last 0.5 m before the face's rim, with a smoothstep of how far inside `p` is: an emitter moving along a wall does not switch its echo on and off.
3. Its bands are the material's mirrored shares times the fade; at the qualities that check (`careful`), times what stands on each leg, and no way if the legs are stopped.
4. Its length is `|image − h|` and it arrives from `p`.
5. **Scatter**: what the face scatters goes to the reverberation, from the face's point nearest `p` (the mirror point may be off the face), with energy `mean(scattered)/L²` times what its legs let through squared, at the time that way takes.

**Air**, per metre at the scene's temperature and humidity (15 °C and 70 % unless it says otherwise), from ISO 9613-1[^iso9613]; the engine turns it into two one-pole low-passes per way ([sound delay lines](/algorithms/sound-delay-lines.md)).

# Invariants

- No way changes by a bit when the faces are listed in another order (tested with faces drawn at random).
- Nothing steps as an emitter goes behind a face, whatever the face lets through (tested by sweeping an emitter behind a pane and a wall).
- A pane gone through leaves a screen beside it its edge; a wall hides wherever it is listed.
- Distance, the floor's reflection and its delay, what a pane lets through and an echo's time of arrival match the physics within the tests' tolerances.

# Cost

Per emitter and per look: one segment test per face for the straight way, four edges per crossed face, and per reflecting face an image, a point and (careful) two segment tests. On the Pont de Bir-Hakeim (41 emitters, a few dozen faces), finding the ways is a few hundredths of the engine's whole time; see `tools/paris-soundscape/DESIGN.md` for the measurements.

# Failure modes

- One edge per way: an emitter under the middle of a deck, which only a way round two edges would reach, is silent rather than faint.
- One reflection per face: no reflection of a reflection.
- The loss of a thin screen, applied to thick objects, is a little too high for some angles.
- A face that moves mirrors from where it is now, not from where it was when the sound met it.

# Alternatives

Ray or beam tracing finds many orders of reflection and diffraction at a far higher cost; the uniform theory of diffraction (UTD) or Biot–Tolstoy–Medwin models give an edge's phase, which a game rarely needs. Precomputed propagation (a probe grid baked per scene) is cheap at run time but not for moving geometry.

# Oracle

The tests of `crates/osage-sound/tests/physics.rs`, which measure renders with [osage-sound-analysis](/modules/osage-sound-analysis.md); [soundscape verify](/oracles/soundscape-verify.md) for the whole scene.

# Reuse

Any engine with flat or flattened occluders: the edge loss continued into the lit side, the two ways added near the edge, and tie-breaking by the ways' own geometry carry over unchanged.

[^maekawa]: Noise reduction by screens
[^arup]: Barrier attenuation, the piecewise form of Maekawa's curve
[^allen-berkley]: J. Allen and D. Berkley, Image method for efficiently simulating small-room acoustics, JASA 65(4), 1979
[^iso9613]: ISO 9613
