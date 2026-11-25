// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! `against`: the controlled scenes (`scenes/controlled-*.json`), one sound and one question each,
//! rendered by the engine and read beside the same geometries captured from another engine, figure for
//! figure (issue #19). The other engine's captures are files in a folder given by `--reference`, named
//! after the scenes (`circle.wav`, `passby.wav`, `far.wav`, `click-near.wav`, `click-far.wav`,
//! `screen.wav`, `screen-absent.wav`, `floor.wav`, `floor-absent.wav`); a file that is not there leaves
//! its column empty. Every figure is read alike from every take, by `osage-sound-analysis`.

use std::{path::Path, sync::Arc};

use osage_sound::{
    ears::Listening,
    engine::{Options, Quality, RATE},
    hrir::Hrir,
    render::render,
    scene::Scene,
};
use osage_sound_analysis::{Band, band_levels, cue, db, gcc_phat_between, tone_track, wav};

/// Where the scenes are, beside this crate.
const SCENES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/scenes");
/// Speed of sound the scenes are made for, m/s (20 °C).
const SOUND: f64 = 343.0;
/// A channel this far under the take's loudest second is silent, dB.
const SILENT_DB: f64 = 100.0;

/// Two channels and their rate.
pub struct Take {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    pub rate: u32,
}

impl Take {
    fn read(path: &Path) -> Result<Take, String> {
        let audio = wav::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let left = audio.channels.first().cloned().unwrap_or_default();
        let right = audio.channels.get(1).cloned().unwrap_or_else(|| left.clone());
        Ok(Take { left, right, rate: audio.rate })
    }

    fn span(&self, from: f64, to: f64) -> (&[f32], &[f32]) {
        let at = |t: f64| ((t * f64::from(self.rate)) as usize).min(self.left.len());
        (&self.left[at(from)..at(to)], &self.right[at(from)..at(to)])
    }

    /// Mean power of both channels between two instants, dB.
    fn level(&self, from: f64, to: f64) -> [f64; 2] {
        let (l, r) = self.span(from, to);
        let power = |x: &[f32]| x.iter().map(|&v| f64::from(v) * f64::from(v)).sum::<f64>() / x.len().max(1) as f64;
        [db(power(l)), db(power(r))]
    }

    /// The first sample after `from`, s, whose size reaches `under` dB below the peak from there on.
    fn onset(&self, from: f64, under: f64) -> Option<f64> {
        let first = ((from * f64::from(self.rate)) as usize).min(self.left.len());
        let size = |i: usize| self.left[i].abs().max(self.right[i].abs());
        let peak = (first..self.left.len()).map(size).fold(0f32, f32::max);
        let floor = peak * 10f32.powf(-(under as f32) / 20.0);
        (first..self.left.len()).find(|&i| peak > 0.0 && size(i) >= floor).map(|i| i as f64 / f64::from(self.rate))
    }

    /// Both channels' levels in `bands` between two instants, the channels' powers averaged, dB.
    fn bands(&self, bands: &[Band], from: f64, to: f64) -> Vec<f64> {
        let (l, r) = self.span(from, to);
        let (a, b) = (band_levels(l, self.rate, bands), band_levels(r, self.rate, bands));
        a.iter().zip(&b).map(|(x, y)| db((10f64.powf(x / 10.0) + 10f64.powf(y / 10.0)) / 2.0)).collect()
    }
}

/// The engines' takes of one scene: the engine's own, and the other's if it was captured.
struct Takes {
    osage: Vec<(&'static str, Take)>,
    other: Option<Take>,
}

/// The engine's ways of hearing compared: headphones with the sphere model, with measured responses when
/// there are any, and loudspeakers.
fn hearings(measured: &Option<Arc<Hrir>>) -> Vec<(&'static str, Options)> {
    let with = |listening, measured: Option<Arc<Hrir>>| Options {
        quality: Quality::REFERENCE,
        listening,
        measured,
        ..Options::default()
    };
    let mut all = vec![("osage, headphones", with(Listening::Headphones, None))];
    if let Some(set) = measured {
        all.push(("osage, measured ears", with(Listening::Headphones, Some(set.clone()))));
    }
    all.push(("osage, loudspeakers", with(Listening::Speakers, None)));
    all
}

/// A controlled scene, with `edit` applied to its JSON first.
fn scene(name: &str, edit: impl FnOnce(&mut serde_json::Value)) -> Result<Scene, String> {
    let path = format!("{SCENES}/controlled-{name}.json");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let mut json: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
    edit(&mut json);
    Scene::from_json(&json.to_string())
}

/// The scene as the engine hears it, raw: no fade, no gain, so that two scenes' levels can be set side by side.
fn takes(scene: &Scene, hearings: &[(&'static str, Options)], other: Option<Take>) -> Takes {
    let osage = hearings
        .iter()
        .map(|(name, options)| {
            let rendered = render(scene, options, scene.seconds);
            (*name, Take { left: rendered.left, right: rendered.right, rate: RATE })
        })
        .collect();
    Takes { osage, other }
}

fn other(folder: Option<&Path>, file: &str) -> Result<Option<Take>, String> {
    match folder.map(|f| f.join(file)) {
        Some(path) if path.exists() => Take::read(&path).map(Some),
        _ => Ok(None),
    }
}

fn columns(takes: &Takes) -> Vec<(&str, &Take)> {
    let mut all: Vec<(&str, &Take)> = takes.osage.iter().map(|(n, t)| (*n, t)).collect();
    if let Some(take) = &takes.other {
        all.push(("reference A", take));
    }
    all
}

fn header(names: &[&str]) {
    println!("| | {} |", names.join(" | "));
    println!("|---|{}", names.iter().map(|_| "---:|").collect::<String>());
}

fn row(what: &str, cells: impl IntoIterator<Item = String>) {
    println!("| {what} | {} |", cells.into_iter().collect::<Vec<_>>().join(" | "));
}

fn dash(value: Option<f64>, write: impl Fn(f64) -> String) -> String {
    value.map_or_else(|| "—".to_owned(), write)
}

/// A noise 10 m away going round the listener in 20 s: what each ear gets ahead, right, behind and left.
fn circle(takes: &Takes) {
    println!("\n### A noise going round the listener (`controlled-circle`)\n");
    println!("Each second around the instant named: each ear's level under the take's loudest second, then the");
    println!("right ear's level over the left's in three bands (50–500 Hz, 0.5–2 kHz, 2–20 kHz) and how much it leads");
    println!("(µs, broadband). A channel more than {SILENT_DB:.0} dB down is silent (—).\n");
    let all = columns(takes);
    header(&all.iter().map(|c| c.0).collect::<Vec<_>>());
    let bands = [Band::new(50.0, 500.0), Band::new(500.0, 2_000.0), Band::new(2_000.0, 20_000.0)];
    for (name, at) in [("ahead, 0.5 s", 0.5), ("right, 5 s", 5.0), ("behind, 10 s", 10.0), ("left, 15 s", 15.0)] {
        let (from, to) = (at - 0.5, at + 0.5);
        let mut levels = Vec::new();
        let mut differences = Vec::new();
        for (_, take) in &all {
            let loudest =
                (0..20).map(|s| take.level(s as f64, s as f64 + 1.0)).fold(f64::MIN, |m, [l, r]| m.max(l).max(r));
            let [l, r] = take.level(from, to);
            let quiet = |x: f64| x < loudest - SILENT_DB;
            let ear = |x: f64| if quiet(x) { "—".to_owned() } else { format!("{:.1}", x - loudest) };
            levels.push(format!("{} / {}", ear(l), ear(r)));
            if quiet(l) || quiet(r) {
                differences.push("—".to_owned());
                continue;
            }
            let (left, right) = take.span(0.0, 20.0);
            let ild: Vec<String> =
                bands.iter().map(|b| format!("{:+.1}", cue(left, right, take.rate, b, from, to).ild_db)).collect();
            let itd = gcc_phat_between(left, right, take.rate, from, to).itd_us;
            differences.push(format!("{} dB, {itd:+.0} µs", ild.join(" / ")));
        }
        row(&format!("{name}: left / right, dB"), levels);
        row(&format!("{name}: right − left"), differences);
    }
}

/// A 1 kHz tone at 72 km/h on a line 5 m ahead: where it starts, its pitch, and when it comes back under 1 kHz.
fn passby(takes: &Takes) {
    println!("\n### A tone passing at 72 km/h (`controlled-passby`)\n");
    println!("The tone is 1 kHz from the start, 100 m to the left; it is level with the listener at 5 s, 5 m ahead.");
    println!(
        "Sound from the start takes {:.1} ms; at its nearest {:.3} ms, so its pitch comes back under 1 kHz at {:.5} s.",
        (100f64.powi(2) + 25.0).sqrt() / SOUND * 1e3,
        5.0 / SOUND * 1e3,
        5.0 + 5.0 / SOUND
    );
    println!("The engine renders from 3 s before its first sample, so that what a source sent before reaches the ears");
    println!("from the start: its first sound is at 0 by design. Reference A's voice starts with its capture.\n");
    let all = columns(takes);
    header(&all.iter().map(|c| c.0).collect::<Vec<_>>());
    let band = Band::new(800.0, 1_250.0);
    let tracks: Vec<Vec<osage_sound_analysis::Tone>> = all
        .iter()
        .map(|(_, t)| {
            let mono: Vec<f32> = t.left.iter().zip(&t.right).map(|(l, r)| 0.5 * (l + r)).collect();
            tone_track(&mono, t.rate, &band, 0.001)
        })
        .collect();
    row("first sound, ms", all.iter().map(|(_, t)| dash(t.onset(0.0, 120.0), |s| format!("{:.1}", s * 1e3))));
    for at in [0.5, 5.0, 9.5] {
        row(
            &format!("pitch at {at} s, Hz"),
            tracks.iter().map(|track| {
                dash(
                    track.iter().min_by(|a, b| (a.time - at).abs().total_cmp(&(b.time - at).abs())).map(|t| t.hz),
                    |hz| format!("{hz:.2}"),
                )
            }),
        );
    }
    row(
        "back under 1 kHz, s",
        tracks.iter().map(|track| {
            dash(
                track
                    .windows(2)
                    .find(|w| w[0].time > 3.0 && w[0].hz >= 1_000.0 && w[1].hz < 1_000.0)
                    .map(|w| w[0].time + (w[1].time - w[0].time) * (w[0].hz - 1_000.0) / (w[0].hz - w[1].hz)),
                |s| format!("{s:.4}"),
            )
        }),
    );
}

/// The same tone, steady, 300 m ahead.
fn far(takes: &Takes) {
    println!("\n### A tone 300 m ahead (`controlled-far`)\n");
    println!(
        "Sound takes {:.1} ms to come (the engine's renders start with it arrived, as for the passing tone); the level",
        300.0 / SOUND * 1e3
    );
    println!("is that of the steady part, 3 to 5 s.\n");
    let all = columns(takes);
    header(&all.iter().map(|c| c.0).collect::<Vec<_>>());
    row("first sound, ms", all.iter().map(|(_, t)| dash(t.onset(0.0, 120.0), |s| format!("{:.1}", s * 1e3))));
    row(
        "level, left / right, dB",
        all.iter().map(|(_, t)| {
            let [l, r] = t.level(3.0, 5.0);
            format!("{l:.1} / {r:.1}")
        }),
    );
}

/// Wheels knocking on plates: when each knock arrives, on which side.
fn clicks(takes: &Takes, near: Option<&Take>, far: Option<&Take>) {
    println!("\n### Clicks at a distance (`controlled-clicks`)\n");
    println!(
        "The engine's scene: a wheel on a plate at 1 s, 34.3 m to the right (heard at {:.3} s), another at 3 s, 137.2 m to",
        1.0 + 34.3 / SOUND
    );
    println!(
        "the left ({:.3} s). The reference's captures are its own: one sample's click made at its first frame, 34.3 m and 343 m",
        3.0 + 137.2 / SOUND
    );
    println!(
        "away (`click-near.wav`, `click-far.wav`), when it arrives against when it should ({:.0} and {:.0} ms).\n",
        34.3 / SOUND * 1e3,
        343.0 / SOUND * 1e3
    );
    let mut names: Vec<&str> = takes.osage.iter().map(|(n, _)| *n).collect();
    names.push("reference A");
    header(&names);
    let knock = |t: &Take, from: f64| t.onset(from, 40.0).map(|s| (s, t.level(s, s + 0.05)));
    let side =
        |[l, r]: [f64; 2]| if r > l { format!("right by {:.1} dB", r - l) } else { format!("left by {:.1} dB", l - r) };
    let heard = |from: f64| -> Vec<String> {
        takes
            .osage
            .iter()
            .map(|(_, t)| match knock(t, from) {
                Some((s, l)) => format!("{s:.3} s, {}", side(l)),
                None => "—".to_owned(),
            })
            .collect()
    };
    let mut first = heard(0.5);
    first.push(dash(near.and_then(|t| t.onset(0.0, 120.0)), |s| format!("{:.1} ms", s * 1e3)));
    let mut second = heard(2.5);
    second.push(dash(far.and_then(|t| t.onset(0.0, 120.0)), |s| format!("{:.1} ms", s * 1e3)));
    row("nearer click (34.3 m)", first);
    row("farther click (137.2 m here, 343 m there)", second);
}

/// What a surface takes away (or adds) in each octave: the scene with it, over the scene without.
fn effect(title: &str, note: &str, with: &Takes, without: &Takes, comb: bool) {
    println!("\n### {title}\n");
    println!("{note}\n");
    let octaves: Vec<Band> = [250.0, 1_000.0, 4_000.0, 8_000.0].iter().map(|&c| Band::octave(c)).collect();
    let a = columns(with);
    let b = columns(without);
    let pairs: Vec<(&str, &Take, &Take)> = a.iter().zip(&b).map(|((n, x), (_, y))| (*n, *x, *y)).collect();
    header(&pairs.iter().map(|p| p.0).collect::<Vec<_>>());
    for (k, band) in octaves.iter().enumerate() {
        row(
            &format!("octave {:.0} Hz, dB", (band.low * band.high).sqrt()),
            pairs
                .iter()
                .map(|(_, x, y)| format!("{:+.2}", x.bands(&octaves, 0.5, 5.5)[k] - y.bands(&octaves, 0.5, 5.5)[k])),
        );
    }
    if comb {
        // Bands 4 Hz wide from 100 Hz to 1.1 kHz, and the deepest dips of their ratio.
        let fine: Vec<Band> = (0..250).map(|k| Band::new(100.0 + 4.0 * k as f64, 104.0 + 4.0 * k as f64)).collect();
        row(
            "first three dips, Hz (dB)",
            pairs.iter().map(|(_, x, y)| {
                let ratio: Vec<f64> =
                    x.bands(&fine, 0.5, 5.5).iter().zip(y.bands(&fine, 0.5, 5.5)).map(|(p, q)| p - q).collect();
                let dips: Vec<String> = (1..ratio.len() - 1)
                    .filter(|&k| ratio[k] < ratio[k - 1] && ratio[k] <= ratio[k + 1] && ratio[k] < -6.0)
                    .take(3)
                    .map(|k| format!("{:.0} ({:.1})", 102.0 + 4.0 * k as f64, ratio[k]))
                    .collect();
                if dips.is_empty() { "none under −6 dB".to_owned() } else { dips.join(", ") }
            }),
        );
    }
}

/// `against [--reference DIR] [--hrir FILE]`: every controlled scene, as markdown tables.
pub fn run(reference: Option<&Path>, measured: Option<Arc<Hrir>>) -> Result<(), String> {
    let hearings = hearings(&measured);
    println!("## The controlled scenes, the engine beside reference A\n");
    println!(
        "Rendered at reference quality, raw (no fade, no gain); {}.",
        match reference {
            Some(folder) => format!("reference A's captures from {}", folder.display()),
            None => "no captures of reference A given".to_owned(),
        }
    );
    let get = |file: &str| other(reference, file);
    circle(&takes(&scene("circle", |_| {})?, &hearings, get("circle.wav")?));
    passby(&takes(&scene("passby", |_| {})?, &hearings, get("passby.wav")?));
    far(&takes(&scene("far", |_| {})?, &hearings, get("far.wav")?));
    let (near, distant) = (get("click-near.wav")?, get("click-far.wav")?);
    clicks(&takes(&scene("clicks", |_| {})?, &hearings, None), near.as_ref(), distant.as_ref());
    let open = takes(&scene("open", |_| {})?, &hearings, get("screen-absent.wav")?);
    let screen = takes(&scene("screen", |_| {})?, &hearings, get("screen.wav")?);
    effect(
        "A screen 20 m wide and 4 m high halfway to a noise 20 m ahead (`controlled-screen` over `controlled-open`)",
        "Mean power of both ears from 0.5 to 5.5 s, with the screen over without it.",
        &screen,
        &open,
        false,
    );
    let floor = takes(&scene("floor", |_| {})?, &hearings, get("floor.wav")?);
    let bare = takes(
        &scene("floor", |json| {
            if let Some(surfaces) = json.pointer_mut("/nodes/0/extras/osageSound") {
                *surfaces = serde_json::json!({});
            }
        })?,
        &hearings,
        get("floor-absent.wav")?,
    );
    effect(
        "A hard floor under a noise 1 m up, the ears 2 m up and 4 m away (`controlled-floor`, with and without its floor)",
        &format!(
            "The floor's reflection comes {:.3} ms later and should dig dips every {:.1} Hz, the first at {:.1} Hz.",
            ((9f64 + 16.0).sqrt() - (1f64 + 16.0).sqrt()) / SOUND * 1e3,
            SOUND / ((9f64 + 16.0).sqrt() - (1f64 + 16.0).sqrt()),
            SOUND / ((9f64 + 16.0).sqrt() - (1f64 + 16.0).sqrt()) / 2.0
        ),
        &floor,
        &bare,
        true,
    );
    Ok(())
}
