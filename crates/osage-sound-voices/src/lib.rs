// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Voices for osage-sound, each made sample by sample from the state of
//! what makes it ([`Drive`]): synthesised, or played from recordings a
//! world gives ([`recorded`]). First those of a Paris bridge:
//!
//! - **the Seine** ([`river`]): water washing round a pier, lapping, bubbles;
//! - **the road** ([`road`]): a car's tyres and engine, a scooter (the
//!   kart's two-stroke, `osage-kart-sound`);
//! - **the metro** ([`metro`]): a rubber-tyred bogie rolling, a traction
//!   motor, a joint of the runway struck, a span of the steel viaduct
//!   rumbling under the train;
//! - **what is around** ([`ambient`]): the listener's own footsteps, the
//!   city far off;
//! - **what touches** ([`contact`]): a blow that sets ringing what was
//!   struck — a sole on asphalt, a tyre on a steel plate;
//! - **recordings** ([`recorded`]): loops that follow a state and one-shots
//!   for blows, over a synthesised layer, at levels of detail for afar.
//!
//! A voice gives the sound pressure one metre from its emitter; where it is
//! heard from, and through what, is not its business. Deterministic: a
//! seeded noise, no clock (as `osage-kart-sound`).

pub mod ambient;
pub mod contact;
pub mod dsp;
pub mod metro;
pub mod recorded;
pub mod reference;
pub mod river;
pub mod road;

pub use dsp::{NOMINAL_RMS, RATE};

/// What a voice follows while it sounds, given once a block of samples.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drive {
    /// Speed of what carries the emitter along its way, m/s.
    pub speed: f32,
    /// Its acceleration along its way, m/s²; negative when braking.
    pub accel: f32,
    /// How much it is loaded or excited, 0…1: a span of the viaduct by the
    /// bogies on it, water by the current.
    pub load: f32,
    /// Something struck it during this block: a wheel over a joint.
    pub hit: Option<Hit>,
    /// How much of its detail is worth making, 0…1: 1 heard close and alone,
    /// less far off or under louder sounds. A voice may then make a cheaper
    /// sound of the same level and colour (fewer bubbles, fewer partials);
    /// it must not click when this changes.
    pub detail: f32,
    /// The highest frequency still worth making, Hz: what the air has not
    /// taken on the way to the listener. A voice may make less above it.
    pub top: f32,
}

impl Default for Drive {
    fn default() -> Self {
        Self { speed: 0.0, accel: 0.0, load: 0.0, hit: None, detail: 1.0, top: RATE as f32 / 2.0 }
    }
}

/// A blow: when in the block, in samples from its start, and how strong, 0…1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub at: usize,
    pub strength: f32,
}

/// A sound made from its state.
pub trait Voice: Send {
    /// The next `out.len()` samples at [`RATE`]: the pressure one metre from
    /// the emitter, an RMS of [`NOMINAL_RMS`] when driven as the voice's
    /// documentation says. Every sample is finite and within −1…1.
    fn render(&mut self, drive: &Drive, out: &mut [f32]);
}

/// The numbers a scene gives a voice, by name; a voice has a default for
/// each.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Params(Vec<(String, f32)>);

impl Params {
    pub fn with(mut self, name: &str, value: f32) -> Self {
        self.set(name, value);
        self
    }

    pub fn set(&mut self, name: &str, value: f32) {
        match self.0.iter_mut().find(|(n, _)| n == name) {
            Some(entry) => entry.1 = value,
            None => self.0.push((name.to_owned(), value)),
        }
    }

    /// The value given for `name`, or `default` when none is given — or
    /// when the one given is not a finite number: a scene is outside data,
    /// and a voice promises finite samples.
    pub fn get(&self, name: &str, default: f32) -> f32 {
        self.0
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, value)| *value)
            .filter(|value| value.is_finite())
            .unwrap_or(default)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|(name, _)| name.as_str())
    }
}

/// A kind of voice a scene can name.
pub struct Kind {
    pub name: &'static str,
    pub about: &'static str,
    /// The parameters it reads, each with its default.
    pub params: &'static [(&'static str, f32)],
    make: fn(u32, &Params) -> Box<dyn Voice>,
}

/// Every kind of voice, by the name a scene gives it.
pub const KINDS: &[Kind] = &[
    Kind {
        name: "water",
        about: river::ABOUT,
        params: river::PARAMS,
        make: |seed, p| Box::new(river::Water::new(seed, p)),
    },
    Kind { name: "car", about: road::CAR, params: road::CAR_PARAMS, make: |seed, p| Box::new(road::Car::new(seed, p)) },
    Kind {
        name: "scooter",
        about: road::SCOOTER,
        params: road::SCOOTER_PARAMS,
        make: |seed, p| Box::new(road::Scooter::new(seed, p)),
    },
    Kind {
        name: "metro-bogie",
        about: metro::BOGIE,
        params: metro::BOGIE_PARAMS,
        make: |seed, p| Box::new(metro::Bogie::new(seed, p)),
    },
    Kind {
        name: "metro-motor",
        about: metro::MOTOR,
        params: metro::MOTOR_PARAMS,
        make: |seed, p| Box::new(metro::Motor::new(seed, p)),
    },
    Kind {
        name: "metro-joint",
        about: metro::JOINT,
        params: metro::JOINT_PARAMS,
        make: |seed, p| Box::new(metro::Joint::new(seed, p)),
    },
    Kind {
        name: "metro-span",
        about: metro::SPAN,
        params: metro::SPAN_PARAMS,
        make: |seed, p| Box::new(metro::Span::new(seed, p)),
    },
    Kind {
        name: "footsteps",
        about: ambient::FOOTSTEPS,
        params: ambient::FOOTSTEPS_PARAMS,
        make: |seed, p| Box::new(ambient::Footsteps::new(seed, p)),
    },
    Kind {
        name: "city-bed",
        about: ambient::CITY,
        params: ambient::CITY_PARAMS,
        make: |seed, p| Box::new(ambient::CityBed::new(seed, p)),
    },
    Kind {
        name: "impact",
        about: contact::ABOUT,
        params: contact::PARAMS,
        make: |seed, p| Box::new(contact::Impact::new(seed, p)),
    },
    Kind {
        name: "tone",
        about: reference::TONE,
        params: reference::TONE_PARAMS,
        make: |seed, p| Box::new(reference::Tone::new(seed, p)),
    },
    Kind {
        name: "noise",
        about: reference::NOISE,
        params: reference::NOISE_PARAMS,
        make: |seed, p| Box::new(reference::Hiss::new(seed, p)),
    },
];

/// The kind named `name`.
pub fn kind(name: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|kind| kind.name == name)
}

/// A voice of the kind `name`; `seed` keeps two of a kind from sounding
/// alike.
pub fn make(name: &str, seed: u32, params: &Params) -> Option<Box<dyn Voice>> {
    kind(name).map(|kind| (kind.make)(seed, params))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ten seconds of a kind, driven as its documentation says.
    fn ten_seconds(name: &str, seed: u32) -> Vec<f32> {
        let mut voice = make(name, seed, &Params::default()).unwrap();
        let mut out = vec![0.0; RATE as usize * 10];
        let drive = nominal(name);
        for (i, block) in out.chunks_mut(128).enumerate() {
            // A joint is struck twice a second.
            let hit =
                (matches!(name, "metro-joint" | "impact") && i % 188 == 0).then_some(Hit { at: 5, strength: 1.0 });
            voice.render(&Drive { hit, ..drive }, block);
        }
        out
    }

    /// The drive each kind's level is set at.
    pub(crate) fn nominal(name: &str) -> Drive {
        match name {
            "car" | "scooter" => Drive { speed: 30.0 / 3.6, load: 0.3, ..Drive::default() },
            "metro-bogie" | "metro-motor" => Drive { speed: 40.0 / 3.6, load: 0.5, ..Drive::default() },
            "metro-span" => Drive { speed: 40.0 / 3.6, load: 1.0, ..Drive::default() },
            "footsteps" => Drive { speed: 1.3, ..Drive::default() },
            _ => Drive { load: 0.5, ..Drive::default() },
        }
    }

    #[test]
    fn a_parameter_that_is_not_a_finite_number_is_its_default() {
        // Each parameter of each kind given as NaN, as +∞ and as −∞: the voice is the one with its default,
        // sample for sample.
        for kind in KINDS {
            let alike = ten_seconds(kind.name, 6);
            for &(name, _) in kind.params {
                for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                    let mut voice = make(kind.name, 6, &Params::default().with(name, bad)).unwrap();
                    let mut out = vec![0.0; RATE as usize * 10];
                    let drive = nominal(kind.name);
                    for (i, block) in out.chunks_mut(128).enumerate() {
                        let hit = (matches!(kind.name, "metro-joint" | "impact") && i % 188 == 0)
                            .then_some(Hit { at: 5, strength: 1.0 });
                        voice.render(&Drive { hit, ..drive }, block);
                    }
                    assert!(out == alike, "{} with {name} = {bad}", kind.name);
                }
            }
        }
    }

    #[test]
    fn every_kind_sounds_within_bounds_and_the_same_twice() {
        for kind in KINDS {
            let (a, b) = (ten_seconds(kind.name, 6), ten_seconds(kind.name, 6));
            assert!(a.iter().all(|s| s.is_finite() && s.abs() <= 1.0), "{}: out of bounds", kind.name);
            assert!(a == b, "{}: not the same twice", kind.name);
            let rms = (a.iter().map(|s| s * s).sum::<f32>() / a.len() as f32).sqrt();
            assert!(rms > 0.003, "{}: silent ({rms})", kind.name);
            // A pure tone is the same whatever its seed.
            assert!(kind.name == "tone" || ten_seconds(kind.name, 7) != a, "{}: the seed changes nothing", kind.name);
        }
    }

    #[test]
    fn parameters_have_defaults_and_keep_what_is_set() {
        let params = Params::default().with("level", 0.5);
        assert_eq!(params.get("level", 1.0), 0.5);
        assert_eq!(params.get("absent", 1.0), 1.0);
        assert_eq!(params.with("level", 0.25).get("level", 1.0), 0.25);
        assert!(make("no such voice", 1, &Params::default()).is_none());
    }
}
