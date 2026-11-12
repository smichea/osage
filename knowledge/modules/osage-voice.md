---
type: Module
title: osage-voice
description: "Players talking to one another as in a conference call: speech in ADPCM frames over UDP to a relay beside the world, each voice queued and mixed, any of them muted; the microphone opened only when the game asks."
resource: https://github.com/smichea/osage/tree/main/crates/osage-voice
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [audio, voice, conference, udp, adpcm, engine]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-12T23:30:00Z" }
---

# Purpose

Let the players of a world talk to one another while they play, as in a conference call (D-060): everyone hears everyone, each can mute anyone, and nobody's microphone is open unless they chose it. An engine mechanism: when to send (push to talk, an open microphone) is the game's choice and its player's.

# Concepts

- **Frame:** 20 ms of speech, 320 samples at 16 kHz mono, coded in IMA ADPCM into 164 bytes: the predictor and step index, then four bits a sample. Each frame decodes on its own.
- **Relay:** a UDP socket beside the world that knows each player by the address they send from, once they said hello, and hands each one's frames to the others with their number; it sends everyone the roster (numbers and names) every two seconds and when it changes, and forgets a player silent for eight seconds or who said bye.
- **Voice:** a player's end: a socket connected to the relay, a thread that says hello every two seconds and takes in what comes back, the voices queued per speaker.
- **Feed:** the voices mixed, mono at 16 kHz, read by the audio output without waiting.

# Architecture

`wire` is the datagram format: `OSV1`, a kind, then hello (a name of at most 32 bytes), voice (a sequence number, a frame), bye, heard (speaker, sequence, frame), roster. `adpcm` codes frames; the encoder starts each frame from its first sample and a step index fit to its first change.

`Voice::join` binds a socket and connects it to the relay; `say` codes a frame and sends it; frames that come back are decoded and queued for their speaker unless muted, those late or twice dropped by sequence number. A speaker is heard once two frames are queued (40 ms) and stops when the queue runs dry, refilling before starting again; a queue longer than eight frames (160 ms) is cut back to two, so a voice does not fall behind. `Feed::sample` mixes a frame of every speaker at a time, under the lock, then hands it out sample by sample; a soft limiter (tanh) keeps several at once from clipping.

With the `device` feature, `mic::Microphone::open` opens the system's default input (cpal) on a thread of its own, which keeps the stream; its samples are brought to mono, to 16 kHz by linear interpolation, cut into frames, each with its loudness, and queued (frames are dropped if nobody reads). Dropping it closes the microphone. With the `bevy` feature, `audio::VoiceSource` makes a feed a Bevy audio source.

# Interfaces

`Voice::{join, say, mute, set_gain, heard, feed}`, `Heard { name, talking, muted }`, `Feed::sample`; `relay::serve(UdpSocket) -> Relay { port }`; `wire::Packet::{encode, decode}`; `adpcm::{encode, decode, frame_bytes}`; `mic::{Microphone, Spoken}` (`device`); `audio::{VoicePlugin, VoiceSource}` (`bevy`); `RATE`, `FRAME`, `FRAME_BYTES`.

# Invariants

- A voice comes back from ADPCM more than 20 dB above its coding noise; silence stays silence; a short frame is refused. Tested.
- Every packet reads back; a name is cut at a character's edge; what is not a packet is refused. Tested.
- What one says, the others hear, unless they muted them; nobody hears themselves; a player who leaves leaves the roster. Tested (three players through a relay).

# Knowledge

- UDP and a short queue suit speech: a frame late is worth less than none.
- On a local network the queue rarely passes two frames; over the internet it would need to be deeper, and Opus would pay for itself.
- An open microphone with loudspeakers sends the game's sound too: the gate opens on it. A headset, or push to talk.

# Reuse

Any world whose players are together: a party in a hub, a team in a match, a lesson in a classroom world. The relay is a few dozen lines to put beside any server.

# Limits

No echo cancellation, no noise suppression, no automatic gain; no encryption (the local network is trusted, as for the game); the relay admits anyone who can reach it; linear resampling of the microphone; no voice in the browser client yet.
