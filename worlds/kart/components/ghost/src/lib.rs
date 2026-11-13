// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A kart's run kept to race against (G4.1, D-062): the state it started
//! from and what it did in every tick of the world after that, replayed by
//! the physics, which gives the same run to the last bit — the kart's
//! path is not stored, it is driven again.
//!
//! A tick holds the steps the world applied to the kart in it: one as a
//! rule, none while it waited for a late step of its driver's, two when it
//! caught up. Replayed tick for tick, the ghost is where the kart was at
//! every instant, and crosses the line when it did.
//!
//! Nothing here knows of laps or lines: whoever keeps a ghost says what it
//! is a run of, and checks a ghost read from a file by driving it
//! ([`Ghost::drive`]) — one recorded on another shape of the track, or
//! with other physics, no longer ends where it says.
//!
//! Ghosts are kept one a file ([`Shelf`]), in plain text: the header, a
//! line a field, then the ticks, runs of the same tick on one line.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    io,
    path::{Path, PathBuf},
};

use osage_kart_physics::{Build, Controls, Kart, Track, step};

/// The first line of a ghost's file: its format.
pub const HEADER: &str = "kart-ghost/1";
/// The longest run kept, ticks: ten minutes at 60 a second.
pub const LONGEST: usize = 36_000;
/// The largest file read.
const LARGEST: u64 = 4 << 20;
/// Longest name kept, in characters.
const LONGEST_NAME: usize = 40;

/// What a kart did in one tick of the world: up to two steps.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tick {
    steps: [Controls; 2],
    count: u8,
}

impl Tick {
    /// The steps applied in a tick; more than two are not kept.
    pub fn of(steps: &[Controls]) -> Self {
        let mut tick = Self::default();
        for (kept, step) in tick.steps.iter_mut().zip(steps) {
            *kept = *step;
            tick.count += 1;
        }
        tick
    }

    pub fn steps(&self) -> &[Controls] {
        &self.steps[..usize::from(self.count)]
    }
}

/// A run: who drove what, where, how long it took, and how to drive it again.
#[derive(Clone, Debug, PartialEq)]
pub struct Ghost {
    /// The track's id.
    pub track: String,
    /// Who drove: the name the world knows the driver by.
    pub name: String,
    /// The driver character and the kart body chosen (roster ids).
    pub driver: String,
    pub kart: String,
    /// What it is a run of: this many laps, in this time, ms.
    pub laps: u32,
    pub ms: u64,
    /// When, seconds since 1970 (UTC).
    pub at: u64,
    /// The kart as the run started, its build included.
    pub start: Kart,
    pub ticks: Vec<Tick>,
}

impl Ghost {
    /// Drives the run again on `track`: `each` sees the kart after every
    /// tick (0 the first).
    pub fn drive(&self, track: &Track, mut each: impl FnMut(usize, &Kart)) -> Kart {
        let mut kart = self.start;
        for (i, tick) in self.ticks.iter().enumerate() {
            for controls in tick.steps() {
                step(&mut kart, *controls, track);
            }
            each(i, &kart);
        }
        kart
    }

    /// The file's text. Numbers are written so that they read back to the
    /// same bits.
    pub fn to_text(&self) -> String {
        let (k, b) = (&self.start, &self.start.build);
        let mut out = format!(
            "{HEADER}\ntrack {}\nname {}\ndriver {}\nkart {}\nlaps {}\nms {}\nat {}\n",
            clean(&self.track),
            clean(&self.name),
            clean(&self.driver),
            clean(&self.kart),
            self.laps,
            self.ms,
            self.at
        );
        let state = [
            k.x, k.z, k.heading, k.speed, k.slide, k.drift, k.charge, k.boost, k.rocket, k.spin, k.height, k.climb,
            k.trick, k.stall, k.draft, k.held,
        ];
        out.push_str("start");
        for value in state {
            let _ = write!(out, " {value}");
        }
        let _ = writeln!(
            out,
            "\nbuild {} {} {} {} {}\nticks {}",
            b.top,
            b.accel,
            b.handling,
            b.grip,
            b.weight,
            self.ticks.len()
        );
        let mut at = 0;
        while at < self.ticks.len() {
            let tick = self.ticks[at];
            let run = self.ticks[at..].iter().take_while(|t| **t == tick).count();
            let _ = write!(out, "{run} {}", tick.count);
            for s in tick.steps() {
                let _ = write!(out, " {} {} {}", s.throttle, s.steer, u8::from(s.drift));
            }
            out.push('\n');
            at += run;
        }
        out
    }

    /// Reads what [`to_text`](Self::to_text) wrote, or says what is wrong
    /// with it: a ghost is whole or it is none.
    pub fn from_text(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some(HEADER) {
            return Err("not a kart ghost".to_owned());
        }
        let mut field = |name: &str| -> Result<String, String> {
            let line = lines.next().ok_or_else(|| format!("no {name}"))?;
            let rest = line.strip_prefix(name).ok_or_else(|| format!("no {name}"))?;
            Ok(rest.trim().to_owned())
        };
        let number = |text: String, name: &str| text.parse::<u64>().map_err(|_| format!("{name}: not a number"));
        let (track, name, driver, kart) = (field("track")?, field("name")?, field("driver")?, field("kart")?);
        let laps = number(field("laps")?, "laps")? as u32;
        let ms = number(field("ms")?, "ms")?;
        let at = number(field("at")?, "at")?;
        let floats = |text: String, name: &str, count: usize| -> Result<Vec<f32>, String> {
            let values: Vec<f32> = text.split_whitespace().map_while(|v| v.parse().ok()).collect();
            let whole = values.len() == count && text.split_whitespace().count() == count;
            if whole && values.iter().all(|v| v.is_finite()) {
                Ok(values)
            } else {
                Err(format!("{name}: not {count} numbers"))
            }
        };
        let s = floats(field("start")?, "start", 16)?;
        let b = floats(field("build")?, "build", 5)?;
        let count = number(field("ticks")?, "ticks")? as usize;
        if name.is_empty() || laps == 0 || count == 0 || count > LONGEST {
            return Err("not a run".to_owned());
        }
        let build = Build { top: b[0], accel: b[1], handling: b[2], grip: b[3], weight: b[4] };
        if build != build.sanitized() {
            return Err("build: not a kart's".to_owned());
        }
        let start = Kart {
            x: s[0],
            z: s[1],
            heading: s[2],
            speed: s[3],
            slide: s[4],
            drift: s[5],
            charge: s[6],
            boost: s[7],
            rocket: s[8],
            spin: s[9],
            height: s[10],
            climb: s[11],
            trick: s[12],
            stall: s[13],
            draft: s[14],
            held: s[15],
            // A run starts on clean tyres.
            slick: 0.0,
            build,
        };
        let mut ticks = Vec::with_capacity(count);
        for line in lines {
            let mut words = line.split_whitespace();
            let (Some(run), Some(steps)) = (words.next(), words.next()) else { continue };
            let bad = || format!("ticks: {line:?}");
            let run: usize = run.parse().map_err(|_| bad())?;
            let steps: usize = steps.parse().map_err(|_| bad())?;
            let rest: Vec<&str> = words.collect();
            if run == 0 || steps > 2 || rest.len() != steps * 3 || ticks.len() + run > count {
                return Err(bad());
            }
            let mut taken = [Controls::default(); 2];
            for (controls, words) in taken.iter_mut().zip(rest.as_chunks::<3>().0) {
                let throttle: f32 = words[0].parse().map_err(|_| bad())?;
                let steer: f32 = words[1].parse().map_err(|_| bad())?;
                let read = Controls { throttle, steer, drift: words[2] == "1" };
                if read != read.sanitized() || !matches!(words[2], "0" | "1") {
                    return Err(bad());
                }
                *controls = read;
            }
            ticks.extend(std::iter::repeat_n(Tick::of(&taken[..steps]), run));
        }
        if ticks.len() != count {
            return Err(format!("ticks: {} of {count}", ticks.len()));
        }
        Ok(Self { track, name, driver, kart, laps, ms, at, start, ticks })
    }
}

/// Records a run as it is driven.
#[derive(Clone, Debug)]
pub struct Recorder {
    start: Kart,
    ticks: Vec<Tick>,
}

impl Recorder {
    /// A run that starts from `start`: the kart before its first step.
    pub fn new(start: Kart) -> Self {
        Self { start, ticks: Vec::new() }
    }

    /// The steps applied to the kart in this tick of the world. A run
    /// longer than [`LONGEST`] is not kept: [`ghost`](Self::ghost) gives none.
    pub fn tick(&mut self, steps: &[Controls]) {
        if self.ticks.len() <= LONGEST {
            self.ticks.push(Tick::of(steps));
        }
    }

    pub fn ticks(&self) -> usize {
        self.ticks.len()
    }

    /// The run, over: `about` says what it was (its `start` and `ticks` are
    /// replaced by what was recorded).
    pub fn ghost(&self, about: Ghost) -> Option<Ghost> {
        let whole = !self.ticks.is_empty() && self.ticks.len() <= LONGEST;
        whole.then(|| Ghost { start: self.start, ticks: self.ticks.clone(), ..about })
    }
}

/// A ghost being driven again, a tick at a time.
#[derive(Clone, Debug)]
pub struct Replay {
    ghost: std::sync::Arc<Ghost>,
    at: usize,
}

impl Replay {
    pub fn new(ghost: std::sync::Arc<Ghost>) -> Self {
        Self { ghost, at: 0 }
    }

    pub fn ghost(&self) -> &Ghost {
        &self.ghost
    }

    /// The kart as the run starts.
    pub fn start(&self) -> Kart {
        self.ghost.start
    }

    /// The steps of the next tick; none once the run is over.
    pub fn next_tick(&mut self) -> Option<Tick> {
        let tick = self.ghost.ticks.get(self.at).copied();
        self.at += usize::from(tick.is_some());
        tick
    }
}

/// The ghosts a world keeps: each driver's best run on each track for each
/// number of laps, in memory and, with a folder, in files that survive a
/// restart (`FOLDER/TRACK.LAPS/NAME-HASH.ghost`).
#[derive(Debug, Default)]
pub struct Shelf {
    folder: Option<PathBuf>,
    /// By (track, laps), then by driver name.
    kept: BTreeMap<(String, u32), BTreeMap<String, std::sync::Arc<Ghost>>>,
    /// Files that did not read, with why: for whoever opened the shelf to say.
    pub unread: Vec<(PathBuf, String)>,
}

impl Shelf {
    /// A shelf kept in `folder`, or in memory only.
    pub fn open(folder: Option<PathBuf>) -> Self {
        Self { folder, ..Self::default() }
    }

    fn runs(&mut self, track: &str, laps: u32) -> &mut BTreeMap<String, std::sync::Arc<Ghost>> {
        let key = (clean(track), laps);
        if !self.kept.contains_key(&key) {
            let mut runs = BTreeMap::new();
            if let Some(folder) = &self.folder {
                for path in files(&folder.join(format!("{}.{laps}", slug(track)))) {
                    match read(&path) {
                        Ok(ghost) if ghost.track == key.0 && ghost.laps == laps => {
                            let faster =
                                runs.get(&ghost.name).is_none_or(|old: &std::sync::Arc<Ghost>| ghost.ms < old.ms);
                            if faster {
                                runs.insert(ghost.name.clone(), std::sync::Arc::new(ghost));
                            }
                        }
                        Ok(_) => self.unread.push((path, "of another track or number of laps".to_owned())),
                        Err(why) => self.unread.push((path, why)),
                    }
                }
            }
            self.kept.insert(key.clone(), runs);
        }
        self.kept.get_mut(&key).expect("just there")
    }

    /// A driver's best run of `laps` laps on `track`.
    pub fn of(&mut self, track: &str, laps: u32, name: &str) -> Option<std::sync::Arc<Ghost>> {
        self.runs(track, laps).get(&clean(name)).cloned()
    }

    /// Every driver's best run of `laps` laps on `track`, the fastest first.
    pub fn all(&mut self, track: &str, laps: u32) -> Vec<std::sync::Arc<Ghost>> {
        let mut all: Vec<_> = self.runs(track, laps).values().cloned().collect();
        all.sort_by(|a, b| a.ms.cmp(&b.ms).then_with(|| a.at.cmp(&b.at)));
        all
    }

    /// A run that was not what it said: forgotten, its file left where it is.
    pub fn forget(&mut self, ghost: &Ghost) {
        self.runs(&ghost.track, ghost.laps).remove(&ghost.name);
    }

    /// A run just driven: kept if it beats the driver's best (`Ok(true)`),
    /// and written whole to a new file renamed over the old one.
    pub fn keep(&mut self, ghost: Ghost) -> io::Result<bool> {
        let ghost = Ghost {
            track: clean(&ghost.track),
            name: clean(&ghost.name),
            driver: clean(&ghost.driver),
            kart: clean(&ghost.kart),
            ..ghost
        };
        if ghost.name.is_empty() || ghost.ticks.is_empty() || ghost.ticks.len() > LONGEST {
            return Ok(false);
        }
        let folder = self.folder.clone();
        let runs = self.runs(&ghost.track, ghost.laps);
        if runs.get(&ghost.name).is_some_and(|old| old.ms <= ghost.ms) {
            return Ok(false);
        }
        let saved = match folder {
            Some(folder) => {
                let folder = folder.join(format!("{}.{}", slug(&ghost.track), ghost.laps));
                std::fs::create_dir_all(&folder)?;
                let path = folder.join(format!("{}-{:08x}.ghost", slug(&ghost.name), hash(&ghost.name)));
                let mut new = path.clone();
                new.as_mut_os_string().push(".new");
                std::fs::write(&new, ghost.to_text())?;
                std::fs::rename(&new, &path)
            }
            None => Ok(()),
        };
        // Kept in memory even when the file could not be written.
        runs.insert(ghost.name.clone(), std::sync::Arc::new(ghost));
        saved.map(|()| true)
    }
}

fn files(folder: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|e| e == "ghost"))
        .collect();
    paths.sort();
    paths
}

fn read(path: &Path) -> Result<Ghost, String> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > LARGEST {
        return Err("too large for a ghost".to_owned());
    }
    Ghost::from_text(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
}

/// A name as the file keeps it: one line, trimmed, short.
fn clean(text: &str) -> String {
    let one_line: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    one_line.trim().chars().take(LONGEST_NAME).collect()
}

/// A name as a file's: letters, digits, `-` and `_`, the rest `_`.
fn slug(text: &str) -> String {
    let slug: String = clean(text)
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c.to_ascii_lowercase() } else { '_' })
        .collect();
    if slug.is_empty() { "_".to_owned() } else { slug }
}

/// FNV-1a: two names with the same slug get two files.
fn hash(text: &str) -> u32 {
    clean(text).bytes().fold(0x811c_9dc5, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circuit() -> Track {
        let points: Vec<[f32; 2]> = (0..48)
            .map(|i| {
                let a = i as f32 / 48.0 * std::f32::consts::TAU;
                [60.0 * a.cos(), 40.0 * a.sin()]
            })
            .collect();
        Track::new(points, 5.0, 3.0)
    }

    /// A run of `ticks` ticks round the circuit, driven as an agent's kart
    /// is — or with a keyboard's all or nothing — with ticks of no step
    /// and of two among them.
    fn run(track: &Track, ticks: usize, name: &str, ms: u64) -> (Ghost, Kart, Vec<Kart>) {
        let ([x, z], heading) = track.at(0.0);
        let mut kart = Kart { x, z, heading, ..Kart::default() };
        let mut recorder = Recorder::new(kart);
        let mut path = Vec::new();
        let keys = name.starts_with("keys");
        for i in 0..ticks {
            let mut controls = osage_kart_racecraft::follow(&kart, 0.9, 0.0, i % 240 > 200, track);
            if keys {
                controls.steer = if controls.steer.abs() > 0.3 { controls.steer.signum() } else { 0.0 };
                controls.throttle = 1.0;
            }
            let steps: &[Controls] = match i % 97 {
                13 => &[],
                14 => &[controls, controls],
                _ => &[controls],
            };
            for controls in steps {
                step(&mut kart, *controls, track);
            }
            recorder.tick(steps);
            path.push(kart);
        }
        let about = Ghost {
            track: "ring".into(),
            name: name.into(),
            driver: "ana".into(),
            kart: "sprint".into(),
            laps: 3,
            ms,
            at: 1_794_000_000,
            start: Kart::default(),
            ticks: Vec::new(),
        };
        (recorder.ghost(about).unwrap(), kart, path)
    }

    #[test]
    fn a_ghost_drives_the_run_again_to_the_last_bit() {
        let track = circuit();
        let (ghost, end, path) = run(&track, 1500, "alice", 25_000);
        assert!(end.speed > 10.0 && track.locate(end.x, end.z).lateral.abs() < 8.0, "it drove: {end:?}");
        let mut seen = 0;
        let again = ghost.drive(&track, |i, kart| {
            assert_eq!(*kart, path[i], "tick {i}");
            seen += 1;
        });
        assert_eq!((again, seen), (end, 1500));
        // A tick at a time, as a world replays it.
        let mut replay = Replay::new(std::sync::Arc::new(ghost.clone()));
        let mut kart = replay.start();
        let mut ticks = 0;
        while let Some(tick) = replay.next_tick() {
            for controls in tick.steps() {
                step(&mut kart, *controls, &track);
            }
            ticks += 1;
        }
        assert_eq!((kart, ticks), (end, 1500));
        assert_eq!(replay.next_tick(), None);
    }

    #[test]
    fn a_ghost_reads_back_as_written_or_not_at_all() {
        let track = circuit();
        let (ghost, ..) = run(&track, 900, "keys of Zoë ✨", 15_000);
        let text = ghost.to_text();
        assert!(text.starts_with("kart-ghost/1\ntrack ring\nname keys of Zoë ✨\n"), "{}", &text[..70]);
        assert!(text.lines().count() < 600, "runs of the same tick share a line: {}", text.lines().count());
        assert_eq!(Ghost::from_text(&text).unwrap(), ghost);
        // Damaged: none, with why.
        assert!(Ghost::from_text("something else\n").is_err());
        let cut = &text[..text.len() - 40];
        assert!(Ghost::from_text(cut).unwrap_err().starts_with("ticks"), "{:?}", Ghost::from_text(cut));
        assert!(Ghost::from_text(&text.replace("laps 3", "laps many")).is_err());
        assert!(Ghost::from_text(&text.replace("\nbuild 1 1 1 1 1\n", "\nbuild 9 1 1 1 1\n")).is_err());
        // Controls beyond their range, in the last run of ticks.
        let (rest, last) = text.trim_end().rsplit_once('\n').unwrap();
        let run = last.split(' ').next().unwrap();
        let wild = format!("{rest}\n{run} 1 7 0 0\n");
        assert!(Ghost::from_text(&format!("{rest}\n{run} 1 1 0 0\n")).is_ok());
        assert_eq!(Ghost::from_text(&wild).unwrap_err(), format!("ticks: \"{run} 1 7 0 0\""));
    }

    #[test]
    fn a_shelf_keeps_each_drivers_best_and_survives_a_restart() {
        let track = circuit();
        let folder = std::env::temp_dir().join(format!("osage-kart-ghost-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let mut shelf = Shelf::open(Some(folder.clone()));
        assert!(shelf.all("ring", 3).is_empty());
        assert!(shelf.keep(run(&track, 600, "alice", 31_000).0).unwrap());
        assert!(shelf.keep(run(&track, 500, "a/lice", 30_500).0).unwrap(), "another name, another file");
        assert!(!shelf.keep(run(&track, 700, "alice", 31_200).0).unwrap(), "slower than her best");
        assert!(shelf.keep(run(&track, 400, "alice", 30_100).0).unwrap());
        let names = |shelf: &mut Shelf| -> Vec<(String, u64, usize)> {
            shelf.all("ring", 3).iter().map(|g| (g.name.clone(), g.ms, g.ticks.len())).collect()
        };
        let kept = [("alice".to_owned(), 30_100, 400), ("a/lice".to_owned(), 30_500, 500)];
        assert_eq!(names(&mut shelf), kept);
        assert!(shelf.all("ring", 5).is_empty() && shelf.all("harbour", 3).is_empty(), "other runs, other shelves");
        // A file that is no ghost is told, not fatal.
        std::fs::write(folder.join("ring.3").join("broken.ghost"), "kart-ghost/1\nnothing\n").unwrap();
        let mut again = Shelf::open(Some(folder.clone()));
        assert_eq!(names(&mut again), kept);
        assert_eq!(again.unread.len(), 1, "{:?}", again.unread);
        assert_eq!(again.of("ring", 3, "alice").unwrap().ms, 30_100);
        assert!(again.of("ring", 3, "bob").is_none());
        let alice = again.of("ring", 3, "alice").unwrap();
        again.forget(&alice);
        assert!(again.of("ring", 3, "alice").is_none());
        assert_eq!(std::fs::read_dir(folder.join("ring.3")).unwrap().count(), 3, "no .new left behind");
        std::fs::remove_dir_all(&folder).unwrap();
        // With no folder: in memory.
        let mut memory = Shelf::open(None);
        assert!(memory.keep(run(&track, 300, "bob", 20_000).0).unwrap());
        assert_eq!(memory.of("ring", 3, "bob").unwrap().ticks.len(), 300);
    }
}
