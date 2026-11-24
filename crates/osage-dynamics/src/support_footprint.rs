// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Horizontal footprint of the upward contact witnesses in one numbered solver substep.
use crate::{Contact, Part, Pose};
use anyhow::{Result, ensure};
use glam::{DQuat, DVec2, DVec3, Vec3Swizzles};
use serde::Serialize;

#[derive(Serialize)]
pub struct ProjectedSupport {
    pub horizon_s: f64,
    pub center_xz_m: [f64; 2],
    pub hull_xz_m: Vec<[f64; 2]>,
    pub margin_m: Option<f64>,
}

#[derive(Serialize)]
pub struct SupportFootprint {
    pub substep: u8,
    pub max_separation_m: f32,
    pub center_of_mass_m: [f64; 3],
    pub center_velocity_m_s: [f64; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projected: Option<ProjectedSupport>,
    pub supporting_parts: Vec<usize>,
    pub witness_count: usize,
    pub hull_xz_m: Vec<[f64; 2]>,
    pub area_m2: f64,
    /// Positive inside the polygon, zero on its boundary, negative outside. Empty has no distance.
    pub margin_m: Option<f64>,
}

fn chain<'a>(points: impl Iterator<Item = &'a DVec2>) -> Vec<DVec2> {
    let mut hull: Vec<DVec2> = Vec::new();
    for &p in points {
        while hull.len() >= 2 {
            let a = hull[hull.len() - 2];
            let b = hull[hull.len() - 1];
            if (b - a).perp_dot(p - a) > 0. {
                break;
            }
            hull.pop();
        }
        hull.push(p);
    }
    hull
}

fn outline(mut points: Vec<DVec2>) -> Vec<DVec2> {
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    points.dedup();
    if points.len() <= 2 {
        return points;
    }
    let mut lower = chain(points.iter());
    let mut upper = chain(points.iter().rev());
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn geometry(hull: &[DVec2], p: DVec2) -> (f64, Option<f64>) {
    let Some(&origin) = hull.first() else { return (0., None) };
    if hull.len() == 1 {
        return (0., Some(-origin.distance(p)));
    }
    let mut twice_area = 0.;
    let mut closest = f64::INFINITY;
    let mut inside = hull.len() >= 3;
    for (&a, &b) in hull.iter().zip(hull.iter().cycle().skip(1)) {
        let edge = b - a;
        twice_area += (a - origin).perp_dot(b - origin);
        inside &= edge.perp_dot(p - a) >= 0.;
        let t = ((p - a).dot(edge) / edge.length_squared()).clamp(0., 1.);
        closest = closest.min((p - a - t * edge).length());
    }
    (0.5 * twice_area, Some(if inside { closest } else { -closest }))
}

/// Use the physical centre and mass of every body, without requiring named feet.
/// The caller chooses the largest positive manifold separation admitted in metres.
pub fn measure(
    parts: &[Part],
    poses: &[Pose],
    contacts: &[Contact],
    substep: u8,
    max_separation_m: f32,
) -> Result<SupportFootprint> {
    sample(parts, poses, contacts, substep, max_separation_m, None, |_| None)
}

/// Advance the centre at its mass-weighted velocity and each current witness as a
/// material point of its supporting rigid surface. A missing surface pose denotes
/// fixed terrain. No new contacts or future contact forces are assumed.
pub fn project(
    parts: &[Part],
    poses: &[Pose],
    contacts: &[Contact],
    substep: u8,
    max_separation_m: f32,
    horizon_s: f64,
    surface_pose: impl Fn(usize) -> Option<Pose>,
) -> Result<SupportFootprint> {
    ensure!(horizon_s.is_finite() && horizon_s >= 0., "invalid support projection horizon");
    sample(parts, poses, contacts, substep, max_separation_m, Some(horizon_s), surface_pose)
}

fn moved_witness(point: DVec3, pose: Pose, dt: f64) -> Result<DVec3> {
    let center = DVec3::from(pose.center.map(f64::from));
    let translation = DVec3::from(pose.linear_velocity.map(f64::from)) * dt;
    let angle = DVec3::from(pose.angular_velocity.map(f64::from)) * dt;
    let moved = center + translation + DQuat::from_scaled_axis(angle) * (point - center);
    ensure!(moved.is_finite(), "invalid projected surface witness");
    Ok(moved)
}

fn sample(
    parts: &[Part],
    poses: &[Pose],
    contacts: &[Contact],
    substep: u8,
    max_separation_m: f32,
    horizon: Option<f64>,
    surface_pose: impl Fn(usize) -> Option<Pose>,
) -> Result<SupportFootprint> {
    ensure!(!parts.is_empty() && parts.len() == poses.len() && substep > 0, "invalid support footprint snapshot");
    ensure!(max_separation_m.is_finite() && max_separation_m >= 0., "invalid contact separation limit");
    let mut weighted = DVec3::ZERO;
    let mut momentum = DVec3::ZERO;
    let mut mass = 0.;
    for (part, pose) in parts.iter().zip(poses) {
        let m = f64::from(part.mass);
        let center = DVec3::from(pose.center.map(f64::from));
        ensure!(m.is_finite() && m > 0. && center.is_finite(), "invalid mass or body centre");
        let velocity = DVec3::from(pose.linear_velocity.map(f64::from));
        ensure!(velocity.is_finite(), "invalid body velocity");
        momentum += m * velocity;
        mass += m;
        weighted += m * center;
    }
    let center = weighted / mass;
    let velocity = momentum / mass;
    let mut points = Vec::new();
    let mut future = Vec::new();
    let mut bearing = vec![false; parts.len()];
    for c in contacts.iter().filter(|c| c.substep == substep && c.normal[1] > 0.5 && c.separation <= max_separation_m) {
        ensure!(c.part < parts.len() && c.point.iter().all(|v| v.is_finite()), "invalid support witness");
        bearing[c.part] = true;
        let p = DVec3::from(c.point.map(f64::from));
        // Canonicalise signed zeros for the lexicographic total ordering.
        points.push(p.xz() + DVec2::ZERO);
        if let Some(dt) = horizon {
            let moved = match surface_pose(c.surface) {
                Some(pose) => moved_witness(p, pose, dt)?,
                None => p,
            };
            future.push(moved.xz() + DVec2::ZERO);
        }
    }
    let witness_count = points.len();
    let hull = outline(points);
    let (area_m2, margin_m) = geometry(&hull, DVec2::new(center.x, center.z));
    let projected = if let Some(dt) = horizon {
        let p = center.xz() + dt * velocity.xz();
        ensure!(p.is_finite(), "invalid projected centre of mass");
        let hull = outline(future);
        Some(ProjectedSupport {
            horizon_s: dt,
            center_xz_m: p.to_array(),
            margin_m: geometry(&hull, p).1,
            hull_xz_m: hull.iter().map(DVec2::to_array).collect(),
        })
    } else {
        None
    };
    Ok(SupportFootprint {
        projected,
        center_velocity_m_s: velocity.to_array(),
        substep,
        max_separation_m,
        center_of_mass_m: center.to_array(),
        supporting_parts: bearing.iter().enumerate().filter_map(|(i, b)| b.then_some(i)).collect(),
        witness_count,
        hull_xz_m: hull.iter().map(DVec2::to_array).collect(),
        area_m2,
        margin_m,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projected_support_carries_each_witness_with_its_surface() {
        let part = Part::between("a", None, [[0., 0., 0.], [0., 1., 0.]], 0.1, 1., [[0.; 2]; 3]);
        let mut heavy = part.clone();
        heavy.mass = 3.;
        let base = Pose {
            center: [0.; 3],
            rotation_xyzw: [0., 0., 0., 1.],
            linear_velocity: [0.; 3],
            angular_velocity: [0.; 3],
        };
        let moving =
            Pose { linear_velocity: [2., 0., 0.], angular_velocity: [0., std::f32::consts::FRAC_PI_2, 0.], ..base };
        let contact = Contact {
            part: 0,
            surface: 5,
            surface_triangle: None,
            substep: 1,
            separation: 0.,
            point: [1., 0., 0.],
            normal: [0., 1., 0.],
            applied_offset_s: None,
        };
        let fixed = Contact { surface: 4, point: [0., 0., 2.], ..contact };
        let result = project(&[part, heavy], &[base, moving], &[contact, fixed], 1, 0., 1., |surface| {
            (surface == 5).then_some(moving)
        })
        .unwrap();
        assert_eq!(result.center_velocity_m_s, [1.5, 0., 0.]);
        let forecast = result.projected.unwrap();
        assert_eq!(forecast.center_xz_m, [1.5, 0.]);
        assert_eq!(forecast.hull_xz_m[0], [0., 2.]);
        assert!(DVec2::from(forecast.hull_xz_m[1]).distance(DVec2::new(2., -1.)) < 1e-6);
    }

    #[test]
    fn edges_points_and_outside_corners_have_euclidean_margins() {
        let square = outline(vec![DVec2::ONE, DVec2::X, DVec2::ZERO, DVec2::Y, DVec2::splat(0.5), DVec2::ZERO]);
        assert_eq!(geometry(&square, DVec2::splat(0.25)), (1., Some(0.25)));
        assert_eq!(geometry(&square, DVec2::new(4., 5.)), (1., Some(-5.)));
        let line = outline(vec![DVec2::ZERO, DVec2::X, 2. * DVec2::X]);
        assert_eq!(line.len(), 2);
        assert_eq!(geometry(&line, DVec2::ONE), (0., Some(-1.)));
        assert_eq!(geometry(&line[..1], DVec2::X), (0., Some(-1.)));
        assert_eq!(geometry(&[], DVec2::ZERO), (0., None));
    }
}
