// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The world's records (D-053, [`osage_kart_records`]): every lap and every
//! race a human or an agent finishes is offered to the boards; what enters
//! is saved at once and published on the race board (`kart.records/1`),
//! marked fresh until the next race lines up. Bots set no records.

use std::{
    collections::BTreeSet,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use bevy::prelude::*;
use osage_kart_records::{Entered, Event, Record, Records};
use osage_world_module_api::Discrete;
use tracing::warn;

use crate::{Course, Ids, RaceBoard, Rules, proto, race::Race};

/// A time set this tick: who, a bot or not, a lap or a race, ms.
pub struct Timed {
    pub name: String,
    pub bot: bool,
    /// The driver and kart it was set with.
    pub pair: crate::roster::Pair,
    pub event: Event,
    pub ms: u64,
}

/// Times set this tick, from [`crate::race::progress`].
#[derive(Resource, Default)]
pub struct Times(pub Vec<Timed>);

#[derive(Resource)]
pub struct Book {
    records: Records,
    /// The file they are kept in; none, kept in memory only.
    path: Option<PathBuf>,
    /// Records set since the race lined up: (event, name).
    fresh: BTreeSet<(Event, String)>,
    /// The race they were set in.
    race: u32,
    /// The track they were last published for.
    published: Option<String>,
}

impl Book {
    /// The records in `path`, or none kept anywhere.
    pub fn open(path: Option<PathBuf>) -> Self {
        let records = match &path {
            Some(path) => Records::load(path).unwrap_or_else(|e| {
                warn!("records: {}: {e}; starting without", path.display());
                Records::default()
            }),
            None => Records::default(),
        };
        Self { records, path, fresh: BTreeSet::new(), race: 0, published: None }
    }
}

pub fn keep(
    rules: Res<Rules>,
    ids: Res<Ids>,
    race: Res<Race>,
    course: Res<Course>,
    mut times: ResMut<Times>,
    mut book: ResMut<Book>,
    mut board: Query<&mut Discrete, With<RaceBoard>>,
) {
    // The track the world races on, as its records name it (D-056).
    let track = course.0.about.id.as_str();
    let mut changed = book.published.as_deref() != Some(track);
    if book.race != race.races() {
        book.race = race.races();
        changed |= !book.fresh.is_empty();
        book.fresh.clear();
    }
    let at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let mut entered = false;
    for time in times.0.drain(..).filter(|t| !t.bot) {
        let (kart, driver) = (time.pair.1.name.to_owned(), time.pair.0.name.to_owned());
        let record = Record { name: time.name.clone(), ms: time.ms, at, kart, driver };
        if let Entered::At(_) = book.records.offer(track, time.event, record) {
            book.fresh.insert((time.event, time.name));
            entered = true;
        }
    }
    if entered
        && let Some(path) = &book.path
        && let Err(e) = book.records.save(path)
    {
        warn!("records: {}: {e}", path.display());
    }
    if (changed || entered)
        && let Ok(mut board) = board.single_mut()
    {
        board.set(ids.records, &to_proto(&book, track, rules.laps));
        book.published = Some(track.to_owned());
    }
}

fn to_proto(book: &Book, track: &str, laps: u32) -> proto::Records {
    let list = |event: Event| {
        book.records
            .board(track, event)
            .iter()
            .map(|r| proto::Record {
                name: r.name.clone(),
                ms: r.ms.min(u64::from(u32::MAX)) as u32,
                at: r.at,
                kart: r.kart.clone(),
                driver: r.driver.clone(),
                fresh: book.fresh.contains(&(event, r.name.clone())),
            })
            .collect()
    };
    proto::Records {
        track: track.to_owned(),
        laps: list(Event::Lap),
        races: list(Event::Race { laps }),
        race_laps: laps,
        trials: list(Event::Trial { laps }),
    }
}
