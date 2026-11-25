// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The ways sound takes from an emitter to a head, found from the faces of
//! the objects between and around them:
//!
//! - **straight**, when nothing hides the emitter — or through what hides
//!   it, with what its material lets through, when that arrives louder than
//!   round its edge;
//! - **round an edge**, when a face hides it: over the edge by which it
//!   arrives loudest (the one that lengthens the way least, among faces that
//!   stop everything), quieter and duller by how much longer the way is,
//!   counted in wavelengths (the Fresnel number) — and already a little
//!   quieter when the straight way only grazes an edge, so that nothing
//!   switches as an emitter goes behind a deck;
//! - **mirrored** by each face that reflects, from the emitter's image
//!   behind it, less what the face's material takes in each band; what the
//!   material scatters goes to the reverberation instead.
//!
//! What stands on a leg of a way round an edge or of a mirrored way takes
//! what its material does not let through, band by band: all of it for a
//! wall, nothing for a pane that lets everything through. Any number of
//! faces may hide, and the order they are listed in changes no way by a
//! bit: which way is followed is told by the ways alone, and the faces one
//! goes through are taken in the order it meets them.
//!
//! and what the air takes on the way (ISO 9613-1).
//!
//! Sources: J. Allen and D. Berkley, "Image method for efficiently simulating
//! small-room acoustics" (JASA 65(4), 1979); Z. Maekawa, "Noise reduction by
//! screens" (Applied Acoustics 1, 1968) in the piecewise form that continues
//! into the lit side; ISO 9613-1:1993.

use crate::{
    faces::Faces,
    scene::Material,
    space::{Placement, Vec3, vec3},
};

/// The speed of sound, m/s, as `osage-kart-native` takes it.
pub const SOUND: f64 = 343.0;

/// The bands a way is shaped in, Hz: the object format's three, and the top
/// of the spectrum, which the air and edges take first.
pub const BANDS: [f64; 4] = [250.0, 1_000.0, 4_000.0, 9_000.0];

/// A reflection fades out over this many metres as its mirror point nears
/// the rim of its face.
const RIM: f64 = 0.5;

/// A flat face in the world: four corners in order round it, or three — a
/// triangle is a face whose last two corners are one.
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub corners: [Vec3; 4],
    pub normal: Vec3,
    /// For each side, the direction into the face, in its plane.
    inward: [Vec3; 4],
    /// For each side, whether the face ends there: an edge sound bends
    /// round and a reflection fades out at. A side shared with a face of the
    /// same plane is not one ([`crate::faces::sheets`]); nor is the side of
    /// no length a triangle has.
    pub open: [bool; 4],
    /// Which sheet it is of: the number of its first face, among the faces
    /// it is given with (its own, alone). A way that goes through a sheet
    /// goes round the sheet's edges, wherever they are; a sheet mirrors from
    /// its plane, by the faces the mirror point is on, and scatters once.
    pub sheet: usize,
    pub material: Material,
    pub reflects: bool,
    pub occludes: bool,
}

impl Face {
    pub fn new(corners: [Vec3; 4], material: Material, reflects: bool, occludes: bool) -> Face {
        let normal = (corners[1] - corners[0]).cross(corners[3] - corners[0]);
        // A triangle: its normal from its three corners.
        let normal =
            if normal.length() > 1e-12 { normal } else { (corners[1] - corners[0]).cross(corners[2] - corners[0]) };
        let normal = normal.unit_or(Vec3::Y);
        let triangle = corners[2].distance(corners[3]) < 1e-9;
        let centre = if triangle {
            (corners[0] + corners[1] + corners[2]) / 3.0
        } else {
            (corners[0] + corners[1] + corners[2] + corners[3]) / 4.0
        };
        let open: [bool; 4] = std::array::from_fn(|i| corners[i].distance(corners[(i + 1) % 4]) >= 1e-9);
        let mut inward: [Vec3; 4] = std::array::from_fn(|i| {
            let side = corners[(i + 1) % 4] - corners[i];
            let across = normal.cross(side).unit_or(Vec3::X);
            // Whichever way round the corners are given.
            if (centre - corners[i]).dot(across) < 0.0 { -across } else { across }
        });
        // A side of no length bounds nothing of its own: it repeats the
        // side before it, whose line it lies on.
        for i in 0..4 {
            if !open[i] {
                inward[i] = inward[(i + 3) % 4];
            }
        }
        Face { corners, normal, inward, open, sheet: usize::MAX, material, reflects, occludes }
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

    /// Whether a point of its plane is on the face. On a side it shares
    /// with another face of its plane, it is on one of the two only — the
    /// one whose direction into it comes first by x, then y, then z — so
    /// that nothing passes between two faces of one wall, and nothing is
    /// stopped twice.
    pub fn covers(&self, p: Vec3) -> bool {
        (0..4).all(|i| {
            let d = (p - self.corners[i]).dot(self.inward[i]);
            if self.open[i] || !owns(self.inward[i]) { d > 0.0 } else { d >= 0.0 }
        })
    }

    /// How much of a reflection whose mirror point is `p` the face sends:
    /// 1 inside, fading to nothing over [`RIM`] as `p` nears a side where
    /// the face ends, and across a side it shares, half on it, so that the
    /// two faces' shares sum to one.
    fn rim(&self, p: Vec3) -> f64 {
        (0..4)
            .map(|i| {
                let d = (p - self.corners[i]).dot(self.inward[i]) / RIM;
                if self.open[i] { d } else { 0.5 + d }
            })
            .fold(f64::INFINITY, f64::min)
            .clamp(0.0, 1.0)
    }

    /// Where the straight line from `a` to `b` goes through the face, if it
    /// does strictly between them.
    pub fn crossed(&self, a: Vec3, b: Vec3) -> Option<Vec3> {
        let (ha, hb) = (self.height(a), self.height(b));
        if ha * hb >= 0.0 || ha.abs() < 1e-7 || hb.abs() < 1e-7 {
            return None;
        }
        let p = a + (b - a) * (ha / (ha - hb));
        self.covers(p).then_some(p)
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

    /// The box the face lies in.
    pub fn bounds(&self) -> [Vec3; 2] {
        let mut lo = self.corners[0];
        let mut hi = lo;
        for c in &self.corners[1..] {
            lo = vec3(lo.x.min(c.x), lo.y.min(c.y), lo.z.min(c.z));
            hi = vec3(hi.x.max(c.x), hi.y.max(c.y), hi.z.max(c.z));
        }
        [lo, hi]
    }

    /// The direction into the face across its side `side`, in its plane.
    pub fn inward(&self, side: usize) -> Vec3 {
        self.inward[side]
    }

    /// Its area, m².
    pub fn area(&self) -> f64 {
        let [a, b, c, d] = self.corners;
        ((b - a).cross(c - a).length() + (c - a).cross(d - a).length()) / 2.0
    }
}

/// Of the two faces that share a side, whether the one whose direction into
/// it is `inward` has the side as its own.
fn owns(inward: Vec3) -> bool {
    for c in [inward.x, inward.y, inward.z] {
        if c.abs() > 1e-9 {
            return c > 0.0;
        }
    }
    false
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
    /// By each face (in the engine, by each sheet), in their order; `None` where it mirrors
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

/// What two edges `between` metres apart take, dB, for a way longer by
/// `detour` metres than the straight one, at `frequency`: ISO 9613-2's
/// double diffraction, 10·lg(3 + 20·C₃·N) with N the Fresnel number and
/// C₃ = (1 + (5λ/e)²) / (1/3 + (5λ/e)²) — one edge's when they are close —
/// and 25 dB at most.
pub fn edges_loss(detour: f64, between: f64, frequency: f64) -> f64 {
    let wavelength = SOUND / frequency;
    let n = 2.0 * detour.max(0.0) / wavelength;
    let close = (5.0 * wavelength / between.max(1e-9)).powi(2);
    let c3 = (1.0 + close) / (1.0 / 3.0 + close);
    (10.0 * (3.0 + 20.0 * c3 * n).log10()).min(25.0)
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

/// What is left of the pressure, in each of [`BANDS`], of a sound that goes
/// straight from `a` to `b`: 1 with nothing between, and what each face
/// that hides lets through for those it crosses (but those of the sheet
/// `but`, which the way ends on). The product is the same to
/// the last bit however the faces are listed: two shares multiply alike in
/// either order, and three or more are taken in the order the way meets
/// their faces ([`through_many`]).
fn through(a: Vec3, b: Vec3, faces: &Faces, but: Option<usize>) -> [f64; 4] {
    let mut left = [1.0; 4];
    let mut count = 0;
    let mut many = false;
    faces.along(a, b, 0.0, |i| {
        let face = &faces[i];
        if many || !face.occludes || Some(face.sheet) == but || face.crossed(a, b).is_none() {
            return;
        }
        count += 1;
        if count > 2 {
            many = true;
            return;
        }
        let passed = face.material.passed();
        for (band, share) in left.iter_mut().enumerate() {
            *share *= passed[band.min(2)];
        }
    });
    if many { through_many(a, b, faces, but) } else { left }
}

/// [`through`] for a way that crosses three faces or more: what they let
/// through, multiplied in the order the way meets them — and, of two met at
/// one point, in the order of the faces.
fn through_many(a: Vec3, b: Vec3, faces: &Faces, but: Option<usize>) -> [f64; 4] {
    let mut met: Vec<(f64, usize, [f64; 3])> = Vec::new();
    faces.along(a, b, 0.0, |i| {
        let face = &faces[i];
        if face.occludes
            && Some(face.sheet) != but
            && let Some(at) = face.crossed(a, b)
        {
            met.push((a.distance(at), i, face.material.passed()));
        }
    });
    met.sort_unstable_by(|one, other| one.0.total_cmp(&other.0).then(one.1.cmp(&other.1)));
    let mut left = [1.0; 4];
    for (_, _, passed) in &met {
        for (band, share) in left.iter_mut().enumerate() {
            *share *= passed[band.min(2)];
        }
    }
    left
}

/// Under this share of its pressure, 60 dB down, a way is taken for hidden.
const HIDDEN: f64 = 1e-3;

fn heard(bands: &[f64; 4]) -> bool {
    bands.iter().any(|share| *share > HIDDEN)
}

/// The ways from `source` to `head` among `faces`, the emitter standing
/// still. `mirrors`: whether the faces send this emitter back; `careful`:
/// whether a mirrored way is checked for what stands on it. (The engine asks
/// for each way by itself, [`direct`] and [`mirrored`]: an emitter that
/// moves was not in the same place when the sound of each left it.)
pub fn ways(source: Vec3, head: Vec3, faces: &[Face], mirrors: bool, careful: bool) -> Ways {
    let faces = Faces::new(faces.to_vec(), &[]);
    let mut out = Ways { direct: direct(source, head, &faces), mirrored: vec![None; faces.len()], scattered: 0.0 };
    if mirrors {
        let mut found = Vec::new();
        for i in (0..faces.len()).filter(|&i| faces[i].sheet == i) {
            found.clear();
            let scatter = mirrored(i, source, head, &faces, careful, &mut found);
            for &(k, way) in &found {
                out.mirrored[k] = Some(way);
            }
            out.scattered += scatter.map_or(0.0, |(_, scatter)| scatter.energy);
        }
    }
    out
}

/// Whether the way `one` is followed rather than `other`: the one that
/// arrives louder at 1 kHz; of two as loud, the shorter; of two as long, the
/// one whose turning point comes first by its x, then its y, then its z;
/// and of two by the same point, the one that keeps more, band by band.
/// Nothing but the two ways enters — never the order the faces are listed
/// in.
fn rather(one: &Way, other: &Way) -> bool {
    let key = |way: &Way| {
        let loud = way.bands[1] / way.length.max(1e-9);
        (-loud, way.length, way.from.x, way.from.y, way.from.z, -way.bands[0], -way.bands[2], -way.bands[3])
    };
    key(one) < key(other)
}

/// The way that is not mirrored: straight, through what lets sound through,
/// or round an edge.
pub fn direct(source: Vec3, head: Vec3, faces: &Faces) -> Option<Way> {
    let straight = source.distance(head);
    // The straight way: of each face it goes through, what the face lets
    // through; and of the sheets it passes beside, what grazing an edge
    // leaves — as much of it as the sheet stops (a pane that lets everything
    // through has no edge to sound), and of all those beside, the one that
    // takes most. A sheet gone through does not take the others' edges away.
    let mut bands = [1.0f64; 4];
    let mut beside = [1.0f64; 4];
    let mut crossing: Vec<usize> = Vec::new();
    faces.along(source, head, 0.0, |k| {
        if faces[k].occludes && faces[k].crossed(source, head).is_some() {
            crossing.push(k);
        }
    });
    crossing.sort_unstable();
    let gone_through = crossing.len();
    for &k in &crossing {
        let passed = faces[k].material.passed();
        for (band, share) in bands.iter_mut().enumerate() {
            *share *= passed[band.min(2)];
        }
    }
    // The sheets an edge of which can sound on the straight way: an edge
    // lengthens a way by at least 2·√(L²/4 + h²) − L when it is h from it,
    // and none is heard beyond a quarter of a half wavelength at the lowest
    // band. Taken in their order; a sheet gone through has its edges for the
    // way round it.
    let reach = 0.25 * SOUND / (2.0 * BANDS[0]);
    let mut near: Vec<usize> = Vec::new();
    faces.sheets_along(source, head, (reach * (2.0 * straight + reach)).sqrt() / 2.0 + reach, |sheet| {
        if faces[sheet].occludes && !crossing.iter().any(|&k| faces[k].sheet == sheet) {
            near.push(sheet);
        }
    });
    near.sort_unstable();
    for sheet in near {
        let mut nearest = f64::INFINITY;
        for (a, b, _) in faces.edges(sheet) {
            let p = apex(a, b, source, head);
            nearest = nearest.min(source.distance(p) + p.distance(head) - straight);
        }
        // The faces of a sheet are alike.
        let passed = faces[sheet].material.passed();
        for (b, band) in beside.iter_mut().enumerate() {
            let loss = edge_loss(-nearest, BANDS[b]);
            if loss > 0.0 {
                let (edge, through) = (10f64.powf(-loss / 20.0), passed[b.min(2)]);
                *band = band.min(through + (1.0 - through) * edge);
            }
        }
    }
    if gone_through > 2 {
        // Three shares or more are multiplied in the order the way meets their faces.
        bands = through(source, head, faces, None);
    }
    for (band, beside) in bands.iter_mut().zip(beside) {
        *band *= beside;
    }
    let passing = Way { length: straight, from: source, bands };
    if gone_through == 0 {
        return Some(passing);
    }
    // Round an edge of a sheet that hides it — the faces of the sheets it
    // goes through that end somewhere: of all the edges, the one by
    // which it arrives loudest — the shortest way round, among faces that
    // stop everything — with what that face stops, since what it lets
    // through does not go round, and what its two legs keep.
    let mut round: Option<Way> = None;
    // Of the one kept: what its face stops and its legs keep at 1 kHz.
    let mut kept = 0.0;
    let mut sheets: Vec<usize> = crossing.iter().map(|&k| faces[k].sheet).collect();
    sheets.sort_unstable();
    sheets.dedup();
    for (a, b, k) in sheets.iter().flat_map(|&sheet| faces.edges(sheet)) {
        let passed = faces[k].material.passed();
        let p = apex(a, b, source, head);
        let length = source.distance(p) + p.distance(head);
        // A longer way round an edge that stops no more arrives quieter:
        // it is not looked at further.
        if round.is_some_and(|best| best.bands[1] > 0.0 && length > best.length && 1.0 - passed[1] <= kept) {
            continue;
        }
        let mut bands: [f64; 4] = std::array::from_fn(|b| {
            10f64.powf(-edge_loss(length - straight, BANDS[b]) / 20.0) * (1.0 - passed[b.min(2)])
        });
        // Nor is one that with both legs clear would be no louder than
        // the one kept.
        if round.is_some_and(|best| !rather(&Way { length, from: p, bands }, &best)) {
            continue;
        }
        // Each leg stops a hair short of the edge, so that the faces
        // that meet there do not count as standing on it.
        let (near_source, near_head) =
            (p + (source - p).unit_or(Vec3::Y) * 1e-4, p + (head - p).unit_or(Vec3::Y) * 1e-4);
        let (first, second) = (through(source, near_source, faces, None), through(near_head, head, faces, None));
        for (b, band) in bands.iter_mut().enumerate() {
            *band *= first[b] * second[b];
        }
        let way = Way { length, from: p, bands };
        if heard(&bands) && round.is_none_or(|best| rather(&way, &best)) {
            round = Some(way);
            kept = (1.0 - passed[1]) * first[1] * second[1];
        }
    }
    // Through the faces or round an edge: whichever arrives louder is the
    // way followed — they differ in length and in where they come from, and
    // a deck lets almost nothing through. Just behind an edge the two are
    // still one sound: where their lengths differ by less than half a
    // wavelength the other is added, all of it at the edge itself, so that
    // nothing steps as an emitter goes behind a face, whatever it lets
    // through.
    // Round no single edge: over two, one after the other — down from a
    // deck's underside and up past its top, round both sides of a wall that
    // is thick — when the sheets gone through are not wholly closed.
    if round.is_none() {
        round = twice_round(source, head, straight, faces, &sheets);
    }
    match round {
        Some(round) => {
            let together = BANDS.map(|hz| (1.0 - 2.0 * (round.length - straight) * hz / SOUND).clamp(0.0, 1.0));
            let (kept, other) = if passing.bands[1] / straight.max(1e-9) > round.bands[1] / round.length.max(1e-9) {
                (passing, round)
            } else {
                (round, passing)
            };
            Some(Way { bands: std::array::from_fn(|b| kept.bands[b] + other.bands[b] * together[b]), ..kept })
        }
        None if heard(&passing.bands) => Some(passing),
        None => None,
    }
}

/// The way round two edges of the sheets `sheets`, from `source` to `head`
/// (`straight` apart), by which it arrives loudest, with what the faces of
/// both edges stop and what its three legs keep; `None` if every such way
/// is hidden.
fn twice_round(source: Vec3, head: Vec3, straight: f64, faces: &Faces, sheets: &[usize]) -> Option<Way> {
    let edges: Vec<(Vec3, Vec3, usize)> = sheets.iter().flat_map(|&sheet| faces.edges(sheet)).collect();
    let mut best: Option<Way> = None;
    for (i, &(a1, b1, k1)) in edges.iter().enumerate() {
        for (j, &(a2, b2, k2)) in edges.iter().enumerate() {
            if i == j {
                continue;
            }
            // The shortest way by the two: each turning point is where the
            // way from the other end to the other point is shortest.
            let mut second = apex(a2, b2, source, head);
            let mut first = apex(a1, b1, source, second);
            for _ in 0..6 {
                second = apex(a2, b2, first, head);
                first = apex(a1, b1, source, second);
            }
            let between = first.distance(second);
            if between < 1e-6 {
                // One point: a single edge's way, already looked at.
                continue;
            }
            let length = source.distance(first) + between + second.distance(head);
            let (passed1, passed2) = (faces[k1].material.passed(), faces[k2].material.passed());
            let mut bands: [f64; 4] = std::array::from_fn(|b| {
                10f64.powf(-edges_loss(length - straight, between, BANDS[b]) / 20.0)
                    * (1.0 - passed1[b.min(2)])
                    * (1.0 - passed2[b.min(2)])
            });
            if best.is_some_and(|best| !rather(&Way { length, from: second, bands }, &best)) {
                continue;
            }
            // Each leg stops a hair short of its edges.
            let near = |p: Vec3, to: Vec3| p + (to - p).unit_or(Vec3::Y) * 1e-4;
            let legs = [
                through(source, near(first, source), faces, None),
                through(near(first, second), near(second, first), faces, None),
                through(near(second, head), head, faces, None),
            ];
            for (b, band) in bands.iter_mut().enumerate() {
                *band *= legs.iter().map(|leg| leg[b]).product::<f64>();
            }
            let way = Way { length, from: second, bands };
            if heard(&bands) && best.is_none_or(|best| rather(&way, &best)) {
                best = Some(way);
            }
        }
    }
    best
}

/// What a face scatters of an emitter's sound towards a head.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scatter {
    /// Of the energy the emitter sends at one metre.
    pub energy: f64,
    /// The way it takes, m: to the point of the face nearest where the face
    /// would mirror the emitter, and on to the head. Where the face does
    /// mirror it, that is the mirrored way's own length.
    pub length: f64,
    /// That point.
    pub from: Vec3,
}

/// The ways mirrored by the sheet `sheet` (a face alone, or faces of one
/// plane that share sides), from the emitter's image behind its plane, each
/// by a face the mirror point is on, into `ways`; and what the sheet
/// scatters towards the head, with the face it is scattered from. With
/// `careful`, what stands on either leg takes its share, or all of it.
pub fn mirrored(
    sheet: usize,
    source: Vec3,
    head: Vec3,
    faces: &Faces,
    careful: bool,
    ways: &mut Vec<(usize, Way)>,
) -> Option<(usize, Scatter)> {
    mirrored_then(sheet, source, head, faces, careful, None, ways)
}

/// [`mirrored`] by a sheet that moves, as it was when the sound met it:
/// `was` takes a point of the sheet where it is now to where it then was.
/// The image, the mirror point and what stands on the legs are found where
/// the sheet then was; its faces' rims, where they are now, at the point
/// that was the mirror point then.
pub fn mirrored_then(
    sheet: usize,
    source: Vec3,
    head: Vec3,
    faces: &Faces,
    careful: bool,
    was: Option<&Placement>,
    ways: &mut Vec<(usize, Way)>,
) -> Option<(usize, Scatter)> {
    let plane = &faces[sheet];
    if !plane.reflects {
        return None;
    }
    let (normal, on) = match was {
        Some(was) => (was.direction(plane.normal), was.point(plane.corners[0])),
        None => (plane.normal, plane.corners[0]),
    };
    let height = |x: Vec3| (x - on).dot(normal);
    let (hs, hh) = (height(source), height(head));
    // Both on the same side, and off the face.
    if hs * hh <= 0.0 || hs.abs() < 1e-3 || hh.abs() < 1e-3 {
        return None;
    }
    let image = source - normal * (2.0 * hs);
    // The mirror point where the sheet was, and the same point of it now.
    let then = image + (head - image) * (hs.abs() / (hs.abs() + hh.abs()));
    let p = was.map_or(then, |was| was.local(then));
    let back = |x: Vec3| was.map_or(x, |was| was.point(x));
    // The sheet's faces near the mirror point, in their order.
    let mut near: Vec<usize> = Vec::new();
    faces.along(p, p, RIM, |k| {
        if faces[k].sheet == sheet {
            near.push(k);
        }
    });
    near.sort_unstable();
    // What the sheet scatters reaches the head from all of it: from its
    // point nearest the mirror point, as far as that way is — if that point
    // sees both: a deck that hides the emitter from the head hides it from
    // the road under the head as well. That point is the mirror point when
    // a face is under it, and otherwise on one of the faces where the sheet
    // ends.
    let nearest = |k: usize| (faces[k].nearest(p), k);
    let (q, by_face) = match near.iter().copied().find(|&k| faces[k].inside(p) >= 0.0) {
        Some(k) => nearest(k),
        None if faces.alone(sheet) => nearest(sheet),
        // The nearest point of its edges.
        None => faces
            .edges(sheet)
            .map(|(a, b, k)| {
                let along = b - a;
                let t = ((p - a).dot(along) / along.dot(along).max(1e-300)).clamp(0.0, 1.0);
                (a + along * t, k)
            })
            .min_by(|a, b| a.0.distance(p).total_cmp(&b.0.distance(p)).then(a.1.cmp(&b.1)))
            .unwrap_or_else(|| nearest(sheet)),
    };
    let q = back(q);
    let (to, fro) = (through(source, q, faces, Some(sheet)), through(q, head, faces, Some(sheet)));
    let scattered = faces[by_face].material.scattered();
    let by = source.distance(q) + q.distance(head);
    let seen = (to[1] * fro[1]).powi(2);
    let energy = (scattered[0] + scattered[1] + scattered[2]) / 3.0 / (by * by).max(1e-6) * seen;
    let scatter = (energy > 0.0).then_some((by_face, Scatter { energy, length: by, from: q }));

    // What stands on the legs to the mirror point: the same for every face.
    let mut legs: Option<([f64; 4], [f64; 4])> = None;
    for k in near {
        let face = &faces[k];
        let rim = face.rim(p);
        if rim <= 0.0 || !face.reflects {
            continue;
        }
        // Smooth at both ends of the rim.
        let fade = rim * rim * (3.0 - 2.0 * rim);
        let share = face.material.mirrored();
        let mut bands: [f64; 4] = std::array::from_fn(|b| share[b.min(2)] * fade);
        if careful {
            let (to, fro) = *legs.get_or_insert_with(|| {
                (through(source, then, faces, Some(sheet)), through(then, head, faces, Some(sheet)))
            });
            for (b, band) in bands.iter_mut().enumerate() {
                *band *= to[b] * fro[b];
            }
            // A way whose legs something stops is not one; a face that
            // mirrors nothing in itself is still a way, a silent one.
            if !heard(&std::array::from_fn(|b| to[b] * fro[b])) {
                continue;
            }
        }
        ways.push((k, Way { length: image.distance(head), from: then, bands }));
    }
    scatter
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

    /// [`super::direct`] among faces given as a list, indexed — half of them
    /// as faces that move, which the index does not hold, so that both
    /// kinds are looked at.
    fn direct(source: Vec3, head: Vec3, faces: &[Face]) -> Option<Way> {
        super::direct(source, head, &indexed(faces))
    }

    /// [`super::mirrored`] by the face `i` among faces given as a list,
    /// each a sheet of its own.
    fn mirrored(i: usize, source: Vec3, head: Vec3, faces: &[Face], careful: bool) -> (Option<Way>, Option<Scatter>) {
        let mut ways = Vec::new();
        let scatter = super::mirrored(i, source, head, &indexed(faces), careful, &mut ways);
        (ways.first().map(|&(_, way)| way), scatter.map(|(_, scatter)| scatter))
    }

    fn indexed(faces: &[Face]) -> Faces {
        let moving: Vec<bool> = (0..faces.len()).map(|i| i % 2 == 1).collect();
        Faces::new(faces.to_vec(), &moving)
    }

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
    fn under_the_middle_of_a_thick_deck_the_sound_goes_round_two_edges() {
        // A deck 20 m square and half a metre thick, a source 5 m under its middle and a head 5 m over it:
        // no single edge leads from one to the other — the underside hides the top's edges, the top the
        // underside's. Round two edges, the underside's then the top's at the nearest side.
        let deck = [slab(0.0, [-10.0, 10.0], [-10.0, 10.0], HARD), slab(-0.5, [-10.0, 10.0], [-10.0, 10.0], HARD)];
        let (source, head) = (vec3(0.0, -5.0, 0.0), vec3(0.0, 5.0, 0.0));
        let way = ways(source, head, &deck, false, true).direct.expect("heard round two edges");
        let length = (100.0f64 + 4.5 * 4.5).sqrt() + 0.5 + (100.0f64 + 25.0).sqrt();
        assert!((way.length - length).abs() < 1e-6, "{} for {length}", way.length);
        assert!((way.from.y - 0.0).abs() < 1e-9 && (way.from.x.abs() - 10.0).max(way.from.z.abs() - 10.0).abs() < 1e-9);
        // So long a detour takes all that two edges take, at every band.
        for band in way.bands {
            assert!((band - 10f64.powf(-25.0 / 20.0)).abs() < 1e-9, "{band}");
        }
        // Closed at its sides, a solid slab: the same way, down its side from one edge to the other.
        let side = |x: f64| {
            Face::new(
                [vec3(x, -0.5, -10.0), vec3(x, 0.0, -10.0), vec3(x, 0.0, 10.0), vec3(x, -0.5, 10.0)],
                HARD,
                true,
                true,
            )
        };
        let end = |z: f64| {
            Face::new(
                [vec3(-10.0, -0.5, z), vec3(-10.0, 0.0, z), vec3(10.0, 0.0, z), vec3(10.0, -0.5, z)],
                HARD,
                true,
                true,
            )
        };
        let mut closed = deck.to_vec();
        closed.extend([side(-10.0), side(10.0), end(-10.0), end(10.0)]);
        let solid = ways(source, head, &closed, false, true).direct.expect("heard round a solid slab");
        assert!((solid.length - length).abs() < 1e-6 && solid.bands == way.bands, "{solid:?}");
    }

    #[test]
    fn two_edges_take_what_the_standard_says() {
        // ISO 9613-2: close together, one edge's 10·lg(3 + 20·N); far apart, C₃ → 3; never more than 25 dB.
        let n = |detour: f64, hz: f64| 2.0 * detour * hz / SOUND;
        let close = edges_loss(0.1, 1e-6, 1_000.0);
        assert!((close - 10.0 * (3.0 + 20.0 * n(0.1, 1_000.0)).log10()).abs() < 1e-6, "{close}");
        let apart = edges_loss(0.1, 1e6, 1_000.0);
        assert!((apart - 10.0 * (3.0 + 60.0 * n(0.1, 1_000.0)).log10()).abs() < 1e-6, "{apart}");
        assert_eq!(edges_loss(100.0, 1.0, 4_000.0), 25.0);
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
    fn two_detours_as_long_are_told_apart_by_where_they_turn_not_by_their_order() {
        // Two walls, one behind the other and each a little to one side: round the left of the first or
        // round the right of the second, the way is as long to the last bit.
        let wall = |z: f64, from: f64, to: f64| {
            Face::new([vec3(from, -5.0, z), vec3(to, -5.0, z), vec3(to, 5.0, z), vec3(from, 5.0, z)], HARD, false, true)
        };
        let (first, second) = (wall(-1.0, -1.0, 0.2), wall(1.0, -0.2, 1.0));
        let (source, head) = (vec3(0.0, 0.0, -6.0), vec3(0.0, 0.0, 6.0));
        let one = direct(source, head, &[first.clone(), second.clone()]).unwrap();
        let other = direct(source, head, &[second, first]).unwrap();
        assert!((one.length - (26f64.sqrt() + 50f64.sqrt())).abs() < 1e-12, "{}", one.length);
        assert_eq!((one.length, one.from, one.bands), (other.length, other.from, other.bands));
        // Of the two, the one whose turning point has the smaller x.
        assert!(one.from.distance(vec3(-1.0, 0.0, -1.0)) < 1e-9, "{:?}", one.from);
    }

    #[test]
    fn two_faces_that_meet_at_an_edge_hide_alike_in_either_order() {
        // Two walls from the same edge, one that stops everything and one that lets through 64 % of the
        // energy: round their edge the way is the same, and so is what is left of it, whichever is listed
        // first — what the wall that stops everything leaves.
        let wall = |z: f64, transmission: f64| {
            let of = Material { absorption: [0.0; 3], transmission: [transmission; 3], scattering: 0.0 };
            Face::new(
                [vec3(-1.0, -5.0, 0.0), vec3(4.0, -5.0, z), vec3(4.0, 5.0, z), vec3(-1.0, 5.0, 0.0)],
                of,
                false,
                true,
            )
        };
        let (stops, lets) = (wall(-2.0, 0.0), wall(2.0, 0.64));
        let (source, head) = (vec3(0.0, 0.0, -6.0), vec3(0.0, 0.0, 6.0));
        let one = direct(source, head, &[stops.clone(), lets.clone()]).unwrap();
        let other = direct(source, head, &[lets, stops.clone()]).unwrap();
        assert_eq!(one, other);
        assert!((one.length - 2.0 * 37f64.sqrt()).abs() < 1e-9 && one.from.distance(vec3(-1.0, 0.0, 0.0)) < 1e-9);
        let alone = direct(source, head, &[stops]).unwrap();
        assert!((one.bands[1] / alone.bands[1] - 1.0).abs() < 1e-9, "{:?} for {:?}", one.bands, alone.bands);
    }

    #[test]
    fn a_pane_gone_through_leaves_a_screen_its_edge() {
        // A screen whose edge the straight way grazes, and what it takes there: half the pressure. A pane
        // that lets everything through, stood across the way, changes nothing of it — on the edge, a
        // millimetre before it, a millimetre behind it.
        let clear = Material { absorption: [0.0; 3], transmission: [1.0; 3], scattering: 0.0 };
        let screen = Face::new(
            [vec3(0.0, -5.0, 0.0), vec3(0.0, 5.0, 0.0), vec3(0.0, 5.0, 9.0), vec3(0.0, -5.0, 9.0)],
            HARD,
            false,
            true,
        );
        let pane = Face::new(
            [vec3(0.5, -5.0, -5.0), vec3(0.5, 5.0, -5.0), vec3(0.5, 5.0, 5.0), vec3(0.5, -5.0, 5.0)],
            clear,
            false,
            true,
        );
        for z in [-0.001, 0.0, 0.001] {
            let (source, head) = (vec3(-1.0, 0.0, z), vec3(1.0, 0.0, z));
            let without = direct(source, head, std::slice::from_ref(&screen)).unwrap();
            for faces in [[screen.clone(), pane.clone()], [pane.clone(), screen.clone()]] {
                let with = direct(source, head, &faces).unwrap();
                for b in 0..4 {
                    assert!(
                        (with.bands[b] / without.bands[b] - 1.0).abs() < 1e-9,
                        "at {z}: {:?} for {:?}",
                        with.bands,
                        without.bands
                    );
                }
                assert!((with.length - without.length).abs() < 1e-12);
            }
            if z == 0.0 {
                assert!((without.bands[1] - 0.501).abs() < 0.001, "{:?}", without.bands);
            }
        }
    }

    #[test]
    fn the_order_the_faces_are_listed_in_never_changes_a_way() {
        use osage_sound_voices::dsp::Noise;
        // Faces drawn on a coarse grid, so that edges are shared, detours are as long as each other and ways
        // graze: walls, floors and panes of every kind, listed in one order and in others.
        let mut chance = Noise::new(2026);
        let mut pick = |from: &[f64]| from[((chance.unit() * from.len() as f32) as usize).min(from.len() - 1)];
        let grid = [-4.0, -2.0, -1.0, 0.0, 1.0, 2.0, 4.0];
        // (Shares that do not multiply evenly: three of them in another order differ in the last bit.)
        let shares = [0.0, 0.07, 0.31, 0.53, 0.83, 1.0];
        for scene in 0..600 {
            let count = 2 + scene % 11;
            let mut faces = Vec::new();
            for _ in 0..count {
                let transmission = [pick(&shares), pick(&shares), pick(&shares)];
                let absorption = transmission.map(|t| (1.0 - t) * pick(&[0.0, 0.1, 0.5]));
                let of = Material { absorption, transmission, scattering: pick(&[0.0, 0.3, 1.0]) };
                let (a, b, low, high) = (pick(&grid), pick(&grid), pick(&[-5.0, -1.0, 0.0]), pick(&[1.0, 5.0]));
                let (c, d) = (pick(&grid), pick(&grid));
                // A wall from (a, c) to (b, d) on the ground, or a floor between them at a height.
                let corners = if pick(&[0.0, 0.0, 1.0]) == 0.0 {
                    [vec3(a, low, c), vec3(b, low, d), vec3(b, high, d), vec3(a, high, c)]
                } else {
                    [vec3(a, low, c), vec3(b, low, c), vec3(b, low, d), vec3(a, low, d)]
                };
                if (corners[1] - corners[0]).cross(corners[3] - corners[0]).length() < 1e-9 {
                    continue;
                }
                faces.push(Face::new(corners, of, pick(&[0.0, 1.0]) == 1.0, pick(&[0.0, 1.0, 1.0]) == 1.0));
            }
            let (source, head) = (
                vec3(pick(&grid), pick(&[0.0, 0.5, 2.0]), pick(&[-6.0, -3.0, 0.5])),
                vec3(pick(&grid), pick(&[0.0, 0.5, 2.0]), pick(&[6.0, 3.0, -0.5])),
            );
            let straight = direct(source, head, &faces);
            let by: Vec<_> = (0..faces.len()).map(|i| mirrored(i, source, head, &faces, true)).collect();
            // Turned end to end, and each face moved a few places on.
            for turn in 1..=3 {
                let order: Vec<usize> = match turn {
                    1 => (0..faces.len()).rev().collect(),
                    _ => (0..faces.len()).map(|i| (i * 2 + turn) % faces.len()).collect::<Vec<_>>(),
                };
                let mut seen = vec![false; faces.len()];
                order.iter().for_each(|&i| seen[i] = true);
                if seen.contains(&false) {
                    continue;
                }
                let listed: Vec<Face> = order.iter().map(|&i| faces[i].clone()).collect();
                assert_eq!(direct(source, head, &listed), straight, "scene {scene}, order {order:?}");
                for (at, &i) in order.iter().enumerate() {
                    assert_eq!(
                        mirrored(at, source, head, &listed, true),
                        by[i],
                        "scene {scene}, face {i} in {order:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_pane_has_as_much_of_an_edge_as_it_stops() {
        // A source sinking behind the edge of a pane, a millimetre at a time across the line where the pane
        // comes between: whatever the pane lets through, nothing steps there.
        let head = vec3(0.0, 2.0, 6.0);
        for transmission in [1.0, 0.64, 0.25, 0.04, 0.0] {
            let glass = Material { absorption: [0.0; 3], transmission: [transmission; 3], scattering: 0.0 };
            let pane = [slab(0.0, [-50.0, 50.0], [0.0, 30.0], glass)];
            let level = |z: f64| direct(vec3(0.0, -3.0, z), head, &pane).unwrap().bands;
            // The straight way meets the pane's edge when the source is at z = −9.
            let mut last = level(-9.03);
            for step in 1..=60 {
                let now = level(-9.03 + f64::from(step) * 0.001);
                for b in 0..4 {
                    let moved = 20.0 * (now[b] / last[b]).log10();
                    assert!(moved.abs() < 0.1, "{transmission}: a step of {moved} dB in band {b} at step {step}");
                }
                last = now;
            }
            // At the edge itself: what passes, and half of what is stopped.
            let through = transmission.sqrt();
            for z in [-9.000_001, -8.999_999] {
                assert!(
                    (level(z)[1] - (through + (1.0 - through) * 0.5)).abs() < 0.01,
                    "{transmission} at {z}: {:?}",
                    level(z)
                );
            }
        }
        // A pane that lets everything through is not there: on its edge, a millimetre inside, well inside.
        let clear = Material { absorption: [0.0; 3], transmission: [1.0; 3], scattering: 0.0 };
        let pane = [slab(0.0, [-50.0, 50.0], [0.0, 30.0], clear)];
        for z in [-9.0, -8.999, -8.0, 0.0] {
            let way = direct(vec3(0.0, -3.0, z), head, &pane).unwrap();
            assert_eq!((way.bands, way.from), ([1.0; 4], vec3(0.0, -3.0, z)), "at {z}");
        }
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
    fn the_order_and_the_number_of_faces_change_nothing() {
        // A wall between an emitter and a head, among 64 faces that hide nothing: wherever it is listed.
        let wall = Face::new(
            [vec3(0.0, -5.0, -5.0), vec3(0.0, 5.0, -5.0), vec3(0.0, 5.0, 5.0), vec3(0.0, -5.0, 5.0)],
            HARD,
            false,
            true,
        );
        let idle = |k: usize| {
            let y = 20.0 + k as f64;
            Face::new(
                [vec3(-1.0, y, -1.0), vec3(-1.0, y, 1.0), vec3(1.0, y, 1.0), vec3(1.0, y, -1.0)],
                HARD,
                false,
                false,
            )
        };
        let (source, head) = (vec3(-1.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0));
        let mut first: Vec<Face> = vec![wall.clone()];
        first.extend((0..64).map(idle));
        let mut last: Vec<Face> = (0..64).map(idle).collect();
        last.push(wall);
        let (a, b) = (direct(source, head, &first).unwrap(), direct(source, head, &last).unwrap());
        // Round the wall's edge, 5 m away: twice the root of 26.
        assert!((a.length - 2.0 * 26f64.sqrt()).abs() < 1e-9, "{}", a.length);
        assert_eq!((a.length, a.bands), (b.length, b.bands));
        assert!(a.bands[1] < 0.15);
    }

    #[test]
    fn a_pane_on_a_mirrored_way_takes_what_it_does_not_let_through() {
        // An emitter and a head 1 m above a floor, 2 m apart; a pane stands on the way down to the floor.
        let floor = slab(0.0, [-20.0, 20.0], [-20.0, 20.0], HARD);
        let pane = |transmission: f64| {
            let glass = Material { absorption: [0.0; 3], transmission: [transmission; 3], scattering: 0.0 };
            Face::new(
                [vec3(-0.5, 0.2, -2.0), vec3(-0.5, 0.2, 2.0), vec3(-0.5, 0.8, 2.0), vec3(-0.5, 0.8, -2.0)],
                glass,
                false,
                true,
            )
        };
        let (source, head) = (vec3(-1.0, 1.0, 0.0), vec3(1.0, 1.0, 0.0));
        let echo = |transmission: f64| mirrored(0, source, head, &[floor.clone(), pane(transmission)], true).0;
        // All of it through: the floor's echo as if nothing stood there.
        let clear = echo(1.0).unwrap();
        assert!((clear.length - 8f64.sqrt()).abs() < 1e-12);
        assert_eq!(clear.bands, [1.0; 4]);
        // A quarter of the energy through: half of the pressure.
        let half = echo(0.25).unwrap();
        assert!(half.bands.iter().all(|band| (band - 0.5).abs() < 1e-12), "{:?}", half.bands);
        // Nothing through: no echo. Unchecked, the pane is not looked at.
        assert!(echo(0.0).is_none());
        assert!(mirrored(0, source, head, &[floor, pane(0.0)], false).0.is_some());
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
