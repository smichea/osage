// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The soundscape of the Pont de Bir-Hakeim, rendered offline:
//!
//!     paris-soundscape render --out bir-hakeim.wav
//!     paris-soundscape tree --at 25
//!
//! See the README beside this crate.

use std::{path::PathBuf, process::ExitCode, time::Instant};

use osage_paris_soundscape::{
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
             --part NAME         only the bridge, the ring or the way
  measure FILE.wav
           what any stereo file tells of where its sounds are: peaks, loudness, octaves, and second
           by second the level and time differences between the ears (--json for a receipt)

  --scene FILE on either: another scene than the bridge that comes with the program.
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
        None => Scene::bundled(),
    };
    if let Some(seed) = args.number("--seed")? {
        scene.seed = seed as u64;
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

fn options(args: &Args) -> Result<Options, String> {
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
    })
}

fn run_render(args: &Args) -> Result<(), String> {
    let scene = scene(args)?;
    let options = options(args)?;
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
    let options = options(args)?;
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
        // Sample for sample: what one ear heard, the other hears.
        check(
            "and each ear then hears what the other heard",
            river.left == turned.right && river.right == turned.left,
            "the two renders are each other's mirror, sample for sample".to_owned(),
        );
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

fn run_bench(args: &Args) -> Result<(), String> {
    use osage_paris_soundscape::bench::{cost, peak_memory, ring, unbounded};
    use osage_paris_soundscape::engine::way_cost;
    let quick = args.0.iter().any(|a| a == "--quick");
    let (runs, ring_seconds) = if quick { (1, 4.0) } else { (3, 10.0) };
    let both = [("headphones", Listening::Headphones), ("speakers", Listening::Speakers)];
    let mut receipt = vec![format!("\"machine\": {:?}", machine()), format!("\"runs\": {runs}")];
    let part = |name: &str| args.value("--part").is_none_or(|only| only == name);

    let scene = scene(args)?;
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
                    &Options { quality, listening, ..Options::default() },
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
        "verify" => run_verify(&args),
        "bench" => run_bench(&args),
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
