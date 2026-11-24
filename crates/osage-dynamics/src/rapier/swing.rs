// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Position and direction of angular stops for a joint with coupled swing.
use glam::{DVec3, Quat, Vec3};
use rapier3d::prelude::*;

// The loaded-joint replay compares passes on the existing system of impulses
// against smaller time divisions. A band keeps small solver residuals cheap.
const REFINEMENT_MARGIN: f32 = 1. / 16.;
const EXTRA_PASSES: usize = 3;
const MAX_LOADED_PASSES: usize = 8 * EXTRA_PASSES;

struct ProjectionBand {
    joint: ImpulseJointHandle,
    during_contact: Option<bool>,
}

enum SwingStop {
    Circular { threshold_sin2: f32, near_stop_sin2: f32, twist_stops: [f32; 2] },
    Elliptical { joint: ImpulseJointHandle, widths: [f64; 2], bands: Vec<ProjectionBand> },
}

pub(super) struct JointEnvelope {
    twist: ImpulseJointHandle,
    swing: SwingStop,
    twist_thresholds: [f32; 2],
    loaded_thresholds: [f32; 2],
    bodies: [RigidBodyHandle; 2],
    bind: [Quat; 2],
}

impl JointEnvelope {
    pub(super) fn new(
        sim: &mut PhysicsWorld,
        bodies: [RigidBodyHandle; 2],
        frame: Quat,
        twist: ImpulseJointHandle,
        limits_deg: [[f32; 2]; 3],
    ) -> Self {
        let bind = bodies.map(|body| sim.bodies[body].rotation().conjugate() * frame);
        let widths = [limits_deg[1][1], limits_deg[2][1]];
        let swing = if widths[0] == widths[1] {
            let expanded = (widths[0] * (1. + REFINEMENT_MARGIN)).min(180.).to_radians();
            let near_stop_sin2 = if widths[0] < 180. {
                (0.5 * (widths[0] * (1. - REFINEMENT_MARGIN)).to_radians()).sin().powi(2)
            } else {
                f32::INFINITY
            };
            SwingStop::Circular {
                threshold_sin2: (expanded * 0.5).sin().powi(2),
                near_stop_sin2,
                twist_stops: twist_thresholds(limits_deg[0], widths[0], 0.),
            }
        } else {
            let radians = widths.map(|v| f64::from(v).to_radians());
            let bands = insert_projection_bands(sim, bodies, bind, radians);
            let mut row = GenericJointBuilder::new(JointAxesMask::empty())
                .contacts_enabled(false)
                .limits(JointAxis::AngX, [-std::f32::consts::PI, 0.])
                .build();
            row.limit_axes = JointAxesMask::empty();
            SwingStop::Elliptical { joint: sim.insert_impulse_joint(bodies[0], bodies[1], row), widths: radians, bands }
        };
        let narrow = widths[0].min(widths[1]);
        let loaded_thresholds = twist_thresholds(limits_deg[0], narrow, narrow);
        let twist_thresholds = twist_thresholds(limits_deg[0], narrow, f32::INFINITY);
        Self { twist, swing, twist_thresholds, loaded_thresholds, bodies, bind }
    }

    pub(super) fn update(&self, sim: &mut PhysicsWorld, loaded: bool, contact_projections: bool) {
        let [parent, child] = self.bodies.map(|h| &sim.bodies[h]);
        if parent.is_sleeping() && child.is_sleeping() {
            return;
        }
        let parent_frame = *parent.rotation() * self.bind[0];
        let child_frame = *child.rotation() * self.bind[1];
        let relative = parent_frame.conjugate() * child_frame;

        // A native single-axis stop measures twist but applies its impulse on frame X.
        // Align both X axes with the twist gradient while preserving the signed angle.
        let [frame1, frame2] = match twist_midpoint(relative) {
            Some((half_swing, twist)) => {
                let midpoint = parent_frame * half_swing;
                [parent.rotation().conjugate() * midpoint, child.rotation().conjugate() * midpoint * twist]
            }
            None => self.bind,
        };
        let primary = &mut sim.impulse_joints.get_mut(self.twist, false).unwrap().data;
        primary.local_frame1.rotation = frame1;
        primary.local_frame2.rotation = frame2;

        let (swing_exceeded, compound_stop) = match self.swing {
            SwingStop::Circular { threshold_sin2, near_stop_sin2, twist_stops } => {
                let tilt = relative.y * relative.y + relative.z * relative.z;
                let norm = relative.length_squared();
                // A twist breach beside a loaded cone rim couples the two stop rows.
                let compound = loaded && tilt >= near_stop_sin2 * norm && twist_exceeded(relative, twist_stops);
                (tilt > threshold_sin2 * norm, compound)
            }
            SwingStop::Elliptical { joint, widths, ref bands } => {
                // On the rear hemisphere a lateral band would resist the path back
                // through the equator. The directional and circular stops carry it.
                let front = relative.y * relative.y + relative.z * relative.z <= 0.5 * relative.length_squared();
                for band in bands {
                    let enabled = front && band.during_contact.is_none_or(|condition| condition == contact_projections);
                    if sim.impulse_joints.get(band.joint).unwrap().data.is_enabled() != enabled {
                        sim.impulse_joints.get_mut(band.joint, false).unwrap().data.set_enabled(enabled);
                    }
                }
                let row = &mut sim.impulse_joints.get_mut(joint, false).unwrap().data;
                if let Some(limit) = violation(relative, widths) {
                    // Express the normal in world space. This row has no translation constraint.
                    let basis = parent_frame * Quat::from_rotation_arc(Vec3::X, limit.normal.as_vec3());
                    row.local_frame1.rotation = parent.rotation().conjugate() * basis;
                    row.local_frame2.rotation =
                        child.rotation().conjugate() * basis * Quat::from_rotation_x(limit.correction as f32);
                    row.limit_axes = JointAxesMask::ANG_X;
                    (limit.refine, false)
                } else {
                    row.limit_axes = JointAxesMask::empty();
                    (false, false)
                }
            }
        };
        let loaded_ellipse = loaded && matches!(self.swing, SwingStop::Elliptical { .. });
        let thresholds = if loaded_ellipse { self.loaded_thresholds } else { self.twist_thresholds };
        // Repeated breaches under contact need more passes through the coupled
        // impulses. An admissible pose releases the request immediately.
        let exceeded = swing_exceeded || twist_exceeded(relative, thresholds);
        let passes = if loaded_ellipse && exceeded {
            (2 * child.additional_pgs_iterations()).clamp(EXTRA_PASSES, MAX_LOADED_PASSES)
        } else if compound_stop || exceeded {
            EXTRA_PASSES
        } else {
            0
        };
        if child.additional_pgs_iterations() != passes {
            sim.bodies[self.bodies[1]].set_additional_pgs_iterations(passes);
        }
    }
}

/// The ellipse contains a circular core of radius m. Outside that core
/// sin(theta)/theta is at most sin(m)/m. Projecting the angular YZ ellipse
/// onto the transverse direction h then bounds the corresponding chord.
fn projection_half_angle([y_width, z_width]: [f64; 2], h: DVec3) -> Option<f64> {
    let narrow = y_width.min(z_width);
    let quarter_turn = std::f64::consts::FRAC_PI_2;
    if narrow >= quarter_turn {
        return None;
    }
    let support = (z_width * h.y).hypot(y_width * h.z) * (narrow.sin() / narrow);
    // A unit projection spans the whole axis and cannot furnish a stop.
    (support < 1.).then(|| support.asin())
}

fn insert_projection_bands(
    sim: &mut PhysicsWorld,
    bodies: [RigidBodyHandle; 2],
    bind: [Quat; 2],
    widths: [f64; 2],
) -> Vec<ProjectionBand> {
    let mut bands = Vec::new();
    let [y_width, z_width] = widths;
    // Keep the principal directions. Switch the two oblique directions under
    // load to the transformed normals of the ellipse's unit disk.
    for (i, contact_only) in [(0, false), (1, false), (2, false), (3, false), (1, true), (3, true)] {
        let angle = f64::from(i) * std::f64::consts::FRAC_PI_4;
        let (sin, cos) = angle.sin_cos();
        let (y, z) = if z_width <= y_width { (cos, sin) } else { (sin, cos) };
        let h = if contact_only { DVec3::new(0., y / z_width, z / y_width).normalize() } else { DVec3::new(0., y, z) };
        let Some(half) = projection_half_angle(widths, h) else { continue };
        let basis = Quat::from_rotation_arc(Vec3::X, h.as_vec3());
        let center = std::f32::consts::FRAC_PI_2;
        let row = GenericJointBuilder::new(JointAxesMask::empty())
            .local_frame1(rapier3d::math::Pose::from_rotation(bind[0] * basis))
            .local_frame2(rapier3d::math::Pose::from_rotation(bind[1]))
            .contacts_enabled(false)
            .coupled_axes(JointAxesMask::ANG_Y | JointAxesMask::ANG_Z)
            .limits(JointAxis::AngY, [center - half as f32, center + half as f32]);
        let mut row = row.build();
        row.set_enabled(!contact_only);
        bands.push(ProjectionBand {
            joint: sim.insert_impulse_joint(bodies[0], bodies[1], row),
            during_contact: (i % 2 != 0).then_some(contact_only),
        });
    }
    bands
}

// At a one-sided or locked twist stop, use the other available range to size
// the margin. The native stop itself continues to use the authored bounds.
fn twist_thresholds([lo, hi]: [f32; 2], swing_width: f32, margin_cap: f32) -> [f32; 2] {
    let opposite = if hi > 0. || lo < 0. { hi.max(-lo) } else { swing_width };
    [-lo, hi].map(|width| {
        let margin = if width > 0. { width } else { opposite };
        let expanded = width + margin.min(margin_cap) * REFINEMENT_MARGIN;
        if expanded >= 180. { f32::INFINITY } else { (0.5 * expanded.to_radians()).tan() }
    })
}

fn twist_exceeded(q: Quat, thresholds: [f32; 2]) -> bool {
    let positive = q.x.is_sign_negative() == q.w.is_sign_negative();
    let slope = thresholds[usize::from(positive)];
    slope.is_finite() && q.x.abs() > slope * q.w.abs()
}

/// Unit twist and halfway swing for q = swing * twist. The halfway X axis has
/// the direction of the spatial derivative of 2 atan2(q.x, q.w). At opposing
/// axes that derivative is undefined, so the caller uses the bind frames.
fn twist_midpoint(relative: Quat) -> Option<(Quat, Quat)> {
    let cos_half_swing = relative.x.hypot(relative.w);
    if cos_half_swing <= 1e-5 {
        return None;
    }
    let twist = Quat::from_xyzw(relative.x / cos_half_swing, 0., 0., relative.w / cos_half_swing);
    let swing = relative * twist.conjugate();
    Some((Quat::IDENTITY.slerp(swing, 0.5), twist))
}

struct SwingViolation {
    normal: DVec3,
    correction: f64,
    refine: bool,
}

/// Angular distance outside the cone, divided by the magnitude of its gradient
/// with respect to spatial relative rotation. Both are in the parent joint frame.
fn violation(relative: Quat, [y_width, z_width]: [f64; 2]) -> Option<SwingViolation> {
    let q = relative.as_dquat().normalize();
    let d = q * DVec3::X;
    let lateral = d.y.hypot(d.z);
    let angle = lateral.atan2(d.x);
    if angle < y_width.min(z_width) {
        return None;
    }
    let axis = if lateral > 1e-12 {
        DVec3::new(0., -d.z, d.y) / lateral
    } else {
        // At the antipode choose the represented half-turn's transverse axis.
        let sign = if q.y < 0. || (q.y == 0. && q.z < 0.) { -1. } else { 1. };
        DVec3::new(0., q.y * sign, q.z * sign).normalize()
    };
    let width = 1. / (axis.y / y_width).hypot(axis.z / z_width);
    let excess = angle - width;
    if excess < 0. {
        return None;
    }
    let mut gradient = axis;
    if lateral > 1e-12 {
        let derivative = width.powi(3) * axis.y * axis.z * (y_width.recip().powi(2) - z_width.recip().powi(2));
        let azimuth_gradient = DVec3::X - DVec3::new(0., d.y, d.z) * (d.x / lateral.powi(2));
        gradient -= derivative * azimuth_gradient;
    }
    let magnitude = gradient.length();
    Some(SwingViolation {
        normal: gradient / magnitude,
        correction: (excess / magnitude).min(std::f64::consts::FRAC_PI_2),
        refine: excess > width * f64::from(REFINEMENT_MARGIN),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_bands_contain_the_whole_authored_cone() {
        for degrees in [[20_f64, 5.], [5., 20.], [111., 12.], [60., 130.], [95., 125.], [15., 180.]] {
            let widths = degrees.map(f64::to_radians);
            for azimuth in (0..180).step_by(9) {
                let (z, y) = f64::from(azimuth).to_radians().sin_cos();
                let axis = DVec3::new(0., y, z);
                let Some(half) = projection_half_angle(widths, axis) else { continue };
                for direction in (0..360).step_by(11) {
                    let (v, u) = f64::from(direction).to_radians().sin_cos();
                    let boundary = 1. / (u / widths[0]).hypot(v / widths[1]);
                    for fraction in [0., 0.25, 0.5, 0.75, 1.] {
                        let rotation = glam::DQuat::from_axis_angle(DVec3::new(0., u, v), boundary * fraction);
                        let tilt = rotation * DVec3::X;
                        assert!(axis.dot(tilt).abs() <= half.sin() + 1e-12, "{degrees:?}, {direction}");
                    }
                }
            }
        }
        let widths = [20_f64.to_radians(), 5_f64.to_radians()];
        assert!((projection_half_angle(widths, DVec3::Y).unwrap() - widths[1]).abs() < 1e-12);
        assert!(projection_half_angle(widths, DVec3::Z).unwrap() >= widths[0]);
        assert!(projection_half_angle([2., 3.], DVec3::Y).is_none());
    }

    #[test]
    fn an_inverted_link_can_return_through_the_equator() {
        let mut sim = PhysicsWorld::new();
        let bodies = [(); 2].map(|_| sim.insert_body(RigidBodyBuilder::dynamic()));
        let twist = sim.insert_impulse_joint(bodies[0], bodies[1], GenericJointBuilder::new(JointAxesMask::empty()));
        let envelope =
            JointEnvelope::new(&mut sim, bodies, Quat::IDENTITY, twist, [[-45., 45.], [-20., 20.], [-5., 5.]]);
        let SwingStop::Elliptical { ref bands, .. } = envelope.swing else { unreachable!() };
        assert!(!bands.is_empty());
        assert_eq!(bands.iter().filter(|b| b.during_contact == Some(true)).count(), 2);
        for (angle, front, loaded) in [(0., true, false), (0., true, true), (3.1, false, true), (1.4, true, false)] {
            sim.bodies[bodies[1]].set_rotation(Quat::from_rotation_y(angle), true);
            envelope.update(&mut sim, loaded, loaded);
            for band in bands {
                assert_eq!(
                    sim.impulse_joints.get(band.joint).unwrap().data.is_enabled(),
                    front && band.during_contact.is_none_or(|condition| condition == loaded)
                );
            }
        }
    }

    #[test]
    fn the_child_requests_extra_passes_only_outside_the_refinement_band() {
        for widths in [[2., 2.], [20., 2.]] {
            let mut sim = PhysicsWorld::new();
            let bodies = [(); 2].map(|_| sim.insert_body(RigidBodyBuilder::dynamic()));
            let twist =
                sim.insert_impulse_joint(bodies[0], bodies[1], GenericJointBuilder::new(JointAxesMask::empty()));
            let limits = [[-2., 2.], [-widths[0], widths[0]], [-widths[1], widths[1]]];
            let envelope = JointEnvelope::new(&mut sim, bodies, Quat::IDENTITY, twist, limits);
            for (pose, passes) in [
                (Quat::from_rotation_z(2.05_f32.to_radians()), 0),
                (Quat::from_rotation_z(2.2_f32.to_radians()), EXTRA_PASSES),
                (Quat::IDENTITY, 0),
                (Quat::from_rotation_x(-2.3_f32.to_radians()), EXTRA_PASSES),
                (Quat::from_rotation_x(1.8_f32.to_radians()), 0),
            ] {
                sim.bodies[bodies[1]].set_rotation(pose, true);
                envelope.update(&mut sim, false, false);
                assert_eq!(sim.bodies[bodies[1]].additional_pgs_iterations(), passes);
                assert_eq!(sim.bodies[bodies[0]].additional_pgs_iterations(), 0);
            }
        }
    }

    #[test]
    fn a_loaded_ellipse_grows_its_budget_only_while_the_stop_is_exceeded() {
        for (widths, loaded_passes) in [([20., 5.], [3, 6, 12, 24, 24]), ([5., 5.], [0; 5])] {
            let mut sim = PhysicsWorld::new();
            let bodies = [(); 2].map(|_| sim.insert_body(RigidBodyBuilder::dynamic()));
            let twist =
                sim.insert_impulse_joint(bodies[0], bodies[1], GenericJointBuilder::new(JointAxesMask::empty()));
            let envelope = JointEnvelope::new(
                &mut sim,
                bodies,
                Quat::IDENTITY,
                twist,
                [[-45., 45.], [-widths[0], widths[0]], [-widths[1], widths[1]]],
            );
            sim.bodies[bodies[1]].set_rotation(Quat::from_rotation_x(46_f32.to_radians()), true);
            envelope.update(&mut sim, false, false);
            assert_eq!(sim.bodies[bodies[1]].additional_pgs_iterations(), 0);
            for passes in loaded_passes {
                envelope.update(&mut sim, true, true);
                assert_eq!(sim.bodies[bodies[1]].additional_pgs_iterations(), passes);
            }
            sim.bodies[bodies[1]].set_rotation(Quat::IDENTITY, true);
            envelope.update(&mut sim, true, true);
            assert_eq!(sim.bodies[bodies[1]].additional_pgs_iterations(), 0);
        }
    }

    #[test]
    fn circular_stops_request_coupled_passes_only_near_a_loaded_rim() {
        let mut sim = PhysicsWorld::new();
        let bodies = [(); 2].map(|_| sim.insert_body(RigidBodyBuilder::dynamic()));
        let joint = sim.insert_impulse_joint(bodies[0], bodies[1], GenericJointBuilder::new(JointAxesMask::empty()));
        let envelope =
            JointEnvelope::new(&mut sim, bodies, Quat::IDENTITY, joint, [[-20., 30.], [-30., 30.], [-30., 30.]]);
        for (tilt, twist, loaded, expected) in [
            (30_f32, -20.5_f32, false, 0),
            (25., -20.5, true, 0),
            (30., 29.5, true, 0),
            (29., -20.5, true, 3),
            (30., -20.5, true, 3),
            (30., -19., true, 0),
            (30., 30.5, true, 3),
            (30., 30.5, false, 0),
        ] {
            let rotation = Quat::from_rotation_y(tilt.to_radians()) * Quat::from_rotation_x(twist.to_radians());
            sim.bodies[bodies[1]].set_rotation(rotation, true);
            envelope.update(&mut sim, loaded, false);
            assert_eq!(sim.bodies[bodies[1]].additional_pgs_iterations(), expected);
        }
    }

    #[test]
    fn refinement_respects_signed_twist_ranges_and_quaternion_sign() {
        for (range, angle, wanted) in [
            ([-2., 30.], -2.05_f32, false),
            ([-2., 30.], -2.2, true),
            ([0., 30.], -1., false),
            ([0., 30.], -2., true),
            ([0., 0.], 0.2, false),
            ([0., 0.], 0.4, true),
            ([-180., 180.], 180., false),
            ([-30., 30.], 32., true),
        ] {
            let q = Quat::from_rotation_y(0.7) * Quat::from_rotation_x(angle.to_radians());
            for pose in [q, -q] {
                assert_eq!(twist_exceeded(pose, twist_thresholds(range, 5., f32::INFINITY)), wanted);
            }
        }
    }

    #[test]
    fn midpoint_follows_the_twist_derivative_and_preserves_its_sign() {
        for lean in [0_f32, 30., 85., 175.] {
            let swing = Quat::from_axis_angle(Vec3::new(0., 0.6, -0.8), lean.to_radians());
            for turn in [-120_f32, 0., 70.] {
                let twist = Quat::from_rotation_x(turn.to_radians());
                for q in [swing * twist, -(swing * twist)] {
                    let (mid, actual) = twist_midpoint(q).unwrap();
                    assert!(actual.dot(twist).abs() > 1. - 1e-6);
                    let q = q.as_dquat();
                    let rate = |axis: DVec3| {
                        let angle = |t| {
                            let pose = glam::DQuat::from_axis_angle(axis, t) * q;
                            2. * pose.x.atan2(pose.w)
                        };
                        let eps = 1e-6;
                        let diff = angle(eps) - angle(-eps);
                        diff.sin().atan2(diff.cos()) / (2. * eps)
                    };
                    let gradient = DVec3::from_array([DVec3::X, DVec3::Y, DVec3::Z].map(rate));
                    assert!(gradient.normalize().dot((mid * Vec3::X).as_dvec3()) > 1. - 1e-5);
                }
            }
        }
        assert!(twist_midpoint(Quat::from_rotation_z(std::f32::consts::PI)).is_none());
    }

    #[test]
    fn angular_ellipse_normal_matches_a_spatial_rotation_difference() {
        let widths = [20_f64.to_radians(), 5_f64.to_radians()];
        let az = 37_f64.to_radians();
        let swing = DVec3::new(0., az.cos(), az.sin());
        let q = glam::DQuat::from_axis_angle(swing, 23_f64.to_radians()) * glam::DQuat::from_rotation_x(-0.9);
        let limit = violation(q.as_quat(), widths).unwrap();
        let residual = |q: glam::DQuat| {
            let d = q * DVec3::X;
            let r = d.y.hypot(d.z);
            r.atan2(d.x) - r / (d.z / widths[0]).hypot(d.y / widths[1])
        };
        let eps = 1e-6;
        let gradient = DVec3::from_array([DVec3::X, DVec3::Y, DVec3::Z].map(|axis| {
            (residual(glam::DQuat::from_axis_angle(axis, eps) * q)
                - residual(glam::DQuat::from_axis_angle(axis, -eps) * q))
                / (2. * eps)
        }));
        assert!(limit.normal.distance(gradient.normalize()) < 1e-6);
        assert!((limit.correction * gradient.length() - residual(q)).abs() < 1e-7);
        assert!(violation(Quat::from_rotation_x(2.), widths).is_none());
        for r in [Quat::from_xyzw(0., 1., 0., 0.), Quat::from_xyzw(0., -1., 0., 0.)] {
            let limit = violation(r, widths).unwrap();
            assert_eq!(limit.normal, DVec3::Y);
            assert!(limit.correction.is_finite() && limit.correction > 0. && limit.refine);
        }
    }
}
