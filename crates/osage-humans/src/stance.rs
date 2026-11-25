// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

mod cycle;
mod human_cycle;
pub use cycle::{FootCycle, FootCyclePhase};
pub use human_cycle::PlannedHumanSteps;
mod skin;
mod surface;
use skin::SkinFrame;
pub use skin::SoleSkin;
pub use surface::triangle_support_lift;

use crate::{Human, Move, Skeleton};
use bevy::{app::AnimationSystems, prelude::*, transform::helper::TransformHelper};
use std::collections::HashSet;

type SurfaceQuery = dyn Fn([Vec3; 3], std::ops::RangeInclusive<f32>) -> Option<f32> + Send + Sync;

/// Samples the stationary support under a world-space point, returning its world-space Y in metres.
/// A missing sample leaves that leg in its animated pose.
#[derive(Resource)]
pub struct FootGround {
    max_step_m: f32,
    surface: Option<Box<SurfaceQuery>>,
    sample: Box<dyn Fn(Vec3) -> Option<f32> + Send + Sync>,
}

impl FootGround {
    /// The world supplies its traversable step height, measured from the actor's root.
    pub fn new(max_step_m: f32, sample: impl Fn(Vec3) -> Option<f32> + Send + Sync + 'static) -> Self {
        Self { max_step_m, sample: Box::new(sample), surface: None }
    }

    /// Query the greatest signed terrain-to-sole lift across a world-space face, within
    /// the supplied range of traversable terrain heights. None denotes no overlapping support.
    pub fn with_surface_lift(
        mut self,
        query: impl Fn([Vec3; 3], std::ops::RangeInclusive<f32>) -> Option<f32> + Send + Sync + 'static,
    ) -> Self {
        self.surface = Some(Box::new(query));
        self
    }

    fn support(&self, point: Vec3, root_y: f32) -> Option<f32> {
        (self.sample)(point).filter(|y| y.is_finite() && (*y - root_y).abs() <= self.max_step_m)
    }

    fn vertical_offset(&self, toe: Vec3, root_y: f32, height: f32, band: f32) -> Option<f32> {
        let surface = self.support(toe, root_y)?;
        let weight = (1. - (height - band) / 0.08).clamp(0., 1.);
        let blend = weight * weight * (3. - 2. * weight);
        Some(((surface - root_y) * blend).max(surface - toe.y))
    }
}

/// Four consecutive nodes in the model: two links ending at the ankle, plus its contact point.
#[derive(Clone, Copy, Debug)]
pub struct FootChain {
    pub upper: Entity,
    pub lower: Entity,
    pub ankle: Entity,
    pub contact: Entity,
}

impl FootChain {
    fn nodes(self) -> [Entity; 4] {
        [self.upper, self.lower, self.ankle, self.contact]
    }
}

/// Rigid sole samples in ankle-local coordinates. Attach to the ankle of a bound foot.
#[derive(Component)]
#[component(immutable)]
pub struct SolePoints(Vec<Vec3>);

impl SolePoints {
    pub fn new(points: Vec<Vec3>) -> Option<Self> {
        (!points.is_empty() && points.iter().all(|p| p.is_finite())).then_some(Self(points))
    }

    fn clearance_lift(
        &self,
        shoe: GlobalTransform,
        horizontal: Vec3,
        root_y: f32,
        ground: Option<&FootGround>,
    ) -> Option<f32> {
        self.0
            .iter()
            .filter_map(|&local| {
                let point = shoe.transform_point(local) + horizontal;
                let surface = match ground {
                    Some(ground) => ground.support(point, root_y)?,
                    None => root_y,
                };
                Some(surface - point.y)
            })
            .reduce(f32::max)
    }
}

/// A model's independently driven limbs. The visual armature must be a direct child of the actor.
#[derive(Component)]
#[component(immutable)]
#[require(ContactHeight)]
pub struct FootRig {
    actor: Entity,
    armature: Entity,
    chains: Vec<FootChain>,
}

/// Height of a low foot above the actor origin in an uncorrected pose, in unit-scale metres.
/// Set to None during an animation blend or flight.
#[derive(Component, Default, PartialEq)]
pub struct ContactHeight(pub Option<f32>);

impl FootRig {
    /// Bind disjoint paths from the loaded model once. Reject broken paths and chains
    /// whose animated parent would be rotated by another limb's solve.
    pub fn bind(world: &World, actor: Entity, armature: Entity, chains: Vec<FootChain>) -> Result<Self, &'static str> {
        let parent = |id| world.get::<ChildOf>(id).map(ChildOf::parent);
        if parent(armature) != Some(actor) || chains.is_empty() {
            return Err("a foot rig needs an armature under the actor and at least one limb");
        }
        let mut used = HashSet::new();
        for chain in &chains {
            let ids = chain.nodes();
            if !ids.windows(2).all(|pair| parent(pair[1]) == Some(pair[0])) {
                return Err("the four limb nodes must be a direct parent-child chain");
            }
            if !ids.into_iter().all(|id| used.insert(id)) {
                return Err("two limbs cannot share driven or contact nodes");
            }
        }
        for chain in &chains {
            let mut node = chain.upper;
            loop {
                node = parent(node).ok_or("limb is outside the visual armature")?;
                if node == armature {
                    break;
                }
                if used.contains(&node) {
                    return Err("one limb is under another limb");
                }
            }
        }
        Ok(Self { actor, armature, chains })
    }
}

pub(super) fn bind_human(
    commands: &mut Commands,
    actor: Entity,
    armature: Entity,
    bones: &std::collections::HashMap<String, Entity>,
) {
    let mut chains = Vec::new();
    for names in [["thigh_l", "calf_l", "foot_l", "ball_l"], ["thigh_r", "calf_r", "foot_r", "ball_r"]] {
        let [Some(&upper), Some(&lower), Some(&ankle), Some(&contact)] = names.map(|n| bones.get(n)) else { return };
        chains.push(FootChain { upper, lower, ankle, contact });
    }
    commands.queue(move |world: &mut World| {
        if let Ok(rig) = FootRig::bind(world, actor, armature, chains) {
            world.entity_mut(actor).insert(rig);
        }
    });
}

/// Keep each low forefoot planted during a walk or jog. With no FootGround the actor stands on a level plane.
/// The actor origin is at the ground and the model's up axis is world +Y.
#[derive(Component, Default)]
#[require(Stance)]
pub struct PlantFeet;

#[derive(Default)]
struct Foot {
    surface_positions: Vec<Vec3>,
    guided: bool,
    anchor: Option<Vec3>,
    release: Vec3,
    weight: f32,
    original: Option<[(Entity, Quat); 3]>,
    target: Option<(GlobalTransform, Vec3)>,
}

#[derive(Component, Default)]
struct Stance {
    feet: Vec<Foot>,
    band: Option<f32>,
    previous_root: Option<Vec3>,
    drop_m: f32,
    armature_rest: Option<(Entity, Vec3)>,
}

/// Apply stance correction to model-defined limbs, independently of the human asset loader.
pub struct FootPlantPlugin;
impl Plugin for FootPlantPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, restore.before(AnimationSystems))
            .add_systems(PostUpdate, plant.after(AnimationSystems).before(TransformSystems::Propagate));
    }
}

pub(super) fn install(app: &mut App) {
    if !app.is_plugin_added::<FootPlantPlugin>() {
        app.add_plugins(FootPlantPlugin);
    }
    human_cycle::install(app);
    app.add_systems(PostUpdate, human_gait.after(AnimationSystems).before(plant));
}

fn human_contact_height(gait: Move) -> Option<f32> {
    match gait {
        Move::WALK => Some(0.035),
        Move::JOG => Some(0.0225),
        _ => None,
    }
}

fn human_gait(
    mut actors: Query<(&Human, &Skeleton, &mut ContactHeight, Has<PlannedHumanSteps>), With<PlantFeet>>,
    players: Query<&AnimationPlayer>,
    library: Res<crate::Library>,
) {
    for (human, skeleton, mut height, planned) in &mut actors {
        let band = players.get(skeleton.armature).ok().and_then(|player| {
            let mut playing = player.playing_animations().map(|(&node, _)| node);
            let first = playing.next()?;
            if !planned {
                return human_contact_height(human.act).filter(|_| playing.next().is_none());
            }
            // Only combine grounded moves. An idle foot can still need clearance during a fade.
            let nodes = &library.ready.as_ref()?.nodes;
            let node_band = |node| {
                [Move::WALK, Move::JOG, Move::IDLE]
                    .into_iter()
                    .find(|m| nodes.get(m.0) == Some(&node))
                    .map(|m| human_contact_height(m).unwrap_or(0.))
            };
            playing.try_fold(node_band(first)?, |band, node| Some(band.min(node_band(node)?)))
        });
        height.set_if_neq(ContactHeight(band));
    }
}

// Restore the clip pose before Bevy writes this frame, including after the world removes PlantFeet.
fn restore(mut actors: Query<(&mut Stance, Has<PlantFeet>)>, mut joints: Query<&mut Transform>) {
    for (mut stance, enabled) in &mut actors {
        if let Some((id, translation)) = stance.armature_rest.take()
            && let Ok(mut armature) = joints.get_mut(id)
        {
            armature.translation = translation;
        }
        for foot in &mut stance.feet {
            if let Some(original) = foot.original.take() {
                for (id, rotation) in original {
                    if let Ok(mut transform) = joints.get_mut(id) {
                        transform.rotation = rotation;
                    }
                }
            }
        }
        if !enabled {
            *stance = default();
        }
    }
}

impl Foot {
    fn offset(&mut self, toe: Vec3, height: f32, band: f32, scale: f32) -> Vec3 {
        if height <= band {
            let target = *self.anchor.get_or_insert(toe + self.release * self.weight);
            self.weight = 1.;
            self.release = ((target - toe) * Vec3::new(1., 0., 1.)).clamp_length_max(0.12 * scale);
            return self.release;
        }
        self.anchor = None;
        let t = ((height - band) / 0.04).clamp(0., 1.);
        self.weight = self.weight.min(1. - t * t * (3. - 2. * t));
        self.release * self.weight
    }
}

// At very small angles the foot can still travel a visible fraction of a millimetre.
// Keep the cross product for these rotations, including when the dot rounds to one.
fn align(from: Vec3, to: Vec3) -> Quat {
    let dot = from.dot(to);
    let cross = from.cross(to);
    if dot < -0.999 {
        let sin = cross.length();
        if sin > f32::EPSILON {
            return Quat::from_axis_angle(cross / sin, sin.atan2(dot));
        }
        return Quat::from_axis_angle(from.any_orthonormal_vector(), std::f32::consts::PI);
    }
    Quat::from_vec4(cross.extend(1. + dot)).normalize()
}

/// Rotate the two links in their parent's coordinates, keeping the knee on its animated
/// side of the hip-to-ankle line. Counter-rotate the ankle to preserve the authored shoe angle.
/// Supply three consecutive rigid local transforms and a displacement in the hip parent's
/// frame. Returns absolute local rotations in chain order. Zero-length links have no solution.
pub fn solve_two_bone(hip: Transform, knee: Transform, ankle: Transform, delta: Vec3) -> Option<[Quat; 3]> {
    if !hip.is_finite() || !knee.is_finite() || !ankle.is_finite() || !delta.is_finite() {
        return None;
    }
    let mid = hip.transform_point(knee.translation);
    let lower = hip.mul_transform(knee);
    let end = lower.transform_point(ankle.translation);
    let upper_vec = mid - hip.translation;
    let lower_vec = end - mid;
    let a = upper_vec.length();
    let b = lower_vec.length();
    if a <= 1e-6 || b <= 1e-6 {
        return None;
    }
    let target = end + delta - hip.translation;
    let upper_dir = upper_vec.try_normalize()?;
    let old_axis = (end - hip.translation).try_normalize().unwrap_or(upper_dir);
    let axis = target.try_normalize().unwrap_or(old_axis);
    let distance = target.length().clamp((a - b).abs(), a + b);
    let new_upper = if distance <= f32::EPSILON * (a + b) {
        // Equal links can fold onto the hip in any plane. Keep the current knee.
        upper_vec
    } else if distance >= a + b {
        axis * a
    } else {
        // For an initially straight leg, select a stable pole in the upper joint frame.
        let projected = |v: Vec3| {
            let normal = old_axis.cross(v);
            // Near collinearity the measured bend is dominated by f32 roundoff.
            (normal.length_squared() > 1e-12).then(|| normal.cross(old_axis).normalize())
        };
        let old_pole = projected(upper_dir)
            .or_else(|| projected(hip.rotation * Vec3::Z))
            .unwrap_or_else(|| old_axis.any_orthonormal_vector());
        let pole = if old_axis.dot(axis) < -0.99999 {
            (old_pole - axis * old_pole.dot(axis)).try_normalize()?
        } else {
            align(old_axis, axis) * old_pole
        };
        let along = (0.5 * (distance + (a - b) * (a + b) / distance)).clamp(-a, a);
        let bend = ((a - along) * (a + along)).max(0.).sqrt();
        axis * along + pole * bend
    };
    let new_lower = axis * distance - new_upper;
    let swing = align(upper_vec.try_normalize()?, new_upper.try_normalize()?);
    let shin = align(lower_vec.try_normalize()?, new_lower.try_normalize()?);
    let thigh_rotation = (swing * hip.rotation).normalize();
    let shin_rotation = (shin * lower.rotation).normalize();
    Some([
        thigh_rotation,
        thigh_rotation.inverse() * shin_rotation,
        shin_rotation.inverse() * lower.rotation * ankle.rotation,
    ])
}

// The lowest pelvis position needed to keep both leg lengths when the ankle target
// is below the hip. The horizontal reach consumes part of the leg's full span.
fn needed_drop(hip: Vec3, knee: Vec3, ankle: Vec3, target: Vec3) -> f32 {
    let span = hip.distance(knee) + knee.distance(ankle);
    let horizontal = (target - hip).xz().length_squared();
    let vertical = (span * span - horizontal).max(0.).sqrt();
    (hip.y - target.y - vertical).max(0.)
}

type PlantingRig<'a> = (Entity, Ref<'a, FootRig>, &'a ContactHeight, Option<&'a FootCyclePhase>, &'a mut Stance);
type FootBinding<'a> = (Option<&'a SolePoints>, Option<&'a SoleSkin>, Option<Ref<'a, FootCycle>>);

fn plant(
    mut actors: Query<PlantingRig, With<PlantFeet>>,
    parents: Query<&ChildOf>,
    bindings: Query<FootBinding>,
    ground: Option<Res<FootGround>>,
    time: Res<Time>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    for (entity, rig, height, phase, mut stance) in &mut actors {
        let Some(band) = height.0.filter(|h| h.is_finite() && *h >= 0.) else {
            *stance = default();
            continue;
        };
        if entity != rig.actor || parents.get(rig.armature).map(ChildOf::parent).ok() != Some(entity) {
            continue;
        }
        let Ok(root) = transforms.p0().compute_global_transform(entity) else { continue };
        let origin = root.translation();
        let scale = root.affine().transform_vector3(Vec3::Y).length();
        if !scale.is_finite() || scale < 1e-6 {
            continue;
        }
        let departed = stance.previous_root.is_some_and(|p| {
            p.distance(origin) > 0.25 * scale || (ground.is_none() && (p.y - origin.y).abs() > 0.001 * scale)
        });
        if rig.is_changed() || stance.band != Some(band) || departed {
            *stance = default();
        }
        stance.band = Some(band);
        stance.feet.resize_with(rig.chains.len(), Foot::default);
        stance.previous_root = Some(origin);
        let mut required_drop = 0.0_f32;
        for (foot, chain) in stance.feet.iter_mut().zip(&rig.chains) {
            foot.target = None;
            let [h, k, a, t] = chain.nodes();
            let Ok(base) = parents.get(h) else { continue };
            let Ok(parent) = transforms.p0().compute_global_transform(base.parent()) else { continue };
            let joints = transforms.p1();
            let Ok([hip, knee, ankle, toe]) = joints.get_many([h, k, a, t]) else { continue };
            let thigh = parent.mul_transform(*hip);
            let shin = thigh.mul_transform(*knee);
            let shoe = shin.mul_transform(*ankle);
            let position = shoe.transform_point(toe.translation);
            let height = (position.y - origin.y) / scale;
            let (sole, skin, cycle) = bindings.get(a).unwrap_or_default();
            let plan = phase.and_then(|phase| cycle.as_ref()?.offset(phase.0));
            if foot.guided != plan.is_some() || cycle.as_ref().is_some_and(|c| c.is_changed()) {
                foot.anchor = None;
                foot.weight = 0.;
                foot.release = Vec3::ZERO;
            }
            foot.guided = plan.is_some();
            let planned = plan
                .map(|p| root.affine().transform_vector3(Vec3::new(p.x, 0., p.y)) * Vec3::new(1., 0., 1.))
                .unwrap_or_default();
            let mut offset = planned + foot.offset(position + planned, height, band, scale);
            let lift = sole.and_then(|sole| sole.clearance_lift(shoe, offset, origin.y, ground.as_deref()));
            let skin_lift = skin.and_then(|skin| {
                skin.lift(
                    [thigh, shin, shoe, shoe.mul_transform(*toe)],
                    offset,
                    ground.as_deref(),
                    origin.y,
                    &mut foot.surface_positions,
                )
            });
            let lift = lift.into_iter().chain(skin_lift).reduce(f32::max);
            let vertical = match ground.as_deref() {
                Some(ground) => ground.vertical_offset(position + offset, origin.y, height, band),
                None => Some(0.),
            };
            match (vertical, lift) {
                (Some(v), Some(lift)) => offset.y = v.max(lift),
                (Some(v), None) => offset.y = v,
                (None, Some(lift)) => offset.y = lift.max(0.),
                (None, None) => {
                    *foot = default();
                    continue;
                }
            }
            // A planned footprint can also exhaust reach horizontally on level ground.
            let drop_budget = if foot.guided && height <= band { offset.length() } else { -offset.y };
            if drop_budget > 0. {
                let drop = needed_drop(
                    thigh.translation(),
                    shin.translation(),
                    shoe.translation(),
                    shoe.translation() + offset,
                );
                required_drop = required_drop.max(drop.min(drop_budget));
            }
            foot.target = Some((parent, offset));
        }
        // Keep the controller's root motion. Lower the visual rig immediately when
        // a support needs it, then let the body rise over subsequent frames.
        stance.drop_m = required_drop.max(stance.drop_m - scale * time.delta_secs());
        let body_offset = Vec3::NEG_Y * stance.drop_m;
        let mut joints = transforms.p1();
        if stance.drop_m > 0.
            && let Ok(mut armature) = joints.get_mut(rig.armature)
        {
            stance.armature_rest = Some((rig.armature, armature.translation));
            armature.translation += root.affine().inverse().transform_vector3(body_offset);
        }
        for (foot, chain) in stance.feet.iter_mut().zip(&rig.chains) {
            let Some((parent, offset)) = foot.target.take() else { continue };
            let [h, k, a, t] = chain.nodes();
            let offset = offset - body_offset;
            if offset.length_squared() < 1e-12 {
                continue;
            }
            let Ok([mut hip, mut knee, mut ankle, toe]) = joints.get_many_mut([h, k, a, t]) else { continue };
            let local = parent.affine().inverse().transform_vector3(offset);
            if let Some(rotations) = solve_two_bone(*hip, *knee, *ankle, local) {
                let [rh, rk, ra] = match bindings.get(a) {
                    Ok((_, Some(skin), _)) => SkinFrame {
                        parent: GlobalTransform::from_translation(body_offset) * parent,
                        joints: [*hip, *knee, *ankle, *toe],
                        ground: ground.as_deref(),
                        root_y: origin.y,
                        scale,
                    }
                    .correct(skin, local, rotations, &mut foot.surface_positions),
                    _ => rotations,
                };
                foot.original = Some([(h, hip.rotation), (k, knee.rotation), (a, ankle.rotation)]);
                hip.rotation = rh;
                knee.rotation = rk;
                ankle.rotation = ra;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sole_samples_use_the_surface_beneath_each_point_and_release_over_a_hole() {
        let sole = SolePoints::new(vec![Vec3::new(0., -0.02, -0.08), Vec3::new(0., -0.02, 0.08)]).unwrap();
        let ground = FootGround::new(0.15, |p| (p.z >= 0.).then_some(0.08));
        let shoe = GlobalTransform::from_translation(Vec3::Y * 0.03);
        let lift = sole.clearance_lift(shoe, Vec3::ZERO, 0., Some(&ground)).unwrap();
        assert!((lift - 0.07).abs() < 1e-6);
        assert_eq!(sole.clearance_lift(shoe, -Vec3::Z, 0., Some(&ground)), None);
        assert!(SolePoints::new(vec![Vec3::NAN]).is_none());
    }

    #[test]
    fn world_support_honours_holes_step_height_and_swing_clearance() {
        let ground = FootGround::new(0.2, |p| (p.x >= 0.).then_some(0.15));
        assert_eq!(ground.vertical_offset(Vec3::NEG_X, 0., 0.02, 0.035), None);
        assert_eq!(ground.vertical_offset(Vec3::ZERO, -1., 0.02, 0.035), None);
        assert_eq!(ground.vertical_offset(Vec3::Y * 0.02, 0., 0.02, 0.035), Some(0.15));
        assert_eq!(ground.vertical_offset(Vec3::Y * 0.4, 0., 0.4, 0.035), Some(0.));
        let low = Vec3::Y * 0.12;
        assert!(low.y + ground.vertical_offset(low, 0., 0.12, 0.035).unwrap() >= 0.15);
    }

    #[test]
    fn symmetric_leg_reaches_the_elevated_target_and_extends_for_the_lower_one() {
        // Model-space Z is vertical in this fixture. The solver accepts any parent frame.
        let hip = Transform::from_xyz(-0.11, 0., 0.92);
        let knee = Transform::from_xyz(0., 0.085, -0.42);
        let ankle = Transform::from_xyz(0., -0.085, -0.42);
        for dz in [0.18, -0.12] {
            let [h, k, a] = solve_two_bone(hip, knee, ankle, Vec3::Z * dz).unwrap();
            let bent = hip.with_rotation(h).mul_transform(knee.with_rotation(k));
            let end = bent.mul_transform(ankle.with_rotation(a));
            let length = knee.translation.length();
            let end_z = (0.08_f32 + dz).max(hip.translation.z - 2. * length);
            let mid_z = (hip.translation.z + end_z) / 2.;
            let mid_y = (length * length - (hip.translation.z - mid_z).powi(2)).max(0.).sqrt();
            assert!(end.translation.distance(Vec3::new(-0.11, 0., end_z)) < 1e-5);
            assert!(bent.translation.distance(Vec3::new(-0.11, mid_y, mid_z)) < 0.0003);
        }
    }

    #[test]
    fn straight_oblique_limbs_can_bend_towards_a_reachable_target() {
        for link in [Vec3::splat(0.1), Vec3::new(0.2, 0.3, 0.6)] {
            let joint = Transform::from_translation(link);
            let target = Vec3::new(0.05, 0.18, 0.11);
            let [h, k, a] = solve_two_bone(Transform::IDENTITY, joint, joint, target - 2. * link).unwrap();
            let mid = Transform::from_rotation(h).mul_transform(joint.with_rotation(k));
            let end = mid.mul_transform(joint.with_rotation(a));
            assert!(end.translation.distance(target) < 1e-6);
            assert!((mid.translation.length() - link.length()).abs() < 1e-6);
        }
    }

    #[test]
    fn lower_the_rig_only_as_far_as_the_lower_support_requires() {
        let hip = Vec3::Y;
        let knee = Vec3::Y * 0.5;
        let ankle = Vec3::ZERO;
        assert!((needed_drop(hip, knee, ankle, Vec3::NEG_Y * 0.15) - 0.15).abs() < 1e-6);
        assert_eq!(needed_drop(hip, knee, ankle, Vec3::Y * 0.15), 0.);
    }

    #[test]
    fn a_leg_can_fold_onto_the_hip_and_turn_past_full_extension() {
        let hip = Transform::from_xyz(0., 0., 0.6);
        let knee = Transform::from_xyz(0., 0.1, -0.26);
        let ankle = Transform::from_xyz(0., -0.1, -0.26);
        let end = hip.mul_transform(knee).mul_transform(ankle).translation;
        for goal in [hip.translation, hip.translation + Vec3::Z * 0.25] {
            let [h, k, a] = solve_two_bone(hip, knee, ankle, goal - end).unwrap();
            let shin = hip.with_rotation(h).mul_transform(knee.with_rotation(k));
            let foot = shin.mul_transform(ankle.with_rotation(a));
            assert!(shin.is_finite() && foot.is_finite());
            assert!(foot.translation.distance(goal) < 1e-5);
            assert!((shin.translation.distance(foot.translation) - ankle.translation.length()).abs() < 1e-6);
        }
        let straight = Transform::from_xyz(0., -0.3, 0.);
        assert!(solve_two_bone(Transform::IDENTITY, straight, straight, Vec3::Y * 0.2).is_some());
    }

    #[test]
    fn corrections_smaller_than_a_milliradian_still_rotate_the_link() {
        let target = Vec3::new(0.0002, 1., 0.).normalize();
        assert!((align(Vec3::Y, target) * Vec3::Y).distance(target) < 1e-7);
    }

    #[test]
    fn a_planted_foot_reaches_its_target_without_changing_link_lengths_or_shoe_orientation() {
        let mut hip = Transform::from_xyz(0.1, 1., 0.).with_rotation(Quat::from_rotation_x(-0.25));
        let mut knee = Transform::from_xyz(0., -0.45, 0.).with_rotation(Quat::from_rotation_x(0.78));
        let mut ankle = Transform::from_xyz(0., -0.42, 0.).with_rotation(Quat::from_rotation_x(-0.42));
        let before = hip.mul_transform(knee).mul_transform(ankle);
        let delta = Vec3::new(0.025, 0., 0.05);
        [hip.rotation, knee.rotation, ankle.rotation] = solve_two_bone(hip, knee, ankle, delta).unwrap();
        let shin = hip.mul_transform(knee);
        let after = shin.mul_transform(ankle);
        assert!(after.translation.distance(before.translation + delta) < 1e-5);
        assert!(after.rotation.abs_diff_eq(before.rotation, 1e-5));
        assert!((hip.translation.distance(shin.translation) - 0.45).abs() < 1e-5);
        assert!((after.translation.distance(shin.translation) - 0.42).abs() < 1e-5);
        let [h, k, a] = solve_two_bone(hip, knee, ankle, Vec3::Z * 3.).unwrap();
        let extended = hip.with_rotation(h).mul_transform(knee.with_rotation(k)).mul_transform(ankle.with_rotation(a));
        assert!((hip.translation.distance(extended.translation) - 0.87).abs() < 1e-5);
        assert!(solve_two_bone(hip, Transform::IDENTITY, ankle, delta).is_none());
    }

    #[test]
    fn landing_anchors_the_horizontal_position_and_raising_the_toe_releases_it() {
        let mut foot = Foot::default();
        let band = 0.035;
        let start = Vec3::new(3., 0.03, 4.);
        assert_eq!(foot.offset(start, 0.03, band, 1.), Vec3::ZERO);
        let step = start + Vec3::new(0.01, 0.001, 0.06);
        let offset = foot.offset(step, 0.031, band, 1.);
        assert!((step + offset).xz().distance(start.xz()) < 1e-6);
        assert_eq!(offset.y, 0.);
        assert!(foot.offset(step, band + 0.02, band, 1.).abs_diff_eq(0.5 * offset, 1e-6));
        assert_eq!(foot.offset(step, band + 0.05, band, 1.), Vec3::ZERO);
        assert_eq!(foot.offset(step, band, band, 1.), Vec3::ZERO);
    }
}
