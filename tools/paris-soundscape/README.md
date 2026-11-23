<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# The soundscape of the Pont de Bir-Hakeim

Forty-five seconds on the upstream pavement of the bridge, walking from Passy towards Grenelle. The Seine is below on the left, cars pass at road level on the right, and a line 6 train comes along the viaduct above, passes and goes. Every sound is synthesised from the state of what makes it; nothing is recorded. The render is offline and needs no server: the same seed gives the same bytes.

## Hear it

From the repository root, with Rust 1.90 or later (the only crate from outside the repository is `serde_json`, to read the scene):

```sh
cargo run --release --manifest-path tools/paris-soundscape/Cargo.toml -- \
  render --out tools/paris-soundscape/example/bir-hakeim-headphones.wav
cargo run --release --manifest-path tools/paris-soundscape/Cargo.toml -- \
  render --listening speakers --out tools/paris-soundscape/example/bir-hakeim-speakers.wav
```

Each writes a stereo WAV (16 bits, 48 kHz) and prints its SHA-256. Use the first with headphones: each ear gets its own delay and the head's shadow. Use the second with loudspeakers, where your own head does that. The two examples are not kept in git; they are made again to the same bytes (the digests are at the end of this page).

## Where each sound is

A source is a node path in an object tree and a position local to that node. The tree is `scenes/bir-hakeim.json`, a small made-up bridge in the frame of the city fixture (metres, Y up, +X from Passy to Grenelle, −Z upstream). This prints it, with where each source is heard from at an instant:

```sh
cargo run --release --manifest-path tools/paris-soundscape/Cargo.toml -- tree --at 25.85
```

```
at 25.85 s the listener (bridge / road-deck / pavement-upstream / walker / head) is at [-22.41, 9.60, -11.35]
bridge  at [0.00, 0.00, 0.00]
  upper-deck  at [0.00, 16.00, 0.00]
    track-to-passy  at [0.00, 0.20, -1.50]
      train  at [-22.41, 0.00, 0.00]  moving along [-1.00, 0.00, 0.00] at 50.4 km/h  [stands for its branch beyond 160 m]
        car-3  at [0.00, 0.00, 0.00]
          bogie-5  at [-5.00, 0.00, 0.00]
            ♪ tyres — metro-bogie at [0.00, 0.30, 0.00] +4.0 dB  → 13.0 m, +117° to the right, +32° up
```

(An excerpt.) That emitter is named `bridge / upper-deck / track-to-passy / train / car-3 / bogie-5 : tyres`: the path of its node, then its own name. `--solo` and `--mute` take any part of such a name.

Move or turn a node and everything under it follows. `scenes/make_bir_hakeim.py` is how the file was made.

## Adjusting the metro's passage

The train is the node `bridge / upper-deck / track-to-passy / train`. Three options change it without editing anything:

| Option | What it does | In the scene |
|---|---|---|
| `--metro-at 25.85` | The instant, in seconds, at which the middle of the train is level with the listener. The whole passage moves with it. | `motion.atS`, and the node's `translation` |
| `--metro-speed 50.4` | Its speed over the bridge, km/h. The run keeps its shape: it pulls away, holds this speed, brakes. | `motion.speedKmh`: pairs of seconds from `atS` and km/h |
| `--metro-gain 0` | Louder or quieter by so many decibels: bogies, motors and the viaduct's rumble together. | each emitter's `gainDb` |

For more, edit `scenes/bir-hakeim.json` (or the constants at the top of `scenes/make_bir_hakeim.py`, then run it):

- **Which way and which track:** `motion.alongM` is the way it goes; the track node's `translation` puts it on the upstream or the downstream track.
- **What it sounds like:** the voices' parameters — `metro-motor` (`carrier`, `electrical`, `mesh`, `gear`), `metro-bogie` (`wheel`, `body`, `rattle`), `metro-span` (`tone`, `ring`, `rattle`). `cargo run --manifest-path tools/paris-soundscape/Cargo.toml -p osage-paris-voices --example audition -- --list` names them all.
- **How muffled it is:** the viaduct's deck hides the wheels from the pavement. The `top` and `underside` surfaces of `upper-deck` set where its edge is, and their material what it lets through.
- **From how far each bogie is heard for itself:** `proxyBeyondM` on the train node. Beyond it, two emitters stand for the thirteen.
- **The knock of the axles:** the `runway-joint` surface of `upper-deck`, and `steel-runway-joint` in `contactSounds`.

`--solo upper-deck` renders the train alone, `--mute upper-deck` everything else.

## Checking it

```sh
cargo run --release --manifest-path tools/paris-soundscape/Cargo.toml -- verify
cargo test --manifest-path tools/paris-soundscape/Cargo.toml --workspace
```

`verify` renders the scene several times and checks that the same seed gives the same bytes, that nothing clips (no sample held, true peak under −1 dBTP), that turning the listener half round moves the river to the other ear (sample for sample), that the river stays on its side, and that the train comes, passes and goes. `measure FILE.wav` prints the same figures for any stereo file, whatever made it.

## More

- [DESIGN.md](DESIGN.md): how it works, what an object carries for sound (what it emits, what it reflects, how it sounds when touched), what each quality costs, and what a second batch should do.
- [bench/README.md](bench/README.md): the same machine's cost for OpenAL Soft and for Chromium's Web Audio.

The examples made on 2026-11-23 (x86-64 Linux, rustc 1.99, seed 6):

```
bir-hakeim-headphones.wav  sha256 8e4a95781e69ea47850f97cdff4c1541951be53f195ab3b39532927503da7738
bir-hakeim-speakers.wav    sha256 83f6ebbab2fa2a883e7d50af28c064bab3e342b7e90132cd1571eba21031e888
```

The digests hold for one machine and compiler: the voices use the platform's own sine and exponential.
