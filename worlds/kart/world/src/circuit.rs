// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The circuit a race is run on: a track read from its file (D-056) — the
//! paddock by default: a long start straight over a crest (D-051), a
//! sweeping right-hander, an S, a chicane and a hairpin.

use libm::{cosf, sinf};
use osage_kart_physics::{Crest, Kart, Track};
use osage_kart_tracks::{SpanKind, TrackDef};

use crate::proto;

pub const SECTORS: u32 = 8;

/// What a track is beside its road: what clients and records know it by.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct About {
    pub id: String,
    pub name: String,
    pub about: String,
    pub sky: String,
    pub scenery: String,
    /// The chance of rain in a race on it (D-068); the world's to roll.
    pub rain: f32,
    /// The chance of a race on it at night (D-085); the world's to roll.
    pub night: f32,
}

pub struct Circuit {
    pub track: Track,
    /// Distance along the centre line of the start/finish line.
    pub start: f32,
    pub about: About,
    /// Bridges and tunnels (D-077): what, from where along the centre
    /// line, how long.
    pub spans: Vec<(SpanKind, f32, f32)>,
}

impl Default for Circuit {
    fn default() -> Self {
        Self::new()
    }
}

impl Circuit {
    /// The paddock.
    pub fn new() -> Self {
        Self::of(&osage_kart_tracks::by_id("paddock").expect("the paddock is built in"))
    }

    /// A track, built.
    pub fn of(def: &TrackDef) -> Self {
        let (track, start) = def.build();
        let about = About {
            id: def.id.clone(),
            name: def.name.clone(),
            about: def.about.clone(),
            sky: def.sky.clone(),
            scenery: def.scenery.clone(),
            rain: def.rain,
            night: def.night,
        };
        let spans = def.spans_on(&track, start);
        Self { track, start, about, spans }
    }

    /// The circuit a world announced in its `kart.track/1`.
    pub fn from_proto(track: &proto::Track) -> Self {
        let points = track.points.as_chunks::<2>().0.to_vec();
        let crests = track
            .crests
            .iter()
            .map(|c| Crest { along: c.along, rise: c.rise, fall: c.fall, height: c.height })
            .collect();
        let about = About {
            id: track.id.clone(),
            name: track.name.clone(),
            about: track.about.clone(),
            sky: track.sky.clone(),
            scenery: track.scenery.clone(),
            rain: 0.0,
            night: 0.0,
        };
        let spans = track
            .spans
            .iter()
            .filter_map(|s| {
                let kind = match s.kind() {
                    proto::SpanKind::Bridge => SpanKind::Bridge,
                    proto::SpanKind::Tunnel => SpanKind::Tunnel,
                    proto::SpanKind::Unspecified => return None,
                };
                Some((kind, s.along, s.length))
            })
            .collect();
        let built =
            Track::new(points, track.half_width, track.verge).with_crests(crests).with_heights(track.heights.clone());
        Self { track: built, start: track.start, about, spans }
    }

    /// Slot `n` of the starting grid: two columns, rows every 7 m behind the line.
    pub fn grid_slot(&self, n: usize) -> Kart {
        let along = self.start - 8.0 - 7.0 * (n / 2) as f32;
        let (centre, heading) = self.track.at(along);
        let side = if n.is_multiple_of(2) { -3.0 } else { 3.0 };
        // Right of the road is (-cos h, sin h).
        Kart { x: centre[0] - cosf(heading) * side, z: centre[1] + sinf(heading) * side, heading, ..Kart::default() }
    }

    /// Distance from the start line, `0..length`.
    pub fn from_start(&self, along: f32) -> f32 {
        (along - self.start).rem_euclid(self.track.length())
    }

    pub fn sector_length(&self) -> f32 {
        self.track.length() / SECTORS as f32
    }

    pub fn to_proto(&self) -> proto::Track {
        proto::Track {
            points: self.track.points().iter().flatten().copied().collect(),
            half_width: self.track.half_width,
            verge: self.track.verge,
            start: self.start,
            sectors: SECTORS,
            crests: self
                .track
                .crests()
                .iter()
                .map(|c| proto::Crest { along: c.along, rise: c.rise, fall: c.fall, height: c.height })
                .collect(),
            id: self.about.id.clone(),
            name: self.about.name.clone(),
            about: self.about.about.clone(),
            sky: self.about.sky.clone(),
            scenery: self.about.scenery.clone(),
            heights: self.track.heights().to_vec(),
            spans: self
                .spans
                .iter()
                .map(|(kind, along, length)| proto::Span {
                    kind: match kind {
                        SpanKind::Bridge => proto::SpanKind::Bridge,
                        SpanKind::Tunnel => proto::SpanKind::Tunnel,
                    }
                    .into(),
                    along: *along,
                    length: *length,
                })
                .collect(),
        }
    }
}
