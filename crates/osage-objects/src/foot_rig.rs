// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

use super::ObjectNode;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

/// Persistent IDs in asset.extras.osageFootRig. Each contact's three direct
/// ancestors are the ankle, lower link and upper link respectively.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FootBinding {
    armature: String,
    contacts: Vec<String>,
    #[serde(default, rename = "solePoints")]
    sole_points: HashMap<String, Vec<[f32; 3]>>,
}

/// Parent-first object slots, independent of glTF array order and display names.
pub struct ObjectFootRig {
    pub armature: usize,
    pub chains: Vec<[usize; 4]>,
    /// Rigid support points in local metres, keyed by the resolved ankle slot.
    pub sole_points: HashMap<usize, Vec<[f32; 3]>>,
}

impl FootBinding {
    pub(crate) fn resolve(self, nodes: &[ObjectNode]) -> Result<ObjectFootRig> {
        let by_id: HashMap<_, _> = nodes.iter().enumerate().map(|(i, n)| (n.geometry.id.as_str(), i)).collect();
        let armature = *by_id.get(self.armature.as_str()).context("unknown foot rig armature")?;
        ensure!(nodes[armature].parent.is_none(), "foot rig armature must be an object root");
        ensure!(!self.contacts.is_empty(), "empty foot rig");
        let mut used = HashSet::new();
        let mut chains = Vec::new();
        let mut declarations = self.sole_points;
        let mut sole_points = HashMap::new();
        for contact in self.contacts {
            let mut slot = *by_id.get(contact.as_str()).with_context(|| format!("unknown foot contact {contact}"))?;
            let mut chain = [slot; 4];
            for joint in chain[..3].iter_mut().rev() {
                slot = nodes[slot].parent.with_context(|| format!("short foot chain at {contact}"))?;
                *joint = slot;
            }
            ensure!(chain.iter().all(|&i| i != armature && used.insert(i)), "overlapping foot chains at {contact}");
            if let Some(points) = declarations.remove(&contact) {
                ensure!(
                    !points.is_empty() && points.iter().flatten().all(|n| n.is_finite()),
                    "invalid sole points at {contact}"
                );
                sole_points.insert(chain[2], points);
            }
            chains.push(chain);
        }
        for chain in &chains {
            let mut node = chain[0];
            while node != armature {
                node = nodes[node].parent.context("foot chain outside its armature")?;
                ensure!(!used.contains(&node), "one foot chain lies under another");
            }
        }
        ensure!(declarations.is_empty(), "sole points on undeclared foot contact");
        Ok(ObjectFootRig { armature, chains, sole_points })
    }
}

#[cfg(test)]
mod tests {
    use super::super::ObjectProfile;
    use serde_json::json;

    #[test]
    fn contact_ids_follow_edges_across_reordered_source_nodes() {
        let mut model = json!({
            "asset":{"version":"2.0","extras":{
                "osageObject":{"version":1,"id":"fixture","collisionMaterials":{},"acousticMaterials":{}},
                "osageFootRig":{"armature":"body","contacts":["pad"]}}},
            "scene":0,"scenes":[{"nodes":[4]}],
            "nodes":[
                {"extras":{"osage":{"id":"pad"}}},
                {"children":[0],"extras":{"osage":{"id":"ankle"}}},
                {"children":[1],"extras":{"osage":{"id":"lower"}}},
                {"children":[2],"extras":{"osage":{"id":"upper"}}},
                {"children":[3],"extras":{"osage":{"id":"body"}}}]
        });
        let load = |value: &serde_json::Value| {
            let gltf = gltf::Gltf::from_slice(&serde_json::to_vec(value).unwrap()).unwrap();
            ObjectProfile::read(&gltf.document).map(|p| p.unwrap().foot_rig)
        };
        let rig = load(&model).unwrap().unwrap();
        assert_eq!(rig.armature, 0);
        assert_eq!(rig.chains, [[1, 2, 3, 4]]);
        assert!(rig.sole_points.is_empty());
        let binding = &mut model["asset"]["extras"]["osageFootRig"];
        binding["solePoints"] = json!({"pad": [[0, -0.02, 0.04]]});
        assert_eq!(load(&model).unwrap().unwrap().sole_points[&3], [[0., -0.02, 0.04]]);
        for invalid in [json!({"pad":[]}), json!({"typo":[[0,0,0]]})] {
            model["asset"]["extras"]["osageFootRig"]["solePoints"] = invalid;
            assert!(load(&model).is_err());
        }
        model["asset"]["extras"]["osageFootRig"].as_object_mut().unwrap().remove("solePoints");
        for contacts in [json!([]), json!(["missing"]), json!(["lower"]), json!(["pad", "pad"])] {
            model["asset"]["extras"]["osageFootRig"]["contacts"] = contacts;
            assert!(load(&model).is_err());
        }
        model["asset"]["extras"].as_object_mut().unwrap().remove("osageFootRig");
        assert!(load(&model).unwrap().is_none());
    }
}
