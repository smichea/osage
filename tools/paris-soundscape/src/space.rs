// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Points, turns and placements, in metres and in osage's frame: right-handed,
//! Y up (as glTF and the object format). Double precision: a delay is a
//! distance, and a sample is 7 mm of it.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

pub const fn vec3(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3 { x, y, z }
}

impl Vec3 {
    pub const ZERO: Vec3 = vec3(0.0, 0.0, 0.0);
    pub const X: Vec3 = vec3(1.0, 0.0, 0.0);
    pub const Y: Vec3 = vec3(0.0, 1.0, 0.0);
    pub const Z: Vec3 = vec3(0.0, 0.0, 1.0);

    pub fn dot(self, other: Vec3) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(self, other: Vec3) -> Vec3 {
        vec3(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }

    pub fn distance(self, other: Vec3) -> f64 {
        (self - other).length()
    }

    /// The same direction, one metre long; `fallback` for a point with no
    /// direction.
    pub fn unit_or(self, fallback: Vec3) -> Vec3 {
        let length = self.length();
        if length > 1e-12 { self / length } else { fallback }
    }

    pub fn lerp(self, other: Vec3, t: f64) -> Vec3 {
        self + (other - self) * t
    }

    pub fn array(self) -> [f64; 3] {
        [self.x, self.y, self.z]
    }
}

impl From<[f64; 3]> for Vec3 {
    fn from(a: [f64; 3]) -> Self {
        vec3(a[0], a[1], a[2])
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        vec3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl AddAssign for Vec3 {
    fn add_assign(&mut self, o: Vec3) {
        *self = *self + o;
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        vec3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        vec3(-self.x, -self.y, -self.z)
    }
}

impl Mul<f64> for Vec3 {
    type Output = Vec3;
    fn mul(self, k: f64) -> Vec3 {
        vec3(self.x * k, self.y * k, self.z * k)
    }
}

impl Div<f64> for Vec3 {
    type Output = Vec3;
    fn div(self, k: f64) -> Vec3 {
        vec3(self.x / k, self.y / k, self.z / k)
    }
}

/// A turn: a unit quaternion, written x, y, z, w as glTF does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Quat {
    pub const IDENTITY: Quat = Quat { x: 0.0, y: 0.0, z: 0.0, w: 1.0 };

    /// From glTF's four numbers, made unit; `None` for four zeros.
    pub fn from_xyzw(q: [f64; 4]) -> Option<Quat> {
        let norm = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
        (norm > 1e-9).then(|| Quat { x: q[0] / norm, y: q[1] / norm, z: q[2] / norm, w: q[3] / norm })
    }

    /// A turn of `angle` radians about `axis`, counter-clockwise seen from
    /// its tip.
    pub fn about(axis: Vec3, angle: f64) -> Quat {
        let axis = axis.unit_or(Vec3::Y);
        let (s, c) = (angle / 2.0).sin_cos();
        Quat { x: axis.x * s, y: axis.y * s, z: axis.z * s, w: c }
    }

    /// `self` after `other`: the turn that does `other` first.
    pub fn after(self, o: Quat) -> Quat {
        Quat {
            x: self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            y: self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            z: self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
            w: self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
        }
    }

    pub fn turn(self, v: Vec3) -> Vec3 {
        let u = vec3(self.x, self.y, self.z);
        let t = u.cross(v) * 2.0;
        v + t * self.w + u.cross(t)
    }

    /// The turn that undoes this one.
    pub fn back(self) -> Quat {
        Quat { x: -self.x, y: -self.y, z: -self.z, w: self.w }
    }
}

/// Where something is and how it is turned, in its parent or in the world:
/// a point `p` of it is at `rotation · p + translation`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Placement {
    pub translation: Vec3,
    pub rotation: Quat,
}

impl Placement {
    /// A point of the placed thing, in the frame it is placed in.
    pub fn point(&self, local: Vec3) -> Vec3 {
        self.rotation.turn(local) + self.translation
    }

    /// A direction of the placed thing, in the frame it is placed in.
    pub fn direction(&self, local: Vec3) -> Vec3 {
        self.rotation.turn(local)
    }

    /// A point of the frame it is placed in, in the placed thing's own.
    pub fn local(&self, point: Vec3) -> Vec3 {
        self.rotation.back().turn(point - self.translation)
    }

    /// The placement of a child placed at `child` in this one: parent × child,
    /// as the object format composes them.
    pub fn then(&self, child: &Placement) -> Placement {
        Placement { translation: self.point(child.translation), rotation: self.rotation.after(child.rotation) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

    fn near(a: Vec3, b: Vec3) -> bool {
        a.distance(b) < 1e-12
    }

    #[test]
    fn a_quarter_turn_about_up_takes_forward_to_the_left() {
        // Right-handed, Y up: X turned a quarter about Y points to −Z.
        let q = Quat::about(Vec3::Y, FRAC_PI_2);
        assert!(near(q.turn(Vec3::X), -Vec3::Z));
        assert!(near(q.turn(Vec3::Y), Vec3::Y));
        assert!(near(Vec3::X.cross(Vec3::Y), Vec3::Z));
    }

    #[test]
    fn turns_compose_and_placements_nest_as_parent_times_child() {
        let (a, b) = (Quat::about(Vec3::Y, 0.7), Quat::about(Vec3::X, -0.4));
        let v = vec3(0.3, -1.2, 2.0);
        assert!(near(a.after(b).turn(v), a.turn(b.turn(v))));

        let parent = Placement { translation: vec3(10.0, 0.0, 0.0), rotation: Quat::about(Vec3::Y, FRAC_PI_2) };
        let child = Placement { translation: vec3(1.0, 2.0, 0.0), rotation: Quat::IDENTITY };
        let world = parent.then(&child);
        // The child's own X is the parent's, turned: it points to −Z.
        assert!(near(world.translation, vec3(10.0, 2.0, -1.0)));
        assert!(near(world.point(vec3(1.0, 0.0, 0.0)), vec3(10.0, 2.0, -2.0)));
        assert!(near(world.direction(Vec3::X), -Vec3::Z));
        // And back.
        assert!(near(world.local(world.point(vec3(0.3, -2.0, 5.0))), vec3(0.3, -2.0, 5.0)));
    }

    #[test]
    fn a_quaternion_is_made_unit_and_four_zeros_are_refused() {
        let q = Quat::from_xyzw([0.0, 2.0, 0.0, 2.0]).unwrap();
        assert!(near(q.turn(Vec3::X), -Vec3::Z));
        assert!(Quat::from_xyzw([0.0; 4]).is_none());
    }
}
