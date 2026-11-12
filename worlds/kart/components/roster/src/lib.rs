// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Who drives what in the kart world (D-055): eight original drivers in
//! three weight classes, four kart bodies, and the [`Build`] that a driver
//! in a body gives a kart — top speed, acceleration, handling, grip and
//! weight as shares of the standard kart's.
//!
//! A flavour, not a ladder: what a build gains in one share it gives in
//! another, so that every pair laps the paddock circuit within a few
//! tenths of the others (the world's tests). The light accelerate and are
//! pushed about; the heavy hold their line and are slow to get going; a
//! sprint body leaps out of bends, a cruiser holds speed, a drifter turns
//! in and slides.

use osage_kart_physics::Build;

/// A driver's weight class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    Light,
    Medium,
    Heavy,
}

impl Class {
    pub fn name(self) -> &'static str {
        match self {
            Class::Light => "light",
            Class::Medium => "medium",
            Class::Heavy => "heavy",
        }
    }

    /// What a driver of this class does to a build.
    pub fn build(self) -> Build {
        match self {
            Class::Light => Build { top: 0.993, accel: 1.1, handling: 1.03, grip: 1.0, weight: 0.85 },
            Class::Medium => Build::STANDARD,
            Class::Heavy => Build { top: 1.007, accel: 0.9, handling: 0.97, grip: 1.03, weight: 1.2 },
        }
    }
}

/// A driver: an original character of the kart world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Driver {
    /// How the protocol names it.
    pub id: &'static str,
    pub name: &'static str,
    pub class: Class,
    /// The colour of its kart and suit, an index into the clients' palette.
    pub colour: u32,
    /// In a line, for the selection screen.
    pub about: &'static str,
}

/// A kart body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub id: &'static str,
    pub name: &'static str,
    pub build: Build,
    pub about: &'static str,
}

/// The eight drivers, the default first.
pub const DRIVERS: [Driver; 8] = [
    Driver { id: "nova", name: "Nova", class: Class::Medium, colour: 0, about: "steady, never far from the front" },
    Driver { id: "pixel", name: "Pixel", class: Class::Light, colour: 1, about: "quick off the line, easily pushed" },
    Driver { id: "bolt", name: "Bolt", class: Class::Heavy, colour: 2, about: "hard to move, slow to get going" },
    Driver { id: "comet", name: "Comet", class: Class::Medium, colour: 3, about: "an old hand at every bend" },
    Driver { id: "zippy", name: "Zippy", class: Class::Light, colour: 4, about: "darts through any gap" },
    Driver { id: "echo", name: "Echo", class: Class::Light, colour: 5, about: "light and careful" },
    Driver { id: "flash", name: "Flash", class: Class::Heavy, colour: 6, about: "all speed down the straights" },
    Driver { id: "juno", name: "Juno", class: Class::Medium, colour: 7, about: "even-tempered, any kart suits" },
];

/// The four bodies, the default first.
pub const BODIES: [Body; 4] = [
    Body { id: "classic", name: "Classic", build: Build::STANDARD, about: "the club kart: no weakness" },
    Body {
        id: "sprint",
        name: "Sprint",
        build: Build { top: 0.99, accel: 1.15, handling: 1.04, grip: 1.0, weight: 0.94 },
        about: "leaps out of bends, gives a little at the top",
    },
    Body {
        id: "cruiser",
        name: "Cruiser",
        build: Build { top: 1.01, accel: 0.9, handling: 0.94, grip: 1.06, weight: 1.08 },
        about: "holds its speed and its line, slow to wake",
    },
    Body {
        id: "drifter",
        name: "Drifter",
        build: Build { top: 0.998, accel: 1.0, handling: 1.1, grip: 0.86, weight: 1.0 },
        about: "turns in sharply and slides: drift it",
    },
];

/// The driver of that id, or none.
pub fn driver(id: &str) -> Option<&'static Driver> {
    DRIVERS.iter().find(|d| d.id == id)
}

/// The body of that id, or none.
pub fn body(id: &str) -> Option<&'static Body> {
    BODIES.iter().find(|b| b.id == id)
}

/// The build of `driver` in `body`: their shares multiplied.
pub fn build(driver: &Driver, body: &Body) -> Build {
    let (d, b) = (driver.class.build(), body.build);
    Build {
        top: d.top * b.top,
        accel: d.accel * b.accel,
        handling: d.handling * b.handling,
        grip: d.grip * b.grip,
        weight: d.weight * b.weight,
    }
    .sanitized()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_the_defaults_are_standard() {
        for (i, a) in DRIVERS.iter().enumerate() {
            assert!(DRIVERS[i + 1..].iter().all(|b| b.id != a.id && b.colour != a.colour));
            assert_eq!(driver(a.id), Some(a));
        }
        for (i, a) in BODIES.iter().enumerate() {
            assert!(BODIES[i + 1..].iter().all(|b| b.id != a.id));
            assert_eq!(body(a.id), Some(a));
        }
        assert_eq!(build(&DRIVERS[0], &BODIES[0]), Build::STANDARD);
        assert!(driver("mario").is_none() && body("").is_none());
    }

    #[test]
    fn every_class_is_in_the_roster_and_builds_stay_within_reason() {
        for class in [Class::Light, Class::Medium, Class::Heavy] {
            assert!(DRIVERS.iter().filter(|d| d.class == class).count() >= 2, "{class:?}");
        }
        for d in &DRIVERS {
            for b in &BODIES {
                let built = build(d, b);
                assert_eq!(built, built.sanitized(), "{} in a {}", d.name, b.name);
            }
        }
        // The light are lighter and quicker off the line than the heavy.
        let (light, heavy) = (build(&DRIVERS[1], &BODIES[0]), build(&DRIVERS[2], &BODIES[0]));
        assert!(light.weight < heavy.weight && light.accel > heavy.accel && light.top < heavy.top);
    }
}
