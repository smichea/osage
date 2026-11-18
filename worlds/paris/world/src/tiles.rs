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

use bevy::math::{IVec3, Vec3};
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

/// The tiles a walker whose eyes are at `eye` needs within `horizon`
/// seconds at `speed` m/s: those of the blocks of every cube it can reach
/// by then.
pub fn needed(eye: Vec3, speed: f32, horizon: f32) -> Vec<Tile> {
    let mut tiles = BTreeSet::new();
    for cube in reachable(eye, speed, horizon) {
        tiles.extend(Block { cube }.tiles());
    }
    tiles.into_iter().collect()
}

/// The probes such a walker needs (D-137): those of every cube it can
/// reach by then.
pub fn needed_probes(eye: Vec3, speed: f32, horizon: f32) -> Vec<String> {
    reachable(eye, speed, horizon).into_iter().flat_map(probe_keys).collect()
}

/// The light such a walker needs, the nearest cube first (a client decodes
/// in that order): its cubes' probes where `there` says they are (D-137),
/// else their blocks' surface tiles (D-136); only what is there, each once.
/// First the centres' probes, with which alone a client can draw a cube;
/// then the tiles of the cubes without probes; then the other probes.
pub fn needed_light(eye: Vec3, speed: f32, horizon: f32, there: impl Fn(&str) -> bool) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut keys = Vec::new();
    let mut add = |key: String| {
        if there(&key) && seen.insert(key.clone()) {
            keys.push(key);
        }
    };
    let cubes = reachable(eye, speed, horizon);
    let (baked, bare): (Vec<IVec3>, Vec<IVec3>) = cubes.iter().partition(|&&cube| there(&probe_keys(cube)[0]));
    for &cube in &baked {
        add(probe_keys(cube).swap_remove(0));
    }
    for &cube in &bare {
        Block { cube }.tiles().into_iter().for_each(|t| add(t.key()));
    }
    for &cube in &baked {
        probe_keys(cube).into_iter().skip(1).for_each(&mut add);
    }
    keys
}

/// The cubes a walker whose eyes are at `eye` can reach within `horizon`
/// seconds at `speed` m/s, and a metre more, the nearest first: on its
/// layer, and the next one up or down when its eyes are near it.
pub fn reachable(eye: Vec3, speed: f32, horizon: f32) -> Vec<IVec3> {
    let reach = speed.max(0.0) * horizon.max(0.0) + 1.0;
    let mut layers = BTreeSet::new();
    for dy in [-0.75, 0.0, 0.75] {
        layers.insert(((eye.y + dy) / CUBE).floor() as i32);
    }
    let (x0, x1) = (((eye.x - reach) / CUBE).floor() as i32, ((eye.x + reach) / CUBE).floor() as i32);
    let (z0, z1) = (((eye.z - reach) / CUBE).floor() as i32, ((eye.z + reach) / CUBE).floor() as i32);
    let mut cubes = Vec::new();
    for x in x0..=x1 {
        for z in z0..=z1 {
            // The cube's nearest point within reach.
            let nearest = Vec3::new(
                eye.x.clamp(x as f32 * CUBE, (x + 1) as f32 * CUBE),
                eye.y,
                eye.z.clamp(z as f32 * CUBE, (z + 1) as f32 * CUBE),
            );
            if nearest.distance(eye) > reach {
                continue;
            }
            let gap = nearest.distance(eye);
            cubes.extend(layers.iter().map(|&y| (gap, IVec3::new(x, y, z))));
        }
    }
    // The walker's own cube first, then by how near each comes.
    let own = (eye / CUBE).floor().as_ivec3();
    cubes.sort_by(|a, b| (a.1 != own).cmp(&(b.1 != own)).then(a.0.total_cmp(&b.0)));
    cubes.into_iter().map(|(_, cube)| cube).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_walker_needs_the_probes_of_the_cubes_it_can_reach() {
        let eye = Vec3::new(25.0, 1.9, -61.6);
        let still = needed_probes(eye, 0.0, 2.0);
        assert_eq!(still, probe_keys(IVec3::new(8, 0, -21)), "standing in the middle of its cube");
        assert_eq!(still[0], "c_8_0_-21_0");
        let walking = needed_probes(eye, 1.4, 2.0);
        assert!(walking.len() > still.len() && walking.len().is_multiple_of(PROBES));
        assert!(walking.contains(&"c_8_0_-20_0".to_owned()), "the cube ahead to the south");
        // A cube with its probes baked sends them, one without its block's tiles.
        let baked = |k: &str| k.starts_with("c_8_0_-21_") || !k.starts_with("c_");
        let light = needed_light(eye, 0.0, 2.0, baked);
        assert_eq!(light, probe_keys(IVec3::new(8, 0, -21)));
        let light = needed_light(Vec3::new(25.0, 1.9, -58.6), 0.0, 2.0, baked);
        assert_eq!(light.len(), 30, "the block of a cube without probes: its tiles");
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
    fn a_walker_needs_more_ahead_when_it_goes_faster() {
        let eye = Vec3::new(10.0, 1.7, 4.0);
        let standing = needed(eye, 0.0, 2.0);
        assert!(Block::of(eye).tiles().iter().all(|t| standing.contains(t)));
        let running = needed(eye, 4.0, 2.0);
        assert!(running.len() > standing.len());
    }
}
