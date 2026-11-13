// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The kart world's tracks as data (G3.2, D-056). A track is a short text
//! file — its centre line as control points of a closed Catmull-Rom spline,
//! its road and verge widths, its crests, where the start line is, its sky
//! and its scenery — that [`TrackDef::parse`] reads, [`TrackDef::problems`]
//! checks (the loop does not cross itself, its bends can be driven, its
//! crests and start stand on straight road) and [`TrackDef::build`] makes
//! into the physics' [`Track`]. Nothing about a track is code: the land,
//! the roadside and the landmarks follow the road.
//!
//! ```text
//! kart-track/1
//! id paddock
//! name The Paddock
//! about A club circuit in the hills.
//! sky noon
//! scenery meadow
//! half-width 7
//! verge 4
//! point 0 -90
//! point 0 -20 start
//! …
//! crest 42 12 6 1.4
//! ```
//!
//! A `#` starts a comment. `crest AFTER RISE FALL HEIGHT`: a crest starting
//! AFTER metres past the start line.

use std::fmt;

use libm::{fabsf, hypotf};
use osage_kart_physics::{Crest, Track, wrap};

/// The first line of every track file.
pub const HEADER: &str = "kart-track/1";

/// Samples of the spline between two control points.
pub const SAMPLES_PER_SPAN: usize = 10;

/// The tightest bend allowed, metres of radius on the centre line: a
/// hairpin a kart takes at about 45 km/h.
pub const TIGHTEST: f32 = 12.0;

/// The tracks that come with the game, in the order of a cup.
pub const BUILT_IN: &[&str] = &[
    include_str!("../tracks/paddock.track"),
    include_str!("../tracks/coast.track"),
    include_str!("../tracks/forest.track"),
    include_str!("../tracks/harbour.track"),
];

/// The battle arenas that come with the game (D-065): tracks too — a wide
/// loop with walls — but fought on, not raced.
pub const ARENAS: &[&str] = &[include_str!("../tracks/yard.track")];

/// A track, as its file describes it.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackDef {
    pub id: String,
    pub name: String,
    pub about: String,
    /// The sky it is raced under: `noon`, `evening`.
    pub sky: String,
    /// What surrounds it: `meadow`, `coast`, `forest`, `town`.
    pub scenery: String,
    pub half_width: f32,
    pub verge: f32,
    /// Control points of the closed centre line, metres, in racing order.
    pub points: Vec<[f32; 2]>,
    /// The control point the start line is at.
    pub start: usize,
    pub crests: Vec<CrestDef>,
}

/// A crest, placed from the start line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrestDef {
    /// Metres past the start line where the road begins to rise.
    pub after: f32,
    pub rise: f32,
    pub fall: f32,
    pub height: f32,
}

/// Why a track file does not read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

impl TrackDef {
    /// Reads a track file.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut lines = text
            .lines()
            .enumerate()
            .map(|(n, line)| (n + 1, line.split('#').next().unwrap_or("").trim()))
            .filter(|(_, line)| !line.is_empty());
        let fail = |line, message: &str| ParseError { line, message: message.to_owned() };
        match lines.next() {
            Some((_, HEADER)) => {}
            Some((n, _)) => return Err(fail(n, "not a kart-track/1 file")),
            None => return Err(fail(0, "empty")),
        }
        let mut def = TrackDef {
            id: String::new(),
            name: String::new(),
            about: String::new(),
            sky: "noon".to_owned(),
            scenery: "meadow".to_owned(),
            half_width: 7.0,
            verge: 4.0,
            points: Vec::new(),
            start: 0,
            crests: Vec::new(),
        };
        let mut start = None;
        for (n, line) in lines {
            let (key, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            let rest = rest.trim();
            let numbers = || -> Result<Vec<f32>, ParseError> {
                rest.split_whitespace()
                    .filter(|word| *word != "start")
                    .map(|word| word.parse::<f32>().ok().filter(|v| v.is_finite()).ok_or_else(|| fail(n, "a number")))
                    .collect()
            };
            let count = |want: usize| -> Result<Vec<f32>, ParseError> {
                let values = numbers()?;
                if values.len() == want { Ok(values) } else { Err(fail(n, &format!("{key} takes {want} numbers"))) }
            };
            match key {
                "id" => def.id = rest.to_owned(),
                "name" => def.name = rest.to_owned(),
                "about" => def.about = rest.to_owned(),
                "sky" => def.sky = rest.to_owned(),
                "scenery" => def.scenery = rest.to_owned(),
                "half-width" => def.half_width = count(1)?[0],
                "verge" => def.verge = count(1)?[0],
                "point" => {
                    let xz = count(2)?;
                    if rest.split_whitespace().any(|word| word == "start") {
                        if start.is_some() {
                            return Err(fail(n, "a second start"));
                        }
                        start = Some(def.points.len());
                    }
                    def.points.push([xz[0], xz[1]]);
                }
                "crest" => {
                    let c = count(4)?;
                    def.crests.push(CrestDef { after: c[0], rise: c[1], fall: c[2], height: c[3] });
                }
                _ => return Err(fail(n, &format!("unknown key {key}"))),
            }
        }
        if def.id.is_empty() || def.id.contains(char::is_whitespace) {
            return Err(fail(0, "an id, one word"));
        }
        if def.points.len() < 4 {
            return Err(fail(0, "at least four points"));
        }
        def.start = start.ok_or_else(|| fail(0, "a point marked start"))?;
        Ok(def)
    }

    /// The centre line, sampled: the control points joined by a closed
    /// Catmull-Rom spline.
    pub fn centre_line(&self) -> Vec<[f32; 2]> {
        let control = &self.points;
        let n = control.len();
        let mut points = Vec::with_capacity(n * SAMPLES_PER_SPAN);
        for i in 0..n {
            let [p0, p1, p2, p3] = [control[(i + n - 1) % n], control[i], control[(i + 1) % n], control[(i + 2) % n]];
            for s in 0..SAMPLES_PER_SPAN {
                let t = s as f32 / SAMPLES_PER_SPAN as f32;
                points.push([catmull_rom(p0[0], p1[0], p2[0], p3[0], t), catmull_rom(p0[1], p1[1], p2[1], p3[1], t)]);
            }
        }
        points
    }

    /// The physics' track and the distance along it of the start line.
    pub fn build(&self) -> (Track, f32) {
        let track = Track::new(self.centre_line(), self.half_width, self.verge);
        let [x, z] = self.points[self.start];
        let start = track.locate(x, z).along;
        let length = track.length();
        let crests = self
            .crests
            .iter()
            .map(|c| Crest {
                along: (start + c.after).rem_euclid(length),
                rise: c.rise,
                fall: c.fall,
                height: c.height,
            })
            .collect();
        (track.with_crests(crests), start)
    }

    /// What is wrong with the track, if anything: a road narrower or wider
    /// than karts race on, a loop that crosses or comes near itself, a bend
    /// tighter than [`TIGHTEST`], a crest or a grid in a bend.
    pub fn problems(&self) -> Vec<String> {
        self.checked(400.0, true)
    }

    /// What is wrong with it as a battle arena (D-065): as a track, but it
    /// may be shorter — a bowl, where the karts meet — and has no grid.
    pub fn arena_problems(&self) -> Vec<String> {
        self.checked(200.0, false)
    }

    fn checked(&self, shortest: f32, grid: bool) -> Vec<String> {
        let mut out = Vec::new();
        if !(4.0..=12.0).contains(&self.half_width) {
            out.push(format!("half width {} outside 4..12 m", self.half_width));
        }
        if !(1.0..=10.0).contains(&self.verge) {
            out.push(format!("verge {} outside 1..10 m", self.verge));
        }
        let (track, start) = self.build();
        let length = track.length();
        if !(shortest..=3000.0).contains(&length) {
            out.push(format!("length {length:.0} m outside {shortest}..3000 m"));
        }
        // Two stretches of road further apart along the loop than a bend
        // must keep their barriers apart.
        let step = 2.0;
        let samples: Vec<(f32, [f32; 2])> =
            (0..(length / step) as usize).map(|i| i as f32 * step).map(|a| (a, track.at(a).0)).collect();
        let room = 2.0 * (self.half_width + self.verge) + 2.0;
        'outer: for (i, (a, p)) in samples.iter().enumerate() {
            for (b, q) in &samples[i + 1..] {
                let apart = (b - a).min(length - (b - a));
                if apart > room * 2.5 && hypotf(p[0] - q[0], p[1] - q[1]) < room {
                    out.push(format!("the road at {a:.0} m comes within {room:.0} m of itself at {b:.0} m"));
                    break 'outer;
                }
            }
        }
        if let Some((along, radius)) = tightest(&track)
            && radius < TIGHTEST
        {
            out.push(format!("a bend of {radius:.1} m radius at {along:.0} m, tighter than {TIGHTEST} m"));
        }
        let turn = |from: f32, to: f32| fabsf(wrap(track.at(to).1 - track.at(from).1));
        for crest in &self.crests {
            if crest.height <= 0.0 || crest.height > 3.0 || crest.rise < 4.0 || crest.fall < 3.0 {
                out.push(format!("the crest {} m after the start is not a crest a kart lands from", crest.after));
            }
            // From just before it to where a kart flat out lands.
            let from = start + crest.after - 5.0;
            if turn(from, start + crest.after + crest.rise + crest.fall + 15.0) > 0.25 {
                out.push(format!("the crest {} m after the start is in a bend", crest.after));
            }
        }
        if grid && turn(start - 40.0, start + 10.0) > 0.25 {
            out.push("the start and its grid are in a bend".to_owned());
        }
        out
    }
}

/// The track's tightest bend: where, and its radius (heading over 12 m).
pub fn tightest(track: &Track) -> Option<(f32, f32)> {
    let length = track.length();
    (0..(length / 2.0) as usize)
        .map(|i| i as f32 * 2.0)
        .map(|along| {
            let turn = fabsf(wrap(track.at(along + 6.0).1 - track.at(along - 6.0).1));
            (along, if turn > 1e-4 { 12.0 / turn } else { f32::INFINITY })
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// The built-in tracks, in the order of a cup.
pub fn built_in() -> Vec<TrackDef> {
    BUILT_IN.iter().map(|text| TrackDef::parse(text).expect("a built-in track reads")).collect()
}

/// A built-in track by its id.
pub fn by_id(id: &str) -> Option<TrackDef> {
    built_in().into_iter().find(|track| track.id == id)
}

/// The battle arenas that come with the game.
pub fn arenas() -> Vec<TrackDef> {
    ARENAS.iter().map(|text| TrackDef::parse(text).expect("a built-in arena reads")).collect()
}

/// A built-in arena by its id.
pub fn arena(id: &str) -> Option<TrackDef> {
    arenas().into_iter().find(|arena| arena.id == id)
}

fn catmull_rom(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let (t2, t3) = (t * t, t * t * t);
    0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_track_reads_and_has_no_problem() {
        let tracks = built_in();
        assert_eq!(tracks.len(), BUILT_IN.len());
        for track in &tracks {
            let (built, _) = track.build();
            println!(
                "{}: {:.0} m, tightest {:?}, {} crests",
                track.id,
                built.length(),
                tightest(&built),
                track.crests.len()
            );
            assert_eq!(track.problems(), Vec::<String>::new(), "{}", track.id);
            assert!(!track.name.is_empty() && !track.about.is_empty(), "{}", track.id);
        }
        let mut ids: Vec<_> = tracks.iter().map(|t| t.id.as_str()).collect();
        ids.dedup();
        assert_eq!(ids.len(), tracks.len());
    }

    #[test]
    fn every_arena_reads_and_is_wide() {
        for arena in arenas() {
            assert_eq!(arena.arena_problems(), Vec::<String>::new(), "{}", arena.id);
            assert!(arena.half_width >= 10.0 && by_id(&arena.id).is_none(), "{}: wide, and not a track", arena.id);
        }
        assert!(arena("yard").is_some());
    }

    #[test]
    fn a_file_that_does_not_read_says_where() {
        assert_eq!(TrackDef::parse("").unwrap_err().message, "empty");
        assert_eq!(TrackDef::parse("kart-track/2\n").unwrap_err().line, 1);
        let bad = "kart-track/1\nid x\npoint 0 0 start\npoint 1 nope\n";
        assert_eq!(TrackDef::parse(bad).unwrap_err().line, 4);
        let unknown = "kart-track/1\nid x\nbanking 3\n";
        assert!(TrackDef::parse(unknown).unwrap_err().message.contains("banking"));
        let no_start = "kart-track/1\nid x\npoint 0 0\npoint 100 0\npoint 100 100\npoint 0 100\n";
        assert!(TrackDef::parse(no_start).unwrap_err().message.contains("start"));
    }

    #[test]
    fn a_track_that_crosses_itself_or_bends_too_tight_is_refused() {
        // A figure of eight: the road crosses itself in the middle.
        let eight = "kart-track/1\nid eight\npoint 0 0 start\npoint 120 120\npoint 240 0\npoint 120 -120\npoint 0 0\n\
                     point -120 120\npoint -240 0\npoint -120 -120\n";
        let problems = TrackDef::parse(eight).unwrap().problems();
        assert!(problems.iter().any(|p| p.contains("within")), "{problems:?}");
        // A square with sharp corners.
        let square = "kart-track/1\nid square\npoint 0 0 start\npoint 0 150\npoint 2 152\npoint 150 152\npoint 152 150\n\
                      point 152 0\npoint 150 -2\npoint 2 -2\n";
        let problems = TrackDef::parse(square).unwrap().problems();
        assert!(problems.iter().any(|p| p.contains("tighter")), "{problems:?}");
    }

    #[test]
    fn the_paddock_is_the_circuit_it_was() {
        // The world's circuit before tracks were data (D-051): the same
        // control points, crest and start.
        let paddock = by_id("paddock").unwrap();
        let (track, start) = paddock.build();
        assert_eq!(paddock.points.len(), 17);
        assert_eq!(paddock.points[paddock.start], [0.0, -20.0]);
        assert_eq!(track.crests()[0].along, start + 42.0);
        assert_eq!(track.points().len(), 17 * SAMPLES_PER_SPAN);
    }
}
