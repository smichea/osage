<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# The soundscape of the Pont de Bir-Hakeim

Forty-five seconds on the upstream pavement of the bridge, walking from Passy towards Grenelle. The Seine is below on the left, cars pass at road level on the right, and a line 6 train comes along the viaduct above, passes and goes. Every sound is synthesised from the state of what makes it; nothing is recorded. The render is offline and needs no server: the same seed gives the same bytes.

## Hear it

From the repository root, with Rust 1.90 or later. The command is a member of the repository's workspace; the sound itself is the engine's: `crates/osage-sound` (the scene, the ways, the ears, the engine), `crates/osage-sound-voices` (the voices) and `crates/osage-sound-analysis` (what `verify` and `measure` read in a recording). The only crate they take from outside the repository is `serde_json`, to read the scene.

```sh
cargo run --release -p paris-soundscape -- \
  render --out tools/paris-soundscape/example/bir-hakeim-headphones.wav
cargo run --release -p paris-soundscape -- \
  render --listening speakers --out tools/paris-soundscape/example/bir-hakeim-speakers.wav
```

Each writes a stereo WAV (16 bits, 48 kHz) and prints its SHA-256. Use the first with headphones: each ear gets its own delay and the head's shadow. Use the second with loudspeakers, where your own head does that. The two examples are not kept in git; they are made again to the same bytes (the digests are at the end of this page).

## Where each sound is

A source is a node path in an object tree and a position local to that node. The tree is `scenes/bir-hakeim.json`, a small made-up bridge in the frame of the city fixture (metres, Y up, +X from Passy to Grenelle, −Z upstream). This prints it, with where each source is heard from at an instant:

```sh
cargo run --release -p paris-soundscape -- tree --at 25.85
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

(An excerpt.) That emitter is named `bridge / upper-deck / track-to-passy / train / car-3 / bogie-5 : tyres`: the path of its node, then its own name. `--solo` and `--mute` take any part of such a name. A contact bears the names of the two that met — the surface struck, named as an emitter is, and the node that struck it: `bridge / road-deck : joint-plate : contact with bridge / road-deck / pavement-upstream / walker`. So `--solo walker` renders the steps, and `--mute road-deck` silences them with the road. A contact left out takes none of the voices kept for contacts; nor does one that does not reach the ears — too far, or behind what stops it — and a contact takes a voice only from one that is heard less. Its strength and its grain are its own: the same whoever else walks or rolls in the scene.

The order the file lists its nodes in, and each node its emitters, surfaces and touches, is no part of the scene: the same file in another order renders to the same samples.

A file may hold several glTF scenes. Only the one its `scene` names is read: its roots and what is under them.

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
- **What it sounds like:** the voices' parameters — `metro-motor` (`carrier`, `electrical`, `mesh`, `gear`), `metro-bogie` (`wheel`, `body`, `rattle`), `metro-span` (`tone`, `ring`, `rattle`). `cargo run --release -p osage-sound-voices --example audition -- --list` names them all.
- **How muffled it is:** the viaduct's deck hides the wheels from the pavement. The `top` and `underside` surfaces of `upper-deck` set where its edge is, and their material what it lets through.
- **From how far each bogie is heard for itself:** `proxyBeyondM` on the train node. Beyond it, two emitters stand for the thirteen.
- **The knock of the axles:** the `runway-joint` surface of `upper-deck`, and `steel-runway-joint` in `contactSounds`.

`--solo upper-deck` renders the train alone, `--mute upper-deck` everything else.

## Checking it

```sh
cargo run --release -p paris-soundscape -- verify
cargo test -p osage-sound -p osage-sound-voices -p osage-sound-analysis -p paris-soundscape
```

`verify` renders the scene several times and checks that the same seed gives the same bytes, that nothing clips (no sample held, true peak under −1 dBTP), that the file as written is as loud as the scene asks (within 0.3 LU, or held under it by its peaks, which are then at the ceiling), that turning the listener half round moves the river to the other ear (sample for sample: the river feeds no reverberation, which does not turn with the head), that the river stays on its side a second at a time and in the render's last second whatever its length (what it does for less than a second is in its second's mean), and that the train comes, passes and goes. These are the brief's checks: on a scene without a river or a train, or a render too short to hold the train's passage, the check that cannot be made has failed, and says why. `measure FILE.wav` prints the same figures for any mono or stereo file, whatever made it.

## Measured ears

For headphones the engine hears with a model of a head unless it is given measured responses. Those are SADIE II's KEMAR (University of York, Apache-2.0), fetched and derived on this machine (D-242):

```sh
python3 tools/fetch-assets.py          # assets/sadie/D2_HRIR_SOFA.zip, among the others
python3 tools/make-hrir.py             # assets/sadie/d2-48k.hrir (needs h5dump, from HDF5)
python3 tools/make-hrir.py --check     # a noise round the head, against the measured set
cargo run --release -p paris-soundscape -- render --hrir assets/sadie/d2-48k.hrir
```

`--hrir` is taken by `render`, `verify`, `bench`, `live` and `ears`, at high and reference quality.

## Against another engine

```sh
cargo run --release -p paris-soundscape -- against --reference DIR [--hrir assets/sadie/d2-48k.hrir]
```

renders the controlled scenes (`scenes/controlled-*.json`, made by `scenes/make_controlled.py`: a noise going round, a tone passing, a tone far away, clicks at a distance, a screen, a floor) raw at reference quality, and reads them beside another engine's captures of the same geometries in `DIR` (`circle.wav`, `passby.wav`, `far.wav`, `click-near.wav`, `click-far.wav`, `screen.wav`, `screen-absent.wav`, `floor.wav`, `floor-absent.wav`; a missing file leaves its column empty). It prints markdown tables: where the ears hear the sound, its pitch, when it arrives, what a surface takes away. The figures against reference A are in [research 11](../../research/11-sound-against-reference.md). A blind listening test of the same takes has its protocol and kit in [listening/](listening/PROTOCOL.md).

## Recorded voices

```sh
cargo run --release -p paris-soundscape -- render --scene tools/paris-soundscape/scenes/bir-hakeim-recorded.json --out recorded.wav
cargo run --release -p paris-soundscape -- lod
```

The first renders the bridge with recorded voices: the scooter's engine from the kart's loops, the plate's knocks from the kart's knocks. Both come from CC0 recordings in the asset manifest (`assets/freesound/kart/`, made by `tools/make-sounds.py` from originals a person downloads). `--assets DIR` says where they are (default: `assets`). The scene is made from the bridge's by `scenes/make_recorded.py`.

The second measures what each level of detail keeps and saves, by third of an octave below what the air leaves:

- the loops' levels and their bytes;
- what a loop voice costs a sample at each level;
- a loop going away from 10 m to 880 m beside the same at full detail;
- the recorded bridge with and without levels of detail.

See [recorded voices](../../knowledge/algorithms/recorded-voices.md).

## Live

```sh
cargo run --release -p paris-soundscape -- live --quality medium
```

plays the scene as it goes, as an audio output takes it (`--buffer-ms`, 10 by default, at a time on the clock) while a game walks the listener along the bridge with a pose sixty times a second; it prints what the output found missing and how late a pose was heard. Nothing is sent to a device: it measures. A client plays the same engine through `osage_sound::live` (with the `bevy` feature, as a Bevy audio source).

In a browser, the engine is `crates/osage-sound-web`, played in an AudioWorklet:

```sh
cargo build --release --target wasm32-unknown-unknown -p osage-sound-web
python3 tools/paris-soundscape/web/live.py check --quality medium   # the worklet's samples are the module's
python3 tools/paris-soundscape/web/live.py play --quality low       # 45 s in Chromium: what each block costs
```

`web/live.html` is the page; `crates/osage-sound-web/web/` holds the worklet and its wrapper.

## Far sound as a field

```sh
cargo run --release -p paris-soundscape -- far --sphere 64          # a field against every emitter by its own ways, and the cost
cargo run --release -p paris-soundscape -- far                      # the same on the bridge, beyond 60 m
cargo run --release -p paris-soundscape -- verify --far-beyond 60 --far-order 2
python3 tools/paris-soundscape/bench/field_opus.py                    # its bit rate in Opus
```

`--far-beyond M` on `render`, `verify` and `live` hears what is farther than M from the centre of the listener's cube (`--cube`, 27 m) as an ambisonic field of order `--far-order` (1 or 2) made there. `field` writes that field as a WAV file of 4 or 9 channels, and `decode` hears one; `compare A.wav B.wav` prints how two renders differ.

## More

- [DESIGN.md](DESIGN.md): how it works, what an object carries for sound (what it emits, what it reflects, how it sounds when touched), what each quality costs, and what a second batch should do.
- [bench/README.md](bench/README.md): the same machine's cost for OpenAL Soft and for Chromium's Web Audio.

The examples made on 2026-11-26 (x86-64 Linux, rustc 1.99, seed 6), after the review of the voices (issue #20):

```
bir-hakeim-headphones.wav  sha256 a390278d5b252c121b54095b7ccbff0b11b87e84f78509e85c77de4a3c880cf5
bir-hakeim-speakers.wav    sha256 583e77b8d8306d9b94bcbdebaa5f1a8e2d697bda98a7277661573581c15e565c
```

The digests hold for one machine and compiler: the voices use the platform's own sine and exponential.
