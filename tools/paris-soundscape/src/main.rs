// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The soundscape of the Pont de Bir-Hakeim, rendered offline:
//!
//!     paris-soundscape render --out bir-hakeim.wav
//!     paris-soundscape tree --at 25
//!
//! See the README beside this crate.

mod against;
mod lod;

use std::{path::PathBuf, process::ExitCode, time::Instant};

use osage_sound::{
    ears::Listening,
    engine::{Options, Quality, RATE},
    render::{fade, level, master, render},
    scene::{Reach, Scene},
    space::Vec3,
};
use osage_sound_analysis::{sha256, wav};

const HELP: &str = "\
paris-soundscape: a scene's sound placed in its object tree and carried to two ears, offline.

  render   the scene to a stereo WAV file (16 bits, 48 kHz)
             --out FILE          where (default: bir-hakeim.wav)
             --listening HOW     headphones (default) or speakers
             --quality NAME      low, medium, high or reference (default)
             --seconds S         how long (default: the scene's own, 45)
             --seed N            the seed of every noise (default: the scene's own)
             --turn DEGREES      the listener's head turned to its left by so much
             --solo TEXT         only emitters whose name holds TEXT (may be given again)
             --mute TEXT         not the emitters whose name holds TEXT
                                 (a contact bears the names of the surface struck and of the node that struck it)
             --gain-db DB        this gain instead of the one that brings the file to the scene's loudness
             --metro-at S        the instant the train's middle is level with the listener
             --metro-speed KMH   its speed over the bridge
             --metro-gain DB     louder or quieter than the scene has it
             --far-beyond M      what is farther than M from the centre of the listener's cube, heard as an
                                 ambisonic field made there (on render, verify and live)
             --far-order N       the field's order, 1 (4 channels, default) or 2 (9)
             --cube M            the cube's side (default 27; 0: the field made where the listener stands)
             --hrir FILE         measured head responses for headphones at high and reference
                                 (assets/sadie/d2-48k.hrir, made by tools/make-hrir.py; on render, verify,
                                 bench, live and ears)
  far      far sound as a field against every emitter by its own ways, at the first and second orders: the
           differences in loudness, in each octave and in what the ears compare (level and time, by band,
           second by second); and what it costs for 1, 4 and 16 listeners in one cube
             --far-beyond M      the boundary (default 60 on a scene, half the radius on a sphere)
             --sphere N          N noises on a sphere round the listener instead of the scene (--radius M,
                                 default 150), the field made where the listener stands
             takes --quality (default medium), --listening, --seconds (default 10)
  compare A.wav B.wav
           the same differences between two files of the same length (--json for a receipt)
  field    the far part of a scene as its ambisonic field, a WAV file of 4 or 9 channels (ACN, SN3D)
             --out FILE --gain-db DB   (takes --far-beyond, --far-order, --cube, --sphere, --radius, --seconds)
  decode FIELD.wav
           a field heard by a listener at its centre, looking ahead: a stereo WAV file
             --out FILE --gain-db DB   (takes --listening and --quality)
  tree     the scene's nodes, emitters and surfaces
             --at S              where everything is at that instant, from the listener
  verify   the scene rendered several times and checked: the same seed gives the same bytes, nothing
           clips, the file is as loud as the scene asks, turning the listener half round moves a side
           source to the other side, the river stays on its side, the train comes, passes and goes
           (takes --quality, --listening, --seconds); a check whose sound is missing has failed
  bench    what the engine costs on this machine: processor time for a second of sound at each quality,
           on the bridge and on a ring of 1 to 256 noises (the scene of bench/), and the memory it keeps
             --json FILE         a receipt
             --quick             fewer and shorter runs
             --part NAME         only the bridge, the faces (cut 10, 100 and 1 000 times), the ring or the way
  hrir     a measured set's file for the engine, from the arrays tools/make-hrir.py takes out of SOFA
             --ir FILE --positions FILE --out FILE   (--taps 256, --rate 48000, --keep 64; 54 loudspeakers in rings)
  ears     a source going round the listener, then from below to above, heard by the sphere model and
           by the measured responses: the time and level between the ears by band, against the set
             --hrir FILE (needed)
  against  the controlled scenes (scenes/controlled-*.json) rendered raw at reference quality, read beside
           another engine's captures of the same geometries: markdown tables, figure for figure
             --reference DIR     the captures (circle.wav, passby.wav, far.wav, click-near.wav,
                                 click-far.wav, screen.wav, screen-absent.wav, floor.wav, floor-absent.wav)
             --hrir FILE         also with measured ears
  lod      what each level of detail of a recorded voice keeps and saves: the kart's engine loops at their
           levels, what a loop voice costs a sample at each, a loop going away beside the same at its best
           (by third of an octave below what the air leaves), the recorded bridge with and without levels
           of detail (takes --assets)
  measure FILE.wav
           what any stereo file tells of where its sounds are: peaks, loudness, octaves, and second
           by second the level and time differences between the ears (--json for a receipt)

  live     the scene played as it goes, as an audio output takes it, its listener walked by a game: what
           the output found missing, and how late a pose is heard (takes --quality, --listening, --seconds)
             --buffer-ms MS      how much the output takes at once (default 10)
             --ahead N           blocks kept made for it (default: the buffer in blocks, and one more)

  --scene FILE on either: another scene than the bridge that comes with the program
               (scenes/bir-hakeim-recorded.json: the bridge with recorded engines and knocks).
  --assets DIR the folder the recordings a scene names are read from (default: assets).
";

struct Args(Vec<String>);

impl Args {
    fn value(&self, flag: &str) -> Option<&str> {
        self.0.iter().position(|a| a == flag).and_then(|i| self.0.get(i + 1)).map(String::as_str)
    }

    fn number(&self, flag: &str) -> Result<Option<f64>, String> {
        match self.value(flag) {
            Some(text) => text.parse().map(Some).map_err(|_| format!("{flag} {text}: not a number")),
            None => Ok(None),
        }
    }

    fn all(&self, flag: &str) -> Vec<String> {
        self.0.windows(2).filter(|pair| pair[0] == flag).map(|pair| pair[1].clone()).collect()
    }
}

/// A file's peaks between samples stay this far under full scale, dB: what
/// EBU R 128 asks, so that it can be played and encoded without clipping.
const CEILING_DB: f64 = -1.0;
/// A file starts and ends in silence: seconds of fade at each end, less in a file too short for them.
const FADE_IN: f64 = 0.4;
const FADE_OUT: f64 = 1.5;

/// The node of the scene's train, if it has one.
const TRAIN: [&str; 4] = ["bridge", "upper-deck", "track-to-passy", "train"];

fn scene(args: &Args) -> Result<Scene, String> {
    let mut scene = match args.value("--scene") {
        Some(path) => Scene::from_json(&std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?)?,
        None => paris_soundscape::bridge(),
    };
    if let Some(seed) = args.number("--seed")? {
        scene.seed = seed as u64;
    }
    // The recordings it names, from the assets' folder.
    if !scene.clips.is_empty() {
        scene.load_clips(std::path::Path::new(args.value("--assets").unwrap_or("assets")))?;
    }
    let train = scene.tree.find(&TRAIN);
    if let (Some(train), Some(at)) = (train, args.number("--metro-at")?) {
        // Level with the listener at that instant: where the listener then is, along the track.
        let head = scene.head(at).at;
        let parent = scene.tree.nodes[train].parent.map_or(Default::default(), |p| scene.tree.placement(p, at));
        let node = &mut scene.tree.nodes[train];
        if let Some(motion) = &mut node.motion {
            let along = motion.along;
            motion.at = at;
            let here = node.rest.translation;
            node.rest.translation = here + along * (parent.local(head) - here).dot(along);
        }
    }
    if let (Some(train), Some(kmh)) = (train, args.number("--metro-speed")?)
        && let Some(motion) = &mut scene.tree.nodes[train].motion
    {
        // The same run, faster or slower: every speed in proportion to the fastest.
        let fastest = motion.speed.iter().fold(0.0f64, |m, knot| m.max(knot.1));
        if fastest > 0.0 {
            for knot in &mut motion.speed {
                knot.1 *= kmh / 3.6 / fastest;
            }
        }
    }
    if let Some(decibels) = args.number("--metro-gain")? {
        let viaduct = scene.tree.find(&TRAIN[..2]);
        for emitter in &mut scene.emitters {
            if emitter.voice.starts_with("metro-") && viaduct.is_some_and(|v| scene.tree.under(emitter.node, v)) {
                emitter.gain *= 10f64.powf(decibels / 20.0);
            }
        }
    }
    Ok(scene)
}

fn options(args: &Args, scene: &Scene) -> Result<Options, String> {
    let quality = match args.value("--quality") {
        Some(name) => {
            Quality::named(name).ok_or_else(|| format!("--quality {name}: low, medium, high or reference"))?
        }
        None => Quality::REFERENCE,
    };
    let listening = match args.value("--listening") {
        None | Some("headphones") => Listening::Headphones,
        Some("speakers") => Listening::Speakers,
        Some(other) => return Err(format!("--listening {other}: headphones or speakers")),
    };
    Ok(Options {
        quality,
        listening,
        turn: args.number("--turn")?.unwrap_or(0.0).to_radians(),
        solo: args.all("--solo"),
        mute: args.all("--mute"),
        start: 0.0,
        far: far(args, scene)?,
        measured: measured(args)?,
    })
}

/// Measured head responses, if `--hrir FILE` names a file made by
/// `tools/make-hrir.py` (assets/sadie/d2-48k.hrir).
fn measured(args: &Args) -> Result<Option<std::sync::Arc<osage_sound::hrir::Hrir>>, String> {
    let Some(path) = args.value("--hrir") else { return Ok(None) };
    let bytes = std::fs::read(path).map_err(|e| format!("--hrir {path}: {e}"))?;
    let set = osage_sound::hrir::Hrir::from_bytes(&bytes).map_err(|e| format!("--hrir {path}: {e}"))?;
    if set.rate != osage_sound::engine::RATE {
        return Err(format!(
            "--hrir {path}: measured at {} Hz, the engine runs at {}",
            set.rate,
            osage_sound::engine::RATE
        ));
    }
    Ok(Some(std::sync::Arc::new(set)))
}

/// A scene of one noise 3 m away in `direction` (head frame: x right, y up, z ahead) and nothing else.
fn alone(direction: Vec3) -> Result<Scene, String> {
    let at = direction.unit_or(Vec3::Z) * 3.0;
    // The listener looks along its −Z: ahead is −z in the scene.
    let text = format!(
        r#"{{"asset": {{"version": "2.0", "extras": {{
          "osageObject": {{"version": 1, "id": "alone", "collisionMaterials": {{}}, "acousticMaterials": {{}}}},
          "osageSound": {{"version": 0, "contactSounds": {{}}}}}}}},
        "scene": 0,
        "scenes": [{{"nodes": [0], "extras": {{"osageSound": {{"seconds": 2, "seed": 2,
          "listener": {{"node": ["world", "ears"]}}, "reverb": {{"secondsT60": [0.3, 0.3, 0.3]}},
          "air": {{"celsius": 20, "humidityPercent": 50}}}}}}}}],
        "nodes": [
          {{"children": [1, 2], "extras": {{"osage": {{"id": "world"}}}}}},
          {{"extras": {{"osage": {{"id": "ears"}}}}}},
          {{"translation": [{}, {}, {}], "extras": {{"osage": {{"id": "source"}}, "osageSound": {{
            "emitters": [{{"id": "noise", "voice": "noise", "reflections": false}}]}}}}}}
        ]}}"#,
        at.x, at.y, -at.z
    );
    Scene::from_json(&text)
}

/// `ears`: what the measured responses make of a source round the head,
/// against the measured set itself and against the sphere model.
fn run_ears(args: &Args) -> Result<(), String> {
    use osage_sound::hrir::sofa_direction;
    use osage_sound_analysis::cues::{FINE_STRUCTURE, cue, cue_bands};
    let mut options = options(args, &alone(Vec3::Z)?)?;
    let Some(set) = options.measured.clone() else { return Err("--hrir FILE".to_owned()) };
    options.listening = Listening::Headphones;
    let model = Options { measured: None, ..options.clone() };
    // The measured set itself, for the truth: its raw responses, if given.
    let raw = match (args.value("--ir"), args.value("--positions")) {
        (Some(ir), Some(positions)) => {
            let read = |path: &str| -> Result<Vec<f64>, String> {
                let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
                Ok(bytes.as_chunks::<8>().0.iter().map(|b| f64::from_le_bytes(*b)).collect())
            };
            Some((read(ir)?, read(positions)?))
        }
        _ => None,
    };
    let taps = 256;
    let bands = cue_bands();
    let seconds = 1.0;
    // The noise every way is made from, for the truth.
    let mut noise = osage_sound_voices::dsp::Noise::new(5);
    let source: Vec<f32> = (0..(seconds * RATE as f64) as usize).map(|_| noise.next() * 0.1).collect();
    let mut directions: Vec<(String, f64, f64)> =
        (0..12).map(|i| (format!("round {:>3}°", i * 30), f64::from(i * 30), 0.0)).collect();
    for e in [-45.0, -20.0, 0.0, 20.0, 45.0, 70.0, 90.0] {
        directions.push((format!("ahead, up {e:>3}°"), 0.0, e));
    }
    for e in [70.0, 45.0, 20.0, 0.0, -20.0, -45.0] {
        directions.push((format!("behind, up {e:>3}°"), 180.0, e));
    }
    println!(
        "A noise 3 m away, heard by {}: the time between the ears (µs, the right leading) and the level",
        options.quality.name
    );
    println!(
        "between them (dB, the right louder) at 250 Hz, 1, 4 and 8 kHz — measured set / its virtual loudspeakers / sphere model."
    );
    let cues_of = |l: &[f32], r: &[f32]| {
        let itd = cue(l, r, RATE, &FINE_STRUCTURE, 0.2, seconds).itd_us;
        let ild: Vec<f64> =
            [1usize, 3, 5, 6].iter().map(|&b| cue(l, r, RATE, &bands[b], 0.2, seconds).ild_db).collect();
        let level: Vec<f64> = bands.iter().map(|b| cue(l, r, RATE, b, 0.2, seconds).level_db).collect();
        (itd, ild, level)
    };
    let (mut worst_itd, mut worst_ild) = (0.0f64, [0.0f64; 4]);
    let mut levels = Vec::new();
    for (name, azimuth, elevation) in &directions {
        let d = sofa_direction(*azimuth, *elevation);
        let scene = alone(d)?;
        let ours = render(&scene, &options, seconds);
        let theirs = render(&scene, &model, seconds);
        let (a, b) = (cues_of(&ours.left, &ours.right), cues_of(&theirs.left, &theirs.right));
        let truth = raw.as_ref().map(|(ir, positions)| {
            let count = positions.len() / 3;
            let m = (0..count)
                .max_by(|&i, &j| {
                    sofa_direction(positions[3 * i], positions[3 * i + 1])
                        .dot(d)
                        .total_cmp(&sofa_direction(positions[3 * j], positions[3 * j + 1]).dot(d))
                })
                .expect("a direction");
            let convolve = |ear: usize| -> Vec<f32> {
                let h = &ir[(2 * m + ear) * taps..(2 * m + ear + 1) * taps];
                (0..source.len())
                    .map(|n| (0..taps.min(n + 1)).map(|k| f64::from(source[n - k]) * h[k]).sum::<f64>() as f32)
                    .collect()
            };
            cues_of(&convolve(0), &convolve(1))
        });
        let show = |x: Option<f64>| x.map_or("    –".to_owned(), |x| format!("{x:>5.0}"));
        print!("  {name:<18} {} / {:>5.0} / {:>5.0} µs  ", show(truth.as_ref().map(|t| t.0)), a.0, b.0);
        for (k, worst) in worst_ild.iter_mut().enumerate() {
            let t = truth.as_ref().map(|t| t.1[k]);
            print!(" {}/{:>5.1}/{:>5.1}", t.map_or("   –".to_owned(), |t| format!("{t:>5.1}")), a.1[k], b.1[k]);
            if let Some(t) = t {
                *worst = worst.max((a.1[k] - t).abs());
            }
        }
        println!();
        if let Some(t) = &truth {
            worst_itd = worst_itd.max((a.0 - t.0).abs());
        }
        levels.push((name.clone(), a.2.clone(), b.2.clone(), truth.map(|t| t.2)));
    }
    if raw.is_some() {
        println!(
            "Against the measured set, the loudspeakers differ by at most {worst_itd:.0} µs in time and {:.1}, {:.1}, {:.1} and {:.1} dB in level.",
            worst_ild[0], worst_ild[1], worst_ild[2], worst_ild[3]
        );
    }
    // Ahead and behind: the same angle from the line through the ears, told apart by the spectrum.
    println!("Ahead against behind (the octaves' levels, both ears, behind minus ahead, dB, 125 Hz to 8 kHz):");
    for (front, back) in [(0usize, 6usize), (1, 5), (11, 7), (2, 4)] {
        let (f, b) = (&levels[front], &levels[back]);
        let row = |x: &[f64], y: &[f64]| {
            x.iter().zip(y).map(|(p, q)| format!("{:>5.1}", q - p)).collect::<Vec<_>>().join(" ")
        };
        println!("  {} → {}", f.0.trim(), b.0.trim());
        if let (Some(tf), Some(tb)) = (&f.3, &b.3) {
            println!("    measured set        {}", row(tf, tb));
        }
        println!("    virtual speakers    {}", row(&f.1, &b.1));
        println!("    sphere model        {}", row(&f.2, &b.2));
    }
    drop(set);
    Ok(())
}

/// `hrir`: a measured set's file for the engine, from the raw arrays
/// `tools/make-hrir.py` takes out of a SOFA file.
fn run_hrir(args: &Args) -> Result<(), String> {
    let floats = |flag: &str| -> Result<Vec<f64>, String> {
        let path = args.value(flag).ok_or_else(|| format!("{flag} FILE"))?;
        let bytes = std::fs::read(path).map_err(|e| format!("{flag} {path}: {e}"))?;
        Ok(bytes.as_chunks::<8>().0.iter().map(|b| f64::from_le_bytes(*b)).collect())
    };
    let (ir, positions) = (floats("--ir")?, floats("--positions")?);
    let number = |flag: &str, default: f64| -> Result<f64, String> { Ok(args.number(flag)?.unwrap_or(default)) };
    let set = osage_sound::hrir::derive(
        &ir,
        &positions,
        number("--taps", 256.0)? as usize,
        number("--rate", 48_000.0)? as u32,
        &osage_sound::hrir::rings(),
        number("--keep", 64.0)? as usize,
    )?;
    let out = args.value("--out").ok_or("--out FILE")?;
    std::fs::write(out, set.to_bytes()).map_err(|e| format!("{out}: {e}"))?;
    let most = set.itd.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    println!(
        "{out}: {} loudspeakers, {} triangles, {} taps; the time between the ears up to {:.0} µs",
        set.speakers.len(),
        set.triangles.len(),
        set.taps,
        most * 1e6
    );
    Ok(())
}

/// Far sound as a field, if `--far-beyond` asks: made at the centre of the
/// cube of `--cube` metres (27 by default) the listener stands in at the
/// start, of every emitter beyond that many metres from it, at
/// `--far-order` (1 or 2, 1 by default).
fn far(args: &Args, scene: &Scene) -> Result<Option<osage_sound::field::Far>, String> {
    let Some(beyond) = args.number("--far-beyond")? else { return Ok(None) };
    let order = args.number("--far-order")?.unwrap_or(1.0) as usize;
    if !(1..=2).contains(&order) {
        return Err("--far-order: 1 or 2".to_owned());
    }
    let side = args.number("--cube")?.unwrap_or(27.0);
    let at = scene.tree.placement(scene.listener, 0.0).point(scene.ears);
    // A cube of no size: the field made where the listener stands.
    let snap = |v: f64| if side > 0.0 { (v / side).floor() * side + side / 2.0 } else { v };
    let centre = Vec3 { x: snap(at.x), y: snap(at.y), z: snap(at.z) };
    Ok(Some(osage_sound::field::Far { centre, beyond, order }))
}

fn run_render(args: &Args) -> Result<(), String> {
    let scene = scene(args)?;
    let options = options(args, &scene)?;
    let seconds = args.number("--seconds")?.unwrap_or(scene.seconds);
    let out = PathBuf::from(args.value("--out").unwrap_or("bir-hakeim.wav"));
    let started = Instant::now();
    let mut rendered = render(&scene, &options, seconds);
    let took = started.elapsed().as_secs_f64();
    fade(&mut rendered, FADE_IN, FADE_OUT);
    // One gain for the whole file as it will be heard: the scene's loudness, or as near as its peaks allow.
    let (mut gain, mut loudness) = level(&rendered, scene.loudness, CEILING_DB);
    if let Some(decibels) = args.number("--gain-db")? {
        // A gain of one's own choosing: to compare one emitter with another, each rendered alone.
        loudness += decibels - 20.0 * gain.log10();
        gain = 10f64.powf(decibels / 20.0);
    }
    let mastered = master(&rendered, gain, scene.seed);
    let bytes = wav::pcm16(RATE, 2, &mastered.samples);
    if let Some(folder) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    }
    std::fs::write(&out, &bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("{}", out.display());
    println!(
        "  {seconds} s, {} quality, for {}, seed {}",
        options.quality.name,
        if options.listening == Listening::Headphones { "headphones" } else { "speakers" },
        scene.seed
    );
    println!(
        "  gain {:+.1} dB to {loudness:.1} LUFS (the scene asks {:.1}), samples held at the limit: {}",
        20.0 * gain.log10(),
        scene.loudness,
        mastered.held
    );
    println!(
        "  rendered in {took:.2} s ({:.0} times faster than it plays); {:.1} ways and {:.1} voices a block, {} ways at most, {} contacts",
        seconds / took,
        rendered.tally.ways as f64 / rendered.tally.blocks as f64,
        rendered.tally.voices as f64 / rendered.tally.blocks as f64,
        rendered.tally.most_ways,
        rendered.tally.contacts
    );
    let t = &rendered.tally;
    println!(
        "  of which: touching {:.2} s, finding the ways {:.2} s, voices {:.2} s, carrying {:.2} s, reverberation {:.2} s",
        t.touching, t.finding, t.voicing, t.carrying, t.tailing
    );
    println!("  sha256 {}", sha256(&bytes));
    Ok(())
}

fn run_tree(args: &Args) -> Result<(), String> {
    let scene = scene(args)?;
    let at = args.number("--at")?;
    let head = at.map(|t| scene.head(t));
    println!(
        "{} — {} nodes, {} emitters, {} surfaces",
        scene.id,
        scene.tree.nodes.len(),
        scene.emitters.len(),
        scene.surfaces.len()
    );
    if let (Some(t), Some(head)) = (at, &head) {
        println!("at {t} s the listener ({}) is at {}", scene.tree.path_text(scene.listener), place(head.at));
    }
    // Each node under its parent, as the tree has them.
    let mut order: Vec<usize> = Vec::new();
    let mut ahead: Vec<usize> =
        (0..scene.tree.nodes.len()).rev().filter(|&i| scene.tree.nodes[i].parent.is_none()).collect();
    while let Some(node) = ahead.pop() {
        order.push(node);
        ahead.extend(scene.tree.nodes[node].children.iter().rev());
    }
    for node in order {
        let depth = scene.tree.path(node).len() - 1;
        let n = &scene.tree.nodes[node];
        let mut line = format!("{}{}  at {}", "  ".repeat(depth), n.id, place(n.rest.translation));
        if let Some(motion) = &n.motion {
            line += &format!(
                "  moving along {} at {:.1} km/h",
                place(motion.along),
                motion.speed(at.unwrap_or(motion.at)) * 3.6
            );
        }
        if let Some(beyond) = n.proxy_beyond {
            line += &format!("  [stands for its branch beyond {beyond} m]");
        }
        println!("{line}");
        for emitter in scene.emitters.iter().filter(|e| e.node == node) {
            let mut line = format!(
                "{}♪ {} — {} at {} {:+.1} dB",
                "  ".repeat(depth + 1),
                emitter.id,
                emitter.voice,
                place(emitter.offset),
                20.0 * emitter.gain.log10()
            );
            match (emitter.reach, at, &head) {
                (Reach::Global { pan }, ..) => line += &format!("  [everywhere, pan {pan:+.1}]"),
                (Reach::Positional, Some(t), Some(head)) => {
                    let from = scene.tree.point(node, emitter.offset, t) - head.at;
                    let (x, y, z) = (from.dot(head.right), from.dot(head.up), from.dot(head.forward));
                    line += &format!(
                        "  → {:.1} m, {:+.0}° to the right, {:+.0}° up",
                        from.length(),
                        x.atan2(z).to_degrees(),
                        (y / from.length().max(1e-9)).asin().to_degrees()
                    );
                }
                _ => {}
            }
            println!("{line}");
        }
        for surface in scene.surfaces.iter().filter(|s| s.node == node) {
            println!(
                "{}▭ {}{}{}{}",
                "  ".repeat(depth + 1),
                surface.id,
                if surface.reflects { "  mirrors" } else { "" },
                if surface.occludes { "  hides" } else { "" },
                surface.collision.as_ref().map_or(String::new(), |key| format!("  underfoot: {key}"))
            );
        }
    }
    Ok(())
}

/// The bytes of the file a render makes, the loudness it was brought to (the scene's, or less if its peaks
/// would pass the ceiling) and how many samples had to be held.
fn file(scene: &Scene, options: &Options, seconds: f64) -> (Vec<u8>, f64, usize) {
    let mut rendered = render(scene, options, seconds);
    fade(&mut rendered, FADE_IN, FADE_OUT);
    let (gain, reached) = level(&rendered, scene.loudness, CEILING_DB);
    let mastered = master(&rendered, gain, scene.seed);
    (wav::pcm16(RATE, 2, &mastered.samples), reached, mastered.held)
}

/// A render of `seconds` cut into spans of `span` seconds, to its very end: when something is left over, one
/// span more ends where the render does, as long as the others and over the one before it — how a render
/// ends is read as finely as the rest of it. A render shorter than a span is one span.
fn spans(seconds: f64, span: f64) -> Vec<(f64, f64)> {
    let whole = (seconds / span) as usize;
    let mut spans: Vec<(f64, f64)> = (0..whole).map(|k| (k as f64 * span, (k + 1) as f64 * span)).collect();
    if seconds > whole as f64 * span {
        spans.push(((seconds - span).max(0.0), seconds));
    }
    spans
}

fn run_verify(args: &Args) -> Result<(), String> {
    use osage_sound_analysis::{Band, cue, envelope, measure};
    let scene = scene(args)?;
    let options = options(args, &scene)?;
    let seconds = args.number("--seconds")?.unwrap_or(scene.seconds);
    let mut failed = 0;
    let mut check = |name: &str, passed: bool, found: String| {
        println!("{} {name}: {found}", if passed { "ok  " } else { "FAIL" });
        failed += usize::from(!passed);
    };
    println!(
        "{} — {seconds} s, {} quality, for {}, seed {}",
        scene.id,
        options.quality.name,
        if options.listening == Listening::Headphones { "headphones" } else { "speakers" },
        scene.seed
    );

    // The same seed, the same file; another seed, another.
    let (first, reached, held) = file(&scene, &options, seconds);
    let (second, ..) = file(&scene, &options, seconds);
    let (other, ..) = file(&Scene { seed: scene.seed + 1, ..scene.clone() }, &options, seconds);
    check("the same seed gives the same bytes", first == second, format!("sha256 {} twice", &sha256(&first)[..16]));
    check(
        "another seed gives other bytes",
        first != other,
        format!("sha256 {} with seed {}", &sha256(&other)[..16], scene.seed + 1),
    );

    // Nothing clips: no sample held, none at the limits, and room between the samples.
    let report = measure(&wav::parse(&first).map_err(|e| e.to_string())?);
    let at_limits = report.left.clipping.samples + report.right.clipping.samples;
    check(
        "nothing clips",
        held == 0 && at_limits == 0 && report.true_peak_db <= CEILING_DB + 0.1,
        format!(
            "{held} samples held, {at_limits} at the limits, largest sample {:.2} dBFS, true peak {:.2} dBTP (ceiling {CEILING_DB})",
            report.left.sample_peak_db.max(report.right.sample_peak_db),
            report.true_peak_db
        ),
    );
    // As loud as the scene asks: the file as it is written, its ends faded. Quieter only where its peaks
    // would otherwise pass the ceiling — and then they are at the ceiling.
    let held_back = reached < scene.loudness - 0.05;
    match report.loudness_lkfs {
        Some(read) => check(
            "it is as loud as the scene asks, or as its peaks allow",
            (read - reached).abs() <= 0.3 && (!held_back || report.true_peak_db >= CEILING_DB - 0.3),
            format!(
                "{read:.1} LUFS for {:.1} asked{}",
                scene.loudness,
                if held_back {
                    format!(
                        ", held to {reached:.1} by its peaks ({:.2} dBTP for a ceiling of {CEILING_DB})",
                        report.true_peak_db
                    )
                } else {
                    String::new()
                }
            ),
        ),
        None => check(
            "it is as loud as the scene asks, or as its peaks allow",
            false,
            "no loudness to read: the file is silent, or shorter than the 0.4 s a loudness is read over".to_owned(),
        ),
    }

    // A sound on one side: the river. Heard alone, ahead and with the head turned half round.
    let wide = Band::new(500.0, 8_000.0);
    let alone = |text: &str, turn: f64| {
        render(&scene, &Options { solo: vec![text.to_owned()], turn, ..options.clone() }, seconds)
    };
    let river = alone("river", options.turn);
    let turned = alone("river", options.turn + std::f64::consts::PI);
    if river.peak() <= 0.0 {
        // What was to be checked is not there: that is not a check passed.
        check(
            "the river is heard, on one side",
            false,
            "no sound from any emitter named \"river\": its three checks could not be made".to_owned(),
        );
    } else {
        let (ahead, behind) = (
            cue(&river.left, &river.right, RATE, &wide, 0.0, seconds),
            cue(&turned.left, &turned.right, RATE, &wide, 0.0, seconds),
        );
        check(
            "turning the listener half round moves the river to the other side",
            ahead.ild_db < -1.5 && behind.ild_db > 1.5 && (ahead.ild_db + behind.ild_db).abs() < 0.5,
            format!("right minus left {:+.1} dB ahead, {:+.1} dB turned", ahead.ild_db, behind.ild_db),
        );
        // Sample for sample: what one ear heard, the other hears. With far sound as a field, the field
        // stays with the world and only its decoder turns with the head: the two are each other's mirror
        // to the rounding of the turned loudspeakers.
        if options.measured.is_some() && options.listening == Listening::Headphones && options.quality.outer_ear {
            // A measured head is not quite its own mirror: each ear hears what the other heard as loud,
            // octave by octave, within what the two sides of the head differ by.
            let octaves: Vec<Band> = [250.0, 500.0, 1_000.0, 2_000.0, 4_000.0].map(Band::octave).to_vec();
            let levels = |x: &[f32]| osage_sound_analysis::bands::band_levels(x, RATE, &octaves);
            let differ = |a: &[f32], b: &[f32]| {
                levels(a).iter().zip(levels(b)).map(|(p, q)| (p - q).abs()).fold(0.0f64, f64::max)
            };
            let worst = differ(&river.left, &turned.right).max(differ(&river.right, &turned.left));
            check(
                "and each ear then hears what the other heard",
                worst < 2.0,
                format!(
                    "octave by octave within {worst:.1} dB, 250 Hz to 4 kHz (a measured head is not its own mirror)"
                ),
            );
        } else if options.far.is_none() {
            check(
                "and each ear then hears what the other heard",
                river.left == turned.right && river.right == turned.left,
                "the two renders are each other's mirror, sample for sample".to_owned(),
            );
        } else {
            let off = |a: &[f32], b: &[f32]| {
                let (mut d, mut s) = (0.0f64, 0.0f64);
                for (x, y) in a.iter().zip(b) {
                    d += f64::from(x - y).powi(2);
                    s += f64::from(*x).powi(2);
                }
                10.0 * (d / s.max(1e-30)).log10()
            };
            let worst = off(&river.left, &turned.right).max(off(&river.right, &turned.left));
            check(
                "and each ear then hears what the other heard",
                worst < -60.0,
                format!("the two renders are each other's mirror within {worst:.0} dB (far sound as a field)"),
            );
        }
        // A second at a time, and the last second of the render whatever its length: a river that is on the
        // right for one of them has not stayed on the left. (What it does for less than a second is in its
        // second's mean.)
        let worst = spans(seconds, 1.0)
            .into_iter()
            .map(|(from, to)| (cue(&river.left, &river.right, RATE, &wide, from, to).ild_db, from, to))
            .fold((f64::NEG_INFINITY, 0.0, 0.0), |worst, span| if span.0 > worst.0 { span } else { worst });
        check(
            "the river stays on the left",
            worst.0 < -1.0,
            format!(
                "right minus left never above {:+.1} dB (from {} to {} s), a second at a time",
                worst.0, worst.1, worst.2
            ),
        );
    }

    // The train: heard alone, it comes, passes and goes, on the right.
    if let Some(train) = scene.tree.find(&TRAIN) {
        let abeam = scene.tree.nodes[train].motion.as_ref().map_or(0.0, |motion| motion.at);
        let heard = alone("upper-deck", options.turn);
        let both: Vec<f32> = heard.left.iter().zip(&heard.right).map(|(l, r)| (l + r) / 2.0).collect();
        match envelope(&both, RATE, 0.5).smoothed(2.0).passage().filter(|_| heard.peak() > 0.0) {
            Some(passage) => check(
                "the train comes, passes and goes",
                (passage.time - abeam).abs() < 5.0 && passage.rise_db > 10.0 && passage.fall_db > 8.0,
                format!(
                    "loudest at {:.1} s (level with the listener at {abeam:.1} s), {:.0} dB above its start, {:.0} dB above its end",
                    passage.time, passage.rise_db, passage.fall_db
                ),
            ),
            None => check(
                "the train comes, passes and goes",
                false,
                "no sound from any emitter under \"upper-deck\"".to_owned(),
            ),
        }
        if abeam > 3.0 && abeam + 3.0 < seconds && heard.peak() > 0.0 {
            let side = cue(&heard.left, &heard.right, RATE, &wide, abeam - 3.0, abeam + 3.0);
            check(
                "it passes on the right",
                side.ild_db > 1.0,
                format!("right minus left {:+.1} dB as it passes", side.ild_db),
            );
        } else {
            check(
                "it passes on the right",
                false,
                format!(
                    "not heard for 3 s each side of {abeam:.1} s, when it is level with the listener, in the {seconds} s rendered"
                ),
            );
        }
    } else {
        check(
            "the train comes, passes and goes",
            false,
            format!("the scene has no node {}: its two checks could not be made", TRAIN.join(" / ")),
        );
    }
    if failed > 0 {
        return Err(format!("{failed} checks failed"));
    }
    println!("all checks passed");
    Ok(())
}

/// The scene played live, its output taking `--buffer-ms` at a time on the
/// clock and its listener walked at 1.4 m/s by poses told sixty times a
/// second, as a game would: what the output found missing, and how late a
/// pose was heard.
fn run_live(args: &Args) -> Result<(), String> {
    use osage_sound::{
        engine::{BLOCK, Pose},
        live::Live,
        space::Placement,
    };
    let scene = scene(args)?;
    let options = options(args, &scene)?;
    let seconds = args.number("--seconds")?.unwrap_or(scene.seconds);
    let buffer_ms = args.number("--buffer-ms")?.unwrap_or(10.0).max(1.0);
    let frames = (buffer_ms / 1000.0 * RATE as f64).round() as usize;
    let ahead = match args.number("--ahead")? {
        Some(n) => n as usize,
        None => frames.div_ceil(BLOCK) + 1,
    };
    let listener = scene.listener;
    let rest = scene.tree.nodes[listener].rest;
    let walk = Vec3::X * 1.4;
    println!(
        "{} live — {seconds} s, {} quality, for {}; the output takes {frames} frames ({buffer_ms} ms) at once, {ahead} blocks kept ahead",
        scene.id,
        options.quality.name,
        if options.listening == Listening::Headphones { "headphones" } else { "speakers" },
    );
    let live = Live::start(scene, options, &[], ahead);
    let mut heard = live.feed().listen();
    let period = std::time::Duration::from_secs_f64(frames as f64 / RATE as f64);
    let start = Instant::now();
    let cpu = osage_sound::bench::cpu_seconds();
    let (mut told, mut taken) = (0u64, 0usize);
    let total = (seconds * RATE as f64) as usize;
    while taken < total {
        // The game: a pose every sixtieth of a second, on the clock.
        let now = start.elapsed().as_secs_f64();
        while (told as f64) / 60.0 <= now {
            let at = told as f64 / 60.0;
            let placement = Placement { translation: rest.translation + walk * at, ..rest };
            live.drive(listener, Pose { placement, velocity: walk });
            told += 1;
        }
        // The output: a buffer's worth when its time comes.
        let due = start + period * (taken / frames) as u32;
        if Instant::now() < due {
            std::thread::sleep((due - Instant::now()).min(std::time::Duration::from_micros(500)));
            continue;
        }
        for _ in 0..2 * frames {
            heard.sample();
        }
        taken += frames;
    }
    let report = live.report();
    let block_ms = BLOCK as f64 / RATE as f64 * 1000.0;
    println!(
        "  {} blocks heard, {} frames missing ({:.1} ms of silence){}",
        report.blocks,
        report.underruns,
        report.underruns as f64 / RATE as f64 * 1000.0,
        if report.underruns == 0 { ": the output never waited" } else { "" }
    );
    println!(
        "  a pose heard {:.1} ms after it was told on average, {:.1} ms at most ({} poses; a block is {block_ms:.1} ms), plus the output's own buffer: {:.1} ms to the ears on average",
        report.latency_mean * 1000.0,
        report.latency_max * 1000.0,
        report.latency_count,
        report.latency_mean * 1000.0 + buffer_ms,
    );
    println!(
        "  this thread (the output and the game) used {:.0} ms of processor; the engine's thread is not counted",
        (osage_sound::bench::cpu_seconds() - cpu) * 1000.0
    );
    if report.underruns > 0 {
        return Err(format!("the output found {} frames missing", report.underruns));
    }
    Ok(())
}

fn run_bench(args: &Args) -> Result<(), String> {
    use osage_sound::bench::{cost, peak_memory, ring, unbounded};
    use osage_sound::engine::way_cost;
    let quick = args.0.iter().any(|a| a == "--quick");
    let (runs, ring_seconds) = if quick { (1, 4.0) } else { (3, 10.0) };
    let both = [("headphones", Listening::Headphones), ("speakers", Listening::Speakers)];
    let mut receipt = vec![format!("\"machine\": {:?}", machine()), format!("\"runs\": {runs}")];
    let part = |name: &str| args.value("--part").is_none_or(|only| only == name);

    let scene = scene(args)?;
    let measured = measured(args)?;
    if part("bridge") {
        println!(
            "{} — {} s, {} emitters. Milliseconds of one processor for a second of sound:",
            scene.id,
            scene.seconds,
            scene.emitters.len()
        );
        println!(
            "  {:<10} {:<10} {:>8} {:>7} {:>7}   finding ways / voices / carrying",
            "quality", "for", "ms", "ways", "voices"
        );
        let mut rows = Vec::new();
        for quality in Quality::ALL {
            for (name, listening) in both {
                let found = cost(
                    &scene,
                    &Options { quality, listening, measured: measured.clone(), ..Options::default() },
                    if quick { 15.0 } else { scene.seconds },
                    runs,
                );
                println!(
                    "  {:<10} {:<10} {:>8.1} {:>7.1} {:>7.1}   {:.0} % / {:.0} % / {:.0} %",
                    quality.name,
                    name,
                    found.ms_per_second,
                    found.ways,
                    found.voices,
                    100.0 * found.shares[1],
                    100.0 * found.shares[2],
                    100.0 * found.shares[3]
                );
                rows.push(format!(
                    "{{\"quality\": \"{}\", \"listening\": \"{name}\", \"ms_per_second\": {:.3}, \"ways\": {:.2}, \"voices\": {:.2}}}",
                    quality.name, found.ms_per_second, found.ways, found.voices
                ));
                if quality.name == "reference" && listening == Listening::Headphones {
                    println!(
                        "  it keeps {:.1} MB of sound on its way (one line for each emitter, as long as its sound can take to arrive)",
                        found.ring_bytes as f64 / 1e6
                    );
                    receipt.push(format!("\"bridge_line_bytes\": {}", found.ring_bytes));
                }
            }
        }
        receipt.push(format!("\"bridge\": [{}]", rows.join(", ")));
    }

    if part("faces") {
        use osage_sound::bench::cut;
        println!(
            "\n{} with its faces cut into triangles, headphones. Milliseconds of one processor for a second of sound:",
            scene.id
        );
        let times = [1usize, 10, 100, 1000];
        print!("  {:<10}", "faces");
        let counts: Vec<usize> =
            times.iter().map(|&n| if n == 1 { scene.surfaces.len() } else { scene.surfaces.len() * n }).collect();
        counts.iter().for_each(|n| print!(" {n:>9}"));
        println!();
        let mut rows = Vec::new();
        for quality in Quality::ALL {
            print!("  {:<10}", quality.name);
            for &n in &times {
                let cut_up = if n == 1 { scene.clone() } else { cut(&scene, n / 2) };
                let options = Options { quality, listening: Listening::Headphones, ..Options::default() };
                let found = cost(&cut_up, &options, if quick { 10.0 } else { 20.0 }, runs);
                print!(" {:>9.1}", found.ms_per_second);
                rows.push(format!(
                    "{{\"quality\": \"{}\", \"faces\": {}, \"ms_per_second\": {:.3}}}",
                    quality.name,
                    cut_up.surfaces.len(),
                    found.ms_per_second
                ));
            }
            println!();
        }
        receipt.push(format!("\"faces\": [{}]", rows.join(", ")));
    }

    if part("ring") {
        let counts = [1usize, 8, 32, 64, 128, 256];
        println!(
            "\nA ring of noises turning round the listener at 10 m/s, {ring_seconds} s, every way rendered. Milliseconds of one"
        );
        println!("processor for a second of sound (and nanoseconds for one source and one sample):");
        print!("  {:<22}", "sources");
        counts.iter().for_each(|n| print!(" {n:>13}"));
        println!();
        let mut rows = Vec::new();
        let cases: [(&str, Quality, Listening, bool); 6] = [
            ("low, speakers", Quality::LOW, Listening::Speakers, false),
            ("low, headphones", Quality::LOW, Listening::Headphones, false),
            ("medium, headphones", Quality::MEDIUM, Listening::Headphones, false),
            ("high, headphones", Quality::HIGH, Listening::Headphones, false),
            ("reference, headphones", Quality::REFERENCE, Listening::Headphones, false),
            ("high, over a road", Quality::HIGH, Listening::Headphones, true),
        ];
        for (name, quality, listening, floor) in cases {
            print!("  {name:<22}");
            let mut row = Vec::new();
            for n in counts {
                let options = Options { quality: unbounded(quality), listening, ..Options::default() };
                let found = cost(&ring(n, floor), &options, ring_seconds, runs);
                let ns = found.ms_per_second * 1e6 / (RATE as f64 * n as f64);
                print!(" {:>7.2} ({ns:>3.0})", found.ms_per_second);
                row.push(format!(
                    "{{\"sources\": {n}, \"ms_per_second\": {:.4}, \"ns_per_source_sample\": {ns:.2}, \"ways\": {:.1}}}",
                    found.ms_per_second, found.ways
                ));
            }
            println!();
            rows.push(format!("{{\"case\": \"{name}\", \"runs\": [{}]}}", row.join(", ")));
        }
        receipt.push(format!("\"ring\": [{}]", rows.join(", ")));
    }

    if part("way") {
        println!("\nOne way alone, nanoseconds a sample (no voice, no finding of ways):");
        let mut rows = Vec::new();
        for quality in Quality::ALL {
            for (name, listening) in both {
                let ns = (0..runs)
                    .map(|_| way_cost(quality, listening, if quick { 5.0 } else { 20.0 }))
                    .fold(f64::MAX, f64::min);
                println!("  {:<10} {:<10} {ns:>6.1}", quality.name, name);
                rows.push(format!("{{\"quality\": \"{}\", \"listening\": \"{name}\", \"ns\": {ns:.2}}}", quality.name));
            }
        }
        receipt.push(format!("\"way\": [{}]", rows.join(", ")));
    }
    if let Some(bytes) = peak_memory() {
        println!("\nThe most memory this program held at once: {:.1} MB", bytes as f64 / 1e6);
        receipt.push(format!("\"peak_memory_bytes\": {bytes}"));
    }
    if let Some(path) = args.value("--json") {
        std::fs::write(path, format!("{{\n  {}\n}}\n", receipt.join(",\n  "))).map_err(|e| format!("{path}: {e}"))?;
        println!("{path}");
    }
    Ok(())
}

/// The processor this runs on, as the system names it.
fn machine() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("model name"))
                .and_then(|line| line.split(": ").nth(1))
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

/// What two renders of the same length differ in, as instruments read them.
struct Differences {
    loudness: [Option<f64>; 2],
    /// Octave centre, Hz, and B − A in dB.
    octaves: Vec<(f64, f64)>,
    /// For each band of the ears' cues: its centre, Hz; the mean of
    /// |ΔILD|, dB, and of |ΔITD|, µs; over how many seconds.
    cues: Vec<(f64, f64, f64, usize, usize)>,
}

fn differences(a: [&[f32]; 2], b: [&[f32]; 2], rate: u32) -> Differences {
    use osage_sound_analysis::{band_levels, cue, cue_bands, loudness, octaves};
    let seconds = a[0].len().min(b[0].len()) as f64 / f64::from(rate);
    let mono = |x: [&[f32]; 2]| x[0].iter().zip(x[1]).map(|(l, r)| (l + r) / 2.0).collect::<Vec<f32>>();
    let (ma, mb) = (mono(a), mono(b));
    let bands = octaves();
    let (la, lb) = (band_levels(&ma, rate, &bands), band_levels(&mb, rate, &bands));
    let octaves =
        bands.iter().zip(la.iter().zip(&lb)).map(|(band, (x, y))| ((band.low * band.high).sqrt(), y - x)).collect();
    let spans: Vec<(f64, f64)> = (0..seconds.floor() as usize).map(|s| (s as f64, s as f64 + 1.0)).collect();
    let cues = cue_bands()
        .iter()
        .map(|band| {
            let pairs: Vec<_> = spans
                .iter()
                .map(|&(from, to)| (cue(a[0], a[1], rate, band, from, to), cue(b[0], b[1], rate, band, from, to)))
                .collect();
            // Only the seconds where there is something in this band to hear.
            let loudest = pairs.iter().map(|(x, _)| x.level_db).fold(f64::NEG_INFINITY, f64::max);
            let heard: Vec<_> = pairs.iter().filter(|(x, _)| x.level_db > loudest - 30.0).collect();
            let ild = heard.iter().map(|(x, y)| (x.ild_db - y.ild_db).abs()).sum::<f64>() / heard.len().max(1) as f64;
            let timed: Vec<_> = heard.iter().filter(|(x, y)| x.coherence > 0.3 && y.coherence > 0.3).collect();
            let itd = timed.iter().map(|(x, y)| (x.itd_us - y.itd_us).abs()).sum::<f64>() / timed.len().max(1) as f64;
            ((band.low * band.high).sqrt(), ild, itd, heard.len(), timed.len())
        })
        .collect();
    Differences { loudness: [loudness(&a, rate), loudness(&b, rate)], octaves, cues }
}

fn print_differences(d: &Differences) {
    let lufs = |x: Option<f64>| x.map_or("silent".to_owned(), |v| format!("{v:.2} LUFS"));
    let delta = match d.loudness {
        [Some(a), Some(b)] => format!("{:+.2} LU", b - a),
        _ => "-".to_owned(),
    };
    println!("    loudness {} against {}: {delta}", lufs(d.loudness[1]), lufs(d.loudness[0]));
    let octaves: Vec<String> = d.octaves.iter().map(|(hz, db)| format!("{}:{db:+.1}", hz.round())).collect();
    println!("    octaves, dB: {}", octaves.join(" "));
    for (hz, ild, itd, heard, timed) in &d.cues {
        println!(
            "    {:>5} Hz: level between the ears off by {ild:.2} dB ({heard} s), time by {itd:.0} µs ({timed} s coherent)",
            hz.round()
        );
    }
}

fn run_compare(args: &Args) -> Result<(), String> {
    let files: Vec<&String> = args.0.iter().filter(|a| !a.starts_with("--")).collect();
    let [a, b] = files[..] else { return Err("compare two files: compare A.wav B.wav".to_owned()) };
    let (x, y) = (wav::read(a).map_err(|e| format!("{a}: {e}"))?, wav::read(b).map_err(|e| format!("{b}: {e}"))?);
    if x.rate != y.rate {
        return Err(format!("{a} is at {} Hz, {b} at {}", x.rate, y.rate));
    }
    let two = |audio: &osage_sound_analysis::Audio| -> [Vec<f32>; 2] {
        let left = audio.channels.first().cloned().unwrap_or_default();
        let right = audio.channels.get(1).cloned().unwrap_or_else(|| left.clone());
        [left, right]
    };
    let (x, y, rate) = (two(&x), two(&y), x.rate);
    let d = differences([&x[0], &x[1]], [&y[0], &y[1]], rate);
    if args.0.iter().any(|a| a == "--json") {
        let octaves: Vec<String> = d.octaves.iter().map(|(hz, db)| format!("[{hz:.0}, {db:.3}]")).collect();
        let cues: Vec<String> =
            d.cues.iter().map(|(hz, ild, itd, ..)| format!("[{hz:.0}, {ild:.3}, {itd:.1}]")).collect();
        let lufs = |v: Option<f64>| v.map_or("null".to_owned(), |v| format!("{v:.3}"));
        println!(
            "{{\"loudness\": [{}, {}], \"octaves_db\": [{}], \"cues_ild_db_itd_us\": [{}]}}",
            lufs(d.loudness[0]),
            lufs(d.loudness[1]),
            octaves.join(", "),
            cues.join(", ")
        );
    } else {
        println!("{b} against {a}");
        print_differences(&d);
    }
    Ok(())
}

/// The scene of `--sphere N` (and `--radius`), or the one asked.
fn scene_or_sphere(args: &Args) -> Result<(Scene, bool), String> {
    match args.number("--sphere")? {
        Some(n) => Ok((osage_sound::bench::sphere(n as usize, args.number("--radius")?.unwrap_or(150.0)), true)),
        None => Ok((scene(args)?, false)),
    }
}

fn run_field(args: &Args) -> Result<(), String> {
    use osage_sound::{
        engine::{BLOCK, Engine},
        field::{Far, MOST, channels},
    };
    let (scene, sphere) = scene_or_sphere(args)?;
    let options = options(args, &scene)?;
    let seconds = args.number("--seconds")?.unwrap_or(10.0);
    let far = match options.far {
        Some(far) => far,
        None if sphere => Far {
            centre: scene.tree.placement(scene.listener, 0.0).point(scene.ears),
            beyond: args.number("--radius")?.unwrap_or(150.0) / 2.0,
            order: args.number("--far-order")?.unwrap_or(1.0) as usize,
        },
        None => return Err("field: --far-beyond M, where the far part starts".to_owned()),
    };
    let order = far.order.clamp(1, 2);
    let count = channels(order);
    let gain = 10f64.powf(args.number("--gain-db")?.unwrap_or(0.0) / 20.0);
    let mut engine = Engine::field(scene, Options { far: Some(far), start: -osage_sound::render::LEAD, ..options });
    let mut block = [[0.0f32; BLOCK]; MOST];
    for _ in 0..(osage_sound::render::LEAD * RATE as f64 / BLOCK as f64).round() as usize {
        engine.render_field(&mut block);
    }
    let frames = (seconds * RATE as f64) as usize;
    let mut samples = Vec::with_capacity(frames * count);
    let (mut made, mut held) = (0, 0usize);
    while made < frames {
        engine.render_field(&mut block);
        for n in 0..BLOCK.min(frames - made) {
            for channel in block.iter().take(count) {
                let value = (f64::from(channel[n]) * gain * 32_767.0).round();
                held += usize::from(value.abs() > 32_767.0);
                samples.push(value.clamp(-32_768.0, 32_767.0) as i16);
            }
        }
        made += BLOCK;
    }
    let out = args.value("--out").unwrap_or("field.wav");
    std::fs::write(out, wav::pcm16(RATE, count as u16, &samples)).map_err(|e| format!("{out}: {e}"))?;
    println!("{out}: {count} channels (order {order}), {seconds} s, {held} samples held at the limits");
    Ok(())
}

fn run_decode(args: &Args) -> Result<(), String> {
    use osage_sound::{
        engine::{BLOCK, FieldEars},
        field::MOST,
        scene::Head,
    };
    let Some(path) = args.0.iter().find(|a| !a.starts_with("--") && a.ends_with(".wav")) else {
        return Err("decode which field?".to_owned());
    };
    let audio = wav::read(path).map_err(|e| format!("{path}: {e}"))?;
    let order = match audio.channels.len() {
        4 => 1,
        9 => 2,
        other => return Err(format!("{path}: {other} channels; a field has 4 or 9")),
    };
    let quality = match args.value("--quality") {
        Some(name) => Quality::named(name).ok_or_else(|| format!("--quality {name}"))?,
        None => Quality::MEDIUM,
    };
    let listening =
        if args.value("--listening") == Some("speakers") { Listening::Speakers } else { Listening::Headphones };
    let gain = 10f64.powf(args.number("--gain-db")?.unwrap_or(0.0) / 20.0);
    let mut ears = FieldEars::new(order, quality, listening);
    let head = Head { at: Vec3::ZERO, forward: Vec3 { x: 0.0, y: 0.0, z: -1.0 }, right: Vec3::X, up: Vec3::Y };
    let frames = audio.frames();
    let mut samples = Vec::with_capacity(frames * 2);
    let mut at = 0;
    while at < frames {
        let mut field = [[0.0f32; BLOCK]; MOST];
        let count = BLOCK.min(frames - at);
        for (c, channel) in audio.channels.iter().enumerate() {
            field[c][..count].copy_from_slice(&channel[at..at + count]);
        }
        let (mut l, mut r) = ([0.0f32; BLOCK], [0.0f32; BLOCK]);
        ears.hear(&field, &head, &mut l, &mut r);
        for n in 0..BLOCK.min(frames - at) {
            for v in [l[n], r[n]] {
                samples.push((f64::from(v) * gain * 32_767.0).round().clamp(-32_768.0, 32_767.0) as i16);
            }
        }
        at += BLOCK;
    }
    let out = args.value("--out").unwrap_or("decoded.wav");
    std::fs::write(out, wav::pcm16(RATE, 2, &samples)).map_err(|e| format!("{out}: {e}"))?;
    println!(
        "{out}: order {order} heard for {}",
        if listening == Listening::Headphones { "headphones" } else { "speakers" }
    );
    Ok(())
}

/// Far sound as a field against every emitter by its own ways.
fn run_far(args: &Args) -> Result<(), String> {
    use osage_sound::{bench, field::Far};
    let sphere = args.number("--sphere")?.map(|n| n as usize);
    let radius = args.number("--radius")?.unwrap_or(150.0);
    let scene = match sphere {
        Some(n) => bench::sphere(n, radius),
        None => scene(args)?,
    };
    let mut options = options(args, &scene)?;
    if args.value("--quality").is_none() {
        options.quality = Quality::MEDIUM;
    }
    let seconds = args.number("--seconds")?.unwrap_or(10.0);
    let beyond = args.number("--far-beyond")?.unwrap_or(if sphere.is_some() { radius / 2.0 } else { 60.0 });
    let side = args.number("--cube")?.unwrap_or(if sphere.is_some() { 0.0 } else { 27.0 });
    let at = scene.tree.placement(scene.listener, 0.0).point(scene.ears);
    let snap = |v: f64| if side > 0.0 { (v / side).floor() * side + side / 2.0 } else { v };
    let centre = Vec3 { x: snap(at.x), y: snap(at.y), z: snap(at.z) };
    println!(
        "{} — {seconds} s at {} quality for {}; far beyond {beyond} m of {}",
        scene.id,
        options.quality.name,
        if options.listening == Listening::Headphones { "headphones" } else { "speakers" },
        place(centre)
    );
    let whole = render(&scene, &Options { far: None, ..options.clone() }, seconds);
    for order in [1, 2] {
        let far = Far { centre, beyond, order };
        let heard = render(&scene, &Options { far: Some(far), ..options.clone() }, seconds);
        println!(
            "  a field of order {order} ({} channels), against every emitter by its own ways:",
            (order + 1) * (order + 1)
        );
        print_differences(&differences([&whole.left, &whole.right], [&heard.left, &heard.right], RATE));
        let (ways, making, hearing) =
            osage_sound::bench::field_cost(&scene, &Options { far: Some(far), ..options.clone() }, seconds.min(4.0));
        println!(
            "    cost, ms a second: by ways {ways:.0} for each listener; the field {making:.0} once for the cube, {hearing:.1} for each listener to hear it"
        );
        for listeners in [1.0, 4.0, 16.0] {
            println!(
                "      {listeners:>2} listeners: by ways {:.0}, by a field {:.0}",
                ways * listeners,
                making + hearing * listeners
            );
        }
    }
    Ok(())
}

fn run_measure(args: &Args) -> Result<(), String> {
    let Some(path) = args.0.iter().find(|a| !a.starts_with("--")) else { return Err("measure which file?".to_owned()) };
    let audio = wav::read(path).map_err(|e| format!("{path}: {e}"))?;
    let report = osage_sound_analysis::measure(&audio);
    if args.0.iter().any(|a| a == "--json") {
        println!("{}", report.to_json());
    } else {
        println!("{path}");
        print!("{report}");
    }
    Ok(())
}

fn place(p: Vec3) -> String {
    format!("[{:.2}, {:.2}, {:.2}]", p.x, p.y, p.z)
}

fn main() -> ExitCode {
    let mut all: Vec<String> = std::env::args().skip(1).collect();
    let command = if all.is_empty() { String::new() } else { all.remove(0) };
    let args = Args(all);
    let done = match command.as_str() {
        "render" => run_render(&args),
        "tree" => run_tree(&args),
        "measure" => run_measure(&args),
        "against" => {
            measured(&args).and_then(|set| against::run(args.value("--reference").map(std::path::Path::new), set))
        }
        "verify" => run_verify(&args),
        "bench" => run_bench(&args),
        "hrir" => run_hrir(&args),
        "ears" => run_ears(&args),
        "live" => run_live(&args),
        "far" => run_far(&args),
        "field" => run_field(&args),
        "decode" => run_decode(&args),
        "compare" => run_compare(&args),
        "lod" => lod::run(std::path::Path::new(args.value("--assets").unwrap_or("assets"))),
        _ => {
            print!("{HELP}");
            return if command.is_empty() || command == "help" || command == "--help" {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            };
        }
    };
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("paris-soundscape: {why}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::spans;

    #[test]
    fn a_render_is_checked_to_its_very_end() {
        assert_eq!(spans(45.0, 1.0).len(), 45);
        assert_eq!(spans(45.0, 1.0)[44], (44.0, 45.0));
        // What is left over is looked at in a whole second of its own: the render's last.
        assert_eq!(spans(45.5, 1.0)[45], (44.5, 45.5));
        assert_eq!(spans(42.75, 1.0)[42], (41.75, 42.75));
        assert_eq!(spans(1.0, 1.0), vec![(0.0, 1.0)]);
        assert_eq!(spans(0.4, 1.0), vec![(0.0, 0.4)]);
        assert_eq!(spans(42.0, 5.0).last(), Some(&(37.0, 42.0)));
        // Every instant is in a span, and every span is as long as the others — but for a render shorter
        // than one.
        for seconds in [0.4, 1.0, 1.25, 1.5, 4.99, 5.0, 5.5, 42.0, 45.0, 45.4, 45.5, 47.0] {
            for span in [1.0, 5.0] {
                let cut = spans(seconds, span);
                assert_eq!((cut[0].0, cut[cut.len() - 1].1), (0.0, seconds));
                assert!(cut.windows(2).all(|pair| pair[1].0 <= pair[0].1));
                assert!(cut.iter().all(|(from, to)| (to - from - span.min(seconds)).abs() < 1e-9), "{cut:?}");
            }
        }
    }
}
