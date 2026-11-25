// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Direct transmission through acoustic surfaces between a contact and its receiver.
use crate::{object::Crossing, scene::Sector};
use osage_objects::BAND_HZ;
use osage_paris_soundscape::{scene::Scene, space::Vec3, ways::Face};
use osage_paris_voices::dsp::Biquad;
use serde::Serialize;

#[derive(Clone, Copy)]
pub(super) struct Geometry<'a> {
    pub object: Option<&'a crate::object::AcousticGeometry>,
    pub colliders: &'a [osage_dynamics::StaticMesh],
    pub bundled_bridge: bool,
}

impl<'a> From<&'a Sector> for Geometry<'a> {
    fn from(sector: &'a Sector) -> Self {
        Self {
            object: sector.object.as_ref().map(|o| &o.sound),
            colliders: &sector.colliders,
            bundled_bridge: sector.bundled_bridge,
        }
    }
}

#[derive(Serialize)]
pub(super) struct DirectPath {
    pub(super) crossings: Vec<Crossing>,
    candidates_tested: usize,
    pub(super) transmission_energy: [f64; 3],
}

impl DirectPath {
    pub(super) fn trace(sector: Geometry, bridge: &Scene, start: Vec3, end: Vec3, seconds: f64) -> Self {
        Self::trace_leg(sector, bridge, start, end, seconds, [None, None])
    }

    pub(super) fn trace_leg(
        sector: Geometry,
        bridge: &Scene,
        start: Vec3,
        end: Vec3,
        seconds: f64,
        ends: [Option<&crate::object::ExteriorWedge>; 2],
    ) -> Self {
        let (crossings, candidates_tested) = if let Some(object) = sector.object {
            object.sound_crossings(
                sector.colliders,
                start.array().map(|x| x as f32),
                end.array().map(|x| x as f32),
                ends,
            )
        } else if sector.bundled_bridge {
            let mut found = Vec::new();
            let mut tested = 0;
            for (index, surface) in bridge.surfaces.iter().enumerate().filter(|(_, s)| s.occludes) {
                let Some(material) = surface.acoustic else { continue };
                let face = Face::new(bridge.corners(index, seconds), material, surface.reflects, true);
                tested += 1;
                if let Some(point) = face.crossed(start, end) {
                    let distance_m = start.distance(point);
                    if distance_m >= 0.0001 && point.distance(end) >= 0.0001 {
                        found.push(Crossing {
                            shells: None,
                            id: surface.id.clone(),
                            owner_path: bridge.tree.path(surface.node).into_iter().map(str::to_owned).collect(),
                            material: None,
                            transmission: Some(material.transmission),
                            distance_m,
                        });
                    }
                }
            }
            found.sort_by(|a, b| {
                a.distance_m
                    .total_cmp(&b.distance_m)
                    .then_with(|| a.owner_path.cmp(&b.owner_path))
                    .then_with(|| a.id.cmp(&b.id))
            });
            (found, tested)
        } else {
            (Vec::new(), 0)
        };
        let mut transmission_energy = [1.; 3];
        for material in crossings.iter().filter_map(|c| c.transmission) {
            for (energy, fraction) in transmission_energy.iter_mut().zip(material) {
                *energy *= fraction;
            }
        }
        Self { crossings, candidates_tested, transmission_energy }
    }
}

// Two cascaded fourth-order Butterworth filters make an eighth-order
// Linkwitz–Riley branch. Low and high branches share phase and sum to unit magnitude.
struct Branch([Biquad; 4]);

impl Branch {
    fn new(crossover: f32, low: bool) -> Self {
        Self(std::array::from_fn(|i| {
            let pole = if i % 2 == 0 { 1. } else { 3. };
            let q = 0.5 / (pole * std::f32::consts::PI / 8.).cos();
            let mut section = Biquad::default();
            if low {
                section.low_pass(crossover, q);
            } else {
                section.high_pass(crossover, q);
            }
            section
        }))
    }

    fn pass(&mut self, x: f32) -> f32 {
        self.0.iter_mut().fold(x, |signal, section| section.pass(signal))
    }
}

struct ThreeBands {
    gains: [f32; 3],
    lower_low: Branch,
    lower_high: Branch,
    upper_low: Branch,
    upper_high: Branch,
}

pub(super) struct Transmission {
    gain: f32,
    bands: Option<Box<ThreeBands>>,
}

impl Transmission {
    pub(super) fn new(energy: [f64; 3]) -> Self {
        let gains = energy.map(|e| e.sqrt() as f32);
        let [low, mid, high] = BAND_HZ.map(|hz| hz as f32);
        let bands = (gains[0] != gains[1] || gains[1] != gains[2]).then(|| {
            let lower = (low * mid).sqrt();
            let upper = (mid * high).sqrt();
            Box::new(ThreeBands {
                gains,
                lower_low: Branch::new(lower, true),
                lower_high: Branch::new(lower, false),
                upper_low: Branch::new(upper, true),
                upper_high: Branch::new(upper, false),
            })
        });
        Self { gain: gains[0], bands }
    }

    pub(super) fn apply(&mut self, samples: &mut [f32]) {
        if let Some(b) = &mut self.bands {
            for x in samples {
                let lo = b.lower_low.pass(*x);
                let hi = b.lower_high.pass(*x);
                // Both contributions pass through both crossovers so their phase agrees.
                *x = b.upper_low.pass(b.gains[0] * lo + b.gains[1] * hi) + b.upper_high.pass(b.gains[2] * (lo + hi));
            }
        } else if self.gain != 1. {
            for x in samples {
                *x *= self.gain;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use osage_paris_voices::RATE;

    #[test]
    fn pressure_follows_the_three_energy_bands_and_filter_state_survives_packet_boundaries() {
        for energy in [[0.04, 0.0025, 0.0001], [1., 0., 1.], [0., 1., 0.], [0.25; 3], [0.; 3]] {
            for (band, hz) in BAND_HZ.into_iter().enumerate() {
                let rate = RATE as usize;
                let signal: Vec<_> = (0..rate / 4)
                    .map(|i| (std::f64::consts::TAU * f64::from(hz) * i as f64 / rate as f64).sin() as f32)
                    .collect();
                let mut whole = signal.clone();
                Transmission::new(energy).apply(&mut whole);
                let mut chunks = signal.clone();
                let mut filter = Transmission::new(energy);
                for block in chunks.chunks_mut(47) {
                    filter.apply(block);
                }
                assert_eq!(whole, chunks);
                let rms2 = |x: &[f32]| x.iter().map(|&p| f64::from(p).powi(2)).sum::<f64>();
                let gain = (rms2(&whole[rate / 20..]) / rms2(&signal[rate / 20..])).sqrt();
                let target = energy[band].sqrt();
                assert!((gain - target).abs() < 0.009, "{hz} Hz: {gain} instead of {target}");
                if target > 0. && energy == [0.04, 0.0025, 0.0001] {
                    assert!((20. * (gain / target).log10()).abs() < 0.2);
                }
            }
        }
    }
}
