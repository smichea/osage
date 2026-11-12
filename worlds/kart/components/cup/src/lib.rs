// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cups (G3.4, D-057): a few races on different tracks, points for each
//! finishing place, standings between the races and a podium at the end.
//! Nothing here knows of karts: a race's result is an order of names.
//!
//! Standings rank by points; a tie goes to the driver with more wins, then
//! more second places and so on, then to the better place in the latest
//! race; a driver who joins mid-cup starts from nothing.

use std::collections::BTreeMap;

/// Points for first to eighth; ninth and after score none.
pub const POINTS: [u32; 8] = [10, 8, 6, 5, 4, 3, 2, 1];

/// The points a finishing place (0 the winner) scores.
pub fn points(place: usize) -> u32 {
    POINTS.get(place).copied().unwrap_or(0)
}

/// A cup's definition: its name and its tracks, in the order they are raced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cup {
    pub id: String,
    pub name: String,
    pub tracks: Vec<String>,
}

/// One driver's cup so far.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Standing {
    pub name: String,
    pub bot: bool,
    pub points: u32,
    /// Their place in each race of the cup (0 the winner); none for a race
    /// they were not in.
    pub places: Vec<Option<u32>>,
}

/// A cup being raced: which race is next and the standings so far.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Series {
    pub cup: Cup,
    /// Races run so far.
    pub run: usize,
    standings: BTreeMap<String, Standing>,
}

impl Series {
    pub fn new(cup: Cup) -> Self {
        Self { cup, run: 0, standings: BTreeMap::new() }
    }

    /// The track of the race to run now, or none when the cup is over.
    pub fn track(&self) -> Option<&str> {
        self.cup.tracks.get(self.run).map(String::as_str)
    }

    /// Whether every race has been run.
    pub fn over(&self) -> bool {
        self.run >= self.cup.tracks.len()
    }

    /// The result of the race just run, fastest first: each driver's name
    /// and whether it is a bot. A name given twice counts once, at its
    /// first place.
    pub fn score(&mut self, order: &[(String, bool)]) {
        if self.over() {
            return;
        }
        let races = self.cup.tracks.len();
        let mut seen = Vec::new();
        for (name, bot) in order {
            if seen.contains(&name) {
                continue;
            }
            seen.push(name);
            let standing = self.standings.entry(name.clone()).or_insert_with(|| Standing {
                name: name.clone(),
                bot: *bot,
                points: 0,
                places: vec![None; races],
            });
            let at = seen.len() - 1;
            standing.points += points(at);
            standing.places[self.run] = Some(at as u32);
        }
        self.run += 1;
    }

    /// The standings, first first.
    pub fn standings(&self) -> Vec<Standing> {
        let mut table: Vec<Standing> = self.standings.values().cloned().collect();
        let latest = self.run.checked_sub(1);
        table.sort_by(|a, b| {
            let count = |s: &Standing| {
                let mut c = [0u32; POINTS.len()];
                for place in s.places.iter().flatten() {
                    if let Some(slot) = c.get_mut(*place as usize) {
                        *slot += 1;
                    }
                }
                c
            };
            let last = |s: &Standing| latest.and_then(|i| s.places[i]).unwrap_or(u32::MAX);
            b.points
                .cmp(&a.points)
                .then_with(|| count(b).cmp(&count(a)))
                .then_with(|| last(a).cmp(&last(b)))
                .then_with(|| a.name.cmp(&b.name))
        });
        table
    }
}

/// The cups that come with the game.
pub fn built_in() -> Vec<Cup> {
    vec![Cup {
        id: "grand-tour".to_owned(),
        name: "Grand Tour".to_owned(),
        tracks: ["paddock", "coast", "forest", "harbour"].map(str::to_owned).to_vec(),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cup(races: usize) -> Series {
        Series::new(Cup { id: "t".into(), name: "T".into(), tracks: (0..races).map(|i| format!("t{i}")).collect() })
    }

    fn order(names: &[&str]) -> Vec<(String, bool)> {
        names.iter().map(|n| (n.to_string(), n.starts_with("bot"))).collect()
    }

    #[test]
    fn places_score_points_and_the_cup_goes_track_by_track() {
        let mut series = cup(4);
        assert_eq!(series.track(), Some("t0"));
        series.score(&order(&["ana", "bob", "bot-1"]));
        assert_eq!(series.track(), Some("t1"));
        series.score(&order(&["bob", "ana", "bot-1"]));
        let table = series.standings();
        assert_eq!(table.iter().map(|s| s.points).collect::<Vec<_>>(), [18, 18, 12]);
        // A tie of points and of wins: bob was ahead in the latest race.
        assert_eq!(table[0].name, "bob");
        series.score(&order(&["ana", "bob"]));
        series.score(&order(&["ana", "bot-1", "bob"]));
        assert!(series.over());
        assert_eq!(series.track(), None);
        let table = series.standings();
        assert_eq!(table[0].name, "ana");
        assert_eq!(table[0].points, 38);
        assert_eq!(table[0].places, [Some(0), Some(1), Some(0), Some(0)]);
        // The cup is over: a fifth result changes nothing.
        series.score(&order(&["bot-1"]));
        assert_eq!(series.standings(), table);
    }

    #[test]
    fn more_wins_break_a_tie_and_a_late_joiner_starts_from_nothing() {
        let mut series = cup(3);
        // carl: 10 + 0; dee: 5 + 5 (two fourth places).
        series.score(&order(&["carl", "x1", "x2", "dee"]));
        series.score(&order(&["x1", "x2", "x3", "dee", "x4", "x5", "x6", "x7", "carl"]));
        let table = series.standings();
        let carl = table.iter().position(|s| s.name == "carl").unwrap();
        let dee = table.iter().position(|s| s.name == "dee").unwrap();
        assert_eq!((table[carl].points, table[dee].points), (10, 10));
        assert!(carl < dee, "a win beats two fourths");
        assert_eq!(table[carl].places[1], Some(8));
        series.score(&order(&["late"]));
        let late = series.standings().into_iter().find(|s| s.name == "late").unwrap();
        assert_eq!((late.points, late.places.clone()), (10, vec![None, None, Some(0)]));
    }

    #[test]
    fn a_name_twice_counts_once() {
        let mut series = cup(1);
        series.score(&order(&["ana", "ana", "bob"]));
        let table = series.standings();
        assert_eq!(table.len(), 2);
        assert_eq!((table[1].name.as_str(), table[1].points), ("bob", 8));
    }
}
