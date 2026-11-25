// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::scene::{Primitive, Sector};
use anyhow::{Context, Result, bail};
use bevy::prelude::{Mat4, Quat, Vec3};
use osage_dynamics::Part;
use std::{fmt, str::FromStr, sync::Arc};

#[derive(Clone)]
pub enum Model {
    Capsule,
    Human,
    Imported(Arc<ImportedRig>),
}

#[derive(serde::Serialize)]
pub struct BodyBinding {
    source_node: usize,
    bone: String,
    material: String,
}

pub struct ImportedRig {
    pub bindings: Vec<BodyBinding>,
    parts: Vec<Part>,
    pub visuals: Vec<(usize, Primitive)>,
}

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Capsule => "capsule",
            Self::Human => "human",
            Self::Imported(_) => "imported",
        })
    }
}

impl Model {
    pub fn read_rig(bytes: &[u8]) -> Result<Self> {
        let (scene, source_nodes) = Sector::read_actor_page(bytes).context("reading the actor GLB")?;
        let obj = scene.object.context("actor GLB needs an Osage object profile")?;
        let rig = obj.profile.header.articulation.context("actor GLB has no articulation")?;
        let resolved = rig.resolve(&obj.profile.nodes)?;
        let bind_bones: std::collections::HashMap<_, _> =
            rig.bodies.iter().enumerate().map(|(index, b)| (b.bone.as_str(), index)).collect();
        // An unmodelled skeleton bone follows its nearest rigid ancestor.
        let mut ancestors = Vec::with_capacity(obj.profile.nodes.len());
        let mut bindings: Vec<_> = (0..rig.bodies.len()).map(|_| None).collect();
        for (source_node, node) in source_nodes.into_iter().zip(&obj.profile.nodes) {
            let bound = bind_bones.get(node.geometry.id.as_str()).copied();
            if let Some(slot) = bound {
                bindings[slot] = Some(BodyBinding {
                    source_node,
                    bone: node.geometry.id.clone(),
                    material: rig.bodies[slot].material.clone(),
                });
            }
            let owner = bound.or_else(|| node.parent.and_then(|p| ancestors[p]));
            ancestors.push(owner);
        }
        let mut visuals = Vec::new();
        for (inst, &node) in scene.instances.iter().zip(&obj.visual_nodes) {
            let index = ancestors[node].context("a rigid actor visual needs a physical ancestor")?;
            let b = &resolved[index];
            let bind = Mat4::from_rotation_translation(Quat::from_array(b.rotation_xyzw), b.center_m.into());
            let local = bind.inverse() * inst.transform;
            let primitive = crate::object::bake(&scene.meshes[inst.mesh], local)?;
            visuals.extend(primitive.into_iter().map(|p| (index, p)));
        }
        let parts: Vec<_> = rig
            .bodies
            .into_iter()
            .zip(resolved)
            .map(|(body, bind)| {
                let (radius, half_segment) = body.shape.dimensions();
                let (anchor, joint_rotation, limits_deg, swing_envelope) = match bind.joint.zip(body.joint) {
                    Some((frame, j)) => (
                        frame.anchor_m.into(),
                        Quat::from_array(frame.rotation_xyzw),
                        j.limits_rad.map(|r| r.map(f32::to_degrees)),
                        j.swing_envelope,
                    ),
                    None => (Vec3::from_array(bind.center_m), Quat::IDENTITY, [[0.; 2]; 3], Default::default()),
                };
                Part {
                    name: body.name,
                    parent: body.parent,
                    center: bind.center_m.into(),
                    rotation: Quat::from_array(bind.rotation_xyzw),
                    radius,
                    half_segment,
                    mass: body.mass_kg,
                    inertia: Some(body.inertia_kg_m2.into()),
                    material: Some(obj.profile.header.collision_materials[&body.material]),
                    joint_rotation,
                    anchor,
                    limits_deg,
                    swing_envelope,
                }
            })
            .collect();
        let bindings = bindings.into_iter().collect::<Option<Vec<_>>>().context("unbound physical actor body")?;
        Ok(Self::Imported(Arc::new(ImportedRig { parts, visuals, bindings })))
    }

    pub fn parts(&self) -> Option<Vec<Part>> {
        match self {
            Self::Capsule => None,
            Self::Human => Some(human()),
            Self::Imported(rig) => Some(rig.parts.clone()),
        }
    }
}

impl FromStr for Model {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "capsule" => Ok(Self::Capsule),
            "human" => Ok(Self::Human),
            _ => bail!("unknown actor {s}"),
        }
    }
}

// Bind landmarks in metres, rounded from the CC0 Quaternius male skeleton in the asset manifest.
// Radius, kilograms and angular limits in degrees are model parameters for this experiment.
pub fn human() -> Vec<Part> {
    let fixed = [0., 0.];
    let base = [0., 0.949, -0.043];
    let spine = [0., 1.072, -0.007];
    let neck = [0., 1.520, -0.041];
    let mut parts = vec![
        Part::between("pelvis", None, [[0., 0.84, -0.043], spine], 0.13, 11., [fixed; 3]),
        Part::between("spine_01", Some(0), [spine, neck], 0.15, 25., [[-30., 30.], [-25., 25.], [-25., 25.]]),
        Part::between("Head", Some(1), [neck, [0., 1.79, -0.017]], 0.10, 5., [[-40., 40.], [-55., 55.], [-30., 30.]]),
    ];
    for (s, names) in [
        (1., ["upperarm_l", "lowerarm_l", "hand_l", "thigh_l", "calf_l", "foot_l"]),
        (-1., ["upperarm_r", "lowerarm_r", "hand_r", "thigh_r", "calf_r", "foot_r"]),
    ] {
        let shoulder = [s * 0.212, 1.455, -0.065];
        let elbow = [s * 0.463, 1.455, -0.073];
        let wrist = [s * 0.706, 1.455, -0.065];
        let arm = parts.len();
        parts.push(Part::between(
            names[0],
            Some(1),
            [shoulder, elbow],
            0.061,
            3.,
            [[-80., 80.], [-85., 85.], [-90., 90.]],
        ));
        let elbow_flex = if s > 0. { [-130., 2.] } else { [-2., 130.] };
        parts.push(Part::between(names[1], Some(arm), [elbow, wrist], 0.052, 1.55, [fixed, elbow_flex, fixed]));
        parts.push(Part::between(
            names[2],
            Some(arm + 1),
            [wrist, [s * 0.83, 1.455, -0.06]],
            0.041,
            0.45,
            [[-35., 35.], [-35., 35.], [-35., 35.]],
        ));
        let hip = [s * 0.114, base[1] + 0.022, -0.036];
        let knee = [s * 0.114, 0.542, -0.036];
        let ankle = [s * 0.114, 0.086, -0.088];
        let leg = parts.len();
        parts.push(Part::between(names[3], Some(0), [hip, knee], 0.086, 7., [[-90., 45.], [-45., 45.], [-50., 50.]]));
        parts.push(Part::between(names[4], Some(leg), [knee, ankle], 0.063, 3.7, [[-3., 125.], fixed, fixed]));
        parts.push(Part::between(
            names[5],
            Some(leg + 1),
            [ankle, [s * 0.114, 0.047, 0.135]],
            0.048,
            1.3,
            [[-40., 30.], [-12., 12.], [-18., 18.]],
        ));
    }
    parts
}
