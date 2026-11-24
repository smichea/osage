// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

use super::{CollisionMaterial, ObjectNode};
use anyhow::{Context, Result, ensure};
use glam::{DMat4, DQuat, DVec3, Quat};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::f32::consts::PI;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Articulation {
    #[serde(default, skip_serializing_if = "RigSpace::is_actor")]
    pub space: RigSpace,
    pub bodies: Vec<RigBody>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RigSpace {
    #[default]
    Actor,
    Bone,
}

impl RigSpace {
    fn is_actor(&self) -> bool {
        matches!(self, Self::Actor)
    }
}

/// Binding data is kept at JSON precision until the assembled pose enters the solver.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RigFrame {
    pub anchor_m: [f64; 3],
    pub rotation_xyzw: [f64; 4],
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedJoint {
    pub anchor_m: [f32; 3],
    pub rotation_xyzw: [f32; 4],
}

impl RigFrame {
    fn valid(self) -> bool {
        self.anchor_m.into_iter().all(|v| (v as f32).is_finite())
            && Quat::from_array(self.rotation_xyzw.map(|v| v as f32)).is_normalized()
    }

    fn then(self, local: Self) -> Self {
        let rotation = DQuat::from_array(self.rotation_xyzw);
        // Vector transport needs a unit rotation. Preserve the authored quaternion's rounding
        // in the product so changing coordinate frames does not change the solver input.
        Self {
            anchor_m: (DVec3::from_array(self.anchor_m) + rotation.normalize() * DVec3::from_array(local.anchor_m))
                .to_array(),
            rotation_xyzw: (rotation * DQuat::from_array(local.rotation_xyzw)).to_array(),
        }
    }

    fn narrow(self) -> Result<ResolvedJoint> {
        ensure!(self.valid(), "assembled bind pose exceeds solver precision");
        Ok(ResolvedJoint {
            anchor_m: self.anchor_m.map(|v| v as f32),
            rotation_xyzw: self.rotation_xyzw.map(|v| v as f32),
        })
    }

    fn from_rigid_node(m: DMat4) -> Result<Self> {
        let [x, y, z] = [m.x_axis.truncate(), m.y_axis.truncate(), m.z_axis.truncate()];
        let tolerance = 3e-5;
        ensure!(
            m.is_finite()
                && [x, y, z].iter().all(|v| (v.length_squared() - 1.).abs() <= tolerance)
                && [x.dot(y), y.dot(z), z.dot(x)].into_iter().all(|d| d.abs() <= tolerance)
                && x.cross(y).dot(z) > 0.,
            "a bone carrying analytic physics needs a rigid right-handed transform"
        );
        let q = DQuat::from_mat4(&m).normalize();
        // q and -q encode the same basis. Give the stored bone frame a reproducible representative.
        let values = q.to_array();
        let largest = (1..4).fold(0, |i, j| if values[j].abs() > values[i].abs() { j } else { i });
        Ok(Self {
            anchor_m: m.w_axis.truncate().to_array(),
            rotation_xyzw: if values[largest] < 0. { (-q).to_array() } else { values },
        })
    }

    fn coincides(self, other: Self) -> bool {
        let a = DVec3::from_array(self.anchor_m);
        let b = DVec3::from_array(other.anchor_m);
        let tolerance = 1e-4 + 4. * f64::EPSILON * a.abs().max(b.abs()).max_element();
        let left = DQuat::from_array(self.rotation_xyzw).normalize();
        let right = DQuat::from_array(other.rotation_xyzw).normalize();
        a.distance(b) <= tolerance
            && [DVec3::X, DVec3::Y, DVec3::Z].into_iter().all(|axis| (left * axis).distance(right * axis) <= 2e-4)
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedRigBody {
    pub name: String,
    pub bone: String,
    pub center_m: [f32; 3],
    pub rotation_xyzw: [f32; 4],
    pub joint: Option<ResolvedJoint>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RigBody {
    pub name: String,
    pub bone: String,
    pub parent: Option<usize>,
    pub center_m: [f64; 3],
    pub rotation_xyzw: [f64; 4],
    pub mass_kg: f32,
    pub inertia_kg_m2: [f32; 3],
    pub material: String,
    pub shape: RigShape,
    pub joint: Option<RigJoint>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase", deny_unknown_fields)]
pub enum RigShape {
    Sphere { radius_m: f32 },
    Capsule { radius_m: f32, half_segment_m: f32 },
}

impl RigShape {
    pub fn dimensions(self) -> (f32, f32) {
        match self {
            Self::Sphere { radius_m } => (radius_m, 0.),
            Self::Capsule { radius_m, half_segment_m } => (radius_m, half_segment_m),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RigJoint {
    pub kind: RigJointKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_m: Option<[f64; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_xyzw: Option<[f64; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_frame: Option<RigFrame>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_frame: Option<RigFrame>,
    pub limits_rad: [[f32; 2]; 3],
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RigJointKind {
    Hinge,
    ThreeAxis,
}

impl RigJoint {
    fn actor_frame(&self) -> Option<RigFrame> {
        Some(RigFrame { anchor_m: self.anchor_m?, rotation_xyzw: self.frame_xyzw? })
    }

    fn valid_frames(&self, space: RigSpace) -> bool {
        match space {
            RigSpace::Actor => {
                self.parent_frame.is_none()
                    && self.child_frame.is_none()
                    && self.actor_frame().is_some_and(RigFrame::valid)
            }
            RigSpace::Bone => {
                self.anchor_m.is_none()
                    && self.frame_xyzw.is_none()
                    && self.parent_frame.is_some_and(RigFrame::valid)
                    && self.child_frame.is_some_and(RigFrame::valid)
            }
        }
    }
}

impl Articulation {
    /// Resolve parent-first rigid poses and check that independently authored joint endpoints coincide.
    pub fn resolve(&self, nodes: &[ObjectNode]) -> Result<Vec<ResolvedRigBody>> {
        let mut world = Vec::with_capacity(nodes.len());
        let mut by_id = HashMap::new();
        if matches!(self.space, RigSpace::Bone) {
            for node in nodes {
                let parent = match node.parent {
                    Some(i) => *world.get(i).context("rig nodes need a parent-first hierarchy")?,
                    None => DMat4::IDENTITY,
                };
                let matrix = parent * node.parent_from_local.as_dmat4();
                world.push(matrix);
                by_id.insert(node.geometry.id.as_str(), matrix);
            }
        }
        let mut poses: Vec<RigFrame> = Vec::with_capacity(self.bodies.len());
        let mut result = Vec::with_capacity(self.bodies.len());
        for body in &self.bodies {
            let local = RigFrame { anchor_m: body.center_m, rotation_xyzw: body.rotation_xyzw };
            let pose = match self.space {
                RigSpace::Actor => local,
                RigSpace::Bone => {
                    let matrix = by_id.get(body.bone.as_str()).context("unreachable rig bone")?;
                    RigFrame::from_rigid_node(*matrix).with_context(|| format!("bone {}", body.bone))?.then(local)
                }
            };
            let joint = match (&body.joint, self.space) {
                (None, _) => None,
                (Some(j), RigSpace::Actor) => Some(j.actor_frame().context("missing actor-space joint frame")?),
                (Some(j), RigSpace::Bone) => {
                    let parent = body.parent.and_then(|i| poses.get(i)).context("unreachable joint parent")?;
                    let a = parent.then(j.parent_frame.context("missing parent-side joint frame")?);
                    let b = pose.then(j.child_frame.context("missing child-side joint frame")?);
                    ensure!(a.coincides(b), "joint endpoints disagree in the assembled bind pose on {}", body.name);
                    Some(a)
                }
            };
            poses.push(pose);
            let pose = pose.narrow()?;
            result.push(ResolvedRigBody {
                name: body.name.clone(),
                bone: body.bone.clone(),
                center_m: pose.anchor_m,
                rotation_xyzw: pose.rotation_xyzw,
                joint: joint.map(RigFrame::narrow).transpose()?,
            });
        }
        Ok(result)
    }

    pub(crate) fn validate(
        &self,
        nodes: &HashSet<String>,
        materials: &BTreeMap<String, CollisionMaterial>,
    ) -> Result<()> {
        let mut names = HashSet::new();
        let mut bones = HashSet::new();
        ensure!(!self.bodies.is_empty(), "empty articulated object");
        for (i, b) in self.bodies.iter().enumerate() {
            let positive = |v: f32| v.is_finite() && v > 0.;
            let (radius, half) = b.shape.dimensions();
            ensure!(
                super::profile::valid_id(&b.name)
                    && names.insert(&b.name)
                    && nodes.contains(&b.bone)
                    && bones.insert(&b.bone)
                    && materials.contains_key(&b.material)
                    && positive(b.mass_kg)
                    && b.inertia_kg_m2.into_iter().all(positive)
                    && b.center_m.into_iter().all(|v| (v as f32).is_finite())
                    && Quat::from_array(b.rotation_xyzw.map(|v| v as f32)).is_normalized()
                    && positive(radius)
                    && half.is_finite()
                    && half >= 0.,
                "invalid articulated body {}",
                b.name
            );
            ensure!(
                if i == 0 {
                    b.parent.is_none() && b.joint.is_none()
                } else {
                    b.parent.is_some_and(|p| p < i) && b.joint.is_some()
                },
                "articulated bodies need a parent-first tree and a joint per non-root body"
            );
            if let Some(j) = &b.joint {
                ensure!(
                    j.valid_frames(self.space)
                        && j.limits_rad.iter().all(|&[lo, hi]| (-PI..=0.).contains(&lo) && (0. ..=PI).contains(&hi))
                        && (!matches!(j.kind, RigJointKind::Hinge) || j.limits_rad[1..] == [[0.; 2]; 2]),
                    "invalid joint on {}",
                    b.name
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use serde_json::{Value, json};

    #[test]
    fn nested_node_placements_transport_bodies_and_both_joint_endpoints() {
        let q = std::f32::consts::FRAC_1_SQRT_2;
        let frame = |y: f32| json!({"anchorM":[0,y,0],"rotationXyzw":[0,0,0,1]});
        let root = json!({"name":"root","bone":"a","parent":null,"centerM":[0,0,0],"rotationXyzw":[0,0,0,1],
            "massKg":1,"inertiaKgM2":[1,1,1],"material":"material","shape":{"type":"sphere","radiusM":0.1}});
        let mut child = root.clone();
        child["name"] = "child".into();
        child["bone"] = "b".into();
        child["parent"] = 0.into();
        child["centerM"] = json!([0, -0.25, 0]);
        child["joint"] =
            json!({"kind":"hinge","parentFrame":frame(1.),"childFrame":frame(-0.75),"limitsRad":[[-1,1],[0,0],[0,0]]});
        let input = json!({
            "asset":{"version":"2.0", "extras":{"osageObject":{"version":1,"id":"rig",
            "collisionMaterials":{"material":{"friction":0.5,"restitution":0}},"acousticMaterials":{},
            "articulation":{"space":"bone","bodies":[root,child]}}}},
            "scene":0, "scenes":[{"nodes":[0]}],
            "nodes":[{"children":[1],"translation":[2,3,4],"rotation":[0,0,q,q],"extras":{"osage":{"id":"outer"}}},
                {"children":[2],"translation":[1,0,2],"rotation":[0,q,0,q],"extras":{"osage":{"id":"inner"}}},
                {"children":[3],"extras":{"osage":{"id":"a"}}},
                {"translation":[0,2,0],"extras":{"osage":{"id":"b"}}}]
        });
        let read = |value: &Value| -> Result<_> {
            let doc = gltf::Gltf::from_slice(&serde_json::to_vec(value)?)?;
            let loaded = super::super::ObjectProfile::read(&doc)?.unwrap();
            loaded.header.articulation.unwrap().resolve(&loaded.nodes)
        };
        let resolved = read(&input).unwrap();
        let near = |a: [f32; 3], b: [f32; 3]| assert!(Vec3::from_array(a).distance(Vec3::from_array(b)) < 2e-6);
        near(resolved[0].center_m, [2., 4., 6.]);
        near(resolved[1].center_m, [0.25, 4., 6.]);
        let joint = resolved[1].joint.unwrap();
        near(joint.anchor_m, [1., 4., 6.]);
        near((Quat::from_array(joint.rotation_xyzw) * Vec3::Y).to_array(), [-1., 0., 0.]);
        let mut bad = input.clone();
        bad["nodes"][0]["scale"] = json!([1, 2, 1]);
        assert!(read(&bad).is_err());
        let mut bad = input.clone();
        bad["asset"]["extras"]["osageObject"]["articulation"]["bodies"][1]["joint"]["childFrame"] = frame(-0.7);
        assert!(read(&bad).is_err());
        let mut bad = input;
        bad["asset"]["extras"]["osageObject"]["articulation"]["bodies"][1]["joint"]["parentFrame"]["rotationXyzw"] =
            json!([0, 0, q, q]);
        assert!(read(&bad).is_err());
    }

    #[test]
    fn joint_tree_requires_reachable_unique_bones_and_valid_rest_limits() {
        let b = json!({
            "name":"base", "bone":"rig/0", "parent":null, "centerM":[0,0,0], "rotationXyzw":[0,0,0,1], "material":"solid",
            "shape":{"type":"sphere","radiusM":0.1}, "massKg":1, "inertiaKgM2":[0.1,0.2,0.2]
        });
        let mut child = b.clone();
        child["name"] = "limb".into();
        child["bone"] = "rig/1".into();
        child["parent"] = 0.into();
        child["shape"] = json!({"type":"capsule", "radiusM":0.05, "halfSegmentM":0.075});
        child["joint"] =
            json!({"kind":"hinge", "frameXyzw":[0,0,0,1], "anchorM":[0,0.1,0], "limitsRad":[[-0.2,1.3],[0,0],[0,0]]});
        let valid = json!({"bodies":[b,child]});
        let nodes = ["rig/0".to_string(), "rig/1".to_string()].into();
        let materials = [("solid".to_owned(), CollisionMaterial { friction: 0.6, restitution: 0. })].into();
        let accepts = |v| serde_json::from_value::<Articulation>(v).unwrap().validate(&nodes, &materials).is_ok();
        assert!(accepts(valid.clone()));
        for (path, value) in [
            ("/bodies/0/parent", json!(1)),
            ("/bodies/1/parent", json!(1)),
            ("/bodies/1/bone", json!("missing")),
            ("/bodies/1/bone", json!("rig/0")),
            ("/bodies/1/material", json!("other")),
            ("/bodies/1/joint/limitsRad/1", json!([-0.1, 0.1])),
            ("/bodies/1/joint/limitsRad/0", json!([0.2, 0.1])),
            ("/bodies/1/joint/frameXyzw", json!([0, 0, 0, 0])),
            ("/bodies/1/inertiaKgM2/1", json!(-1)),
            ("/bodies/1/joint", Value::Null),
        ] {
            let mut invalid = valid.clone();
            *invalid.pointer_mut(path).unwrap_or_else(|| panic!("absent test field {path}")) = value;
            assert!(!accepts(invalid), "{path}");
        }
    }
}
