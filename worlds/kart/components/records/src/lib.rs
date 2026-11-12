// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A kart world's records (D-053): on each track, the best laps and the best
//! races of a given number of laps, one entry per driver name — that
//! driver's best — the fastest first, [`KEPT`] at most.
//!
//! The world keeps them; a board shared between worlds is a service an
//! operator may run from them, not this crate's business (mechanisms, not
//! policy). They are kept in a plain text file, one record a line, written
//! whole to a new file and renamed over the old one, so that a world
//! stopped while it saves loses at most the last record ([`Records::save`]).
//! A line that does not read is skipped, not fatal: a file edited by hand
//! keeps what it can.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    io,
    path::{Path, PathBuf},
};

/// Records kept per board.
pub const KEPT: usize = 10;
/// The first line of the file: its format.
pub const HEADER: &str = "kart-records/1";
/// Longest name kept, in characters.
const LONGEST_NAME: usize = 40;

/// What a record is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Event {
    /// One lap, from line to line.
    Lap,
    /// A whole race of this many laps, from the lights to the flag.
    Race { laps: u32 },
}

impl Event {
    fn label(self) -> String {
        match self {
            Event::Lap => "lap".to_owned(),
            Event::Race { laps } => format!("race:{laps}"),
        }
    }

    fn parse(label: &str) -> Option<Self> {
        match label.split_once(':') {
            None if label == "lap" => Some(Event::Lap),
            Some(("race", laps)) => laps.parse().ok().filter(|l| *l > 0).map(|laps| Event::Race { laps }),
            _ => None,
        }
    }
}

/// One time set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// Who drove: the name the world knows the driver by.
    pub name: String,
    /// The time, ms.
    pub ms: u64,
    /// When, seconds since 1970 (UTC).
    pub at: u64,
    /// The kart and the driver character chosen, empty until there are
    /// several to choose from.
    pub kart: String,
    pub driver: String,
}

/// Where a new time went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entered {
    /// Onto the board, at this place (0 the record), beating the driver's
    /// own best or its first time there.
    At(usize),
    /// Not: slower than the driver's best, or than all the board keeps.
    No,
}

/// Every board of a world.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Records {
    boards: BTreeMap<(String, Event), Vec<Record>>,
}

impl Records {
    /// The board of `event` on `track`, fastest first.
    pub fn board(&self, track: &str, event: Event) -> &[Record] {
        self.boards.get(&(track.to_owned(), event)).map_or(&[], Vec::as_slice)
    }

    /// Every board, by track and event.
    pub fn boards(&self) -> impl Iterator<Item = (&str, Event, &[Record])> {
        self.boards.iter().map(|((track, event), records)| (track.as_str(), *event, records.as_slice()))
    }

    /// A time set: kept if it beats the driver's best on that board and
    /// makes the top [`KEPT`]. A tie keeps the earlier time.
    pub fn offer(&mut self, track: &str, event: Event, record: Record) -> Entered {
        let record =
            Record { name: clean(&record.name), kart: clean(&record.kart), driver: clean(&record.driver), ..record };
        if record.name.is_empty() {
            return Entered::No;
        }
        let board = self.boards.entry((clean(track), event)).or_default();
        if let Some(own) = board.iter().position(|r| r.name == record.name) {
            if board[own].ms <= record.ms {
                return Entered::No;
            }
            board.remove(own);
        }
        let place = board.partition_point(|r| r.ms <= record.ms);
        if place >= KEPT {
            return Entered::No;
        }
        board.insert(place, record);
        board.truncate(KEPT);
        Entered::At(place)
    }

    /// The file's text: the header, then a record a line, tab-separated:
    /// track, event, ms, when, name, kart, driver.
    pub fn to_text(&self) -> String {
        let mut out = format!("{HEADER}\n");
        for ((track, event), records) in &self.boards {
            for r in records {
                let _ = writeln!(
                    out,
                    "{track}\t{}\t{}\t{}\t{}\t{}\t{}",
                    event.label(),
                    r.ms,
                    r.at,
                    r.name,
                    r.kart,
                    r.driver
                );
            }
        }
        out
    }

    /// Reads what [`to_text`](Self::to_text) wrote; lines that do not read
    /// are skipped, and so is a file of another format.
    pub fn from_text(text: &str) -> Self {
        let mut records = Self::default();
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some(HEADER) {
            return records;
        }
        for line in lines {
            let fields: Vec<&str> = line.split('\t').collect();
            let [track, event, ms, at, name, kart, driver] = fields[..] else { continue };
            let (Some(event), Ok(ms), Ok(at)) = (Event::parse(event), ms.parse(), at.parse()) else { continue };
            let record = Record { name: name.to_owned(), ms, at, kart: kart.to_owned(), driver: driver.to_owned() };
            records.offer(track, event, record);
        }
        records
    }

    /// The records in `path`; none if there is no such file.
    pub fn load(path: &Path) -> io::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(Self::from_text(&text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// Writes the records to `path`: to a file beside it first, then
    /// renamed over it, so that the file is always whole.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let mut new = PathBuf::from(path);
        new.as_mut_os_string().push(".new");
        std::fs::write(&new, self.to_text())?;
        std::fs::rename(&new, path)
    }
}

/// A name or label as the file keeps it: one line, no tabs, trimmed, short.
fn clean(text: &str) -> String {
    let one_line: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    one_line.trim().chars().take(LONGEST_NAME).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time(name: &str, ms: u64) -> Record {
        Record { name: name.to_owned(), ms, at: 1_794_000_000 + ms, kart: String::new(), driver: String::new() }
    }

    #[test]
    fn a_driver_keeps_its_best_and_the_board_is_fastest_first() {
        let mut records = Records::default();
        assert_eq!(records.offer("paddock", Event::Lap, time("alice", 31_000)), Entered::At(0));
        assert_eq!(records.offer("paddock", Event::Lap, time("bob", 30_500)), Entered::At(0));
        // Slower than her own best: not kept.
        assert_eq!(records.offer("paddock", Event::Lap, time("alice", 31_200)), Entered::No);
        // Faster: replaces it, and takes the record.
        assert_eq!(records.offer("paddock", Event::Lap, time("alice", 30_100)), Entered::At(0));
        let board = records.board("paddock", Event::Lap);
        assert_eq!(
            board.iter().map(|r| (r.name.as_str(), r.ms)).collect::<Vec<_>>(),
            [("alice", 30_100), ("bob", 30_500)]
        );
        // Laps and races, tracks, numbers of laps: separate boards.
        assert!(records.board("paddock", Event::Race { laps: 3 }).is_empty());
        assert!(records.board("harbour", Event::Lap).is_empty());
    }

    #[test]
    fn a_tie_keeps_the_earlier_time_and_the_board_keeps_ten() {
        let mut records = Records::default();
        records.offer("paddock", Event::Lap, time("first", 30_000));
        assert_eq!(records.offer("paddock", Event::Lap, time("second", 30_000)), Entered::At(1));
        for i in 0..20 {
            records.offer("paddock", Event::Lap, time(&format!("d{i}"), 31_000 + i));
        }
        let board = records.board("paddock", Event::Lap);
        assert_eq!(board.len(), KEPT);
        assert_eq!(board[0].name, "first");
        assert_eq!(records.offer("paddock", Event::Lap, time("slow", 40_000)), Entered::No);
        assert_eq!(records.offer("paddock", Event::Lap, time("quick", 30_500)), Entered::At(2));
        assert_eq!(records.board("paddock", Event::Lap).len(), KEPT);
    }

    #[test]
    fn records_read_back_as_written_and_survive_odd_names() {
        let mut records = Records::default();
        records.offer("paddock", Event::Lap, time("alice", 30_100));
        records.offer(
            "paddock",
            Event::Race { laps: 3 },
            Record { kart: "Sprint".into(), ..time("bob\tthe\nfast", 95_000) },
        );
        records.offer("harbour", Event::Race { laps: 5 }, time("Zoë ✨", 180_000));
        let text = records.to_text();
        assert!(text.starts_with("kart-records/1\n"));
        assert_eq!(Records::from_text(&text), records);
        assert_eq!(records.board("paddock", Event::Race { laps: 3 })[0].name, "bob the fast");
        // Lines that do not read are skipped; another format reads as none.
        let damaged = format!("{text}paddock\tlap\tnot a number\t0\tx\t\t\nharbour\tsprint\t1\t1\ty\t\t\n");
        assert_eq!(Records::from_text(&damaged), records);
        assert_eq!(Records::from_text("something else\npaddock\tlap\t1\t1\tx\t\t\n"), Records::default());
    }

    #[test]
    fn records_survive_a_restart() {
        let dir = std::env::temp_dir().join(format!("osage-records-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("records.txt");
        let _ = std::fs::remove_file(&path);
        assert_eq!(Records::load(&path).unwrap(), Records::default(), "no file yet: no records");
        let mut records = Records::default();
        records.offer("paddock", Event::Lap, time("alice", 30_100));
        records.save(&path).unwrap();
        records.offer("paddock", Event::Lap, time("alice", 29_900));
        records.save(&path).unwrap();
        let again = Records::load(&path).unwrap();
        assert_eq!(again.board("paddock", Event::Lap)[0].ms, 29_900);
        assert!(!dir.join("records.txt.new").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
