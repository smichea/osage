// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the world races (D-057): a cup — its tracks one after the other,
//! points for each place, standings between the races, a podium — or one
//! track, race after race, a time trial on one track (D-062), or a battle
//! in an arena (D-065). The host
//! chooses; the others' choices are votes, and with no host the most wanted
//! event is raced next.

use std::collections::BTreeMap;

use bevy::prelude::*;
use osage_kart_cup::{Cup, POINTS, Series};
use osage_kart_tracks::TrackDef;

use crate::proto;

/// Every event there is to race, as `Choice.event` names them: the cups,
/// then each track alone, then a time trial on each.
pub fn events() -> Vec<String> {
    let tracks = osage_kart_tracks::built_in();
    osage_kart_cup::built_in()
        .iter()
        .map(|cup| format!("cup:{}", cup.id))
        .chain(tracks.iter().map(|track| format!("track:{}", track.id)))
        .chain(tracks.iter().map(|track| format!("trial:{}", track.id)))
        .chain(osage_kart_tracks::arenas().iter().map(|arena| format!("battle:{}", arena.id)))
        .collect()
}

#[derive(Resource)]
pub struct Cups {
    /// The event being raced.
    pub event: String,
    /// The cup, when the event is one.
    pub series: Option<Series>,
    /// What each driver would race next, by session.
    wanted: BTreeMap<u64, String>,
}

impl Cups {
    /// The world racing `event`, and the track of its first race; an event
    /// that is not one races the paddock.
    pub fn new(event: &str) -> (Self, TrackDef) {
        let mut cups = Self { event: String::new(), series: None, wanted: BTreeMap::new() };
        let track = cups.start(event);
        (cups, track)
    }

    /// The world racing one track, race after race: a built-in one or one
    /// read from a file.
    pub fn single(track: &TrackDef) -> Self {
        Self { event: format!("track:{}", track.id), series: None, wanted: BTreeMap::new() }
    }

    /// Starts `event` from its first race; the track it is on.
    fn start(&mut self, event: &str) -> TrackDef {
        let cup = event.strip_prefix("cup:").and_then(|id| osage_kart_cup::built_in().into_iter().find(|c| c.id == id));
        let single = event
            .strip_prefix("track:")
            .or_else(|| event.strip_prefix("trial:"))
            .and_then(osage_kart_tracks::by_id)
            .or_else(|| event.strip_prefix("battle:").and_then(osage_kart_tracks::arena));
        let (event, series, track) = match (cup, single) {
            (Some(cup), _) => {
                let first = cup.tracks.first().and_then(|id| osage_kart_tracks::by_id(id));
                (event.to_owned(), Some(Series::new(cup)), first)
            }
            (None, Some(track)) => (event.to_owned(), None, Some(track)),
            (None, None) => ("track:paddock".to_owned(), None, None),
        };
        self.event = event;
        self.series = series;
        track.unwrap_or_else(|| osage_kart_tracks::by_id("paddock").expect("the paddock is built in"))
    }

    /// Whether the event being raced is a time trial (D-062).
    pub fn trial(&self) -> bool {
        self.event.starts_with("trial:")
    }

    /// Whether the event being fought is a battle (D-065).
    pub fn battle(&self) -> bool {
        self.event.starts_with("battle:")
    }

    /// A driver's choice of what to race next; empty or unknown, none.
    pub fn choose(&mut self, session: u64, event: &str) {
        if events().iter().any(|known| known == event) {
            self.wanted.insert(session, event.to_owned());
        }
    }

    /// A driver gone: their vote with them.
    pub fn forget(&mut self, session: u64) {
        self.wanted.remove(&session);
    }

    /// What is to be raced next: the host's choice, or with no host the
    /// event most drivers want (the first of a tie in [`events`]' order);
    /// `None` to go on with the event being raced.
    pub fn next(&self, host: Option<u64>) -> Option<String> {
        let wanted = match host {
            Some(host) => self.wanted.get(&host).cloned(),
            None => {
                let votes = self.votes();
                let most = votes.iter().map(|v| v.drivers).max()?;
                votes.into_iter().find(|v| v.drivers == most).map(|v| v.event)
            }
        };
        wanted.filter(|wanted| *wanted != self.event)
    }

    /// How many drivers want each event, in [`events`]' order.
    pub fn votes(&self) -> Vec<proto::Vote> {
        events()
            .into_iter()
            .filter_map(|event| {
                let drivers = self.wanted.values().filter(|w| **w == event).count() as u32;
                (drivers > 0).then_some(proto::Vote { event, drivers })
            })
            .collect()
    }

    /// The track of the race to line up: the next event's first if one was
    /// chosen, else the cup's next track (its first again once it is over);
    /// `None` for the same track again.
    pub fn advance(&mut self, host: Option<u64>) -> Option<TrackDef> {
        if let Some(next) = self.next(host) {
            return Some(self.start(&next));
        }
        let series = self.series.as_mut()?;
        if series.over() {
            *series = Series::new(series.cup.clone());
        }
        series.track().and_then(osage_kart_tracks::by_id)
    }

    /// The result of the race just run: (name, bot), first first.
    pub fn score(&mut self, order: &[(String, bool)]) {
        if let Some(series) = &mut self.series {
            series.score(order);
        }
    }

    /// Whether the race just run ended the cup: the podium is shown longer.
    pub fn just_over(&self) -> bool {
        self.series.as_ref().is_some_and(Series::over)
    }

    pub fn to_proto(&self) -> Option<proto::Cup> {
        let series = self.series.as_ref()?;
        let Cup { id, name, tracks } = series.cup.clone();
        Some(proto::Cup {
            id,
            name,
            tracks,
            race: series.run as u32 + 1,
            points: POINTS.to_vec(),
            standings: series
                .standings()
                .into_iter()
                .map(|s| proto::Standing {
                    name: s.name,
                    bot: s.bot,
                    points: s.points,
                    places: s.places.iter().take(series.run).map(|p| p.map_or(0, |p| p + 1)).collect(),
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cup_goes_track_by_track_and_starts_again() {
        let (mut cups, first) = Cups::new("cup:grand-tour");
        assert_eq!(first.id, "paddock");
        let order = vec![("ana".to_owned(), false), ("bot-comet".to_owned(), true)];
        let mut raced = vec![first.id];
        for _ in 0..4 {
            cups.score(&order);
            if cups.just_over() {
                assert_eq!(cups.to_proto().unwrap().standings[0].points, 40);
            }
            raced.push(cups.advance(None).unwrap().id);
        }
        assert_eq!(raced, ["paddock", "coast", "forest", "harbour", "paddock"]);
        assert_eq!(cups.to_proto().unwrap().race, 1);
    }

    #[test]
    fn the_host_chooses_and_without_one_the_most_wanted_is_raced() {
        let (mut cups, first) = Cups::new("track:forest");
        assert_eq!((first.id.as_str(), cups.series.is_none()), ("forest", true));
        assert!(cups.advance(Some(1)).is_none(), "one track, race after race");
        cups.choose(2, "track:harbour");
        cups.choose(3, "track:harbour");
        cups.choose(1, "cup:grand-tour");
        cups.choose(4, "nonsense");
        assert_eq!(cups.votes().len(), 2);
        assert_eq!(cups.next(Some(1)).as_deref(), Some("cup:grand-tour"));
        assert_eq!(cups.next(None).as_deref(), Some("track:harbour"));
        assert_eq!(cups.advance(Some(1)).unwrap().id, "paddock");
        assert_eq!(cups.event, "cup:grand-tour");
        // Chosen and raced: nothing more to change.
        assert_eq!(cups.next(Some(1)), None);
        cups.forget(2);
        cups.forget(3);
        assert_eq!(cups.next(None), None);
    }
}
