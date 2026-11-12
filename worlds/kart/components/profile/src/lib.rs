// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A kart player's profile (design/04 G4.6, D-064), kept on their own
//! computer: the driver, kart and paint they race with, and what they have
//! won — points in cups, cups raced, won, on the podium. Points unlock
//! paints; nothing else, and nothing bought: a paint is how a kart looks,
//! never how it drives.
//!
//! The eight team colours are everyone's from the start; four more are
//! won ([`SPECIAL`]). The file is text, `key = value` a line; a line that
//! does not read is skipped, a file of another format reads as a new
//! profile.

use std::fmt::Write as _;

/// The first line of the file: its format.
pub const HEADER: &str = "kart-profile/1";
/// Paints there are: the eight team colours, then the special ones.
pub const PAINTS: u32 = 12;
/// The team colours: everyone's.
pub const TEAM: u32 = 8;
/// The special paints and the points that unlock each, in order.
pub const SPECIAL: [(&str, u32); 4] = [("Silver", 20), ("Pearl", 50), ("Midnight", 90), ("Gold", 140)];
/// The team colours' names, in the palette's order.
const TEAM_NAMES: [&str; 8] = ["Red", "Blue", "Green", "Amber", "Violet", "Teal", "Orange", "Pink"];
/// Longest id kept, in characters.
const LONGEST: usize = 40;

/// A paint's name; `None` for one there is not.
pub fn paint_name(paint: u32) -> Option<&'static str> {
    match paint {
        p if p < TEAM => Some(TEAM_NAMES[p as usize]),
        p => SPECIAL.get((p - TEAM) as usize).map(|(name, _)| *name),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    /// The driver and the kart body chosen last (roster ids); empty, none yet.
    pub driver: String,
    pub kart: String,
    /// The paint chosen; none, the driver's own colour.
    pub paint: Option<u32>,
    /// Points won in cups, all told.
    pub points: u32,
    /// Cups raced to the end, won, and finished in the first three.
    pub cups: u32,
    pub wins: u32,
    pub podiums: u32,
}

/// A paint unlocked by a cup just counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unlocked(pub u32);

impl Profile {
    /// Whether `paint` is the player's to use.
    pub fn has(&self, paint: u32) -> bool {
        paint < TEAM || SPECIAL.get((paint - TEAM) as usize).is_some_and(|(_, needs)| self.points >= *needs)
    }

    /// Every paint the player may use, in the palette's order.
    pub fn paints(&self) -> Vec<u32> {
        (0..PAINTS).filter(|p| self.has(*p)).collect()
    }

    /// The next paint to win and the points it needs, if one is left.
    pub fn next(&self) -> Option<(u32, u32)> {
        SPECIAL
            .iter()
            .enumerate()
            .find(|(_, (_, needs))| self.points < *needs)
            .map(|(i, (_, needs))| (TEAM + i as u32, *needs))
    }

    /// The paint after the one chosen, among those the player has: none
    /// (the driver's colour) first, then each; round again.
    pub fn next_paint(&self) -> Option<u32> {
        let paints = self.paints();
        match self.paint.and_then(|p| paints.iter().position(|q| *q == p)) {
            None => paints.first().copied(),
            Some(at) => paints.get(at + 1).copied(),
        }
    }

    /// A cup raced to the end: `points` won in it and the place in its
    /// standings (0 the winner). The paints it unlocked.
    pub fn count_cup(&mut self, points: u32, place: usize) -> Vec<Unlocked> {
        let had = self.paints();
        self.points = self.points.saturating_add(points);
        self.cups += 1;
        self.wins += u32::from(place == 0);
        self.podiums += u32::from(place < 3);
        self.paints().into_iter().filter(|p| !had.contains(p)).map(Unlocked).collect()
    }

    /// The file's text.
    pub fn to_text(&self) -> String {
        let mut out = format!("{HEADER}\n");
        let _ = writeln!(out, "driver = {}", clean(&self.driver));
        let _ = writeln!(out, "kart = {}", clean(&self.kart));
        let _ = writeln!(out, "paint = {}", self.paint.map_or("driver".to_owned(), |p| p.to_string()));
        let _ = writeln!(out, "points = {}", self.points);
        let _ = writeln!(out, "cups = {}", self.cups);
        let _ = writeln!(out, "wins = {}", self.wins);
        let _ = writeln!(out, "podiums = {}", self.podiums);
        out
    }

    /// Reads what [`to_text`](Self::to_text) wrote; what does not read is
    /// skipped, another format reads as a new profile.
    pub fn from_text(text: &str) -> Self {
        let mut profile = Self::default();
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some(HEADER) {
            return profile;
        }
        for line in lines {
            let Some((key, value)) = line.split_once('=') else { continue };
            let (key, value) = (key.trim(), value.trim());
            let number = value.parse::<u32>().ok();
            match (key, number) {
                ("driver", _) => profile.driver = clean(value),
                ("kart", _) => profile.kart = clean(value),
                ("paint", Some(p)) if p < PAINTS => profile.paint = Some(p),
                ("paint", _) => profile.paint = None,
                ("points", Some(n)) => profile.points = n,
                ("cups", Some(n)) => profile.cups = n,
                ("wins", Some(n)) => profile.wins = n,
                ("podiums", Some(n)) => profile.podiums = n,
                _ => {}
            }
        }
        // A paint written by hand that is not won yet is not the player's.
        profile.paint = profile.paint.filter(|p| profile.has(*p));
        profile
    }
}

/// An id as the file keeps it: one line, trimmed, short.
fn clean(text: &str) -> String {
    let one_line: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    one_line.trim().chars().take(LONGEST).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_colours_are_everyones_and_points_unlock_the_rest() {
        let mut profile = Profile::default();
        assert_eq!(profile.paints(), (0..8).collect::<Vec<_>>());
        assert_eq!(profile.next(), Some((8, 20)));
        assert!(profile.count_cup(12, 4).is_empty(), "not enough yet");
        assert_eq!(profile.count_cup(40, 0), [Unlocked(8), Unlocked(9)], "52 points: silver and pearl");
        assert_eq!((profile.points, profile.cups, profile.wins, profile.podiums), (52, 2, 1, 1));
        assert!(profile.has(9) && !profile.has(10) && !profile.has(12));
        assert_eq!(profile.next(), Some((10, 90)));
        assert_eq!(paint_name(9), Some("Pearl"));
        assert_eq!(paint_name(12), None);
        profile.count_cup(200, 1);
        assert_eq!(profile.next(), None, "everything won");
    }

    #[test]
    fn the_paint_goes_round_what_the_player_has() {
        let mut profile = Profile { points: 25, ..Profile::default() };
        let mut seen = Vec::new();
        for _ in 0..11 {
            profile.paint = profile.next_paint();
            seen.push(profile.paint);
        }
        let expected: Vec<Option<u32>> = (0..9).map(Some).chain([None, Some(0)]).collect();
        assert_eq!(seen, expected, "the team colours, silver, then the driver's own again");
    }

    #[test]
    fn a_profile_reads_back_as_written_and_damage_is_skipped() {
        let profile = Profile {
            driver: "juno".into(),
            kart: "drifter".into(),
            paint: Some(8),
            points: 33,
            cups: 3,
            wins: 1,
            podiums: 2,
        };
        let text = profile.to_text();
        assert!(text.starts_with("kart-profile/1\ndriver = juno\n"));
        assert_eq!(Profile::from_text(&text), profile);
        let damaged = text.replace("points = 33", "points = lots").replace("kart = drifter", "kart\tdrifter");
        let read = Profile::from_text(&damaged);
        assert_eq!((read.points, read.kart.as_str(), read.paint), (0, "", None), "silver needs points it lost");
        assert_eq!(Profile::from_text("something else\npoints = 9\n"), Profile::default());
        assert_eq!(Profile::from_text(&text.replace("paint = 8", "paint = driver")).paint, None);
    }
}
