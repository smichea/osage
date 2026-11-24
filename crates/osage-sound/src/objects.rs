// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! A scene read from an object of the object format
//! (`design/08-object-format.md`), as `osage-objects` reads it: its faces are
//! the triangles of the meshes its nodes name in `acoustic[]` (with their
//! acoustic material) and `collision[]` (with their collision material), in
//! each node's frame. Two triangles of a mesh that make a flat convex quad
//! are taken as that quad ([`crate::faces::quads`]); the faces of one plane
//! that share a side are one surface ([`crate::faces::sheets`]).
//!
//! What sound adds to the format is read as [`Scene::from_json`] reads it
//! (emitters, contact sounds, the listener). What a mesh does to sound that
//! the format does not say is in `node.extras.osageSound.surfaceSound`, by
//! surface identifier: `reflects` and `occludes` (both true when not said)
//! and `strikes` (whether coming onto it is a blow, false when not said). An
//! acoustic material the format leaves unknown (`{}`) is not met by sound.
//!
//! [`with_meshes`] turns a scene's hand-written four-cornered faces
//! (`osageSound.surfaces[]`) into such meshes, in one inline buffer: the
//! same scene as an object of the format.

use serde_json::{Value, json};

use crate::{
    faces::quads,
    scene::{Material, Scene, Surface},
    space::{Vec3, vec3},
};

type Read<T> = Result<T, String>;

/// A scene from the bytes of a glTF asset (JSON with its buffers inline as
/// `data:` addresses, or a binary `.glb`) that is an object of the format.
pub fn from_gltf(bytes: &[u8]) -> Read<Scene> {
    let gltf = gltf::Gltf::from_slice(bytes).map_err(|e| format!("not a glTF asset: {e}"))?;
    let (profile, _) = osage_objects::ObjectProfile::read_with_source_indices(&gltf.document)
        .map_err(|e| format!("not an object of the format: {e:#}"))?
        .ok_or("a glTF asset, but not an object of the format: no asset.extras.osageObject")?;
    let text = if bytes.starts_with(b"glTF") {
        gltf::json::serialize::to_string(gltf.document.as_json()).map_err(|e| e.to_string())?
    } else {
        String::from_utf8(bytes.to_vec()).map_err(|_| "a glTF asset's JSON is UTF-8".to_owned())?
    };
    let mut scene = Scene::from_json(&text)?;
    let buffers = gltf
        .document
        .buffers()
        .map(|buffer| match buffer.source() {
            gltf::buffer::Source::Bin => gltf.blob.clone().ok_or_else(|| "no binary chunk".to_owned()),
            gltf::buffer::Source::Uri(uri) => inline(uri),
        })
        .collect::<Read<Vec<Vec<u8>>>>()?;
    let json: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let materials = &profile.header.acoustic_materials;
    for (i, node) in profile.nodes.iter().enumerate() {
        let path = profile.node_path(i).expect("a node of the profile");
        let Some(at) = scene.tree.find(&path) else { continue };
        // The flags of its surfaces, by identifier.
        let source = nth_node(&json, &path);
        let flags = source.and_then(|node| node.pointer("/extras/osageSound/surfaceSound"));
        let flag = |id: &str, key: &str, default: bool| {
            flags.and_then(|f| f.get(id)).and_then(|f| f.get(key)).and_then(Value::as_bool).unwrap_or(default)
        };
        for surface in &node.geometry.acoustic {
            let Some(material) = materials.get(&surface.material).and_then(|m| known(*m)) else { continue };
            let (reflects, occludes) = (flag(&surface.id, "reflects", true), flag(&surface.id, "occludes", true));
            for quad in faces_of(&gltf.document, &buffers, surface.mesh)? {
                scene.surfaces.push(Surface {
                    id: surface.id.clone(),
                    node: at,
                    quad,
                    acoustic: Some(material),
                    reflects,
                    occludes,
                    collision: None,
                    strikes: false,
                });
            }
        }
        for surface in &node.geometry.collision {
            let strikes = flag(&surface.id, "strikes", false);
            for quad in faces_of(&gltf.document, &buffers, surface.mesh)? {
                scene.surfaces.push(Surface {
                    id: surface.id.clone(),
                    node: at,
                    quad,
                    acoustic: None,
                    reflects: false,
                    occludes: false,
                    collision: Some(surface.material.clone()),
                    strikes,
                });
            }
        }
    }
    Ok(scene)
}

/// The engine's material for one of the format, if the format knows it.
fn known(m: osage_objects::AcousticMaterial) -> Option<Material> {
    let (a, t, s) = (m.absorption?, m.transmission?, m.scattering?);
    Some(Material { absorption: a.map(f64::from), transmission: t.map(f64::from), scattering: f64::from(s) })
}

/// The glTF node at a path of identifiers, from the scene's roots.
fn nth_node<'a>(json: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let nodes = json.get("nodes")?.as_array()?;
    let id = |n: &Value| n.pointer("/extras/osage/id").and_then(Value::as_str).map(str::to_owned);
    let scene = json.get("scene").and_then(Value::as_u64).unwrap_or(0) as usize;
    let mut here: Vec<usize> = json
        .get("scenes")?
        .get(scene)?
        .get("nodes")?
        .as_array()?
        .iter()
        .filter_map(|v| v.as_u64())
        .map(|v| v as usize)
        .collect();
    let mut found = None;
    for step in path {
        let next = here.iter().copied().find(|&n| nodes.get(n).and_then(id).as_deref() == Some(step))?;
        found = Some(next);
        here = nodes[next]
            .get("children")
            .and_then(Value::as_array)
            .map(|c| c.iter().filter_map(|v| v.as_u64()).map(|v| v as usize).collect())
            .unwrap_or_default();
    }
    nodes.get(found?)
}

/// The faces of a mesh: its triangles, two of a flat convex quad taken as
/// one.
fn faces_of(document: &gltf::Document, buffers: &[Vec<u8>], mesh: usize) -> Read<Vec<[Vec3; 4]>> {
    let mesh = document.meshes().nth(mesh).ok_or("no such mesh")?;
    let mut out = Vec::new();
    for primitive in mesh.primitives() {
        if primitive.mode() != gltf::mesh::Mode::Triangles {
            return Err(format!("mesh {}: sound reads triangles only", mesh.index()));
        }
        let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(Vec::as_slice));
        let corners: Vec<Vec3> = reader
            .read_positions()
            .ok_or_else(|| format!("mesh {}: no positions", mesh.index()))?
            .map(|p| vec3(f64::from(p[0]), f64::from(p[1]), f64::from(p[2])))
            .collect();
        let indices: Vec<u32> = match reader.read_indices() {
            Some(indices) => indices.into_u32().collect(),
            None => (0..corners.len() as u32).collect(),
        };
        if indices.iter().any(|&i| i as usize >= corners.len()) {
            return Err(format!("mesh {}: an index past its positions", mesh.index()));
        }
        let triangles: Vec<[u32; 3]> = indices
            .as_chunks::<3>()
            .0
            .iter()
            .copied()
            // A triangle of no area is no face.
            .filter(|t| {
                let at = |i: u32| corners[i as usize];
                (at(t[1]) - at(t[0])).cross(at(t[2]) - at(t[0])).length() > 1e-12
            })
            .collect();
        out.extend(quads(&corners, &triangles));
    }
    Ok(out)
}

const LETTERS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The bytes of a `data:` address in base 64.
fn inline(uri: &str) -> Read<Vec<u8>> {
    let Some((_, data)) = uri.strip_prefix("data:").and_then(|rest| rest.split_once(";base64,")) else {
        return Err(format!("a buffer at {uri}: sound reads buffers given inline"));
    };
    let mut out = Vec::with_capacity(data.len() * 3 / 4);
    let (mut bits, mut count) = (0u32, 0);
    for c in data.bytes().filter(|&c| c != b'=') {
        let v = LETTERS.iter().position(|&l| l == c).ok_or("not base 64")? as u32;
        bits = bits << 6 | v;
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
        }
    }
    Ok(out)
}

fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { LETTERS[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

/// The JSON of a scene whose faces are hand-written quads
/// (`osageSound.surfaces[]`), as an object of the format: each quad a mesh
/// of two triangles in one inline buffer, named by the node's `acoustic[]`
/// (a quad with both materials names its collision mesh as its acoustic
/// one, `collision-proxy`) and `collision[]`, its flags in `surfaceSound`.
/// An identifier that two nodes give a surface is made the object's own by
/// the node's: `road-deck/underside`.
pub fn with_meshes(text: &str) -> Read<String> {
    let mut json: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
    let mut bytes: Vec<u8> = Vec::new();
    let mut meshes = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    let nodes = json.get_mut("nodes").and_then(Value::as_array_mut).ok_or("no nodes")?;
    // Identifiers given more than once, acoustic and collision apart.
    let mut seen: [std::collections::BTreeMap<String, usize>; 2] = Default::default();
    for node in nodes.iter() {
        for s in node.pointer("/extras/osageSound/surfaces").and_then(Value::as_array).into_iter().flatten() {
            let id = s.get("id").and_then(Value::as_str).unwrap_or_default().to_owned();
            for (k, key) in ["acoustic", "collision"].iter().enumerate() {
                if s.get(*key).is_some() {
                    *seen[k].entry(id.clone()).or_default() += 1;
                }
            }
        }
    }
    for node in nodes.iter_mut() {
        let node_id = node.pointer("/extras/osage/id").and_then(Value::as_str).unwrap_or_default().to_owned();
        let Some(sound) = node.pointer_mut("/extras/osageSound").and_then(Value::as_object_mut) else { continue };
        let Some(Value::Array(surfaces)) = sound.remove("surfaces") else { continue };
        let (mut acoustic, mut collision, mut flags) = (Vec::new(), Vec::new(), serde_json::Map::new());
        for s in &surfaces {
            let id = s.get("id").and_then(Value::as_str).ok_or("a face with no id")?;
            let quad = s.get("quadM").and_then(Value::as_array).ok_or("a face with no quadM")?;
            let mesh = meshes.len();
            // Four corners, two triangles; positions as 32-bit floats.
            let start = bytes.len();
            let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
            for corner in quad {
                for (axis, value) in corner.as_array().ok_or("a corner is three numbers")?.iter().enumerate() {
                    let v = value.as_f64().ok_or("a corner is three numbers")?;
                    lo[axis] = lo[axis].min(v);
                    hi[axis] = hi[axis].max(v);
                    bytes.extend_from_slice(&(v as f32).to_le_bytes());
                }
            }
            let at_indices = bytes.len();
            for i in [0u16, 1, 2, 0, 2, 3] {
                bytes.extend_from_slice(&i.to_le_bytes());
            }
            while !bytes.len().is_multiple_of(4) {
                bytes.push(0);
            }
            let lo: Vec<f32> = lo.iter().map(|&v| v as f32).collect();
            let hi: Vec<f32> = hi.iter().map(|&v| v as f32).collect();
            views.push(json!({"buffer": 0, "byteOffset": start, "byteLength": 48}));
            accessors.push(json!({"bufferView": views.len() - 1, "componentType": 5126, "count": 4, "type": "VEC3", "min": lo, "max": hi}));
            views.push(json!({"buffer": 0, "byteOffset": at_indices, "byteLength": 12}));
            accessors.push(json!({"bufferView": views.len() - 1, "componentType": 5123, "count": 6, "type": "SCALAR"}));
            meshes.push(json!({"primitives": [{"attributes": {"POSITION": accessors.len() - 2}, "indices": accessors.len() - 1}]}));
            let own = |k: usize| {
                if seen[k].get(id).copied().unwrap_or(0) > 1 { format!("{node_id}/{id}") } else { id.to_owned() }
            };
            let has_collision = s.get("collision").is_some();
            if let Some(material) = s.get("collision") {
                collision.push(json!({"id": own(1), "mesh": mesh, "material": material}));
            }
            if let Some(material) = s.get("acoustic") {
                let source = if has_collision { "collision-proxy" } else { "authored" };
                acoustic.push(json!({"id": own(0), "mesh": mesh, "material": material, "source": source}));
            }
            let mut f = serde_json::Map::new();
            for key in ["reflects", "occludes", "strikes"] {
                if let Some(v) = s.get(key) {
                    f.insert(key.to_owned(), v.clone());
                }
            }
            if !f.is_empty() {
                for k in 0..2 {
                    flags.insert(own(k), Value::Object(f.clone()));
                }
            }
        }
        if !flags.is_empty() {
            sound.insert("surfaceSound".to_owned(), Value::Object(flags));
        }
        let osage = node.pointer_mut("/extras/osage").and_then(Value::as_object_mut).ok_or("a node with no id")?;
        if !acoustic.is_empty() {
            osage.insert("acoustic".to_owned(), Value::Array(acoustic));
        }
        if !collision.is_empty() {
            osage.insert("collision".to_owned(), Value::Array(collision));
        }
    }
    let root = json.as_object_mut().ok_or("not an object")?;
    root.insert("meshes".to_owned(), Value::Array(meshes));
    root.insert("accessors".to_owned(), Value::Array(accessors));
    root.insert("bufferViews".to_owned(), Value::Array(views));
    root.insert(
        "buffers".to_owned(),
        json!([{"byteLength": bytes.len(), "uri": format!("data:application/octet-stream;base64,{}", base64(&bytes))}]),
    );
    serde_json::to_string_pretty(&json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_64_goes_there_and_back() {
        for bytes in [&b""[..], b"a", b"ab", b"abc", b"abcd", &[0, 255, 7, 128, 1]] {
            assert_eq!(inline(&format!("data:x;base64,{}", base64(bytes))).unwrap(), bytes);
        }
        assert_eq!(base64(b"Man"), "TWFu");
    }
}
