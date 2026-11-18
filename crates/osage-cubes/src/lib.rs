// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Nested cubes (D-134, D-135): what a viewer sees in 3D, what as
//! holograms, and which views of those holograms it needs soon.
//!
//! A world is cut into cubes of 81 m, 27 m, 9 m and 3 m, each level's grid
//! starting at the world's origin. The 3 m cube the viewer is in is the
//! centre: it and its 26 neighbours, a block of 9 m around the viewer, are
//! drawn in 3D (ray traced where the hardware can). When the viewer steps
//! into a neighbour, that one becomes the centre and the block slides by one
//! cube. Holograms begin beyond it, each coarser level centred the same way
//! at its own pace: the 27 cubes of 9 m around the 9 m cube the viewer is
//! in, the 27 of 27 m around its 27 m cube, then every 81 m cube of the
//! world ([`shown`]). A coarser neighbourhood holds the finer one, but they
//! do not line up — the finer is centred on a smaller cube — so each
//! hologram is drawn without what lies inside the next finer neighbourhood
//! (its [`Region`], told point by point by the hologram's depth): every
//! place is drawn once, by the finest level around it. Cubes with nothing
//! in them are left out.
//!
//! A cube's hologram is photographed from directions of a fixed set
//! ([`Directions`]), and a viewer needs only those it can see the cube from
//! before long: from where it is, or anywhere it can walk to within a few
//! seconds, towards any point of the cube ([`reach`]). Far cubes turn slowly
//! as the viewer walks and need a few views; near ones turn fast and need
//! many. What a world bakes is every view some place a viewer can stand
//! needs ([`Plan`]); what a server sends a viewer, the views its reach needs
//! now.

use std::{
    collections::{BTreeMap, BTreeSet},
    f32::consts::PI,
};

use bevy::math::{IVec3, Vec3};
use serde::{Deserialize, Serialize};

/// The side of a cube at each level, m.
pub const SIZES: [f32; 4] = [81.0, 27.0, 9.0, 3.0];
/// The finest level: the 3 m cubes around the viewer are drawn in 3D.
pub const FINEST: u8 = 3;
/// The finest level drawn as holograms.
pub const FINEST_HOLOGRAM: u8 = FINEST - 1;

/// A cube: its level (0 for 81 m, 3 for 3 m) and its place in that level's
/// grid, which starts at the world's origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Cube {
    pub level: u8,
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Cube {
    /// The cube of `level` that `p` is in.
    pub fn containing(p: Vec3, level: u8) -> Self {
        let i = (p / SIZES[level as usize]).floor().as_ivec3();
        Self { level, x: i.x, y: i.y, z: i.z }
    }

    pub fn size(self) -> f32 {
        SIZES[self.level as usize]
    }

    fn index(self) -> IVec3 {
        IVec3::new(self.x, self.y, self.z)
    }

    /// Its lowest corner.
    pub fn min(self) -> Vec3 {
        self.index().as_vec3() * self.size()
    }

    pub fn max(self) -> Vec3 {
        self.min() + self.size()
    }

    pub fn centre(self) -> Vec3 {
        self.min() + self.size() / 2.0
    }

    /// Half its diagonal: the radius of the sphere its hologram frames.
    pub fn radius(self) -> f32 {
        self.size() * 3f32.sqrt() / 2.0
    }

    pub fn contains(self, p: Vec3) -> bool {
        Self::containing(p, self.level) == self
    }

    /// The cube of the level above it is in; none at the top.
    pub fn parent(self) -> Option<Self> {
        let level = self.level.checked_sub(1)?;
        let i = self.index().div_euclid(IVec3::splat(3));
        Some(Self { level, x: i.x, y: i.y, z: i.z })
    }

    /// Its 27 cubes of the level below; none at the finest.
    pub fn children(self) -> impl Iterator<Item = Self> {
        let level = self.level + 1;
        let base = self.index() * 3;
        let n = if self.level < FINEST { 3 } else { 0 };
        (0..n).flat_map(move |dx| {
            (0..n).flat_map(move |dy| {
                (0..n).map(move |dz| Self { level, x: base.x + dx, y: base.y + dy, z: base.z + dz })
            })
        })
    }

    /// Its name in paths and messages: `LEVEL/X_Y_Z`.
    pub fn key(self) -> String {
        format!("{}/{}_{}_{}", self.level, self.x, self.y, self.z)
    }

    /// The cube a [`key`](Self::key) names.
    pub fn parse(key: &str) -> Option<Self> {
        let (level, rest) = key.split_once('/')?;
        let level: u8 = level.parse().ok().filter(|l| *l <= FINEST)?;
        let mut n = rest.split('_').map(str::parse::<i32>);
        let (x, y, z) = (n.next()?.ok()?, n.next()?.ok()?, n.next()?.ok()?);
        n.next().is_none().then_some(Self { level, x, y, z })
    }
}

/// A box, from its lowest corner to its highest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Region {
    pub min: Vec3,
    pub max: Vec3,
}

impl Region {
    /// The neighbourhood of `cube`: it and its 26 neighbours.
    pub fn around(cube: Cube) -> Self {
        Self { min: cube.min() - cube.size(), max: cube.max() + cube.size() }
    }

    pub fn contains(&self, p: Vec3) -> bool {
        p.cmpge(self.min).all() && p.cmplt(self.max).all()
    }

    /// Whether all of `cube` is inside it.
    pub fn holds(&self, cube: Cube) -> bool {
        cube.min().cmpge(self.min).all() && cube.max().cmple(self.max).all()
    }
}

/// A cube drawn as a hologram, without what lies in `carve`: the next finer
/// neighbourhood, drawn by its own cubes, in 3D or as holograms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hologram {
    pub cube: Cube,
    pub carve: Region,
}

/// What a viewer sees: the 3 m cubes around it in 3D (`models`, the first
/// the one it is in), the others as holograms.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub models: Vec<Cube>,
    pub holograms: Vec<Hologram>,
}

/// The 27 cubes of a cube's neighbourhood, the cube first.
fn neighbourhood(c: Cube) -> impl Iterator<Item = Cube> {
    std::iter::once(c).chain(
        (-1..=1)
            .flat_map(|dx| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dz| (dx, dy, dz))))
            .filter(|&d| d != (0, 0, 0))
            .map(move |(dx, dy, dz)| Cube { level: c.level, x: c.x + dx, y: c.y + dy, z: c.z + dz }),
    )
}

/// What a viewer at `eye` sees, in a world whose 81 m cubes with something
/// in them are `tops`; `filled` tells whether a cube of a lower level has
/// something in it.
pub fn shown(eye: Vec3, tops: &[Cube], filled: impl Fn(Cube) -> bool) -> Shown {
    let centre = |level| Cube::containing(eye, level);
    let models = neighbourhood(centre(FINEST)).filter(|&c| filled(c)).collect();
    let mut holograms = Vec::new();
    for level in 0..=FINEST_HOLOGRAM {
        let carve = Region::around(centre(level + 1));
        let cubes: Vec<Cube> =
            if level == 0 { tops.to_vec() } else { neighbourhood(centre(level)).filter(|&c| filled(c)).collect() };
        // A cube the finer neighbourhood holds whole is drawn by that one.
        holograms.extend(cubes.into_iter().filter(|&c| !carve.holds(c)).map(|cube| Hologram { cube, carve }));
    }
    Shown { models, holograms }
}

/// Directions about an axis: those within `half_angle` (radians) of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cone {
    pub axis: Vec3,
    pub half_angle: f32,
}

impl Cone {
    pub fn holds(&self, direction: Vec3) -> bool {
        self.half_angle >= PI || self.axis.angle_between(direction) <= self.half_angle
    }
}

/// The directions `cube` may be seen from within `horizon` seconds by a
/// viewer now at `eye` who moves at most `speed` m/s: from any point it can
/// reach by then towards any point of the cube, as directions from the cube
/// towards the viewer. Every direction when the viewer is, or may come,
/// that close.
pub fn reach(cube: Cube, eye: Vec3, speed: f32, horizon: f32) -> Cone {
    let to_eye = eye - cube.centre();
    let distance = to_eye.length();
    let spread = speed.max(0.0) * horizon.max(0.0) + cube.radius();
    if distance <= spread {
        return Cone { axis: to_eye.normalize_or(Vec3::Y), half_angle: PI };
    }
    Cone { axis: to_eye / distance, half_angle: (spread / distance).asin() }
}

/// The directions a level's cubes are photographed from: azimuths all
/// around, elevations from straight below to straight above, `step`
/// degrees apart. As in `osage-hologram`, a direction points from the cube
/// towards the camera: azimuth 0 and elevation 0 look along -Z at the
/// cube's +Z face.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Directions {
    pub step: f32,
}

impl Directions {
    /// Azimuths, degrees: -180 up to 180 excluded.
    pub fn azimuths(&self) -> Vec<f32> {
        let n = (360.0 / self.step).round() as usize;
        (0..n).map(|i| -180.0 + i as f32 * self.step).collect()
    }

    /// Elevations, degrees: -90 to 90.
    pub fn elevations(&self) -> Vec<f32> {
        let n = (180.0 / self.step).round() as usize;
        (0..=n).map(|i| -90.0 + i as f32 * self.step).collect()
    }

    /// The unit direction of an azimuth and an elevation, degrees.
    pub fn direction(azimuth: f32, elevation: f32) -> Vec3 {
        let (a, e) = (azimuth.to_radians(), elevation.to_radians());
        Vec3::new(a.sin() * e.cos(), e.sin(), a.cos() * e.cos())
    }

    /// The views (row of elevation, column of azimuth) worth having for the
    /// directions of `cone`: those in it, and those a step out of it that
    /// a view in it is drawn between.
    pub fn within(&self, cone: &Cone) -> Vec<(u16, u16)> {
        let widened = Cone { axis: cone.axis, half_angle: cone.half_angle + self.step.to_radians() * 1.5 };
        let azimuths = self.azimuths();
        let mut views = Vec::new();
        for (row, &elevation) in self.elevations().iter().enumerate() {
            for (column, &azimuth) in azimuths.iter().enumerate() {
                if widened.holds(Self::direction(azimuth, elevation)) {
                    views.push((row as u16, column as u16));
                }
            }
        }
        views
    }
}

/// The views to bake: for every cube, the views of it some viewer needs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub views: BTreeMap<Cube, BTreeSet<(u16, u16)>>,
}

impl Plan {
    /// The views needed from each of `eyes` (the places a viewer's eye can
    /// be, close enough together that between two of them nothing is
    /// missed): of every cube it sees, those towards it.
    pub fn new(
        eyes: impl IntoIterator<Item = Vec3>,
        tops: &[Cube],
        filled: impl Fn(Cube) -> bool,
        directions: impl Fn(u8) -> Directions,
    ) -> Self {
        let mut plan = Self::default();
        for eye in eyes {
            for Hologram { cube, .. } in shown(eye, tops, &filled).holograms {
                let cone = reach(cube, eye, 0.0, 0.0);
                plan.views.entry(cube).or_default().extend(directions(cube.level).within(&cone));
            }
        }
        plan
    }

    /// How many views, every cube together.
    pub fn len(&self) -> usize {
        self.views.values().map(BTreeSet::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn points() -> impl Iterator<Item = Vec3> {
        let mut state = 7u64;
        std::iter::repeat_with(move || {
            let mut next = || {
                state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                ((state >> 40) as f32 / (1u64 << 24) as f32 - 0.5) * 400.0
            };
            Vec3::new(next(), next() * 0.2, next())
        })
        .take(500)
    }

    #[test]
    fn a_point_is_in_its_cube_and_its_cubes_nest() {
        for p in points() {
            for level in 0..=FINEST {
                let cube = Cube::containing(p, level);
                assert!(cube.contains(p));
                let (lo, hi) = (cube.min(), cube.max());
                assert!(p.cmpge(lo).all() && p.cmplt(hi).all(), "{p} not in {lo}..{hi}");
                if let Some(parent) = cube.parent() {
                    assert_eq!(parent, Cube::containing(p, level - 1));
                    assert!(parent.children().any(|c| c == cube));
                }
            }
        }
    }

    #[test]
    fn a_key_names_its_cube() {
        let cube = Cube { level: 2, x: -3, y: 0, z: 14 };
        assert_eq!(cube.key(), "2/-3_0_14");
        assert_eq!(Cube::parse(&cube.key()), Some(cube));
        assert_eq!(Cube::parse("4/0_0_0"), None);
        assert_eq!(Cube::parse("1/0_0"), None);
    }

    /// A world of six 81 m cubes, and places in it.
    fn world() -> (Vec<Cube>, Region) {
        let tops = (-1..2).flat_map(|x| (-1..1).map(move |z| Cube { level: 0, x, y: 0, z })).collect();
        (tops, Region { min: Vec3::new(-81.0, 0.0, -81.0), max: Vec3::new(162.0, 81.0, 81.0) })
    }

    fn inside(region: Region) -> impl Iterator<Item = Vec3> {
        points().map(move |p| {
            let t = (p / 400.0 + 0.5).clamp(Vec3::ZERO, Vec3::splat(0.999)) * Vec3::new(1.0, 5.0, 1.0);
            region.min + (region.max - region.min) * t.fract()
        })
    }

    #[test]
    fn every_place_is_drawn_once() {
        let (tops, area) = world();
        for eye in inside(area).take(60) {
            let shown = shown(eye, &tops, |_| true);
            assert!(shown.models[0].contains(eye));
            assert_eq!(shown.models.len(), 27);
            assert!(shown.models.iter().all(|c| c.level == FINEST));
            assert!(shown.holograms.iter().all(|h| h.cube.level <= FINEST_HOLOGRAM));
            for p in inside(area).chain([eye, eye + Vec3::X * 2.9, eye - Vec3::Z * 8.9, eye + Vec3::Y * 26.0]) {
                if !area.contains(p) {
                    continue;
                }
                let drawn = shown.holograms.iter().filter(|h| h.cube.contains(p) && !h.carve.contains(p));
                let times = drawn.count() + shown.models.iter().filter(|c| c.contains(p)).count();
                assert_eq!(times, 1, "{p} drawn {times} times, the viewer at {eye}");
            }
        }
    }

    #[test]
    fn stepping_into_a_neighbour_makes_it_the_centre() {
        let (tops, _) = world();
        let eye = Vec3::new(10.5, 1.6, 10.5);
        let before = shown(eye, &tops, |_| true);
        let after = shown(eye + Vec3::X * 3.0, &tops, |_| true);
        assert_eq!(after.models[0], Cube { x: before.models[0].x + 1, ..before.models[0] });
        assert!(after.models.contains(&before.models[0]), "the old centre is still in 3D, beside the new");
        for cube in &after.models {
            let c = after.models[0];
            let d = (cube.x - c.x).abs().max((cube.y - c.y).abs()).max((cube.z - c.z).abs());
            assert!(d <= 1, "{cube:?} is not around the centre");
        }
        // No hologram inside the block in 3D around the viewer.
        for h in &after.holograms {
            assert!(!Region::around(after.models[0]).holds(h.cube));
            assert!(h.carve.contains(eye + Vec3::X * 3.0));
        }
    }

    #[test]
    fn empty_cubes_are_left_out() {
        let tops = [Cube { level: 0, x: 0, y: 0, z: 0 }];
        let eye = Vec3::new(40.0, 1.6, 40.0);
        let shown = shown(eye, &tops, |c| c.y == 0);
        assert!(shown.holograms.iter().all(|h| h.cube.y == 0));
        assert!(shown.models.iter().all(|c| c.y == 0));
        assert_eq!(shown.models[0], Cube::containing(eye, 3));
    }

    #[test]
    fn far_cubes_need_few_directions_and_near_ones_all() {
        let far = Cube::containing(Vec3::new(300.0, 10.0, 0.0), 0);
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let cone = reach(far, eye, 1.5, 2.0);
        assert!(cone.half_angle < 0.6, "{}", cone.half_angle);
        let near = Cube::containing(Vec3::new(2.0, 1.0, 0.0), 3);
        assert_eq!(reach(near, eye, 1.5, 2.0).half_angle, PI);
        let directions = Directions { step: 10.0 };
        let few = directions.within(&cone).len();
        let all = directions.within(&reach(near, eye, 1.5, 2.0)).len();
        assert_eq!(all, directions.azimuths().len() * directions.elevations().len());
        assert!(few * 4 < all, "{few} of {all}");
    }

    #[test]
    fn the_view_towards_the_viewer_is_always_there() {
        let directions = Directions { step: 10.0 };
        let (azimuths, elevations) = (directions.azimuths(), directions.elevations());
        for eye in points() {
            let cube = Cube::containing(Vec3::new(500.0, 0.0, 500.0), 1);
            let cone = reach(cube, eye, 0.0, 0.0);
            let views = directions.within(&cone);
            let nearest = (0..elevations.len())
                .flat_map(|r| (0..azimuths.len()).map(move |c| (r, c)))
                .min_by(|&(r1, c1), &(r2, c2)| {
                    let angle =
                        |r: usize, c: usize| cone.axis.angle_between(Directions::direction(azimuths[c], elevations[r]));
                    angle(r1, c1).total_cmp(&angle(r2, c2))
                })
                .unwrap();
            assert!(views.contains(&(nearest.0 as u16, nearest.1 as u16)));
        }
    }

    #[test]
    fn a_plan_has_every_view_a_place_needs() {
        let tops = [Cube { level: 0, x: 0, y: 0, z: 0 }, Cube { level: 0, x: 1, y: 0, z: 0 }];
        let eyes: Vec<Vec3> = (0..40).map(|i| Vec3::new(2.0 + i as f32 * 3.7, 1.6, 30.0)).collect();
        let directions = |_| Directions { step: 15.0 };
        let plan = Plan::new(eyes.iter().copied(), &tops, |_| true, directions);
        for &eye in &eyes {
            for Hologram { cube, .. } in shown(eye, &tops, |_| true).holograms {
                let have = &plan.views[&cube];
                for view in directions(cube.level).within(&reach(cube, eye, 0.0, 0.0)) {
                    assert!(have.contains(&view));
                }
            }
        }
        assert!(!plan.is_empty());
    }
}
