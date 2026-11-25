// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Measured head responses for headphones (decision D-242): a few virtual
//! loudspeakers round the head, each heard through the two ears' measured
//! impulse responses, and a table of the time between the ears over the
//! sphere.
//!
//! - **A way** keeps a delay of its own for each ear, read from the table
//!   for its direction, so that the time between the ears is not blurred.
//!   Its sound is then shared among the three loudspeakers of the triangle
//!   its direction falls in, by weights that sum to one (vector-base
//!   panning, normalised by their sum): the loudspeakers' responses have
//!   their onsets taken out, so their mixture is the response between them.
//! - **The loudspeakers** are filtered once each, for all the ways: what this
//!   costs depends on how many there are, not on the ways.
//! - **The file** is derived from a measured set ([`derive`]) by
//!   `tools/make-hrir.py`; the engine reads it ([`Hrir::from_bytes`]). The set
//!   osage uses is subject D2 (a KEMAR) of the SADIE II database, University
//!   of York, Apache-2.0.
//!
//! Directions are in the head's frame: x to the right, y up, z ahead.

use crate::{
    engine::BLOCK,
    scene::Head,
    space::{Vec3, vec3},
};

const MAGIC: &[u8; 8] = b"OSHRIR01";

/// A measured set, as the engine uses it.
#[derive(Clone, Debug, PartialEq)]
pub struct Hrir {
    pub rate: u32,
    /// Taps of each response.
    pub taps: usize,
    /// Each loudspeaker's direction (unit, head frame) and its two
    /// responses, left then right.
    pub speakers: Vec<(Vec3, [Vec<f32>; 2])>,
    /// The triangles the loudspeakers make, outward.
    pub triangles: Vec<[u16; 3]>,
    /// The time by which the left ear hears later than the right, s, on a
    /// grid of `steps[1]` elevations from −90° to 90° and `steps[0]`
    /// azimuths from straight ahead round to the left.
    pub itd: Vec<f32>,
    pub steps: [usize; 2],
}

/// How a way is shared among the loudspeakers: three, and their weights.
pub type Share = [(u16, f32); 3];

impl Hrir {
    /// A set from its loudspeakers (each a direction, head frame, and its
    /// two responses, all of one length) and its table of the time between
    /// the ears (`steps[1]` rows of elevation from −90° to 90°, each of
    /// `steps[0]` azimuths from ahead round to the left); the triangles are
    /// the loudspeakers' hull.
    pub fn new(
        rate: u32,
        speakers: Vec<(Vec3, [Vec<f32>; 2])>,
        itd: Vec<f32>,
        steps: [usize; 2],
    ) -> Result<Hrir, String> {
        let taps = speakers.first().map_or(0, |(_, [l, _])| l.len());
        if speakers.len() < 4 || taps == 0 || speakers.iter().any(|(_, [l, r])| l.len() != taps || r.len() != taps) {
            return Err("four loudspeakers or more, their responses all of one length".to_owned());
        }
        if steps[0] < 2 || steps[1] < 2 || itd.len() != steps[0] * steps[1] {
            return Err("a table of the time between the ears of its size".to_owned());
        }
        let speakers: Vec<(Vec3, [Vec<f32>; 2])> = speakers.into_iter().map(|(d, r)| (d.unit_or(Vec3::Z), r)).collect();
        let points: Vec<Vec3> = speakers.iter().map(|(d, _)| *d).collect();
        Ok(Hrir { rate, taps, triangles: hull(&points), speakers, itd, steps })
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Hrir, String> {
        let mut read = Reader { bytes, at: 0 };
        if read.take(8)? != MAGIC {
            return Err("not a head response file of osage".to_owned());
        }
        let mut words = [0usize; 6];
        for w in &mut words {
            *w = read.word()? as usize;
        }
        let [rate, taps, count, triangles, az, el] = words;
        if taps == 0 || taps > 4096 || !(4..=4096).contains(&count) || az < 2 || el < 2 || az * el > 1 << 20 {
            return Err("a head response file of no sense".to_owned());
        }
        let mut speakers = Vec::with_capacity(count);
        for _ in 0..count {
            let d = read.floats(3)?;
            let (left, right) = (read.floats(taps)?, read.floats(taps)?);
            speakers.push((vec3(f64::from(d[0]), f64::from(d[1]), f64::from(d[2])).unit_or(Vec3::Z), [left, right]));
        }
        let mut faces = Vec::with_capacity(triangles);
        for _ in 0..triangles {
            let t = read.take(8)?;
            let index = |i: usize| u16::from_le_bytes([t[2 * i], t[2 * i + 1]]);
            let face = [index(0), index(1), index(2)];
            if face.iter().any(|&i| i as usize >= count) {
                return Err("a triangle of no loudspeakers".to_owned());
            }
            faces.push(face);
        }
        let itd = read.floats(az * el)?;
        if itd.iter().chain(speakers.iter().flat_map(|(_, r)| r.iter().flatten())).any(|x| !x.is_finite()) {
            return Err("a head response file with numbers of no sense".to_owned());
        }
        if read.at != bytes.len() {
            return Err("a head response file with more than it says".to_owned());
        }
        Ok(Hrir { rate: rate as u32, taps, speakers, triangles: faces, itd, steps: [az, el] })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        for word in
            [self.rate as usize, self.taps, self.speakers.len(), self.triangles.len(), self.steps[0], self.steps[1]]
        {
            out.extend_from_slice(&(word as u32).to_le_bytes());
        }
        for (d, [left, right]) in &self.speakers {
            for x in [d.x, d.y, d.z] {
                out.extend_from_slice(&(x as f32).to_le_bytes());
            }
            for x in left.iter().chain(right) {
                out.extend_from_slice(&x.to_le_bytes());
            }
        }
        for t in &self.triangles {
            for i in t.iter().chain(&[0]) {
                out.extend_from_slice(&i.to_le_bytes());
            }
        }
        for x in &self.itd {
            out.extend_from_slice(&x.to_le_bytes());
        }
        out
    }

    /// How a sound from `direction` (head frame) is shared among the
    /// loudspeakers: the triangle it falls in, by weights that sum to one.
    pub fn share(&self, direction: Vec3) -> Share {
        let d = direction.unit_or(Vec3::Z);
        let mut best: Option<(f64, Share)> = None;
        for t in &self.triangles {
            let [a, b, c] = t.map(|i| self.speakers[i as usize].0);
            let Some(g) = solve([a, b, c], d) else { continue };
            // The triangle it is most inside: the one whose least weight is greatest.
            let least = g[0].min(g[1]).min(g[2]);
            if best.as_ref().is_none_or(|(l, _)| least > *l) {
                let sum = (g[0] + g[1] + g[2]).max(1e-12);
                let share = [0, 1, 2].map(|k| (t[k], (g[k].max(0.0) / sum) as f32));
                best = Some((least, share));
            }
            if least >= 0.0 {
                break;
            }
        }
        best.map_or([(0, 1.0), (0, 0.0), (0, 0.0)], |(_, share)| share)
    }

    /// By how much the left ear hears later than the right, s, a sound from
    /// `direction` (head frame): read from the table between its four
    /// nearest points.
    pub fn itd(&self, direction: Vec3) -> f64 {
        let d = direction.unit_or(Vec3::Z);
        let elevation = d.y.clamp(-1.0, 1.0).asin().to_degrees();
        // Azimuth from ahead round to the left, as the measured set counts it.
        let azimuth = (-d.x).atan2(d.z).to_degrees().rem_euclid(360.0);
        let [az, el] = self.steps;
        let a = azimuth / 360.0 * az as f64;
        let e = ((elevation + 90.0) / 180.0 * (el - 1) as f64).clamp(0.0, (el - 1) as f64);
        let (a0, e0) = (a.floor() as usize % az, (e.floor() as usize).min(el - 2));
        let (fa, fe) = (a - a.floor(), e - e0 as f64);
        let at = |i: usize, j: usize| f64::from(self.itd[j * az + i % az]);
        let low = at(a0, e0) * (1.0 - fa) + at(a0 + 1, e0) * fa;
        let high = at(a0, e0 + 1) * (1.0 - fa) + at(a0 + 1, e0 + 1) * fa;
        low * (1.0 - fe) + high * fe
    }

    /// The direction of the point `from` in the frame of `head`.
    pub fn towards(from: Vec3, head: &Head) -> Vec3 {
        let t = (from - head.at).unit_or(head.forward);
        vec3(t.dot(head.right), t.dot(head.up), t.dot(head.forward))
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let piece = self.bytes.get(self.at..self.at + n).ok_or("a head response file cut short")?;
        self.at += n;
        Ok(piece)
    }

    fn word(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }

    fn floats(&mut self, n: usize) -> Result<Vec<f32>, String> {
        Ok(self.take(4 * n)?.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect())
    }
}

/// The weights by which three directions sum to `d`, if they are not on
/// one plane through the centre.
fn solve(m: [Vec3; 3], d: Vec3) -> Option<[f64; 3]> {
    let det = m[0].dot(m[1].cross(m[2]));
    if det.abs() < 1e-12 {
        return None;
    }
    Some([d.dot(m[1].cross(m[2])) / det, m[0].dot(d.cross(m[2])) / det, m[0].dot(m[1].cross(d)) / det])
}

/// The loudspeakers' feeds for a block, and what each one's filter keeps of
/// the last: what the ways write into, and what the engine hears at the end
/// of a block.
#[derive(Clone, Debug)]
pub struct Speakers {
    pub hrir: std::sync::Arc<Hrir>,
    /// For each loudspeaker and ear, the block's feed.
    pub feed: Vec<[[f32; BLOCK]; 2]>,
    /// Whether anything was written to it this block.
    pub fed: Vec<bool>,
    /// For each loudspeaker and ear, the last `taps − 1` samples of its feed.
    past: Vec<[Vec<f32>; 2]>,
    /// Whether that is all nothing.
    quiet: Vec<bool>,
}

impl Speakers {
    pub fn new(hrir: std::sync::Arc<Hrir>) -> Speakers {
        let n = hrir.speakers.len();
        let keep = hrir.taps - 1;
        Speakers {
            feed: vec![[[0.0; BLOCK]; 2]; n],
            fed: vec![false; n],
            past: vec![[vec![0.0; keep], vec![0.0; keep]]; n],
            quiet: vec![true; n],
            hrir,
        }
    }

    /// Each loudspeaker's feed through its two responses, added to `left`
    /// and `right`; the feeds are then cleared.
    pub fn hear(&mut self, left: &mut [f32; BLOCK], right: &mut [f32; BLOCK]) {
        let taps = self.hrir.taps;
        let keep = taps - 1;
        let mut line = vec![0.0f32; keep + BLOCK];
        for s in 0..self.feed.len() {
            if !self.fed[s] && self.quiet[s] {
                continue;
            }
            for (ear, out) in [&mut *left, &mut *right].into_iter().enumerate() {
                line[..keep].copy_from_slice(&self.past[s][ear]);
                line[keep..].copy_from_slice(&self.feed[s][ear]);
                let h = &self.hrir.speakers[s].1[ear];
                // Tap by tap over the whole block: each a sum of products along
                // the block, which a processor does several at once.
                let mut sum = [0.0f32; BLOCK];
                for (k, &tap) in h.iter().enumerate() {
                    let from = &line[keep - k..keep - k + BLOCK];
                    for (acc, x) in sum.iter_mut().zip(from) {
                        *acc += tap * x;
                    }
                }
                for (to, x) in out.iter_mut().zip(&sum) {
                    *to += x;
                }
                self.past[s][ear].copy_from_slice(&line[BLOCK..]);
                self.feed[s][ear] = [0.0; BLOCK];
            }
            self.quiet[s] = !self.fed[s] && self.past[s].iter().flatten().all(|x| x.abs() < 1e-12);
            if self.quiet[s] {
                self.past[s].iter_mut().for_each(|p| p.fill(0.0));
            }
            self.fed[s] = false;
        }
    }
}

/// The loudspeakers' directions: rings, as alike on the left and on the
/// right, closest together on the horizon, where nearly every sound of a
/// city is — 16 on it, 12 at 35° above and below, 6 at 70°, and the poles.
pub fn rings() -> Vec<Vec3> {
    let mut out = Vec::new();
    for (elevation, count) in [(0.0, 16), (35.0, 12), (-35.0, 12), (70.0, 6), (-70.0, 6)] {
        for i in 0..count {
            out.push(sofa_direction(i as f64 * 360.0 / count as f64, elevation));
        }
    }
    out.push(sofa_direction(0.0, 90.0));
    out.push(sofa_direction(0.0, -90.0));
    out
}

/// The triangles of the convex hull of points on a sphere, outward.
fn hull(points: &[Vec3]) -> Vec<[u16; 3]> {
    let n = points.len();
    let mut out = Vec::new();
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                let (a, b, c) = (points[i], points[j], points[k]);
                let normal = (b - a).cross(c - a);
                if normal.length() < 1e-9 {
                    continue;
                }
                let (mut above, mut below) = (false, false);
                for (m, p) in points.iter().enumerate() {
                    if m == i || m == j || m == k {
                        continue;
                    }
                    let side = normal.dot(*p - a);
                    above |= side > 1e-9;
                    below |= side < -1e-9;
                    if above && below {
                        break;
                    }
                }
                if above && below {
                    continue;
                }
                // Outward: away from the centre.
                if normal.dot(a) >= 0.0 && !above {
                    out.push([i as u16, j as u16, k as u16]);
                } else if !below {
                    out.push([i as u16, k as u16, j as u16]);
                }
            }
        }
    }
    out
}

/// A measured set's direction in the head's frame: SOFA counts azimuth from
/// ahead round to the left, and elevation up, in degrees.
pub fn sofa_direction(azimuth: f64, elevation: f64) -> Vec3 {
    let (a, e) = (azimuth.to_radians(), elevation.to_radians());
    vec3(-e.cos() * a.sin(), e.sin(), e.cos() * a.cos())
}

/// The time by which the first of two responses hears later than the
/// second, samples: where their low frequencies (under 1.5 kHz) agree
/// best, within a millisecond, to a part of a sample.
fn delay(first: &[f64], second: &[f64], rate: f64) -> f64 {
    let low = |x: &[f64]| {
        let a = 1.0 - (-std::f64::consts::TAU * 1_500.0 / rate).exp();
        let mut s = 0.0;
        x.iter()
            .map(|&v| {
                s += (v - s) * a;
                s
            })
            .collect::<Vec<f64>>()
    };
    let (a, b) = (low(first), low(second));
    let most = (rate * 1e-3) as isize;
    let at = |lag: isize| -> f64 {
        (0..a.len() as isize)
            .filter_map(|t| {
                let u = t - lag;
                (u >= 0 && (u as usize) < b.len()).then(|| a[t as usize] * b[u as usize])
            })
            .sum()
    };
    let scores: Vec<f64> = (-most..=most).map(at).collect();
    let (best, _) = scores.iter().enumerate().fold((0, f64::MIN), |m, (i, &s)| if s > m.1 { (i, s) } else { m });
    let mut lag = best as f64 - most as f64;
    if best > 0 && best + 1 < scores.len() {
        let (l, c, r) = (scores[best - 1], scores[best], scores[best + 1]);
        let bend = l - 2.0 * c + r;
        if bend.abs() > 1e-300 {
            lag += 0.5 * (l - r) / bend;
        }
    }
    lag
}

/// A measured set's file for the engine, from its responses `ir` (for each
/// of `positions.len() / 3` directions, the left ear's `taps` samples then
/// the right's) and their directions (azimuth and elevation in degrees, and
/// distance, as SOFA gives them): loudspeakers at the directions `speakers`
/// ([`rings`]), each moved to the nearest measured direction, their
/// responses cut to `keep`
/// taps from a little before each ear's peak, levels such that a sound from
/// ahead keeps its energy at each ear; and the time between the ears on a
/// grid of 5°.
pub fn derive(
    ir: &[f64],
    positions: &[f64],
    taps: usize,
    rate: u32,
    speakers: &[Vec3],
    keep: usize,
) -> Result<Hrir, String> {
    let count = positions.len() / 3;
    if count < speakers.len() || speakers.len() < 4 || ir.len() != count * 2 * taps || keep > taps || keep < 8 {
        return Err(format!("{count} directions, {} samples: not a set of {taps} taps", ir.len()));
    }
    let dirs: Vec<Vec3> = (0..count).map(|m| sofa_direction(positions[3 * m], positions[3 * m + 1])).collect();
    let response = |m: usize, ear: usize| &ir[(2 * m + ear) * taps..(2 * m + ear + 1) * taps];
    let nearest = |d: Vec3, taken: &[usize]| {
        (0..count)
            .filter(|m| !taken.contains(m))
            .max_by(|&a, &b| dirs[a].dot(d).total_cmp(&dirs[b].dot(d)))
            .expect("a direction")
    };
    // The loudspeakers, each at a measured direction.
    let mut chosen: Vec<usize> = Vec::new();
    for &d in speakers {
        let m = nearest(d, &chosen);
        chosen.push(m);
    }
    // The time between the ears, on the grid.
    let steps = [72usize, 37usize];
    let mut table = Vec::with_capacity(steps[0] * steps[1]);
    let mut known: std::collections::HashMap<usize, f64> = std::collections::HashMap::new();
    for j in 0..steps[1] {
        for i in 0..steps[0] {
            let d =
                sofa_direction(i as f64 * 360.0 / steps[0] as f64, -90.0 + j as f64 * 180.0 / (steps[1] - 1) as f64);
            let m = nearest(d, &[]);
            let lag = *known.entry(m).or_insert_with(|| delay(response(m, 0), response(m, 1), f64::from(rate)));
            table.push((lag / f64::from(rate)) as f32);
        }
    }
    // Both ears' responses cut from a little before the nearer ear's peak,
    // the farther ear's moved earlier by the time between them, so that the
    // pair has none of its own: each way gives it its own. Faded at both ends.
    let before = keep / 6;
    let window = |h: &[f64], start: usize| -> Vec<f64> {
        let w = |x: f64| 0.5 - 0.5 * (std::f64::consts::PI * x).cos();
        (0..keep)
            .map(|k| {
                let fade_in = ((k as f64 + 0.5) / 4.0).min(1.0);
                let fade_out = (((keep - k) as f64 - 0.5) / (keep as f64 / 4.0)).min(1.0);
                h.get(start + k).copied().unwrap_or(0.0) * w(fade_in) * w(fade_out)
            })
            .collect()
    };
    let peak =
        |h: &[f64]| h.iter().enumerate().fold((0, 0.0), |m, (i, &v)| if v.abs() > m.1 { (i, v.abs()) } else { m });
    let pair = |m: usize| -> [Vec<f64>; 2] {
        let (left, right) = (response(m, 0), response(m, 1));
        let lag = delay(left, right, f64::from(rate)).round() as isize;
        let (pl, pr) = (peak(left), peak(right));
        // The nearer ear: the louder.
        let (start_left, start_right) = if pr.1 >= pl.1 {
            let r = pr.0.saturating_sub(before) as isize;
            (r + lag, r)
        } else {
            let l = pl.0.saturating_sub(before) as isize;
            (l, l - lag)
        };
        let clamp = |x: isize| x.clamp(0, (taps - 1) as isize) as usize;
        [window(left, clamp(start_left)), window(right, clamp(start_right))]
    };
    let ahead = nearest(vec3(0.0, 0.0, 1.0), &[]);
    let energy = |h: &[f64]| h.iter().map(|x| x * x).sum::<f64>();
    let front = pair(ahead);
    let scale = 1.0 / ((energy(&front[0]) + energy(&front[1])) / 2.0).sqrt();
    let list = chosen
        .iter()
        .map(|&m| (dirs[m], pair(m).map(|h| h.iter().map(|x| (x * scale) as f32).collect::<Vec<f32>>())))
        .collect::<Vec<_>>();
    Hrir::new(rate, list, table, steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Points spread evenly over the sphere (a Fibonacci lattice).
    fn spread(n: usize) -> Vec<Vec3> {
        let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
        (0..n)
            .map(|i| {
                let y = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
                let r = (1.0 - y * y).sqrt();
                let a = golden * i as f64;
                vec3(r * a.cos(), y, r * a.sin())
            })
            .collect()
    }

    /// A set of `count` loudspeakers whose responses are an impulse, louder
    /// in the ear nearer the sound; the time between the ears from a sphere.
    pub(crate) fn plain(count: usize) -> Hrir {
        let points = spread(count);
        let speakers = points
            .iter()
            .map(|d| {
                let gain = |across: f64| (1.0 + 0.5 * across) as f32;
                let mut l = vec![0.0f32; 16];
                let mut r = vec![0.0f32; 16];
                l[2] = gain(-d.x);
                r[2] = gain(d.x);
                (*d, [l, r])
            })
            .collect();
        let steps = [72, 37];
        let mut itd = Vec::new();
        for j in 0..steps[1] {
            for i in 0..steps[0] {
                let d = sofa_direction(i as f64 * 5.0, -90.0 + j as f64 * 5.0);
                itd.push((d.x * 0.0006) as f32);
            }
        }
        Hrir::new(48_000, speakers, itd, steps).unwrap()
    }

    #[test]
    fn the_rings_are_alike_on_both_sides() {
        let rings = rings();
        assert_eq!(rings.len(), 54);
        for d in &rings {
            assert!(rings.iter().any(|e| e.distance(vec3(-d.x, d.y, d.z)) < 1e-9), "{d:?}");
        }
        assert_eq!(rings.iter().filter(|d| d.y.abs() < 1e-9).count(), 16);
        assert!(hull(&rings).len() >= 2 * 54 - 4);
    }

    #[test]
    fn the_triangles_cover_the_sphere_and_a_share_sums_to_one() {
        let set = plain(50);
        // A closed hull of n points on a sphere has 2n − 4 triangles.
        assert_eq!(set.triangles.len(), 96);
        for d in spread(300) {
            let share = set.share(d);
            let sum: f32 = share.iter().map(|(_, w)| w).sum();
            assert!((sum - 1.0).abs() < 1e-5, "{sum}");
            assert!(share.iter().all(|(_, w)| *w >= 0.0));
            // The mixture of the three directions points where the sound comes from.
            let mixed = share.iter().fold(Vec3::ZERO, |m, (i, w)| m + set.speakers[*i as usize].0 * f64::from(*w));
            assert!(mixed.unit_or(Vec3::Z).dot(d) > 0.97, "{d:?}");
        }
        // A sound straight at a loudspeaker goes to it alone.
        let (d, _) = &set.speakers[7];
        let share = set.share(*d);
        assert!(share.iter().any(|&(i, w)| i == 7 && w > 0.999));
    }

    #[test]
    fn the_table_reads_between_its_points_and_left_is_left() {
        let set = plain(20);
        // From the left (−x): the right ear later, so the left minus the right is negative.
        assert!((set.itd(vec3(-1.0, 0.0, 0.0)) + 0.0006).abs() < 1e-6);
        assert!((set.itd(vec3(1.0, 0.0, 0.0)) - 0.0006).abs() < 1e-6);
        assert!(set.itd(vec3(0.0, 0.0, 1.0)).abs() < 1e-6);
        let d = vec3(0.3, 0.2, 0.9).unit_or(Vec3::Z);
        assert!((set.itd(d) - d.x * 0.0006).abs() < 2e-5);
        // SOFA's directions: azimuth 90° is the left, elevation 90° up.
        assert!(sofa_direction(90.0, 0.0).distance(vec3(-1.0, 0.0, 0.0)) < 1e-12);
        assert!(sofa_direction(0.0, 90.0).distance(vec3(0.0, 1.0, 0.0)) < 1e-12);
    }

    #[test]
    fn a_file_goes_there_and_back_and_a_bad_one_says_so() {
        let set = plain(12);
        let bytes = set.to_bytes();
        let back = Hrir::from_bytes(&bytes).unwrap();
        assert_eq!(back.triangles, set.triangles);
        assert_eq!(back.itd, set.itd);
        assert_eq!(back.speakers[3].1, set.speakers[3].1);
        assert!(Hrir::from_bytes(&bytes[..bytes.len() - 1]).is_err());
        assert!(Hrir::from_bytes(b"nothing of the kind").is_err());
    }

    #[test]
    fn a_set_is_derived_with_its_delays_and_its_level() {
        // Twelve directions round the horizon; each response an impulse, the far ear later by up to 20 samples.
        let (taps, count) = (64, 36);
        let mut ir = Vec::new();
        let mut positions = Vec::new();
        for m in 0..count {
            let (az, el) = ((m % 12) as f64 * 30.0, [-40.0, 0.0, 40.0][m / 12]);
            positions.extend([az, el, 1.2]);
            let x = sofa_direction(az, el).x;
            let lag = (x * 20.0).round() as isize;
            for ear in 0..2 {
                let mut h = vec![0.0; taps];
                let at = 10 + if ear == 0 { lag.max(0) } else { (-lag).max(0) };
                h[at as usize] = 2.0;
                ir.extend(h);
            }
        }
        let six = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z];
        let set = derive(&ir, &positions, taps, 48_000, &six, 32).unwrap();
        assert_eq!(set.speakers.len(), 6);
        // From the right the left ear is later; ahead, no time between.
        assert!(
            (set.itd(vec3(1.0, 0.0, 0.0)) * 48_000.0 - 20.0).abs() < 1.5,
            "{}",
            set.itd(vec3(1.0, 0.0, 0.0)) * 48e3
        );
        assert!(set.itd(vec3(0.0, 0.0, 1.0)).abs() * 48_000.0 < 0.5);
        // Each response cut round its peak, and ahead keeps its energy.
        for (_, pair) in &set.speakers {
            for h in pair {
                assert_eq!(h.len(), 32);
                let peak =
                    h.iter().enumerate().fold((0, 0.0f32), |m, (i, &v)| if v.abs() > m.1 { (i, v.abs()) } else { m });
                assert_eq!(peak.0, 32 / 6);
                assert!((peak.1 - 1.0).abs() < 1e-5, "{}", peak.1);
            }
        }
    }
}
