// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

use super::CollisionMaterial;
use anyhow::{Context, Result, ensure};
use glam::{DVec3, Mat4};
use gltf::Document;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::{BTreeMap, HashSet};

pub const BAND_HZ: [u32; 3] = [250, 1000, 4000];

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcousticMaterial {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub absorption: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transmission: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scattering: Option<f32>,
}

impl AcousticMaterial {
    pub fn valid(self) -> bool {
        match (self.absorption, self.transmission, self.scattering) {
            (None, None, None) => true,
            (Some(a), Some(t), Some(s)) => {
                (0. ..=1.).contains(&s)
                    && a.into_iter()
                        .zip(t)
                        .all(|(a, t)| (0. ..=1.).contains(&a) && (0. ..=1.).contains(&t) && a + t <= 1.000001)
            }
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[serde(rename_all = "camelCase")]
pub enum VisualLod {
    #[default]
    High,
    Medium,
    Low,
    VeryLow,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LodMesh {
    pub level: VisualLod,
    pub mesh: usize,
    #[serde(rename = "maxDeviationM", default, skip_serializing_if = "Option::is_none")]
    pub max_deviation_m: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CollisionSurface {
    pub id: String,
    pub mesh: usize,
    pub material: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcousticSource {
    Authored,
    CollisionProxy,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcousticSurface {
    pub id: String,
    pub mesh: usize,
    pub material: String,
    pub source: AcousticSource,
}

/// A convex room in its owning node's local metre coordinates.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcousticRoom {
    pub bounds_m: [[f32; 3]; 2],
}

/// Ordered perimeter of an aperture joining the named rooms.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcousticPortal {
    pub rooms: [String; 2],
    pub corners_m: [[f32; 3]; 4],
    #[serde(default)]
    pub closure_fraction: f32,
}

/// A replacement becomes active at this detail level and all coarser levels.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubtreeProxy {
    pub visual: Option<VisualLod>,
    pub collision: Option<VisualLod>,
    pub acoustic: Option<VisualLod>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct GeometryLevels {
    pub visual: VisualLod,
    pub collision: VisualLod,
    pub acoustic: VisualLod,
}

#[derive(Clone, Copy, Default)]
pub struct ActiveGeometry {
    pub visual: bool,
    pub collision: bool,
    pub acoustic: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NodeGeometry {
    pub id: String,
    #[serde(default)]
    pub collision: Vec<CollisionSurface>,
    #[serde(default)]
    pub acoustic: Vec<AcousticSurface>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acoustic_room: Option<AcousticRoom>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acoustic_portal: Option<AcousticPortal>,
    #[serde(default)]
    pub visual_lods: Vec<LodMesh>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtree_proxy: Option<SubtreeProxy>,
}

pub struct ObjectNode {
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub parent_from_local: Mat4,
    pub world_from_local: Mat4,
    pub visual: Option<usize>,
    pub geometry: NodeGeometry,
}

impl ObjectNode {
    pub fn visual_at(&self, lod: VisualLod) -> Option<usize> {
        self.geometry.visual_lods.iter().find(|entry| entry.level == lod).map(|entry| entry.mesh).or(self.visual)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectHeader {
    pub version: u32,
    pub id: String,
    pub collision_materials: BTreeMap<String, CollisionMaterial>,
    pub acoustic_materials: BTreeMap<String, AcousticMaterial>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub articulation: Option<super::Articulation>,
}

pub struct ObjectProfile {
    pub foot_rig: Option<super::ObjectFootRig>,
    pub header: ObjectHeader,
    pub roots: Vec<usize>,
    pub nodes: Vec<ObjectNode>,
}

fn metadata<T: DeserializeOwned>(extras: &gltf::json::Extras, key: &str) -> Result<Option<T>> {
    let Some(raw) = extras else { return Ok(None) };
    let mut json = serde_json::from_str::<serde_json::Value>(raw.get())?;
    json.get_mut(key)
        .map(|value| serde_json::from_value(value.take()).with_context(|| format!("invalid {key}")))
        .transpose()
}

pub(crate) fn valid_id(id: &str) -> bool {
    !id.trim().is_empty() && !id.contains(',') && !id.chars().any(char::is_control)
}

impl ObjectProfile {
    /// Stable component IDs from a scene root to the requested node, inclusive.
    /// IDs may themselves contain slashes, so the segments stay separate.
    pub fn node_path(&self, index: usize) -> Option<Vec<&str>> {
        let mut node = self.nodes.get(index)?;
        let mut path = vec![node.geometry.id.as_str()];
        while let Some(parent) = node.parent {
            node = &self.nodes[parent];
            path.push(node.geometry.id.as_str());
        }
        path.reverse();
        Some(path)
    }

    /// Select independent cuts for the eye, contact and sound. A plain group or
    /// part keeps its own geometry and allows its descendants to contribute.
    pub fn active_geometry(&self, levels: GeometryLevels) -> Vec<ActiveGeometry> {
        let mut active = vec![ActiveGeometry::default(); self.nodes.len()];
        let open = ActiveGeometry { visual: true, collision: true, acoustic: true };
        let mut pending: Vec<_> = self.roots.iter().map(|&root| (root, open)).collect();
        while let Some((index, open)) = pending.pop() {
            let node = &self.nodes[index];
            let proxy = node.geometry.subtree_proxy.unwrap_or_default();
            let select = |open: bool, threshold: Option<VisualLod>, level: VisualLod| {
                let replace = threshold.is_some_and(|t| level >= t);
                (open && (threshold.is_none() || replace), open && !replace)
            };
            let (visual, child_visual) = select(open.visual, proxy.visual, levels.visual);
            let (collision, child_collision) = select(open.collision, proxy.collision, levels.collision);
            let (acoustic, child_acoustic) = select(open.acoustic, proxy.acoustic, levels.acoustic);
            active[index] = ActiveGeometry { visual, collision, acoustic };
            let child = ActiveGeometry { visual: child_visual, collision: child_collision, acoustic: child_acoustic };
            if child.visual || child.collision || child.acoustic {
                pending.extend(node.children.iter().map(|&i| (i, child)));
            }
        }
        active
    }

    /// A missing asset tag identifies older or ordinary glTF content. A malformed tag is an error.
    pub fn read(doc: &Document) -> Result<Option<Self>> {
        Self::parse(doc, None)
    }

    /// Return the original glTF node index for each parent-first profile slot.
    pub fn read_with_source_indices(doc: &Document) -> Result<Option<(Self, Vec<usize>)>> {
        let mut indices = Vec::new();
        Ok(Self::parse(doc, Some(&mut indices))?.map(|profile| (profile, indices)))
    }

    fn parse(doc: &Document, mut source_indices: Option<&mut Vec<usize>>) -> Result<Option<Self>> {
        let Some(header): Option<ObjectHeader> = metadata(&doc.as_json().asset.extras, "osageObject")? else {
            return Ok(None);
        };
        ensure!(header.version == 1 && valid_id(&header.id), "unsupported object revision or empty ID");
        ensure!(
            header.collision_materials.iter().all(|(id, m)| valid_id(id) && m.valid()),
            "invalid collision material"
        );
        ensure!(header.acoustic_materials.iter().all(|(id, m)| valid_id(id) && m.valid()), "invalid acoustic material");
        let count = doc.meshes().len();
        let scene = doc.default_scene().context("object needs a default scene")?;
        let mut pending: Vec<_> = scene.nodes().map(|n| (n, None, Mat4::IDENTITY)).collect();
        pending.reverse();
        let mut nodes: Vec<ObjectNode> = Vec::new();
        let mut roots = Vec::new();
        let mut visited = HashSet::new();
        let (mut ids, mut collision_ids, mut acoustic_ids) = (HashSet::new(), HashSet::new(), HashSet::new());
        while let Some((node, parent, base)) = pending.pop() {
            ensure!(visited.insert(node.index()), "object nodes must form a tree");
            let geometry: NodeGeometry =
                metadata(node.extras(), "osage")?.context("object node lacks osage metadata")?;
            ensure!(
                valid_id(&geometry.id) && ids.insert(geometry.id.clone()),
                "duplicate or empty node ID {}",
                geometry.id
            );
            let parent_from_local = Mat4::from_cols_array_2d(&node.transform().matrix());
            let world_from_local = base * parent_from_local;
            let determinant = world_from_local.determinant();
            ensure!(
                world_from_local.is_finite()
                    && determinant.is_finite()
                    && determinant != 0.
                    && world_from_local.row(3) == glam::Vec4::W,
                "invalid affine transform on {}",
                geometry.id
            );
            let mut levels = HashSet::new();
            let visual = node.mesh().map(|m| m.index());
            if let Some(proxy) = geometry.subtree_proxy {
                let channels = [
                    (proxy.visual, visual.is_some()),
                    (proxy.collision, !geometry.collision.is_empty()),
                    (proxy.acoustic, !geometry.acoustic.is_empty()),
                ];
                ensure!(
                    node.children().next().is_some()
                        && channels.iter().any(|(threshold, _)| threshold.is_some())
                        && channels
                            .iter()
                            .all(|(threshold, bound)| threshold.is_none_or(|t| t != VisualLod::High && *bound)),
                    "subtree proxy needs children, a coarse threshold and geometry for each declared channel on {}",
                    geometry.id
                );
            }
            for lod in &geometry.visual_lods {
                ensure!(
                    visual.is_some()
                        && lod.mesh < count
                        && lod.level != VisualLod::High
                        && levels.insert(lod.level)
                        && lod.max_deviation_m.is_none_or(|e| e.is_finite() && e >= 0.),
                    "invalid visual level on {}",
                    geometry.id
                );
            }
            for surface in &geometry.collision {
                ensure!(
                    valid_id(&surface.id)
                        && collision_ids.insert(surface.id.clone())
                        && surface.mesh < count
                        && header.collision_materials.contains_key(&surface.material),
                    "invalid collision surface {}",
                    surface.id
                );
            }
            for surface in &geometry.acoustic {
                ensure!(
                    valid_id(&surface.id)
                        && acoustic_ids.insert(surface.id.clone())
                        && surface.mesh < count
                        && header.acoustic_materials.contains_key(&surface.material),
                    "invalid acoustic surface {}",
                    surface.id
                );
                if matches!(surface.source, AcousticSource::CollisionProxy) {
                    ensure!(
                        geometry.collision.iter().any(|c| c.mesh == surface.mesh),
                        "acoustic proxy lacks its collision mesh"
                    );
                }
            }
            let index = nodes.len();
            if let Some(p) = parent {
                let ancestor: &mut ObjectNode = &mut nodes[p];
                ancestor.children.push(index);
            } else {
                roots.push(index);
            }
            if let Some(indices) = &mut source_indices {
                indices.push(node.index());
            }
            nodes.push(ObjectNode {
                parent,
                children: Vec::new(),
                parent_from_local,
                world_from_local,
                visual,
                geometry,
            });
            let first_child = pending.len();
            pending.extend(node.children().map(|child| (child, Some(index), world_from_local)));
            pending[first_child..].reverse();
        }
        ensure!(!nodes.is_empty(), "empty object tree");
        if let Some(rig) = &header.articulation {
            rig.validate(&ids, &header.collision_materials)?;
            rig.resolve(&nodes)?;
        }
        let foot_rig = metadata::<super::foot_rig::FootBinding>(&doc.as_json().asset.extras, "osageFootRig")?
            .map(|binding| binding.resolve(&nodes))
            .transpose()?;
        Ok(Some(Self { header, roots, nodes, foot_rig }))
    }
}

/// A finite two-sided query in world metres. The endpoints and direction are shared by all tested triangles.
#[derive(Clone, Copy)]
pub struct TriangleRay {
    start: DVec3,
    delta: DVec3,
    length: f64,
}

impl TriangleRay {
    pub fn new(from: [f32; 3], to: [f32; 3]) -> Option<Self> {
        let start = DVec3::from_array(from.map(f64::from));
        let delta = DVec3::from_array(to.map(f64::from)) - start;
        let length = delta.length();
        (length > 0. && length.is_finite()).then_some(Self { start, delta, length })
    }

    /// Distance to one input face, including its edges and vertices.
    pub fn hit(&self, face: [[f32; 3]; 3]) -> Option<f64> {
        let [a, b, c] = face.map(|p| DVec3::from_array(p.map(f64::from)));
        let (ab, ac) = (b - a, c - a);
        let p = self.delta.cross(ac);
        let det = ab.dot(p);
        if det.abs() <= 1e-14 * self.length * ab.length() * ac.length() {
            return None;
        }
        let relative = self.start - a;
        let u = relative.dot(p) / det;
        let q = relative.cross(ab);
        let v = self.delta.dot(q) / det;
        let t = ac.dot(q) / det;
        (u >= -1e-12 && v >= -1e-12 && u + v <= 1. + 1e-12 && (0. ..=1.).contains(&t)).then_some(t * self.length)
    }

    pub fn point(&self, distance_m: f64) -> [f64; 3] {
        (self.start + self.delta * (distance_m / self.length)).to_array()
    }

    /// Segment overlap with a closed axis-aligned box. A parallel segment can lie on a face.
    pub fn intersects_bounds(&self, min: [f32; 3], max: [f32; 3]) -> bool {
        let (mut enter, mut exit) = (0.0_f64, 1.0_f64);
        for axis in 0..3 {
            let (lo, hi) = (f64::from(min[axis]), f64::from(max[axis]));
            let (p, d) = (self.start[axis], self.delta[axis]);
            if d == 0. {
                if p < lo || p > hi {
                    return false;
                }
            } else {
                let (a, b) = ((lo - p) / d, (hi - p) / d);
                enter = enter.max(a.min(b));
                exit = exit.min(a.max(b));
                if enter > exit {
                    return false;
                }
            }
        }
        true
    }
}

/// First two-sided hit in metres along a finite segment. Shared edges yield the same distance.
pub fn first_hit(from: [f32; 3], to: [f32; 3], triangles: impl Iterator<Item = [[f32; 3]; 3]>) -> Option<f64> {
    let ray = TriangleRay::new(from, to)?;
    triangles.filter_map(|face| ray.hit(face)).min_by(f64::total_cmp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn finite_segment_keeps_boundary_boxes_and_uses_f64_for_long_directions() {
        let ray = TriangleRay::new([-3e38, 1., 0.], [3e38, 1., 0.]).unwrap();
        assert!(ray.intersects_bounds([-1., -1., -1.], [1.; 3]));
        assert!(!ray.intersects_bounds([-1., 1.01, -1.], [1., 2., 1.]));
        let face = [[0., 0., -2.], [0., 2., 0.], [0., 0., 2.]];
        let distance = ray.hit(face).unwrap();
        assert_eq!(ray.point(distance), [0., 1., 0.]);
        assert!(TriangleRay::new([0.; 3], [0.; 3]).is_none());
    }

    #[test]
    fn indexed_profile_keeps_file_addresses_across_reordered_and_inactive_nodes() {
        let asset = json!({
            "asset":{"version":"2.0","extras":{"osageObject":{
                "id":"rig","version":1,"collisionMaterials":{},"acousticMaterials":{}}}},
            "scene":0, "scenes":[{"nodes":[2]}],
            "nodes":[{},{"extras":{"osage":{"id":"right"}}},
                {"children":[3,1],"extras":{"osage":{"id":"root"}}},{"extras":{"osage":{"id":"left"}}}]
        });
        let doc = gltf::Gltf::from_slice(&serde_json::to_vec(&asset).unwrap()).unwrap();
        let (profile, indices) = ObjectProfile::read_with_source_indices(&doc).unwrap().unwrap();
        assert_eq!(indices, [2, 3, 1]);
        assert_eq!(profile.node_path(2).unwrap(), ["root", "right"]);
    }

    fn doc(channels: Value) -> gltf::Gltf {
        let root = json!({
            "asset": {"version":"2.0","extras":{"osageObject":{
                "id":"sample","version":1,"collisionMaterials":{"solid":{"friction":0.5,"restitution":0.}},
                "acousticMaterials":{"unknown":{}}
            }}},
            "scene":0,"scenes":[{"nodes":[0]}],
            "nodes":[{"translation":[1,0,0],"children":[1],"extras":{"osage":{"id":"parent"}}},
                {"translation":[0,2,0],"mesh":0,"extras":{"osage":channels}}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],
            "buffers":[{"byteLength":36}], "bufferViews":[{"buffer":0,"byteLength":36}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,1]}]
        });
        gltf::Gltf::from_slice(&serde_json::to_vec(&root).unwrap()).unwrap()
    }

    #[test]
    fn a_tree_composes_transforms_and_checks_each_channel_reference() {
        let document = doc(json!({"id":"slab","collision":[{"id":"floor","mesh":0,"material":"solid"}],
            "acoustic":[{"id":"floor-sound","mesh":0,"material":"unknown","source":"collision-proxy"}]}));
        let profile = ObjectProfile::read(&document).unwrap().unwrap();
        assert_eq!(profile.roots, [0]);
        assert_eq!(profile.nodes[0].children, [1]);
        assert_eq!(profile.nodes[1].parent, Some(0));
        assert_eq!(profile.node_path(1), Some(vec!["parent", "slab"]));
        assert!(profile.node_path(2).is_none());
        assert_eq!(profile.nodes[1].world_from_local.w_axis.truncate(), glam::vec3(1., 2., 0.));
        for invalid in [
            json!({"id":"parent"}),
            json!({"id":"slab","subtreeProxy":{"visual":"low"}}),
            json!({"id":"slab","collision":[{"id":"floor","mesh":3,"material":"solid"}]}),
            json!({"id":"slab","acoustic":[{"id":"s","mesh":0,"source":"collision-proxy","material":"unknown"}]}),
            json!({"id":"slab","acoustic":[{"id":"s","mesh":0,"source":"authored","material":"typo"}]}),
            json!({"id":"slab","visualLods":[{"mesh":0,"level":"low"},{"mesh":0,"level":"low"}]}),
        ] {
            assert!(ObjectProfile::read(&doc(invalid)).is_err());
        }
    }

    #[test]
    fn an_acoustic_material_preserves_unknowns_and_conserves_energy() {
        for (value, valid) in [
            (json!({}), true),
            (json!({"scattering":0.2}), false),
            (json!({"scattering":0.2,"absorption":[0.2,0.4,0.5],"transmission":[0.8,0.6,0.5]}), true),
            (json!({"scattering":0.2,"absorption":[0.3,0.4,0.5],"transmission":[0.8,0.6,0.5]}), false),
        ] {
            assert_eq!(serde_json::from_value::<AcousticMaterial>(value).unwrap().valid(), valid);
        }
    }

    #[test]
    fn a_two_sided_probe_on_a_shared_diagonal_has_one_nearest_distance() {
        let two = [[[-1., 1., -1.], [-1., 1., 1.], [1., 1., 1.]], [[-1., 1., -1.], [1., 1., 1.], [1., 1., -1.]]];
        assert_eq!(first_hit([0., 3., 0.], [0., 0., 0.], two.into_iter()), Some(2.));
        assert_eq!(first_hit([0., 0., 0.], [0., 3., 0.], two.into_iter()), Some(1.));
        assert_eq!(first_hit([0., 3., 0.], [0., 2., 0.], two.into_iter()), None);
        assert_eq!(first_hit([2., 3., 0.], [2., 0., 0.], two.into_iter()), None);
    }
}
