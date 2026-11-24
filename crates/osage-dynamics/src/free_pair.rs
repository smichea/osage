// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! First inward contact between translating capsules and a freely translating oriented cuboid.
//! Both bodies experience the same gravity. Their relative motion is linear until an impulse
//! or the actor's terminal-speed clamp changes it.

use super::{DynamicBox, Part, Pose, Settings, StaticMesh};
use glam::{DQuat, DVec3, Vec3};

// A cap that exactly skims a face has no normal speed to resolve. This allowance covers the
// rounding of authoring inputs to f32; it is much smaller than either solver's contact skin.
const GRAZE_M: f64 = 1.0e-6;
const CONTACT_BAND_M: f64 = 0.03;
const FORECAST_S: f64 = 0.20;

#[derive(Clone, Copy)]
pub(super) struct StickingCandidate {
    pub normal: Vec3,
    pub arms: [Vec3; 2],
    pub friction: f32,
}

#[derive(Clone, Copy)]
struct Crossing {
    at_s: f64,
    guard_s: f32,
    candidate: Option<StickingCandidate>,
}

pub(super) enum Step {
    Native,
    Separated,
    Impact { after_s: f32, guard_s: f32, candidate: Option<StickingCandidate> },
}

/// All vectors in the cuboid's frame. The capsule spine ranges from centre − axis to centre + axis.
#[derive(Clone, Copy)]
struct Sweep {
    center: DVec3,
    velocity: DVec3,
    axis: DVec3,
    extents: DVec3,
    radius: f64,
}

struct Closest {
    gap: f64,
    derivative: f64, // Half the derivative of squared distance between the spine and the box.
    spine: DVec3,
    surface: DVec3,
}

impl Sweep {
    fn closest(&self, time: f64) -> Closest {
        let center = self.center + time * self.velocity;
        // Between consecutive face-plane crossings the squared distance is one quadratic
        // in the spine parameter. Clamp its unconstrained minimizer to that interval.
        let mut knots = [0.; 8];
        knots[..2].copy_from_slice(&[-1., 1.]);
        let mut count = 2;
        for i in 0..3 {
            if self.axis[i].abs() <= 1.0e-15 {
                continue;
            }
            for plane in [-self.extents[i], self.extents[i]] {
                let parameter = (plane - center[i]) / self.axis[i];
                if (-1.0..1.0).contains(&parameter) {
                    knots[count] = parameter;
                    count += 1;
                }
            }
        }
        knots[..count].sort_unstable_by(f64::total_cmp);
        let mut best_squared = f64::INFINITY;
        let mut pair = (DVec3::ZERO, DVec3::ZERO);
        for interval in knots[..count].windows(2) {
            let mid = center + 0.5 * (interval[0] + interval[1]) * self.axis;
            let mut quadratic = 0.;
            let mut linear = 0.;
            for i in 0..3 {
                if mid[i].abs() > self.extents[i] {
                    quadratic += self.axis[i] * self.axis[i];
                    linear += self.axis[i] * (center[i] - mid[i].signum() * self.extents[i]);
                }
            }
            let parameter =
                if quadratic > 0. { (-linear / quadratic).clamp(interval[0], interval[1]) } else { interval[0] };
            let spine = center + parameter * self.axis;
            let surface = spine.clamp(-self.extents, self.extents);
            let squared = spine.distance_squared(surface);
            if squared < best_squared {
                best_squared = squared;
                pair = (spine, surface);
            }
        }
        let (spine, surface) = pair;
        Closest {
            gap: best_squared.sqrt() - self.radius,
            derivative: (spine - surface).dot(self.velocity),
            spine,
            surface,
        }
    }

    /// Squared distance between translating convex sets is convex. First bracket its minimum,
    /// then bisect only the approaching portion to find the earliest inward crossing.
    fn approach(&self, horizon: f64, initial: &Closest) -> (Option<(f64, f64)>, f64) {
        let end = self.closest(horizon);
        let closest_at = if initial.derivative >= 0. {
            0.
        } else if end.derivative <= 0. {
            horizon
        } else {
            bisect(horizon, 38, |t| self.closest(t).derivative < 0.)
        };
        let minimum = self.closest(closest_at).gap;
        if minimum >= -GRAZE_M || initial.gap < -GRAZE_M {
            return (None, minimum);
        }
        let contact = if initial.gap <= 0. { 0. } else { bisect(closest_at, 42, |t| self.closest(t).gap > 0.) };
        (Some((contact, closest_at)), minimum)
    }

    fn crosses_broad_phase(&self, horizon: f64) -> bool {
        let end = self.center + self.velocity * horizon;
        let extent = self.axis.abs() + DVec3::splat(self.radius + CONTACT_BAND_M);
        let low = self.center.min(end) - extent;
        let high = self.center.max(end) + extent;
        low.cmple(self.extents).all() && high.cmpge(-self.extents).all()
    }
}

/// Closest points on a capsule spine and a cuboid, in cuboid-local coordinates.
pub(super) fn spine_box_points(center: DVec3, axis: DVec3, extents: DVec3) -> (DVec3, DVec3) {
    let shape = Sweep { center, axis, extents, radius: 0., velocity: DVec3::ZERO };
    let closest = shape.closest(0.);
    (closest.spine, closest.surface)
}

fn bisect(end: f64, iterations: usize, before: impl Fn(f64) -> bool) -> f64 {
    let mut low = 0.;
    let mut high = end;
    for _ in 0..iterations {
        let mid = (low + high) * 0.5;
        if before(mid) {
            low = mid;
        } else {
            high = mid;
        }
    }
    (low + high) * 0.5
}

#[derive(Clone, Copy)]
struct Forecast {
    relative_center: DVec3,
    relative_velocity: DVec3,
    axis: DVec3,
    box_rotation: DQuat,
    elapsed: f64,
    horizon: f64,
    crossing: Option<Crossing>,
}

impl Forecast {
    fn valid(self, center: DVec3, velocity: DVec3, axis: DVec3, rotation: DQuat, dt: f64) -> bool {
        self.elapsed + dt <= self.horizon + 1.0e-9
            && velocity.distance(self.relative_velocity) < 2.0e-5
            && axis.distance(self.axis) < 1.0e-8
            && rotation.dot(self.box_rotation).abs() > 1. - 1.0e-10
            && center.distance(self.relative_center + self.elapsed * self.relative_velocity) < 7.0e-5
    }
}

pub(super) struct FreePair {
    radius: f64,
    half_segment: f64,
    extents: DVec3,
    sticking_friction: Option<f32>,
    fixed_bounds: Vec<(DVec3, DVec3)>,
    forecast: Option<Forecast>,
}

fn vector(values: [f32; 3]) -> DVec3 {
    Vec3::from_array(values).as_dvec3()
}
fn rotation(pose: Pose) -> DQuat {
    DQuat::from_array(pose.rotation_xyzw.map(f64::from)).normalize()
}

impl FreePair {
    pub(super) fn for_scene(
        parts: &[Part],
        boxes: &[DynamicBox],
        meshes: &[StaticMesh],
        settings: Settings,
    ) -> Option<Self> {
        let [part] = parts else { return None };
        let [cuboid] = boxes else { return None };
        if part.half_segment <= 0. {
            return None;
        }
        let material = part
            .material
            .unwrap_or(osage_objects::CollisionMaterial { friction: settings.capsule.friction, restitution: 0. });
        let fixed_bounds = fixed_mesh_bounds(meshes);
        Some(Self {
            radius: f64::from(part.radius),
            half_segment: f64::from(part.half_segment),
            extents: vector(cuboid.half_extents_m),
            sticking_friction: (material.restitution == 0.).then(|| material.friction.sqrt() * cuboid.friction.sqrt()),
            fixed_bounds,
            forecast: None,
        })
    }

    pub(super) fn set_fixed_bounds(&mut self, bounds: Vec<(DVec3, DVec3)>) {
        self.fixed_bounds = bounds;
        self.forecast = None;
    }

    fn near_static(&self, center: DVec3, velocity: DVec3, radius: f64, horizon: f64, gravity: f64) -> bool {
        near_fixed(&self.fixed_bounds, center, velocity, radius, horizon, gravity)
    }

    fn forecast(
        &self,
        a: Pose,
        b: Pose,
        axis: DVec3,
        box_rotation: DQuat,
        gravity: f64,
        horizon: f64,
    ) -> Option<Forecast> {
        let capsule_position = vector(a.center);
        let box_position = vector(b.center);
        let relative_center = capsule_position - box_position;
        let relative_velocity = vector(a.linear_velocity) - vector(b.linear_velocity);
        let inverse = box_rotation.conjugate();
        let sweep = Sweep {
            center: inverse * relative_center,
            velocity: inverse * relative_velocity,
            axis: (inverse * axis) * self.half_segment,
            extents: self.extents,
            radius: self.radius,
        };
        if !sweep.crosses_broad_phase(horizon)
            || self.near_static(
                capsule_position,
                vector(a.linear_velocity),
                self.radius + self.half_segment,
                horizon,
                gravity,
            )
            || self.near_static(box_position, vector(b.linear_velocity), self.extents.length(), horizon, gravity)
        {
            return None;
        }
        let initial = sweep.closest(0.);
        if initial.gap < -GRAZE_M {
            return None;
        } // Preserve an existing native contact manifold.
        let (crossing, minimum) = sweep.approach(horizon, &initial);
        if crossing.is_none() && minimum > CONTACT_BAND_M {
            return None;
        }
        let crossing = crossing.map(|(at_s, closest_at)| {
            let witness = sweep.closest(at_s);
            let normal = (witness.spine - witness.surface).normalize();
            // A contact along the side can have an entire line of valid witnesses. Leave its
            // impulse distribution to the native manifold. A nonzero axial projection picks
            // one end of the segment and hence a single contacting cap.
            let unique_end = normal.dot(sweep.axis).abs() > 1.0e-8 * self.half_segment;
            let candidate = self.sticking_friction.filter(|_| unique_end).map(|friction| {
                let capsule_arm = witness.spine - sweep.radius * normal - sweep.center - at_s * sweep.velocity;
                StickingCandidate {
                    arms: [(box_rotation * capsule_arm).as_vec3(), (box_rotation * witness.surface).as_vec3()],
                    normal: (box_rotation * normal).as_vec3(),
                    friction,
                }
            });
            // Advancing a few micrometres into the approaching branch lets the f32 native
            // contact detector distinguish a touching pair from a tiny positive gap.
            let guard =
                (2.0e-6_f64).min(self.radius * 1.0e-4 / relative_velocity.length()).min((closest_at - at_s) * 0.25);
            Crossing { at_s, guard_s: guard as f32, candidate }
        });
        Some(Forecast { relative_center, relative_velocity, axis, box_rotation, elapsed: 0., horizon, crossing })
    }

    pub(super) fn plan(&mut self, a: Pose, b: Pose, settings: Settings, dt: f32) -> Step {
        let dt = f64::from(dt);
        let horizon = FORECAST_S.max(2. * dt);
        let gravity = f64::from(settings.gravity);
        let center = vector(a.center) - vector(b.center);
        let velocity = vector(a.linear_velocity) - vector(b.linear_velocity);
        if a.angular_velocity != [0.; 3]
            || b.angular_velocity != [0.; 3]
            || velocity.length_squared() < 1.0e-8
            || f64::from(a.linear_velocity[1]) - gravity * horizon <= -f64::from(settings.terminal_speed)
            || center.length()
                > self.radius + self.half_segment + self.extents.length() + velocity.length() * horizon + CONTACT_BAND_M
        {
            self.forecast = None;
            return Step::Native;
        }
        let axis = rotation(a) * DVec3::Y;
        let box_rotation = rotation(b);
        let cached = self.forecast.take().filter(|f| f.valid(center, velocity, axis, box_rotation, dt));
        let Some(mut forecast) = cached.or_else(|| self.forecast(a, b, axis, box_rotation, gravity, horizon)) else {
            return Step::Native;
        };
        if let Some(hit) = forecast.crossing
            && hit.at_s <= forecast.elapsed + dt
        {
            let after_s = (hit.at_s - forecast.elapsed) as f32;
            return if after_s >= 0. {
                Step::Impact { after_s, guard_s: hit.guard_s, candidate: hit.candidate }
            } else {
                Step::Native
            };
        }
        forecast.elapsed += dt;
        self.forecast = Some(forecast);
        Step::Separated
    }
}

fn fixed_mesh_bounds(meshes: &[StaticMesh]) -> Vec<(DVec3, DVec3)> {
    meshes
        .iter()
        .map(|mesh| {
            let mut low = DVec3::splat(f64::INFINITY);
            let mut high = DVec3::splat(f64::NEG_INFINITY);
            for &p in &mesh.vertices {
                let p = vector(p);
                low = low.min(p);
                high = high.max(p);
            }
            (low, high)
        })
        .collect()
}

fn near_fixed(
    bounds: &[(DVec3, DVec3)],
    center: DVec3,
    velocity: DVec3,
    radius: f64,
    horizon: f64,
    gravity: f64,
) -> bool {
    let end = center + velocity * horizon;
    // Enclose both the gravitational arc and the discrete free-flight integrator.
    let extent = DVec3::splat(radius + CONTACT_BAND_M + gravity * horizon * horizon);
    let low = center.min(end) - extent;
    let high = center.max(end) + extent;
    bounds.iter().any(|(a, b)| low.cmple(*b).all() && high.cmpge(*a).all())
}

/// The first inward crossing of an initially translating joint tree with a freely translating
/// box. The solver still constructs all of the contact and joint constraints itself.
pub(super) struct ArticulatedSweep {
    shapes: Vec<(f64, f64)>,
    box_extents: DVec3,
    fixed_bounds: Vec<(DVec3, DVec3)>,
    finished: bool,
}

impl ArticulatedSweep {
    pub(super) fn for_scene(parts: &[Part], boxes: &[DynamicBox], meshes: &[StaticMesh]) -> Option<Self> {
        let [cuboid] = boxes else { return None };
        if parts.len() < 2 {
            return None;
        }
        Some(Self {
            shapes: parts.iter().map(|p| (f64::from(p.radius), f64::from(p.half_segment))).collect(),
            box_extents: vector(cuboid.half_extents_m),
            fixed_bounds: fixed_mesh_bounds(meshes),
            finished: false,
        })
    }

    pub(super) fn set_fixed_bounds(&mut self, bounds: Vec<(DVec3, DVec3)>) {
        self.fixed_bounds = bounds;
    }

    pub(super) fn first_impact(&mut self, parts: &[Pose], cuboid: Pose, settings: Settings, dt: f32) -> Option<f32> {
        if self.finished {
            return None;
        }
        let dt = f64::from(dt);
        let box_velocity = vector(cuboid.linear_velocity);
        let box_center = vector(cuboid.center);
        let base_velocity = vector(parts[0].linear_velocity);
        let gravity = f64::from(settings.gravity);
        // Look just past a step boundary to distinguish crossing from grazing. Only roots
        // inside this step are accepted.
        let horizon = dt * 1.01;
        const MAX_DEFORMATION_M: f64 = 1.0e-5;
        // Round-off in native joint projection can move a nominally resting joint by several
        // micrometres. A changing pose beyond this spatial tolerance belongs to the native solver.
        if (base_velocity.y - gravity * horizon).min(box_velocity.y - gravity * horizon)
            <= -f64::from(settings.terminal_speed)
            || vector(cuboid.angular_velocity).length() * self.box_extents.length() * horizon > MAX_DEFORMATION_M
            || parts.iter().zip(&self.shapes).any(|(p, &(r, h))| {
                (vector(p.linear_velocity).distance(base_velocity) + vector(p.angular_velocity).length() * (r + h))
                    * horizon
                    > MAX_DEFORMATION_M
            })
        {
            self.finished = true;
            return None;
        }
        let near_static = |center: DVec3, velocity: DVec3, radius: f64| {
            near_fixed(&self.fixed_bounds, center, velocity, radius, horizon, gravity)
        };
        if near_static(box_center, box_velocity, self.box_extents.length()) {
            self.finished = true;
            return None;
        }
        if near_static(vector(parts[0].center), base_velocity, self.shapes[0].0 + self.shapes[0].1) {
            self.finished = true;
            return None;
        }
        let inv = rotation(cuboid).inverse();
        let mut earliest = None::<f64>;
        for (&(radius, half_segment), &part) in self.shapes.iter().zip(parts) {
            let center = vector(part.center);
            let velocity = vector(part.linear_velocity);
            let sweep = Sweep {
                center: inv * (center - box_center),
                velocity: inv * (velocity - box_velocity),
                axis: inv * (rotation(part) * DVec3::Y) * half_segment,
                extents: self.box_extents,
                radius,
            };
            if !sweep.crosses_broad_phase(horizon) {
                continue;
            }
            let initial = sweep.closest(0.);
            if initial.gap < -GRAZE_M {
                self.finished = true;
                return None;
            }
            if initial.derivative >= 0. || initial.gap > sweep.velocity.length() * horizon + GRAZE_M {
                continue;
            }
            let (hit, _) = sweep.approach(horizon, &initial);
            if let Some((at, _)) = hit
                && at <= dt
            {
                earliest = Some(earliest.map_or(at, |t| t.min(at)));
            }
        }
        let at = earliest?;
        self.finished = true;
        if parts
            .iter()
            .zip(&self.shapes)
            .any(|(p, &(r, h))| near_static(vector(p.center), vector(p.linear_velocity), r + h))
        {
            return None;
        }
        Some(at as f32)
    }
}
