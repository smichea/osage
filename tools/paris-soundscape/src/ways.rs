// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The ways sound takes from an emitter to a head, found from the faces of
//! the objects between and around them:
//!
//! - **straight**, when nothing hides the emitter;
//! - **round an edge**, when a face hides it: over the edge that lengthens
//!   the way least, quieter and duller by how much longer the way is, counted
//!   in wavelengths (the Fresnel number) — and already a little quieter when
//!   the straight way only grazes an edge, so that nothing switches as an
//!   emitter goes behind a deck;
//! - **mirrored** by each face that reflects, from the emitter's image
//!   behind it, less what the face's material takes in each band; what the
//!   material scatters goes to the reverberation instead.
//!
//! and what the air takes on the way (ISO 9613-1).
//!
//! Sources: J. Allen and D. Berkley, "Image method for efficiently simulating
//! small-room acoustics" (JASA 65(4), 1979); Z. Maekawa, "Noise reduction by
//! screens" (Applied Acoustics 1, 1968) in the piecewise form that continues
//! into the lit side; ISO 9613-1:1993.

use crate::{scene::Material, space::Vec3};

/// The speed of sound, m/s, as `osage-kart-native` takes it.
pub const SOUND: f64 = 343.0;

/// The bands a way is shaped in, Hz: the object format's three, and the top
/// of the spectrum, which the air and edges take first.
pub const BANDS: [f64; 4] = [250.0, 1_000.0, 4_000.0, 9_000.0];

/// A reflection fades out over this many metres as its mirror point nears
/// the rim of its face.
const RIM: f64 = 0.5;

/// A flat face in the world.
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub corners: [Vec3; 4],
    pub normal: Vec3,
    /// For each side, the direction into the face, in its plane.
    inward: [Vec3; 4],
    pub material: Material,
    pub reflects: bool,
    pub occludes: bool,
}

impl Face {
    pub fn new(corners: [Vec3; 4], material: Material, reflects: bool, occludes: bool) -> Face {
        let normal = (corners[1] - corners[0]).cross(corners[3] - corners[0]).unit_or(Vec3::Y);
        let centre = (corners[0] + corners[1] + corners[2] + corners[3]) / 4.0;
        let inward = std::array::from_fn(|i| {
            let side = corners[(i + 1) % 4] - corners[i];
            let across = normal.cross(side).unit_or(Vec3::X);
            // Whichever way round the corners are given.
            if (centre - corners[i]).dot(across) < 0.0 { -across } else { across }
        });
        Face { corners, normal, inward, material, reflects, occludes }
    }

    /// How far above the face's plane a point is, m, along its normal.
    pub fn height(&self, p: Vec3) -> f64 {
        (p - self.corners[0]).dot(self.normal)
    }

    /// How far inside the face a point of its plane is, m: the distance to
    /// the nearest side, negative outside.
    pub fn inside(&self, p: Vec3) -> f64 {
        (0..4).map(|i| (p - self.corners[i]).dot(self.inward[i])).fold(f64::INFINITY, f64::min)
    }

    /// Where the straight line from `a` to `b` goes through the face, if it
    /// does strictly between them.
    pub fn crossed(&self, a: Vec3, b: Vec3) -> Option<Vec3> {
        let (ha, hb) = (self.height(a), self.height(b));
        if ha * hb >= 0.0 || ha.abs() < 1e-7 || hb.abs() < 1e-7 {
            return None;
        }
        let p = a + (b - a) * (ha / (ha - hb));
        (self.inside(p) > 0.0).then_some(p)
    }

    /// The point of the face nearest to a point of its plane.
    fn nearest(&self, mut p: Vec3) -> Vec3 {
        for _ in 0..2 {
            for i in 0..4 {
                let out = (p - self.corners[i]).dot(self.inward[i]);
                if out < 0.0 {
                    p = p - self.inward[i] * out;
                }
            }
        }
        p
    }
}

/// One way from an emitter to the head.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Way {
    /// How long it is, m.
    pub length: f64,
    /// The point it reaches the head from: the emitter, an edge, a mirror
    /// point.
    pub from: Vec3,
    /// What is left of the pressure in each of [`BANDS`], distance and air
    /// apart.
    pub bands: [f64; 4],
}

/// The ways from one emitter.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ways {
    /// Straight or round an edge.
    pub direct: Option<Way>,
    /// By each face, in the faces' order; `None` where a face mirrors
    /// nothing to the head.
    pub mirrored: Vec<Option<Way>>,
    /// The energy the faces scatter towards the head, of what the emitter
    /// sends at one metre: what feeds the reverberation.
    pub scattered: f64,
}

/// What an edge takes, dB, for a way longer by `detour` metres than the
/// straight one, at `frequency`; a negative detour is a straight way that
/// passes that close to an edge without being hidden.
pub fn edge_loss(detour: f64, frequency: f64) -> f64 {
    // The Fresnel number: the detour in half wavelengths.
    let n = 2.0 * detour * frequency / SOUND;
    let loss = if n < -0.25 {
        0.0
    } else if n < 0.0 {
        6.0 - 12.0 * (-n).sqrt()
    } else if n < 0.25 {
        6.0 + 12.0 * n.sqrt()
    } else if n < 1.0 {
        8.0 + 8.0 * n.sqrt()
    } else {
        16.0 + 10.0 * n.log10()
    };
    // One edge takes 20 dB at most (ISO 9613-2).
    loss.min(20.0)
}

/// The point of the side from `a` to `b` by which the way from `source` to
/// `head` is shortest.
fn apex(a: Vec3, b: Vec3, source: Vec3, head: Vec3) -> Vec3 {
    let length = a.distance(b);
    let along = (b - a) / length.max(1e-12);
    let (zs, zh) = ((source - a).dot(along), (head - a).dot(along));
    // Unfolded about the side, the shortest way is straight.
    let (rs, rh) = ((source - a - along * zs).length(), (head - a - along * zh).length());
    let z = if rs + rh > 1e-12 { zs + (zh - zs) * rs / (rs + rh) } else { (zs + zh) / 2.0 };
    a + along * z.clamp(0.0, length)
}

/// Whether a face that hides stands strictly between `a` and `b`.
fn hidden(a: Vec3, b: Vec3, faces: &[Face], but: Option<usize>) -> bool {
    faces.iter().enumerate().any(|(i, face)| face.occludes && Some(i) != but && face.crossed(a, b).is_some())
}

/// The ways from `source` to `head` among `faces`. `mirrors`: whether the
/// faces send this emitter back; `careful`: whether a mirrored way is checked
/// for what hides it.
pub fn ways(source: Vec3, head: Vec3, faces: &[Face], mirrors: bool, careful: bool) -> Ways {
    let mut out = Ways::default();
    ways_into(&mut out, source, head, faces, mirrors, careful);
    out
}

/// [`ways`], into a `Ways` kept from one call to the next: what is asked
/// for every emitter hundreds of times a second makes no list each time.
pub fn ways_into(out: &mut Ways, source: Vec3, head: Vec3, faces: &[Face], mirrors: bool, careful: bool) {
    let straight = source.distance(head);
    out.direct = None;
    out.mirrored.clear();
    out.mirrored.resize(faces.len(), None);
    out.scattered = 0.0;

    // Straight, or round the edge that lengthens the way least. The faces
    // that hide: one bit each (a scene's first 64 faces can hide).
    let mut hiding = 0u64;
    for (i, face) in faces.iter().enumerate().take(64) {
        if face.occludes && face.crossed(source, head).is_some() {
            hiding |= 1 << i;
        }
    }
    let hiding = (0..faces.len().min(64)).filter(move |i| hiding >> i & 1 == 1);
    if hiding.clone().next().is_none() {
        // Grazing an edge already costs something.
        let mut nearest = f64::INFINITY;
        for face in faces.iter().filter(|face| face.occludes) {
            for i in 0..4 {
                let p = apex(face.corners[i], face.corners[(i + 1) % 4], source, head);
                nearest = nearest.min(source.distance(p) + p.distance(head) - straight);
            }
        }
        let bands = BANDS.map(|f| 10f64.powf(-edge_loss(-nearest, f) / 20.0));
        out.direct = Some(Way { length: straight, from: source, bands });
    } else {
        // What the faces let through.
        let mut through = [1.0; 4];
        for i in hiding.clone() {
            let passed = faces[i].material.passed();
            for (b, t) in through.iter_mut().enumerate() {
                *t *= passed[b.min(2)];
            }
        }
        let mut best: Option<(f64, Vec3)> = None;
        for i in hiding {
            for side in 0..4 {
                let p = apex(faces[i].corners[side], faces[i].corners[(side + 1) % 4], source, head);
                let length = source.distance(p) + p.distance(head);
                if best.is_some_and(|(shortest, _)| shortest <= length) {
                    continue;
                }
                // Each leg stops a hair short of the edge, so that the faces
                // that meet there do not count as hiding it.
                let (near_source, near_head) =
                    (p + (source - p).unit_or(Vec3::Y) * 1e-4, p + (head - p).unit_or(Vec3::Y) * 1e-4);
                if !hidden(source, near_source, faces, None) && !hidden(near_head, head, faces, None) {
                    best = Some((length, p));
                }
            }
        }
        // Through the faces or round an edge: whichever arrives louder. The
        // two are not one way — they differ in length and in where they come
        // from — and a deck lets almost nothing through.
        let passing = Way { length: straight, from: source, bands: through };
        out.direct = match best {
            Some((length, p)) => {
                let bands = BANDS.map(|f| 10f64.powf(-edge_loss(length - straight, f) / 20.0));
                let round = Way { length, from: p, bands };
                Some(if through[1] / straight.max(1e-9) > bands[1] / length.max(1e-9) { passing } else { round })
            }
            None if through.iter().any(|t| *t > 1e-3) => Some(passing),
            None => None,
        };
    }

    if !mirrors {
        return;
    }
    for (i, face) in faces.iter().enumerate().filter(|(_, face)| face.reflects) {
        let (hs, hh) = (face.height(source), face.height(head));
        // Both on the same side, and off the face.
        if hs * hh <= 0.0 || hs.abs() < 1e-3 || hh.abs() < 1e-3 {
            continue;
        }
        let image = source - face.normal * (2.0 * hs);
        let p = image + (head - image) * (hs.abs() / (hs.abs() + hh.abs()));
        // What the face scatters reaches the head from all of it: from its
        // point nearest the mirror point, as far as that way is.
        let q = face.nearest(p);
        // Only if that point sees both: a deck that hides the emitter from
        // the head hides it from the road under the head as well.
        if !hidden(source, q, faces, Some(i)) && !hidden(q, head, faces, Some(i)) {
            let scattered = face.material.scattered();
            let by = source.distance(q) + q.distance(head);
            out.scattered += (scattered[0] + scattered[1] + scattered[2]) / 3.0 / (by * by).max(1e-6);
        }

        let rim = (face.inside(p) / RIM).clamp(0.0, 1.0);
        if rim <= 0.0 {
            continue;
        }
        if careful && (hidden(source, p, faces, Some(i)) || hidden(p, head, faces, Some(i))) {
            continue;
        }
        // Smooth at both ends of the rim.
        let fade = rim * rim * (3.0 - 2.0 * rim);
        let mirrored = face.material.mirrored();
        out.mirrored[i] = Some(Way {
            length: image.distance(head),
            from: p,
            bands: std::array::from_fn(|b| mirrored[b.min(2)] * fade),
        });
    }
}

/// What the air takes from a pure tone, dB a metre, at `celsius` degrees
/// and `humidity` per cent of relative humidity, at sea-level pressure
/// (ISO 9613-1).
pub fn air_loss(frequency: f64, celsius: f64, humidity: f64) -> f64 {
    let t = celsius + 273.15;
    let (t0, t01) = (293.15, 273.16);
    // The share of water vapour in the air, per cent.
    let saturation = 10f64.powf(-6.8346 * (t01 / t).powf(1.261) + 4.6151);
    let h = humidity * saturation;
    // Where oxygen and nitrogen stop following the wave, Hz.
    let oxygen = 24.0 + 4.04e4 * h * (0.02 + h) / (0.391 + h);
    let nitrogen = (t / t0).powf(-0.5) * (9.0 + 280.0 * h * (-4.170 * ((t / t0).powf(-1.0 / 3.0) - 1.0)).exp());
    let f2 = frequency * frequency;
    8.686
        * f2
        * (1.84e-11 * (t / t0).sqrt()
            + (t / t0).powf(-2.5)
                * (0.01275 * (-2239.1 / t).exp() / (oxygen + f2 / oxygen)
                    + 0.1068 * (-3352.0 / t).exp() / (nitrogen + f2 / nitrogen)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::vec3;

    const HARD: Material = Material { absorption: [0.0; 3], transmission: [0.0; 3], scattering: 0.0 };

    /// A horizontal face at height `y`, from `x0` to `x1` and `z0` to `z1`.
    fn slab(y: f64, x: [f64; 2], z: [f64; 2], material: Material) -> Face {
        Face::new(
            [vec3(x[0], y, z[0]), vec3(x[0], y, z[1]), vec3(x[1], y, z[1]), vec3(x[1], y, z[0])],
            material,
            true,
            true,
        )
    }

    #[test]
    fn nothing_between_is_a_straight_way() {
        let found = ways(vec3(0.0, 1.0, 0.0), vec3(3.0, 5.0, 0.0), &[], true, true);
        let way = found.direct.unwrap();
        assert_eq!((way.length, way.from, way.bands), (5.0, vec3(0.0, 1.0, 0.0), [1.0; 4]));
        assert!(found.mirrored.is_empty() && found.scattered == 0.0);
    }

    #[test]
    fn a_floor_mirrors_from_the_image_under_it() {
        // An emitter 1 m and a head 2 m above a floor, 4 m apart.
        let floor = [slab(0.0, [-20.0, 20.0], [-20.0, 20.0], HARD)];
        let found = ways(vec3(0.0, 1.0, 0.0), vec3(4.0, 2.0, 0.0), &floor, true, true);
        assert_eq!(found.direct.unwrap().length, 17f64.sqrt());
        let mirrored = found.mirrored[0].unwrap();
        assert!((mirrored.length - 5.0).abs() < 1e-12);
        // The mirror point: a third of the way, since the heights are 1 and 2.
        assert!(mirrored.from.distance(vec3(4.0 / 3.0, 0.0, 0.0)) < 1e-12);
        assert_eq!(mirrored.bands, [1.0; 4]);
        // Without mirrors, or with the head under the floor: none.
        assert!(ways(vec3(0.0, 1.0, 0.0), vec3(4.0, 2.0, 0.0), &floor, false, true).mirrored[0].is_none());
        assert!(ways(vec3(0.0, 1.0, 0.0), vec3(4.0, -2.0, 0.0), &floor, true, true).mirrored[0].is_none());
    }

    #[test]
    fn a_material_takes_its_share_of_what_it_mirrors() {
        let felt = Material { absorption: [0.19, 0.51, 0.75], transmission: [0.0; 3], scattering: 0.0 };
        let found =
            ways(vec3(0.0, 1.0, 0.0), vec3(4.0, 2.0, 0.0), &[slab(0.0, [-9.0, 9.0], [-9.0, 9.0], felt)], true, true);
        let bands = found.mirrored[0].unwrap().bands;
        for (band, pressure) in bands.iter().zip([0.9, 0.7, 0.5, 0.5]) {
            assert!((band - pressure).abs() < 1e-12, "{bands:?}");
        }
        // What is scattered is not mirrored, and feeds the reverberation.
        let rough = Material { absorption: [0.0; 3], transmission: [0.0; 3], scattering: 0.75 };
        let found =
            ways(vec3(0.0, 1.0, 0.0), vec3(4.0, 2.0, 0.0), &[slab(0.0, [-9.0, 9.0], [-9.0, 9.0], rough)], true, true);
        assert!((found.mirrored[0].unwrap().bands[1] - 0.5).abs() < 1e-12);
        assert!((found.scattered - 0.75 / 25.0).abs() < 1e-12);
    }

    #[test]
    fn a_reflection_fades_out_at_the_rim_of_its_face() {
        // The mirror point is at x = 4/3: a floor that ends before it mirrors nothing.
        let (source, head) = (vec3(0.0, 1.0, 0.0), vec3(4.0, 2.0, 0.0));
        let level =
            |edge: f64| ways(source, head, &[slab(0.0, [-20.0, edge], [-20.0, 20.0], HARD)], true, true).mirrored[0];
        assert!(level(1.0).is_none());
        let (near, far) = (level(4.0 / 3.0 + 0.1).unwrap().bands[0], level(4.0 / 3.0 + 0.4).unwrap().bands[0]);
        assert!(0.0 < near && near < far && far < 1.0, "{near} {far}");
        assert_eq!(level(3.0).unwrap().bands[0], 1.0);
    }

    #[test]
    fn a_deck_between_sends_the_sound_round_its_nearest_edge() {
        // A deck 7.6 m wide, 1.5 m thick; wheels on it, a head below and to the side.
        let deck = [slab(16.0, [-100.0, 100.0], [-3.8, 3.8], HARD), slab(14.5, [-100.0, 100.0], [-3.8, 3.8], HARD)];
        let (wheels, head) = (vec3(0.0, 16.3, -1.5), vec3(0.0, 9.6, -11.35));
        let found = ways(wheels, head, &deck, true, true);
        let way = found.direct.unwrap();
        // Over the top's edge on the head's side.
        assert!(way.from.distance(vec3(0.0, 16.0, -3.8)) < 1e-9, "{:?}", way.from);
        let detour = way.length - wheels.distance(head);
        assert!((detour - 0.3044).abs() < 0.001, "{detour}");
        // Duller as well as quieter: each band loses more than the one under it.
        assert!(way.bands[0] > way.bands[1] && way.bands[1] > way.bands[2] && way.bands[2] >= way.bands[3]);
        assert!((-20.0 * way.bands[1].log10() - edge_loss(detour, 1_000.0)).abs() < 1e-9);
        // The deck's underside mirrors nothing of what is on top of it.
        assert!(found.mirrored.iter().all(Option::is_none));
        // A head above the deck's top sees the wheels.
        assert_eq!(ways(wheels, vec3(0.0, 18.0, -11.35), &deck, false, true).direct.unwrap().from, wheels);
    }

    #[test]
    fn going_behind_an_edge_nothing_switches() {
        // A source sinking behind the edge of a screen: level and length change evenly.
        let screen = [slab(0.0, [-50.0, 50.0], [0.0, 30.0], HARD)];
        let head = vec3(0.0, 2.0, 6.0);
        let mut last: Option<(f64, f64)> = None;
        for step in 0..400 {
            let source = vec3(0.0, -3.0, -12.0 + step as f64 * 0.05);
            let way = ways(source, head, &screen, false, true).direct.unwrap();
            let level = 20.0 * way.bands[1].log10();
            if let Some((before, length)) = last {
                assert!((level - before).abs() < 0.9, "a step of {} dB at {source:?}", level - before);
                assert!((way.length - length).abs() < 0.06);
            }
            last = Some((level, way.length));
        }
        // From seen from well clear of the edge (no loss) to well hidden (near the most an edge takes).
        let seen = ways(vec3(0.0, -3.0, -60.0), head, &screen, false, true).direct.unwrap();
        let gone = ways(vec3(0.0, -3.0, 7.9), head, &screen, false, true).direct.unwrap();
        assert_eq!(seen.bands, [1.0; 4]);
        assert!(gone.bands[2] < 0.12, "{:?}", gone.bands);
    }

    #[test]
    fn a_wall_that_lets_sound_through_is_heard_through() {
        let thin = Material { absorption: [0.5; 3], transmission: [0.25, 0.04, 0.01], scattering: 0.0 };
        // Wide enough that no edge is near: only what passes through.
        let wall = [slab(0.0, [-4000.0, 4000.0], [-4000.0, 4000.0], thin)];
        let way = ways(vec3(0.0, -1.0, 0.0), vec3(0.0, 1.0, 0.0), &wall, false, true).direct.unwrap();
        assert!((way.bands[0] - 0.5).abs() < 0.02 && (way.bands[1] - 0.2).abs() < 0.02, "{:?}", way.bands);
        assert!(way.bands[2] < 0.12);
    }

    #[test]
    fn a_mirrored_way_that_something_hides_is_dropped_when_checked() {
        let floor = slab(0.0, [-20.0, 20.0], [-20.0, 20.0], HARD);
        // A small screen standing on the way down to the mirror point.
        let screen = Face::new(
            [vec3(0.6, 0.0, -2.0), vec3(0.6, 0.0, 2.0), vec3(0.6, 0.6, 2.0), vec3(0.6, 0.6, -2.0)],
            HARD,
            false,
            true,
        );
        let faces = [floor, screen];
        let (source, head) = (vec3(0.0, 1.0, 0.0), vec3(4.0, 2.0, 0.0));
        assert!(ways(source, head, &faces, true, true).mirrored[0].is_none());
        assert!(ways(source, head, &faces, true, false).mirrored[0].is_some());
    }

    #[test]
    fn an_edge_takes_what_the_tables_say() {
        // Grazing: 6 dB; well in the lit side: nothing; deep in the shadow: 20 dB at most.
        assert_eq!(edge_loss(0.0, 1_000.0), 6.0);
        assert_eq!(edge_loss(-1.0, 1_000.0), 0.0);
        assert_eq!(edge_loss(50.0, 4_000.0), 20.0);
        // One wavelength of detour is a Fresnel number of 2: 16 + 10 lg 2.
        assert!((edge_loss(0.343, 1_000.0) - 19.01).abs() < 0.01);
        // It grows with the detour, and its pieces meet: a quarter and one
        // Fresnel number on each side of grazing.
        let mut before = 0.0;
        for i in 0..2_000 {
            let loss = edge_loss(-0.2 + i as f64 * 0.001, 1_000.0);
            assert!(loss >= before - 1e-12, "{loss} after {before}");
            before = loss;
        }
        for number in [-0.25, 0.25, 1.0] {
            let detour = number * SOUND / 2_000.0;
            assert!(
                (edge_loss(detour + 1e-9, 1_000.0) - edge_loss(detour - 1e-9, 1_000.0)).abs() < 1e-3,
                "at {number}"
            );
        }
        assert!(edge_loss(0.05, 4_000.0) > edge_loss(0.05, 250.0));
    }

    #[test]
    fn the_air_takes_what_the_standard_says() {
        // ISO 9613-1 at 20 °C and 70 %, dB a kilometre.
        for (frequency, loss) in [(250.0, 1.12), (1_000.0, 4.98), (4_000.0, 23.09), (8_000.0, 77.63)] {
            let found = air_loss(frequency, 20.0, 70.0) * 1_000.0;
            assert!((found - loss).abs() < 0.02 * loss, "{frequency} Hz: {found} for {loss}");
        }
        // And at 10 °C and 70 %.
        for (frequency, loss) in [(500.0, 1.92), (2_000.0, 9.70), (8_000.0, 118.4)] {
            let found = air_loss(frequency, 10.0, 70.0) * 1_000.0;
            assert!((found - loss).abs() < 0.02 * loss, "{frequency} Hz: {found} for {loss}");
        }
    }
}
