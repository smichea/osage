// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! A scene to hear: an object tree whose nodes carry what the object format
//! (`design/08-object-format.md`) gives them for the eye and for contact,
//! and, beside it, what sound needs:
//!
//! - **emitters:** what a node sounds like by itself — a voice, placed by a
//!   position local to the node;
//! - **surfaces:** what sound meets — flat faces with an acoustic material
//!   (how much of each band a face takes, lets through, scatters), which
//!   reflect and hide; and a collision material, which says what a foot or a
//!   wheel finds under it;
//! - **touches:** what a node touches as it goes — feet, wheels — so that a
//!   contact sounds as the two materials meeting do;
//! - a **listener**: the node that hears.
//!
//! The file is the JSON of a glTF 2.0 asset without meshes. Its
//! `asset.extras.osageObject` header and each `node.extras.osage.id` are the
//! object format's, so the engine's reader loads the same file as a tree;
//! what is new is kept in sibling `osageSound` keys, which that reader steps
//! over. Flat four-cornered faces stand for the meshes of a real object.

use std::collections::BTreeMap;

use osage_sound_voices::Params;
use serde_json::Value;

use crate::{
    space::{Placement, Quat, Vec3, vec3},
    tree::{Motion, Node, Tree, Turning},
};

/// The bands a material answers in, Hz: the object format's.
pub const BANDS: [f64; 3] = [250.0, 1_000.0, 4_000.0];

/// How a surface answers sound, as fractions of the energy that meets it,
/// in each of [`BANDS`]: the object format's acoustic material.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    pub absorption: [f64; 3],
    pub transmission: [f64; 3],
    /// Of what is sent back, the share that is scattered rather than
    /// mirrored.
    pub scattering: f64,
}

impl Material {
    /// The share of the pressure mirrored, in each band.
    pub fn mirrored(&self) -> [f64; 3] {
        std::array::from_fn(|b| {
            ((1.0 - self.absorption[b] - self.transmission[b]).max(0.0) * (1.0 - self.scattering)).sqrt()
        })
    }

    /// The share of the energy scattered, in each band.
    pub fn scattered(&self) -> [f64; 3] {
        std::array::from_fn(|b| (1.0 - self.absorption[b] - self.transmission[b]).max(0.0) * self.scattering)
    }

    /// The share of the pressure let through, in each band.
    pub fn passed(&self) -> [f64; 3] {
        std::array::from_fn(|b| self.transmission[b].max(0.0).sqrt())
    }
}

/// How a collision material sounds when touched: the voice that makes it
/// and what it is given.
#[derive(Clone, Debug, PartialEq)]
pub struct ContactSound {
    pub voice: String,
    pub params: Params,
    /// How hard it is, 0…1: of two things touching, the softer sets how
    /// long the push lasts.
    pub hardness: f64,
    /// How loud a contact on it is, as a gain.
    pub gain: f64,
}

/// A flat face of a node, four corners in the node's frame, in order round
/// it.
#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    pub id: String,
    pub node: usize,
    pub quad: [Vec3; 4],
    /// What it does to sound, when it does anything.
    pub acoustic: Option<Material>,
    pub reflects: bool,
    pub occludes: bool,
    /// What a foot or a wheel finds: a key of the collision materials.
    pub collision: Option<String>,
    /// Whether coming onto it is a blow (a plate, a joint) and not only a
    /// change of ground.
    pub strikes: bool,
}

/// Where an emitter is heard from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reach {
    /// From where it is, through the air.
    Positional,
    /// From nowhere: straight into the ears, `pan` from −1 (left) to 1.
    Global { pan: f64 },
}

/// What loads an emitter's voice (`Drive::load`).
#[derive(Clone, Debug, PartialEq)]
pub enum Load {
    Fixed(f64),
    /// By the emitters of the kind `voice` under the node `under` that are
    /// within `radius` metres: each counts for how near it is, `full` of
    /// them at the very place load it fully.
    Near {
        under: usize,
        voice: String,
        radius: f64,
        full: f64,
    },
}

/// What a node sounds like by itself.
#[derive(Clone, Debug, PartialEq)]
pub struct Emitter {
    pub id: String,
    pub node: usize,
    /// Where it is in its node's frame, m.
    pub offset: Vec3,
    pub voice: String,
    pub params: Params,
    /// Its level one metre away, as a gain on the voice's own.
    pub gain: f64,
    pub reach: Reach,
    pub load: Load,
    /// Whether surfaces send it back (not for what lies on one).
    pub reflections: bool,
    /// Beyond this it is not heard, m.
    pub max_distance: f64,
}

/// What a node touches as it goes.
#[derive(Clone, Debug, PartialEq)]
pub enum Touching {
    /// Feet coming down in turn, one every `stride` metres gone.
    Steps { stride: f64, feet: Vec<Vec3> },
    /// Points that stay on the ground: wheels. A blow when one comes onto
    /// a surface that strikes.
    Wheels { points: Vec<Vec3> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Touch {
    pub node: usize,
    pub touching: Touching,
    /// The collision material of what touches.
    pub material: String,
    /// How strong its contacts are, 0…1.
    pub strength: f64,
}

/// The space's own answer after the first reflections.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reverb {
    /// Seconds to fall by 60 dB, in each of [`BANDS`].
    pub seconds: [f64; 3],
    /// Whether they are found from the faces round the listener instead
    /// (`"secondsT60": "fromFaces"`, [`crate::decay`]); `seconds` are then
    /// what the space rings for until the first are found.
    pub from_faces: bool,
    /// Its level, as a gain on what the surfaces scatter.
    pub gain: f64,
}

/// The air sound crosses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Air {
    pub celsius: f64,
    /// Relative humidity, %.
    pub humidity: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub id: String,
    pub tree: Tree,
    pub surfaces: Vec<Surface>,
    pub emitters: Vec<Emitter>,
    pub touches: Vec<Touch>,
    pub contacts: BTreeMap<String, ContactSound>,
    /// The node that hears, and where its head is in it.
    pub listener: usize,
    pub ears: Vec3,
    pub seconds: f64,
    pub seed: u64,
    /// The loudness a render is brought to, LUFS.
    pub loudness: f64,
    pub reverb: Reverb,
    pub air: Air,
}

/// A value of the file and where it is, to say where a mistake is.
#[derive(Clone)]
struct At<'a> {
    value: &'a Value,
    place: String,
}

type Read<T> = Result<T, String>;

impl<'a> At<'a> {
    fn get(&self, key: &str) -> Option<At<'a>> {
        self.value.get(key).map(|value| At { value, place: format!("{}.{key}", self.place) })
    }

    fn need(&self, key: &str) -> Read<At<'a>> {
        self.get(key).ok_or_else(|| format!("{}: no \"{key}\"", self.place))
    }

    fn number(&self) -> Read<f64> {
        self.value.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("{}: not a number", self.place))
    }

    fn text(&self) -> Read<&'a str> {
        self.value.as_str().ok_or_else(|| format!("{}: not a text", self.place))
    }

    fn flag(&self) -> Read<bool> {
        self.value.as_bool().ok_or_else(|| format!("{}: not true or false", self.place))
    }

    fn list(&self) -> Read<Vec<At<'a>>> {
        let items = self.value.as_array().ok_or_else(|| format!("{}: not a list", self.place))?;
        Ok(items.iter().enumerate().map(|(i, value)| At { value, place: format!("{}[{i}]", self.place) }).collect())
    }

    fn entries(&self) -> Read<Vec<(&'a str, At<'a>)>> {
        let map = self.value.as_object().ok_or_else(|| format!("{}: not an object", self.place))?;
        Ok(map
            .iter()
            .map(|(key, value)| (key.as_str(), At { value, place: format!("{}.{key}", self.place) }))
            .collect())
    }

    fn numbers<const N: usize>(&self) -> Read<[f64; N]> {
        let items = self.list()?;
        if items.len() != N {
            return Err(format!("{}: {} numbers where {N} are wanted", self.place, items.len()));
        }
        let mut out = [0.0; N];
        for (slot, item) in out.iter_mut().zip(&items) {
            *slot = item.number()?;
        }
        Ok(out)
    }

    fn point(&self) -> Read<Vec3> {
        self.numbers::<3>().map(Vec3::from)
    }

    fn number_or(&self, key: &str, default: f64) -> Read<f64> {
        self.get(key).map_or(Ok(default), |at| at.number())
    }

    fn flag_or(&self, key: &str, default: bool) -> Read<bool> {
        self.get(key).map_or(Ok(default), |at| at.flag())
    }

    fn fail<T>(&self, what: impl std::fmt::Display) -> Read<T> {
        Err(format!("{}: {what}", self.place))
    }
}

fn gain(decibels: f64) -> f64 {
    10f64.powf(decibels / 20.0)
}

fn params(at: Option<At>) -> Read<Params> {
    let mut params = Params::default();
    if let Some(at) = at {
        for (name, value) in at.entries()? {
            params.set(name, value.number()? as f32);
        }
    }
    Ok(params)
}

/// A list of identifiers from a root, found in the tree.
fn node_at(tree: &Tree, at: &At) -> Read<usize> {
    let ids: Vec<&str> = at.list()?.iter().map(At::text).collect::<Read<_>>()?;
    tree.find(&ids).ok_or_else(|| format!("{}: no node at {}", at.place, ids.join(" / ")))
}

/// An acoustic material; `None` for one the object format leaves unknown
/// (`{}`), which sound does not meet.
fn material(at: &At) -> Read<Option<Material>> {
    if at.entries()?.is_empty() {
        return Ok(None);
    }
    let absorption: [f64; 3] = at.need("absorption")?.numbers()?;
    let transmission: [f64; 3] = at.need("transmission")?.numbers()?;
    let scattering = at.need("scattering")?.number()?;
    for b in 0..3 {
        // The object format's rule.
        if !(0.0..=1.0).contains(&absorption[b]) || !(0.0..=1.0).contains(&transmission[b]) {
            return at.fail("absorption and transmission are fractions, 0 to 1");
        }
        if absorption[b] + transmission[b] > 1.0 + 1e-9 {
            return at.fail(format!("absorption and transmission pass 1 at {} Hz", BANDS[b]));
        }
    }
    if !(0.0..=1.0).contains(&scattering) {
        return at.fail("scattering is a fraction, 0 to 1");
    }
    Ok(Some(Material { absorption, transmission, scattering }))
}

impl Scene {
    /// A scene from the JSON of a glTF asset; a mistake is told with where
    /// it is.
    pub fn from_json(text: &str) -> Read<Scene> {
        let value: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
        let root = At { value: &value, place: "$".to_owned() };
        let asset = root.need("asset")?.need("extras")?;
        let header = asset.need("osageObject")?;
        if header.need("version")?.number()? != 1.0 {
            return header.fail("an object of another version than 1");
        }
        let id = header.need("id")?.text()?.to_owned();
        let collision: Vec<&str> = header.need("collisionMaterials")?.entries()?.iter().map(|(key, _)| *key).collect();
        let mut acoustic = BTreeMap::new();
        for (key, at) in header.need("acousticMaterials")?.entries()? {
            acoustic.insert(key, material(&at)?);
        }

        // The nodes, as glTF lists them; a parent is whoever lists a child.
        let listed = root.need("nodes")?.list()?;
        let mut parents: Vec<Option<usize>> = vec![None; listed.len()];
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); listed.len()];
        for (i, at) in listed.iter().enumerate() {
            let Some(under) = at.get("children") else { continue };
            for child in under.list()? {
                let c = child.number()? as usize;
                if c >= listed.len() || c == i {
                    return child.fail("no such node");
                }
                if parents[c].is_some() {
                    return child.fail("a node with two parents: the nodes form a forest");
                }
                parents[c] = Some(i);
                children[i].push(c);
            }
        }
        // No node is its own ancestor.
        for start in 0..listed.len() {
            let (mut at, mut steps) = (parents[start], 0);
            while let Some(i) = at {
                steps += 1;
                if steps > listed.len() {
                    return listed[start].fail("a node under itself");
                }
                at = parents[i];
            }
        }

        // The scene heard, and what is under its roots: a file may hold other
        // scenes, and nothing of theirs is in this one — not a node, not a
        // sound.
        let scenes = root.need("scenes")?.list()?;
        let chosen = root.number_or("scene", 0.0)? as usize;
        let Some(scene) = scenes.get(chosen) else { return root.fail("no such scene") };
        let mut within = vec![false; listed.len()];
        let mut ahead = Vec::new();
        for at in scene.need("nodes")?.list()? {
            let top = at.number()? as usize;
            if top >= listed.len() {
                return at.fail("no such node");
            }
            if parents[top].is_some() {
                return at.fail("a scene lists roots: this node has a parent");
            }
            ahead.push(top);
        }
        while let Some(node) = ahead.pop() {
            within[node] = true;
            ahead.extend(&children[node]);
        }
        // The scene's nodes in the order of the file, and each one's number among them.
        let kept: Vec<usize> = (0..listed.len()).filter(|&i| within[i]).collect();
        let mut number = vec![usize::MAX; listed.len()];
        for (i, &from) in kept.iter().enumerate() {
            number[from] = i;
        }
        let mut nodes = Vec::with_capacity(kept.len());
        for &from in &kept {
            let at = &listed[from];
            let id = at.need("extras")?.need("osage")?.need("id")?.text()?.to_owned();
            let translation = at.get("translation").map_or(Ok(Vec3::ZERO), |t| t.point())?;
            let rotation = match at.get("rotation") {
                Some(r) => {
                    Quat::from_xyzw(r.numbers()?).ok_or_else(|| format!("{}: four zeros turn nothing", r.place))?
                }
                None => Quat::IDENTITY,
            };
            if at.get("scale").is_some() || at.get("matrix").is_some() {
                return at.fail("a scale or a matrix: this reader places nodes by translation and rotation only");
            }
            nodes.push(Node {
                id,
                parent: parents[from].map(|parent| number[parent]),
                children: children[from].iter().map(|&child| number[child]).collect(),
                rest: Placement { translation, rotation },
                motion: None,
                turning: None,
                proxy_beyond: None,
            });
        }
        let mut tree = Tree { nodes };
        // The object format's rule: a node's identifier is its own in the whole object.
        for (i, node) in tree.nodes.iter().enumerate() {
            if node.id.trim().is_empty() || tree.nodes[..i].iter().any(|other| other.id == node.id) {
                return listed[kept[i]].fail(format!("a second or an empty node identifier \"{}\"", node.id));
            }
        }

        let sound = asset.need("osageSound")?;
        let mut contacts = BTreeMap::new();
        for (key, at) in sound.need("contactSounds")?.entries()? {
            if !collision.contains(&key) {
                return at.fail("a contact sound for no collision material");
            }
            let voice = at.get("voice").map_or(Ok("impact"), |v| v.text())?.to_owned();
            let mut given = params(at.get("params"))?;
            let hardness = at.number_or("hardness", 0.5)?;
            given.set("hardness", hardness as f32);
            if osage_sound_voices::kind(&voice).is_none() {
                return at.fail(format!("no voice named \"{voice}\""));
            }
            let level = gain(at.number_or("gainDb", 0.0)?);
            contacts.insert(key.to_owned(), ContactSound { voice, params: given, hardness, gain: level });
        }

        let (mut surfaces, mut emitters, mut touches) = (Vec::new(), Vec::new(), Vec::new());
        // Read once the tree is whole, since they name nodes by their path.
        let mut loads: Vec<(usize, At)> = Vec::new();
        for (node, &from) in kept.iter().enumerate() {
            let at = &listed[from];
            let Some(extra) = at.need("extras")?.get("osageSound") else { continue };
            if let Some(motion) = extra.get("motion") {
                let along = motion.need("alongM")?.point()?;
                if along.length() < 1e-9 {
                    return motion.fail("a way of no length");
                }
                let speed = motion.need("speedKmh")?;
                let knots = if speed.value.is_number() {
                    vec![(0.0, speed.number()? / 3.6)]
                } else {
                    let mut knots = Vec::new();
                    for knot in speed.list()? {
                        let [t, v] = knot.numbers::<2>()?;
                        if knots.last().is_some_and(|last: &(f64, f64)| last.0 >= t) {
                            return knot.fail("instants are given soonest first");
                        }
                        knots.push((t, v / 3.6));
                    }
                    if knots.is_empty() {
                        return speed.fail("no speed");
                    }
                    knots
                };
                tree.nodes[node].motion =
                    Some(Motion { along: along.unit_or(Vec3::X), at: motion.number_or("atS", 0.0)?, speed: knots });
            }
            if let Some(turning) = extra.get("turning") {
                let axis = turning.need("axisM")?.point()?;
                if axis.length() < 1e-9 {
                    return turning.fail("an axis of no length");
                }
                tree.nodes[node].turning = Some(Turning {
                    axis: axis.unit_or(Vec3::Y),
                    at: turning.number_or("atS", 0.0)?,
                    rate: turning.need("degreesPerSecond")?.number()?.to_radians(),
                });
            }
            if let Some(beyond) = extra.get("proxyBeyondM") {
                tree.nodes[node].proxy_beyond = Some(beyond.number()?);
            }
            for s in extra.get("surfaces").map_or(Ok(Vec::new()), |s| s.list())? {
                let corners = s.need("quadM")?.list()?;
                if corners.len() != 4 {
                    return s.fail("a face has four corners");
                }
                let quad = [corners[0].point()?, corners[1].point()?, corners[2].point()?, corners[3].point()?];
                let normal = (quad[1] - quad[0]).cross(quad[3] - quad[0]);
                if normal.length() < 1e-9 {
                    return s.fail("a face of no area");
                }
                if (quad[2] - quad[0]).dot(normal.unit_or(Vec3::Y)).abs() > 1e-6 {
                    return s.fail("a face that is not flat");
                }
                let material = match s.get("acoustic") {
                    Some(key) => {
                        *acoustic.get(key.text()?).ok_or_else(|| format!("{}: no such acoustic material", key.place))?
                    }
                    None => None,
                };
                let collision_key = match s.get("collision") {
                    Some(key) if collision.contains(&key.text()?) => Some(key.text()?.to_owned()),
                    Some(key) => return key.fail("no such collision material"),
                    None => None,
                };
                surfaces.push(Surface {
                    id: s.need("id")?.text()?.to_owned(),
                    node,
                    quad,
                    acoustic: material,
                    reflects: s.flag_or("reflects", material.is_some())? && material.is_some(),
                    occludes: s.flag_or("occludes", material.is_some())? && material.is_some(),
                    collision: collision_key,
                    strikes: s.flag_or("strikes", false)?,
                });
            }
            for e in extra.get("emitters").map_or(Ok(Vec::new()), |e| e.list())? {
                let voice = e.need("voice")?.text()?.to_owned();
                let Some(kind) = osage_sound_voices::kind(&voice) else {
                    return e.fail(format!("no voice named \"{voice}\""));
                };
                let given = params(e.get("params"))?;
                if let Some(unknown) = given.names().find(|name| !kind.params.iter().any(|(known, _)| known == name)) {
                    return e.fail(format!("the voice \"{voice}\" has no parameter \"{unknown}\""));
                }
                let reach = match e.get("type").map_or(Ok("positional"), |t| t.text())? {
                    "positional" => Reach::Positional,
                    "global" => Reach::Global { pan: e.number_or("pan", 0.0)?.clamp(-1.0, 1.0) },
                    other => return e.fail(format!("an emitter is positional or global, not \"{other}\"")),
                };
                if let Some(load) = e.get("loadedBy") {
                    loads.push((emitters.len(), load));
                }
                emitters.push(Emitter {
                    id: e.need("id")?.text()?.to_owned(),
                    node,
                    offset: e.get("positionM").map_or(Ok(Vec3::ZERO), |p| p.point())?,
                    voice,
                    params: given,
                    gain: gain(e.number_or("gainDb", 0.0)?),
                    reach,
                    load: Load::Fixed(e.number_or("load", 0.5)?),
                    reflections: e.flag_or("reflections", true)?,
                    max_distance: e.number_or("maxDistanceM", 600.0)?.clamp(1.0, 900.0),
                });
            }
            for t in extra.get("touches").map_or(Ok(Vec::new()), |t| t.list())? {
                let material = t.need("material")?.text()?.to_owned();
                if !contacts.contains_key(&material) {
                    return t.fail(format!("no contact sound for \"{material}\""));
                }
                let points = |key: &str| -> Read<Vec<Vec3>> { t.need(key)?.list()?.iter().map(At::point).collect() };
                let touching = match t.need("kind")?.text()? {
                    "steps" => {
                        let stride = t.need("strideM")?.number()?;
                        if stride <= 0.05 {
                            return t.fail("a stride too short");
                        }
                        Touching::Steps { stride, feet: points("feetM")? }
                    }
                    "wheels" => Touching::Wheels { points: points("pointsM")? },
                    other => return t.fail(format!("feet step and wheels roll; \"{other}\" does neither")),
                };
                touches.push(Touch {
                    node,
                    touching,
                    material,
                    strength: t.number_or("strength", 0.7)?.clamp(0.0, 1.0),
                });
            }
        }
        for (emitter, load) in loads {
            emitters[emitter].load = Load::Near {
                under: node_at(&tree, &load.need("under")?)?,
                voice: load.need("voice")?.text()?.to_owned(),
                radius: load.need("radiusM")?.number()?.max(0.1),
                full: load.number_or("full", 1.0)?.max(0.01),
            };
        }
        for (i, emitter) in emitters.iter().enumerate() {
            if emitters[..i].iter().any(|other| other.node == emitter.node && other.id == emitter.id) {
                return Err(format!("two emitters \"{}\" on {}", emitter.id, tree.path_text(emitter.node)));
            }
        }
        for surface in &surfaces {
            if let Some(key) = &surface.collision
                && !contacts.contains_key(key)
            {
                return Err(format!("surface \"{}\": no contact sound for \"{key}\"", surface.id));
            }
        }

        let about = scene.need("extras")?.need("osageSound")?;
        let hears = about.need("listener")?;
        let reverb = about.need("reverb")?;
        let air = about.need("air")?;
        let seconds = about.need("seconds")?.number()?;
        if !(0.1..=3_600.0).contains(&seconds) {
            return about.fail("between a tenth of a second and an hour");
        }
        let given = reverb.need("secondsT60")?;
        let from_faces = given.value.as_str() == Some("fromFaces");
        let t60: [f64; 3] = if from_faces { [0.5; 3] } else { given.numbers()? };
        if t60.iter().any(|s| !(0.05..=10.0).contains(s)) {
            return reverb.fail("a decay between 0.05 and 10 s, or \"fromFaces\"");
        }
        Ok(Scene {
            id,
            listener: node_at(&tree, &hears.need("node")?)?,
            ears: hears.get("earsM").map_or(Ok(Vec3::ZERO), |p| p.point())?,
            seconds,
            seed: about.number_or("seed", 1.0)? as u64,
            loudness: about.number_or("loudnessLufs", -20.0)?,
            reverb: Reverb { seconds: t60, from_faces, gain: gain(reverb.number_or("gainDb", 0.0)?) },
            air: Air { celsius: air.number_or("celsius", 15.0)?, humidity: air.number_or("humidityPercent", 70.0)? },
            tree,
            surfaces,
            emitters,
            touches,
            contacts,
        })
    }

    /// The emitter `id` of the node at `path`.
    pub fn emitter(&self, path: &[&str], id: &str) -> Option<usize> {
        let node = self.tree.find(path)?;
        self.emitters.iter().position(|e| e.node == node && e.id == id)
    }

    /// The collision surface `id` of the node at `path`: its node, and its
    /// collision material — what a contact on it a solver reports is struck
    /// as ([`crate::engine::Struck`]).
    pub fn collision_surface(&self, path: &[&str], id: &str) -> Option<(usize, String)> {
        let node = self.tree.find(path)?;
        self.surfaces.iter().find(|s| s.node == node && s.id == id).and_then(|s| Some((node, s.collision.clone()?)))
    }

    /// An emitter's name: its node's path and its own identifier.
    pub fn emitter_name(&self, emitter: usize) -> String {
        format!("{} : {}", self.tree.path_text(self.emitters[emitter].node), self.emitters[emitter].id)
    }

    /// The four corners of a surface in the world at the instant `t`.
    pub fn corners(&self, surface: usize, t: f64) -> [Vec3; 4] {
        let placement = self.tree.placement(self.surfaces[surface].node, t);
        self.surfaces[surface].quad.map(|corner| placement.point(corner))
    }

    /// Where the listener's head is in the world, and which way it looks,
    /// where its right ear is and what is up for it, at the instant `t`:
    /// a node looks along its own −Z with +Y up, as a glTF camera does.
    pub fn head(&self, t: f64) -> Head {
        let placement = self.tree.placement(self.listener, t);
        Head {
            at: placement.point(self.ears),
            forward: placement.direction(vec3(0.0, 0.0, -1.0)),
            right: placement.direction(Vec3::X),
            up: placement.direction(Vec3::Y),
        }
    }
}

/// A listener's head in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Head {
    pub at: Vec3,
    pub forward: Vec3,
    pub right: Vec3,
    pub up: Vec3,
}

impl Head {
    /// The head turned by `angle` radians about its own vertical, to its
    /// left for a positive angle.
    pub fn turned(self, angle: f64) -> Head {
        let turn = Quat::about(self.up, angle);
        Head { forward: turn.turn(self.forward), right: turn.turn(self.right), ..self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scene with one of everything.
    pub(crate) const SMALL: &str = r#"{
      "asset": {"version": "2.0", "extras": {
        "osageObject": {"version": 1, "id": "small",
          "collisionMaterials": {"ground": {"friction": 0.8, "restitution": 0.0}, "sole": {"friction": 0.9, "restitution": 0.0}},
          "acousticMaterials": {"hard": {"absorption": [0.02, 0.03, 0.04], "transmission": [0.0, 0.0, 0.0], "scattering": 0.25}}},
        "osageSound": {"version": 0, "contactSounds": {
          "ground": {"params": {"tone": 120, "ring": 0.05}, "hardness": 0.6},
          "sole": {"hardness": 0.3}}}}},
      "scene": 0,
      "scenes": [{"nodes": [0], "extras": {"osageSound": {
        "seconds": 2, "seed": 3, "loudnessLufs": -20,
        "listener": {"node": ["place", "walker", "head"], "earsM": [0, 0.1, 0]},
        "reverb": {"secondsT60": [0.5, 0.4, 0.3], "gainDb": -6},
        "air": {"celsius": 20, "humidityPercent": 50}}}}],
      "nodes": [
        {"children": [1, 3], "extras": {"osage": {"id": "place"}, "osageSound": {
          "surfaces": [{"id": "floor", "quadM": [[-50, 0, -50], [-50, 0, 50], [50, 0, 50], [50, 0, -50]],
                        "acoustic": "hard", "collision": "ground"}]}}},
        {"children": [2], "translation": [0, 0, 5], "rotation": [0, -0.7071067811865476, 0, 0.7071067811865476],
         "extras": {"osage": {"id": "walker"}, "osageSound": {
           "motion": {"alongM": [1, 0, 0], "atS": 0, "speedKmh": 3.6},
           "touches": [{"kind": "steps", "material": "sole", "strideM": 0.7, "feetM": [[0, 0, -0.1], [0, 0, 0.1]]}]}}},
        {"translation": [0, 1.6, 0], "extras": {"osage": {"id": "head"}}},
        {"translation": [0, 1, -5], "extras": {"osage": {"id": "spring"}, "osageSound": {
          "proxyBeyondM": 40,
          "emitters": [{"id": "water", "voice": "water", "positionM": [0, 0.2, 0], "gainDb": 6, "load": 0.5},
                       {"id": "bed", "type": "global", "voice": "city-bed", "pan": -1, "gainDb": -20}]}}}
      ]}"#;

    #[test]
    fn a_scene_reads_into_a_tree_with_its_sounds() {
        let scene = Scene::from_json(SMALL).unwrap();
        assert_eq!((scene.id.as_str(), scene.seconds, scene.seed), ("small", 2.0, 3));
        assert_eq!(scene.tree.path(scene.listener), ["place", "walker", "head"]);
        let water = scene.emitter(&["place", "spring"], "water").unwrap();
        assert_eq!(scene.emitter_name(water), "place / spring : water");
        assert_eq!(scene.emitters[water].offset, vec3(0.0, 0.2, 0.0));
        assert!((scene.emitters[water].gain - 1.995).abs() < 0.01);
        assert_eq!(scene.emitters[1].reach, Reach::Global { pan: -1.0 });
        assert_eq!(scene.tree.nodes[3].proxy_beyond, Some(40.0));
        assert_eq!(scene.surfaces[0].collision.as_deref(), Some("ground"));
        assert!(scene.surfaces[0].reflects && scene.surfaces[0].occludes && !scene.surfaces[0].strikes);
        assert_eq!(scene.contacts["ground"].voice, "impact");
        assert_eq!(scene.contacts["ground"].params.get("tone", 0.0), 120.0);
        assert_eq!(scene.touches.len(), 1);
        assert_eq!(scene.air, Air { celsius: 20.0, humidity: 50.0 });
        assert!(!scene.reverb.from_faces);
        // Or how long the space rings found from its faces.
        let found = Scene::from_json(&SMALL.replace("[0.5, 0.4, 0.3]", r#""fromFaces""#)).unwrap();
        assert!(found.reverb.from_faces);
    }

    #[test]
    fn another_scene_of_the_file_is_not_in_this_one() {
        // A second scene whose root sounds, mirrors and steps, listed before the first one's nodes: every
        // number of the file moves by one, and nothing of the scene read does.
        let other = r#"{"extras": {"osage": {"id": "elsewhere"}, "osageSound": {
          "motion": {"alongM": [1, 0, 0], "atS": 0, "speedKmh": 3.6},
          "surfaces": [{"id": "wall", "quadM": [[-9, 0, 1], [-9, 9, 1], [9, 9, 1], [9, 0, 1]], "acoustic": "hard"}],
          "emitters": [{"id": "tone", "voice": "tone", "params": {"hz": 440}}],
          "touches": [{"kind": "steps", "material": "sole", "strideM": 0.7, "feetM": [[0, 0, 0]]}]}}},
        "#;
        let both = SMALL
            .replace(r#""scenes": [{"nodes": [0], "#, r#""scenes": [{"nodes": [1], "#)
            .replace(r#""humidityPercent": 50}}}}],"#, r#""humidityPercent": 50}}}}, {"nodes": [0]}],"#)
            .replace(r#""children": [1, 3]"#, r#""children": [2, 4]"#)
            .replace(r#""children": [2]"#, r#""children": [3]"#)
            .replace("\"nodes\": [\n", &format!("\"nodes\": [\n{other}"));
        assert!(both.contains("elsewhere") && both.contains(r#"{"nodes": [0]}"#) && both.contains("[2, 4]"));
        let (alone, among) = (Scene::from_json(SMALL).unwrap(), Scene::from_json(&both).unwrap());
        assert_eq!(among, alone);
        assert_eq!(
            (among.tree.nodes.len(), among.emitters.len(), among.surfaces.len(), among.touches.len()),
            (4, 2, 1, 1)
        );
        // Asked for, the other scene is read, and says what it lacks.
        let second = both.replace(r#""scene": 0"#, r#""scene": 1"#);
        assert!(Scene::from_json(&second).unwrap_err().contains("$.scenes[1]"));
        // A scene lists roots.
        let under = SMALL.replace(r#""scenes": [{"nodes": [0], "#, r#""scenes": [{"nodes": [1], "#);
        assert!(Scene::from_json(&under).unwrap_err().contains("has a parent"));
    }

    #[test]
    fn the_listener_walks_and_looks_the_way_its_node_is_turned() {
        let scene = Scene::from_json(SMALL).unwrap();
        // A metre a second along X; the node's −Z turned onto X.
        let head = scene.head(3.0);
        assert!(head.at.distance(vec3(3.0, 1.7, 5.0)) < 1e-9, "{:?}", head.at);
        assert!(head.forward.distance(Vec3::X) < 1e-9, "{:?}", head.forward);
        // Looking along X with Y up, the right hand is towards +Z.
        assert!(head.right.distance(Vec3::Z) < 1e-9, "{:?}", head.right);
        assert!(head.forward.cross(head.up).distance(head.right) < 1e-9);
        // Half a turn: what was on the right is on the left.
        let back = head.turned(std::f64::consts::PI);
        assert!(back.right.distance(-Vec3::Z) < 1e-9 && back.forward.distance(-Vec3::X) < 1e-9);
        assert_eq!(back.at, head.at);
    }

    #[test]
    fn a_material_shares_what_meets_it() {
        let m = Material { absorption: [0.1, 0.2, 0.3], transmission: [0.1, 0.0, 0.0], scattering: 0.25 };
        for b in 0..3 {
            let back = 1.0 - m.absorption[b] - m.transmission[b];
            assert!((m.mirrored()[b].powi(2) + m.scattered()[b] - back).abs() < 1e-12);
            assert!((m.passed()[b].powi(2) - m.transmission[b]).abs() < 1e-12);
        }
    }

    #[test]
    fn mistakes_are_told_with_their_place() {
        let broken = |from: &str, to: &str| {
            assert!(SMALL.contains(from), "{from}");
            Scene::from_json(&SMALL.replace(from, to)).unwrap_err()
        };
        assert!(broken(r#""voice": "water""#, r#""voice": "wind""#).contains("emitters[0]"));
        assert!(broken(r#""load": 0.5"#, r#""params": {"depth": 1}"#).contains("no parameter \"depth\""));
        assert!(broken(r#"["place", "walker", "head"]"#, r#"["place", "head"]"#).contains("no node at place / head"));
        assert!(broken("[0.02, 0.03, 0.04]", "[0.02, 1.03, 0.04]").contains("fractions"));
        assert!(broken(r#""transmission": [0.0, 0.0, 0.0]"#, r#""transmission": [0.99, 0.0, 0.0]"#).contains("250 Hz"));
        assert!(broken(r#""acoustic": "hard""#, r#""acoustic": "soft""#).contains("no such acoustic material"));
        assert!(broken(r#""children": [2]"#, r#""children": [2, 3]"#).contains("two parents"));
        assert!(broken("[50, 0, -50]]", "[50, 3, -50]]").contains("not flat"));
        assert!(broken(r#""material": "sole""#, r#""material": "hoof""#).contains("no contact sound"));
        assert!(broken(r#""version": 1"#, r#""version": 2"#).contains("another version"));
        assert!(broken(r#"{"id": "spring"}"#, r#"{"id": "head"}"#).contains("a second or an empty node identifier"));
        assert!(broken("[0.5, 0.4, 0.3]", r#""fromRoom""#).contains("reverb"));
        assert!(Scene::from_json("{").unwrap_err().contains("not JSON"));
    }
}
