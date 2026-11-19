// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The light coming into a walker's block (D-136).
//!
//! A walker's block is the 9 cubes of 3 m around its own, one layer: the
//! client traces their model. All else reaches it as the light that comes
//! in through the block's outer surface, cut into tiles: the faces of the
//! 3 m cubes on the block's edge. Each tile's light is a picture from its
//! centre looking out (its probe, a fisheye over [`PROBE_FOV`]), with how far each of
//! its pixels lies. A tile is the same for every block it bounds, so the
//! blocks along a street share most of theirs; the server sends a walker
//! those of the blocks it can reach before long ([`needed`]).
//!
//! Frame: the glTF's — x east, y up, z south.

use std::collections::BTreeSet;

use bevy::math::{IVec3, Vec2, Vec3};
use serde::{Deserialize, Serialize};

/// A cube's side, m.
pub const CUBE: f32 = 3.0;
/// The field of a tile's probe, degrees: an equidistant fisheye a little
/// wider than the half sphere, so that what lies along the tile's plane
/// falls well inside its circle, not on its rim.
pub const PROBE_FOV: f32 = 200.0;

/// The way a tile's probe looks: out of the block, through the tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Face {
    NegX,
    PosX,
    NegY,
    PosY,
    NegZ,
    PosZ,
}

impl Face {
    pub const ALL: [Face; 6] = [Face::NegX, Face::PosX, Face::NegY, Face::PosY, Face::NegZ, Face::PosZ];

    /// 0 for x, 1 for y, 2 for z.
    pub fn axis(self) -> usize {
        self as usize / 2
    }

    pub fn positive(self) -> bool {
        self as usize % 2 == 1
    }

    /// Where the probe looks.
    pub fn normal(self) -> Vec3 {
        let mut n = [0.0; 3];
        n[self.axis()] = if self.positive() { 1.0 } else { -1.0 };
        Vec3::from(n)
    }

    /// The probe's up: the sky for the sides, north (-z) for the top and
    /// the bottom.
    pub fn up(self) -> Vec3 {
        if self.axis() == 1 { Vec3::NEG_Z } else { Vec3::Y }
    }

    /// The probe's right.
    pub fn right(self) -> Vec3 {
        self.normal().cross(self.up())
    }

    pub fn name(self) -> &'static str {
        ["-x", "+x", "-y", "+y", "-z", "+z"][self as usize]
    }

    fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.name() == name)
    }
}

/// A tile: a face of a 3 m cube, its probe looking out through it along
/// `face`; `plane` numbers the face's plane along that axis (× 3 m), `a`
/// and `b` the cube along the two other axes, in x, y, z order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Tile {
    pub face: Face,
    pub plane: i32,
    pub a: i32,
    pub b: i32,
}

impl Tile {
    /// The other two axes, in x, y, z order.
    fn axes(face: Face) -> (usize, usize) {
        match face.axis() {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        }
    }

    /// Where its probe stands: the tile's centre.
    pub fn centre(&self) -> Vec3 {
        let mut c = [0.0; 3];
        let (a, b) = Self::axes(self.face);
        c[self.face.axis()] = self.plane as f32 * CUBE;
        c[a] = (self.a as f32 + 0.5) * CUBE;
        c[b] = (self.b as f32 + 0.5) * CUBE;
        Vec3::from(c)
    }

    /// Its name in paths and messages: `FACE_PLANE_A_B`, e.g. `+x_12_0_-3`.
    pub fn key(&self) -> String {
        format!("{}_{}_{}_{}", self.face.name(), self.plane, self.a, self.b)
    }

    pub fn parse(key: &str) -> Option<Self> {
        let mut parts = key.split('_');
        let face = Face::parse(parts.next()?)?;
        let mut n = || parts.next()?.parse::<i32>().ok();
        let (plane, a, b) = (n()?, n()?, n()?);
        Some(Self { face, plane, a, b })
    }
}

/// A walker's block: the 9 cubes of 3 m around its cube, one layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Block {
    pub cube: IVec3,
}

impl Block {
    /// The block of a walker whose eyes are at `eye`.
    pub fn of(eye: Vec3) -> Self {
        Self { cube: (eye / CUBE).floor().as_ivec3() }
    }

    pub fn min(&self) -> Vec3 {
        (self.cube - IVec3::new(1, 0, 1)).as_vec3() * CUBE
    }

    pub fn max(&self) -> Vec3 {
        (self.cube + IVec3::new(2, 1, 2)).as_vec3() * CUBE
    }

    pub fn contains(&self, p: Vec3) -> bool {
        p.cmpge(self.min()).all() && p.cmplt(self.max()).all()
    }

    /// The tiles of its outer surface: 3 on each side, 9 above, 9 below.
    pub fn tiles(&self) -> Vec<Tile> {
        let c = self.cube;
        let mut tiles = Vec::with_capacity(30);
        for k in -1..=1 {
            tiles.push(Tile { face: Face::NegX, plane: c.x - 1, a: c.y, b: c.z + k });
            tiles.push(Tile { face: Face::PosX, plane: c.x + 2, a: c.y, b: c.z + k });
            tiles.push(Tile { face: Face::NegZ, plane: c.z - 1, a: c.x + k, b: c.y });
            tiles.push(Tile { face: Face::PosZ, plane: c.z + 2, a: c.x + k, b: c.y });
            for m in -1..=1 {
                tiles.push(Tile { face: Face::NegY, plane: c.y, a: c.x + k, b: c.z + m });
                tiles.push(Tile { face: Face::PosY, plane: c.y + 1, a: c.x + k, b: c.z + m });
            }
        }
        tiles
    }
}

/// How many probes a cube has inside it (D-137).
pub const PROBES: usize = 5;

/// The names of a cube's probes (D-137, `tools/bistro/cube-probes.py`):
/// `c_X_Y_Z_K`, K from 0, the cube's centre, to 4, towards its corners.
pub fn probe_keys(cube: IVec3) -> Vec<String> {
    (0..PROBES).map(|k| format!("c_{}_{}_{}_{k}", cube.x, cube.y, cube.z)).collect()
}

/// How far either side of its way a walker's light is sent, metres
/// (D-139).
pub const BESIDE: f32 = 1.0;

/// Below this speed, m/s, a walker stands: its own cube's other probes are
/// sent too (D-139).
pub const STANDING: f32 = 0.5;

/// Over this speed, m/s, a walker runs (walking 1.4, running 4): the
/// centres' probes are sent at half their size, where they are packed so
/// (D-140: decoded three times faster, blurred only at the sides, where a
/// runner's eyes are not).
pub const RUNNING: f32 = 2.5;

/// A probe's key at half its size (D-140, `light-pack --half`):
/// `h_X_Y_Z_K` for `c_X_Y_Z_K`.
pub fn half_key(key: &str) -> String {
    key.replacen("c_", "h_", 1)
}

/// The cubes a walker whose eyes are at `eye`, going at `velocity` m/s,
/// meets within `horizon` seconds, `BESIDE` either side of its way: its own
/// first, then in the order it meets them; on its layer, and the next one
/// up or down where its eyes are near it.
pub fn ahead(eye: Vec3, velocity: Vec3, horizon: f32) -> Vec<IVec3> {
    let mut cubes = vec![(eye / CUBE).floor().as_ivec3()];
    let flat = Vec3::new(velocity.x, 0.0, velocity.z);
    let steps = (horizon.max(0.0) * 10.0).ceil() as usize;
    for i in 0..=steps {
        let p = eye + flat * (i as f32 / 10.0).min(horizon.max(0.0));
        let span = |v: f32| ((v - BESIDE) / CUBE).floor() as i32..=((v + BESIDE) / CUBE).floor() as i32;
        let mut here = Vec::new();
        for x in span(p.x) {
            for z in span(p.z) {
                let nearest = Vec2::new(
                    p.x.clamp(x as f32 * CUBE, (x + 1) as f32 * CUBE),
                    p.z.clamp(z as f32 * CUBE, (z + 1) as f32 * CUBE),
                );
                let gap = nearest.distance(Vec2::new(p.x, p.z));
                if gap <= BESIDE {
                    let layers: BTreeSet<i32> =
                        [-0.75, 0.0, 0.75].iter().map(|dy| ((p.y + dy) / CUBE).floor() as i32).collect();
                    here.extend(layers.into_iter().map(|y| (gap, IVec3::new(x, y, z))));
                }
            }
        }
        here.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, cube) in here {
            if !cubes.contains(&cube) {
                cubes.push(cube);
            }
        }
    }
    cubes
}

/// The light a walker whose eyes are at `eye`, going at `velocity` m/s,
/// needs within `horizon` seconds (D-139), in the order a client should
/// decode it: of the cubes on its way (`ahead`) and the four beside its own,
/// into which it could turn, the centres' probes first (D-137: with its
/// centre's alone a client draws a cube); then the surface tiles of the
/// cubes without probes (D-136); then, standing, its own cube's four other
/// probes; running, the centres at half their size (D-140). Only what
/// `there` says is there, each once. All five probes of
/// every cube within reach, as before, came to nine probes a second walking
/// down the whole street, fifteen of a laptop's cores to decode.
pub fn needed_light(eye: Vec3, velocity: Vec3, horizon: f32, there: impl Fn(&str) -> bool) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut keys = Vec::new();
    let mut add = |key: String| {
        if there(&key) && seen.insert(key.clone()) {
            keys.push(key);
        }
    };
    let own = (eye / CUBE).floor().as_ivec3();
    let mut cubes = ahead(eye, velocity, horizon);
    for beside in [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z].map(|d| own + d) {
        if !cubes.contains(&beside) {
            cubes.push(beside);
        }
    }
    let speed = Vec2::new(velocity.x, velocity.z).length();
    let centre = |cube: IVec3| probe_keys(cube).swap_remove(0);
    let (baked, bare): (Vec<IVec3>, Vec<IVec3>) =
        cubes.iter().partition(|&&cube| there(&centre(cube)) || there(&half_key(&centre(cube))));
    for &cube in &baked {
        let (full, half) = (centre(cube), half_key(&centre(cube)));
        let small = (speed > RUNNING && there(&half)) || !there(&full);
        add(if small { half } else { full });
    }
    for &cube in &bare {
        Block { cube }.tiles().into_iter().for_each(|t| add(t.key()));
    }
    if speed < STANDING && baked.contains(&own) {
        probe_keys(own).into_iter().skip(1).for_each(&mut add);
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_walker_is_sent_the_centres_on_its_way_and_its_own_cube_whole_standing() {
        let eye = Vec3::new(25.0, 1.9, -61.6);
        let own = IVec3::new(8, 0, -21);
        let baked = |k: &str| k.starts_with("c_");
        // Standing in its cube: its five probes, the centres of the four
        // beside it and of the one a metre away.
        let standing = needed_light(eye, Vec3::ZERO, 2.0, baked);
        assert_eq!(standing[0], "c_8_0_-21_0");
        assert!(probe_keys(own).iter().all(|k| standing.contains(k)));
        assert_eq!(standing.len(), 9, "{standing:?}");
        // Walking south: the centres of the cubes it meets, not its own others.
        let walking = needed_light(eye, Vec3::new(0.0, 0.0, 1.4), 2.0, baked);
        assert_eq!(walking[0], "c_8_0_-21_0");
        assert!(walking.contains(&"c_8_0_-20_0".to_owned()), "the cube ahead to the south");
        assert!(!walking.contains(&"c_8_0_-21_1".to_owned()), "walking, its own cube's centre alone");
        assert!(walking.iter().all(|k| k.ends_with("_0")));
        // Running: farther ahead, no wider.
        let running = needed_light(eye, Vec3::new(0.0, 0.0, 4.0), 2.0, baked);
        assert!(running.contains(&"c_8_0_-19_0".to_owned()) && running.contains(&"c_8_0_-18_0".to_owned()));
        assert!(!running.contains(&"c_10_0_-20_0".to_owned()), "two cubes to the side");
        // Running where the halves are packed: the centres at half their size.
        let halved = |k: &str| k.starts_with("c_") || k.starts_with("h_");
        let running = needed_light(eye, Vec3::new(0.0, 0.0, 4.0), 2.0, halved);
        assert!(running.iter().all(|k| k.starts_with("h_") && k.ends_with("_0")), "{running:?}");
        assert!(needed_light(eye, Vec3::new(0.0, 0.0, 1.4), 2.0, halved).iter().all(|k| k.starts_with("c_")));
        // A cube without probes: its block's tiles.
        let only = |k: &str| k.starts_with("c_8_0_-21_") || !(k.starts_with("c_") || k.starts_with("h_"));
        let light = needed_light(Vec3::new(25.0, 1.9, -58.6), Vec3::ZERO, 2.0, only);
        assert!(light.len() >= 30, "the block of a cube without probes: its tiles");
    }

    #[test]
    fn a_block_has_thirty_tiles_on_its_surface_looking_out() {
        let block = Block::of(Vec3::new(10.0, 1.7, -4.0));
        assert_eq!(block.cube, IVec3::new(3, 0, -2));
        let tiles = block.tiles();
        assert_eq!(tiles.len(), 30);
        let (lo, hi) = (block.min(), block.max());
        for t in &tiles {
            let c = t.centre();
            let axis = t.face.axis();
            let on = if t.face.positive() { hi[axis] } else { lo[axis] };
            assert!((c[axis] - on).abs() < 1e-4, "{t:?} not on its face");
            // Looking out: the block is behind the probe.
            assert!(((block.min() + block.max()) / 2.0 - c).dot(t.face.normal()) < 0.0);
            assert_eq!(Tile::parse(&t.key()), Some(*t));
            let (r, u, n) = (t.face.right(), t.face.up(), t.face.normal());
            assert!(r.dot(u).abs() < 1e-6 && r.dot(n).abs() < 1e-6 && (r.cross(u) - (-n)).length() < 1e-6);
        }
    }

    #[test]
    fn neighbouring_blocks_share_most_of_their_tiles() {
        let a: BTreeSet<_> = Block::of(Vec3::new(10.0, 1.7, 4.0)).tiles().into_iter().collect();
        let b: BTreeSet<_> = Block::of(Vec3::new(13.0, 1.7, 4.0)).tiles().into_iter().collect();
        let shared = a.intersection(&b).count();
        assert_eq!(shared, 6 + 6 + 2 + 2, "top and bottom 6 each, the sides along x 2 each");
    }

    #[test]
    fn a_walker_meets_more_cubes_ahead_when_it_goes_faster() {
        let eye = Vec3::new(10.0, 1.7, 4.0);
        let standing = ahead(eye, Vec3::ZERO, 2.0);
        assert_eq!(standing[0], Block::of(eye).cube);
        let walking = ahead(eye, Vec3::new(1.4, 0.0, 0.0), 2.0);
        let running = ahead(eye, Vec3::new(4.0, 0.0, 0.0), 2.0);
        assert!(standing.len() < walking.len() && walking.len() < running.len());
        assert!(running.iter().all(|c| (c.z - Block::of(eye).cube.z).abs() <= 1), "a metre either side, no more");
    }
}
