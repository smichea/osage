---
type: Algorithm
title: Space decay from rays
description: How long the space round a listener rings, found from its faces — rays sent from the head bounce from face to face, mirrored or scattered, losing what each material does not send back and what the air takes, until they leave for the sky; the energy still in flight falls as the space's sound does, and its fall from 5 to 25 dB down gives the reverberation its decay in each band, kept up to date a few rays a block.
resource: https://github.com/smichea/osage/blob/main/crates/osage-sound/src/decay.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [sound, reverberation, ray-tracing, room-acoustics]
sources:
  - id: eyring
    resource: "C. F. Eyring, Reverberation time in 'dead' rooms, JASA 1 (1930)"
    title: Reverberation time in 'dead' rooms
  - id: kuttruff
    resource: "H. Kuttruff, Room Acoustics, 5th edition, Spon Press, 2009, chapters 4, 5 and 9"
    title: Room Acoustics
  - id: splitmix
    resource: "G. Steele, D. Lea and C. Flood, Fast splittable pseudorandom number generators, OOPSLA 2014"
    title: Fast splittable pseudorandom number generators
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-25T22:00:00Z" }
---

# Problem

The [feedback delay reverberation](/algorithms/feedback-delay-reverb.md) dies at the decay it is given in three bands. A scene that types those decays in has to guess them, and they stay the same wherever the listener goes: a closed courtyard and an open street ring alike. A game needs the decay of the space round the listener from the faces it already has, cheaply, following the listener as they move.

# Idea

Send rays from the head and follow each from face to face as sound energy would go[^kuttruff]. The energy in flight, summed over all rays, falls with time as the space's sound does. Fitting that fall gives the decay. In a closed room whose faces scatter, the result is Eyring's decay[^eyring]. In an open street, the rays that escape upward at each bounce set it. A few rays each block, with older ones slowly forgotten, keep the estimate following the listener.

# Method

`Decay` keeps, in bins of 5 ms over 10 s, the energy in flight in each band (250 Hz, 1 kHz, 4 kHz), and the energy the rays set off with.

1. **A ray** sets off from the head in a direction drawn evenly over the sphere, from SplitMix64[^splitmix] seeded by the scene, with energy 1 in each band.
2. **The face it meets.** Among the faces that reflect or hide, the ray meets the nearest one crossed by the segment from where it is to 300 m ahead. Of two at the same distance, the first is taken. If it meets nothing, it has left for the sky, and nothing more of it is counted.
3. **In flight.** Until it meets the face, the ray adds its energy to each bin it passes through, in proportion to the time it spends there. It loses what the air takes, `10^(−dB/10)` a metre at each band's centre (ISO 9613-1, at the scene's temperature and humidity).
4. **At the face.** A face that hides without reflecting takes all of it. Otherwise each band keeps `1 − absorption − transmission`: what the face lets through has left this side of it. The ray goes on in a direction drawn by Lambert's law with probability `scattering`, and in the mirrored direction otherwise.
5. **The end.** A ray stops when every band has lost 35 dB (the fit stops at 25), after 400 faces, or after 10 s.
6. **The decay.** For each band, find where the energy in flight falls 5 dB and 25 dB below its first bin, interpolating between bins. The decay is three times the time between the two, kept between 0.05 and 10 s, as a scene's would be. A space the rays all leave at once rings for 0.05 s.
7. **In the engine.** A scene asks for this with `"secondsT60": "fromFaces"`. The decay is computed only where the reverberation is made. The first block sends 1 024 rays from the head. Each block after that forgets 1/512 of what was found and sends 2 more. Every 32 blocks, the tail is given the decay found. The time this takes is counted in the tally's reverberation.

# Invariants

- In a closed box 12 × 5 × 8 m whose faces scatter everything, the decay is Eyring's with Kuttruff's correction for how much the lengths between faces vary in a box (relative variance 0.4), air included. The rays alone match it within 2 % at absorptions of 0.08, 0.2 and 0.4. In the engine, for faces that take a tenth and scatter 0.6, it is within 10 % (tested).
- Without its ceiling, a room rings less than half as long. With no faces, it rings for the shortest decay. Before any ray, there is no decay (tested).
- After a listener steps from a large room into a small one, the decay follows: within 15 % of the small room's own (tested).
- A lower room rings shorter, in the engine (tested).

# Cost

Each ray costs one search of the face index per face it meets. On the bridge asked to ring as its faces say, at reference quality, this added 0.45 s of processor time to a 45 s render: 1 % of the time the sound lasts. In a closed room of hard faces, rays live longer and cost more, up to 400 searches each.

# Failure modes

- One decay for the whole space round the listener. A source in another room is not given that room's decay.
- The faces near the listener stand for the whole space. The bridge's seven faces do not stand for the city round it, so the bridge keeps the decay its scene gives.
- Energy that a face lets through is lost, not carried into the space beyond.
- Changing a decay changes how fast what the tail already holds dies. Nothing crossfades between two decays.

# Alternatives

- **Sabine or Eyring from a volume and a surface** is cheaper. It needs a closed volume, which a street does not have.
- **A full ray or beam tracer** that renders an impulse response gives more than a decay, at a far higher cost.
- **Probes baked per scene** cost nothing at run time, but do not follow geometry that moves.

# Oracle

The tests of `crates/osage-sound/src/decay.rs`, and the engine test `a_space_asked_to_ring_as_its_faces_say_rings_as_a_room_of_them_does` in `crates/osage-sound/tests/physics.rs`.

# Reuse

Any engine with a reverberation whose decay is a parameter: the energy-in-flight histogram with forgetting, and the T20 fit, need only a ray-versus-faces query.

[^eyring]: Reverberation time in 'dead' rooms
[^kuttruff]: Room Acoustics
[^splitmix]: Fast splittable pseudorandom number generators
