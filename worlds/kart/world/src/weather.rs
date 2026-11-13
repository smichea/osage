// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The weather of each race (V2.5, D-068): rain or not, rolled from the
//! track's chance of rain (`rain` in its file) and the race's number, the
//! same on every run. In the rain the road is wet — it grips and brakes
//! less ([`osage_kart_physics::Track::wet`]) — and `Race.rain` says so to
//! the clients, which predict on the same wet road. Time trials are run
//! dry, so that their ghosts compare; a world may also be always dry or
//! always wet.

use bevy::prelude::*;

use crate::{Course, cup::Cups, race::Race};

/// What the world's weather follows.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Weather {
    /// Each track's chance of rain, rolled for every race.
    #[default]
    Track,
    Dry,
    Rain,
}

/// The race's weather, as decided.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Forecast {
    pub rain: bool,
    /// The race and track it was decided for.
    race: u32,
}

/// Whether race `race` on track `id`, whose chance of rain is `chance`,
/// is run in the rain: a roll of dice that is the same everywhere.
pub fn rolled(id: &str, chance: f32, race: u32) -> bool {
    let track = id.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193));
    let mut h = race.wrapping_mul(0x9e37_79b9) ^ track;
    h = (h ^ (h >> 16)).wrapping_mul(0x85eb_ca6b);
    h = (h ^ (h >> 13)).wrapping_mul(0xc2b2_ae35);
    h ^= h >> 16;
    ((h >> 8) as f32 / (1u32 << 24) as f32) < chance
}

/// Decides each race's weather as it lines up, and wets the road.
pub fn forecast(
    weather: Res<Weather>,
    race: Res<Race>,
    cups: Res<Cups>,
    mut course: ResMut<Course>,
    mut forecast: ResMut<Forecast>,
) {
    if forecast.race == race.races() && !course.is_changed() {
        return;
    }
    forecast.race = race.races();
    let about = &course.0.about;
    let rain = !cups.trial()
        && match *weather {
            Weather::Track => rolled(&about.id, about.rain, race.races()),
            Weather::Dry => false,
            Weather::Rain => true,
        };
    forecast.rain = rain;
    let wet = if rain { 1.0 } else { 0.0 };
    if course.0.track.wet != wet {
        course.0.track.wet = wet;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rain_falls_as_often_as_the_track_says_and_the_same_everywhere() {
        let wet = (0..2000).filter(|race| rolled("coast", 0.35, *race)).count();
        assert!((600..800).contains(&wet), "about 35 %: {wet} of 2000");
        assert!((0..500).all(|race| !rolled("paddock", 0.0, race)), "never");
        assert!((0..500).all(|race| rolled("paddock", 1.0, race)), "always");
        let again: Vec<bool> = (0..50).map(|race| rolled("coast", 0.35, race)).collect();
        assert_eq!(again, (0..50).map(|race| rolled("coast", 0.35, race)).collect::<Vec<_>>());
        assert_ne!(
            (0..50).map(|race| rolled("coast", 0.5, race)).collect::<Vec<_>>(),
            (0..50).map(|race| rolled("harbour", 0.5, race)).collect::<Vec<_>>(),
            "each track its own weather"
        );
    }
}
