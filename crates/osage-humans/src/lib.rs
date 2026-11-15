// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! People from rigged models of the asset manifest (Quaternius's Universal
//! Base Characters and Universal Animation Library, CC0): two bodies, their
//! hair, and a library of moves, all on one skeleton.
//!
//! A [`Human`] is an entity: once the models have loaded, its skeleton is
//! built under it (one entity per bone, named as in the model), its body
//! skinned to that skeleton, its hair and eyebrows too, and it plays its
//! [`Move`] from the library, crossfading when the move changes. The
//! skeleton's bones are in [`Skeleton`], to hold things in a hand.
//!
//! **Dressing.** The bodies are bare. Each part of the body is moved by
//! the joints of its own bones, so the joint that weighs most on a vertex
//! says what the vertex wears: the head, the neck and the hands are skin,
//! the torso and the arms a top, the hips and the legs the bottom, the
//! feet shoes. The body's texture is painted once per body and skin tone:
//! what is worn becomes a plain fabric (its shading kept faintly, its
//! relief and its sheen taken away), and each outfit is the body's mesh
//! with its vertex colours set from the parts it covers.
//!
//! The model faces +Z, its feet at the origin, 1.8 m tall.

use std::{collections::HashMap, path::Path, time::Duration};

use bevy::{
    animation::{AnimatedBy, AnimationTargetId},
    asset::{RenderAssetUsages, UntypedAssetId},
    gltf::{Gltf, GltfMaterial, GltfMesh, GltfNode, GltfSkin},
    image::Image,
    mesh::{VertexAttributeValues, skinning::SkinnedMesh},
    pbr::gltf::standard_material_from_gltf_material,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

/// The two bodies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Body {
    Man,
    Woman,
}

/// The skin tones the pack paints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tone {
    Light,
    Dark,
}

/// The pack's hairstyles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Hair {
    Buzzed,
    BuzzedShort,
    Parted,
    Long,
    Buns,
}

impl Hair {
    pub const ALL: [Hair; 5] = [Hair::Buzzed, Hair::BuzzedShort, Hair::Parted, Hair::Long, Hair::Buns];

    fn file(self) -> &'static str {
        match self {
            Hair::Buzzed => "Hair_Buzzed",
            Hair::BuzzedShort => "Hair_BuzzedFemale",
            Hair::Parted => "Hair_SimpleParted",
            Hair::Long => "Hair_Long",
            Hair::Buns => "Hair_Buns",
        }
    }
}

/// What a human wears: the colours of the top, the bottom and the shoes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outfit {
    pub top: Color,
    pub bottom: Color,
    pub shoes: Color,
}

impl Outfit {
    /// A key to share one mesh between humans dressed alike.
    fn key(&self) -> [u8; 9] {
        let mut key = [0; 9];
        for (i, colour) in [self.top, self.bottom, self.shoes].into_iter().enumerate() {
            let c = colour.to_srgba();
            key[3 * i..3 * i + 3].copy_from_slice(&[c.red, c.green, c.blue].map(|v| (v * 255.0).round() as u8));
        }
        key
    }
}

/// A move of the library, by its name there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Move(pub &'static str);

impl Move {
    pub const IDLE: Move = Move("Idle_Loop");
    pub const TALKING: Move = Move("Idle_Talking_Loop");
    /// Standing, one arm raised holding something up (a torch, a flag).
    pub const HOLDING_UP: Move = Move("Idle_Torch_Loop");
    pub const WALK: Move = Move("Walk_Loop");
    pub const JOG: Move = Move("Jog_Fwd_Loop");
    pub const SITTING: Move = Move("Sitting_Idle_Loop");
    pub const SITTING_TALKING: Move = Move("Sitting_Talking_Loop");
    pub const DANCE: Move = Move("Dance_Loop");
    pub const CROUCHING: Move = Move("Crouch_Idle_Loop");
    /// Kneeling, working at something low (a kart's wheel).
    pub const FIXING: Move = Move("Fixing_Kneeling");
    pub const INTERACT: Move = Move("Interact");
    pub const PUSH: Move = Move("Push_Loop");
    pub const DRIVING: Move = Move("Driving_Loop");
}

/// A person. Changing `act` crossfades to the new move.
#[derive(Component, Clone, Debug)]
#[require(Transform, Visibility)]
pub struct Human {
    pub body: Body,
    pub tone: Tone,
    pub hair: Option<Hair>,
    pub hair_colour: Color,
    pub outfit: Outfit,
    pub act: Move,
    /// How fast the move plays, 1 its own pace.
    pub pace: f32,
    /// Where in the move it starts, 0..1: people side by side are not in step.
    pub phase: f32,
}

/// A human's skeleton, once built: the bones by their names in the model
/// (`hand_r`, `Head`, …).
#[derive(Component, Debug)]
pub struct Skeleton {
    pub armature: Entity,
    pub bones: HashMap<String, Entity>,
    playing: Option<Move>,
}

/// Whether the people are among the installed assets (`root`: the folder of
/// the asset manifest). Without them, a world draws its people otherwise.
pub fn installed(root: &Path) -> bool {
    [BODIES[0], BODIES[1], MOVES].iter().all(|file| root.join(file).exists())
}

const BODIES: [&str; 2] =
    ["quaternius/humans/Superhero_Male_FullBody.gltf", "quaternius/humans/Superhero_Female_FullBody.gltf"];
const LIGHT_SKINS: [&str; 2] =
    ["quaternius/humans/T_Superhero_Male_Ligh.png", "quaternius/humans/T_Superhero_Female_Light_BaseColor.png"];
const MOVES: &str = "quaternius/moves/UAL1_Standard.glb";
/// The side of a dressed body's textures, texels.
const DRESSED_SIZE: u32 = 1024;
/// How long one move takes to fade into the next.
const CROSSFADE: Duration = Duration::from_millis(350);

pub struct HumansPlugin;

impl Plugin for HumansPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load).add_systems(Update, (prepare, build, act).chain());
    }
}

/// The models, and what is made from them once.
#[derive(Resource)]
struct Library {
    bodies: [Handle<Gltf>; 2],
    light: [Handle<Image>; 2],
    hair: Vec<(Hair, Handle<Gltf>)>,
    moves: Handle<Gltf>,
    /// Set once everything has loaded.
    ready: Option<Ready>,
    failed: bool,
}

struct Ready {
    graph: Handle<AnimationGraph>,
    nodes: HashMap<String, AnimationNodeIndex>,
    dressed: HashMap<(Body, Tone), Handle<StandardMaterial>>,
    zones: [Vec<Zone>; 2],
    /// Each body with volume given to what it wears ([`clothe`]).
    clothed: [Option<Mesh>; 2],
    outfits: HashMap<(Body, [u8; 9]), Handle<Mesh>>,
    hair_materials: HashMap<(Hair, [u8; 3]), Handle<StandardMaterial>>,
}

fn load(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(Library {
        bodies: BODIES.map(|file| server.load(file)),
        light: LIGHT_SKINS.map(|file| server.load(file)),
        hair: Hair::ALL.iter().map(|&h| (h, server.load(format!("quaternius/hair/{}.gltf", h.file())))).collect(),
        moves: server.load(MOVES),
        ready: None,
        failed: false,
    });
}

/// What a part of the body wears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Zone {
    Unpainted = 0,
    Skin,
    Top,
    Bottom,
    Shoes,
}

/// What the part a bone moves wears, by the bone's name.
fn zone_of(bone: &str) -> Zone {
    let has = |part: &str| bone.contains(part);
    if has("Head") || has("neck") || has("hand") || ["index", "middle", "pinky", "ring", "thumb"].iter().any(|f| has(f))
    {
        Zone::Skin
    } else if has("foot") || has("ball") {
        Zone::Shoes
    } else if has("pelvis") || has("thigh") || has("calf") {
        Zone::Bottom
    } else if has("spine") || has("clavicle") || has("arm") {
        Zone::Top
    } else {
        Zone::Skin
    }
}

/// Once everything has loaded: the moves' graph, and each body's dressed
/// material for both tones.
#[allow(clippy::too_many_arguments)]
fn prepare(
    mut library: ResMut<Library>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    nodes: Res<Assets<GltfNode>>,
    meshes_gltf: Res<Assets<GltfMesh>>,
    skins: Res<Assets<GltfSkin>>,
    gltf_materials: Res<Assets<GltfMaterial>>,
    meshes: Res<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    if library.ready.is_some() || library.failed {
        return;
    }
    let ids: Vec<UntypedAssetId> = library
        .bodies
        .iter()
        .map(|h| h.id().untyped())
        .chain(library.light.iter().map(|h| h.id().untyped()))
        .chain(library.hair.iter().map(|(_, h)| h.id().untyped()))
        .chain([library.moves.id().untyped()])
        .collect();
    if ids.iter().any(|&id| server.load_state(id).is_failed()) {
        warn!("osage-humans: the people's models did not load; run tools/quaternius.py");
        library.failed = true;
        return;
    }
    if !ids.iter().all(|&id| server.is_loaded_with_dependencies(id)) {
        return;
    }
    let Some(moves) = gltfs.get(&library.moves) else { return };
    let mut graph = AnimationGraph::new();
    let mut names: Vec<_> = moves.named_animations.iter().collect();
    names.sort_by(|a, b| a.0.cmp(b.0));
    let root = graph.root;
    let indices =
        names.iter().map(|(name, clip)| (name.to_string(), graph.add_clip((*clip).clone(), 1.0, root))).collect();
    let mut dressed = HashMap::new();
    let mut zones: [Vec<Zone>; 2] = Default::default();
    let mut clothed: [Option<Mesh>; 2] = Default::default();
    for (b, body) in [Body::Man, Body::Woman].into_iter().enumerate() {
        let Some(gltf) = gltfs.get(&library.bodies[b]) else { return };
        let Some((mesh, skin, material)) = body_parts(gltf, &nodes, &meshes_gltf, &meshes) else {
            warn!(?body, "osage-humans: the body's model is not the one expected");
            library.failed = true;
            return;
        };
        let (Some(mesh), Some(skin)) = (meshes.get(&mesh), skins.get(&skin)) else { return };
        let bone_zones: Vec<Zone> =
            skin.joints.iter().map(|joint| nodes.get(joint).map_or(Zone::Skin, |node| zone_of(&node.name))).collect();
        let Some(vertex_zones) = vertex_zones(mesh, &bone_zones) else {
            library.failed = true;
            return;
        };
        let source = material.and_then(|m| gltf_materials.get(&m)).map(standard_material_from_gltf_material);
        let Some(source) = source else { return };
        for tone in [Tone::Light, Tone::Dark] {
            let colour = match tone {
                Tone::Light => Some(library.light[b].clone()),
                Tone::Dark => source.base_color_texture.clone(),
            };
            let material = dress(mesh, &vertex_zones, &source, colour, &mut images);
            dressed.insert((body, tone), materials.add(material));
        }
        clothed[b] = Some(clothe(mesh, &vertex_zones));
        zones[b] = vertex_zones;
    }
    library.ready = Some(Ready {
        graph: graphs.add(graph),
        nodes: indices,
        dressed,
        zones,
        clothed,
        outfits: HashMap::new(),
        hair_materials: HashMap::new(),
    });
    info!("osage-humans: people ready");
}

/// A body's own mesh, its skin and its material.
type BodyParts = (Handle<Mesh>, Handle<GltfSkin>, Option<Handle<GltfMaterial>>);

/// The body's own mesh (the one with most vertices, beside the eyes and
/// the eyebrows), its skin and its material.
fn body_parts(
    gltf: &Gltf,
    nodes: &Assets<GltfNode>,
    gltf_meshes: &Assets<GltfMesh>,
    meshes: &Assets<Mesh>,
) -> Option<BodyParts> {
    let armature = nodes.get(gltf.named_nodes.get("Armature")?)?;
    let mut best: Option<(usize, BodyParts)> = None;
    for child in &armature.children {
        let Some(node) = nodes.get(child) else { continue };
        let (Some(mesh), Some(skin)) = (node.mesh.as_ref().and_then(|m| gltf_meshes.get(m)), node.skin.clone()) else {
            continue;
        };
        for primitive in &mesh.primitives {
            let count = meshes.get(&primitive.mesh).map_or(0, Mesh::count_vertices);
            if best.as_ref().is_none_or(|b| count > b.0) {
                best = Some((count, (primitive.mesh.clone(), skin.clone(), primitive.material.clone())));
            }
        }
    }
    best.map(|(_, parts)| parts)
}

/// Each vertex's part of the body, from the joint that weighs most on it.
fn vertex_zones(mesh: &Mesh, bone_zones: &[Zone]) -> Option<Vec<Zone>> {
    let joints = match mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX)? {
        VertexAttributeValues::Uint16x4(j) => j.clone(),
        _ => return None,
    };
    let weights = match mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT)? {
        VertexAttributeValues::Float32x4(w) => w,
        _ => return None,
    };
    Some(
        joints
            .iter()
            .zip(weights)
            .map(|(j, w)| {
                let k = (0..4).max_by(|&a, &b| w[a].total_cmp(&w[b])).unwrap_or(0);
                bone_zones.get(j[k] as usize).copied().unwrap_or(Zone::Skin)
            })
            .collect(),
    )
}

/// The body's material with what is worn painted as plain fabric.
fn dress(
    mesh: &Mesh,
    zones: &[Zone],
    source: &StandardMaterial,
    colour: Option<Handle<Image>>,
    images: &mut Assets<Image>,
) -> StandardMaterial {
    let mut material = source.clone();
    let Some(mask) = paint_mask(mesh, zones, DRESSED_SIZE) else { return material };
    let worn = |x: u32, y: u32| mask[(y * DRESSED_SIZE + x) as usize] > Zone::Skin as u8;
    // Colour: the fabric a light grey the outfit's colour multiplies, the
    // skin's shading kept faintly in it (folds, shadow under the arms).
    if let Some(image) = colour.as_ref().and_then(|h| images.get(h)) {
        let srgb = image.texture_descriptor.format.is_srgb();
        if let Some(texture) = resample(image, DRESSED_SIZE, |x, y, [r, g, b, a]| {
            if worn(x, y) {
                let shade = (r as f32 * 0.3 + g as f32 * 0.55 + b as f32 * 0.15) / 255.0;
                let v = (196.0 + 60.0 * (shade - 0.6)).clamp(150.0, 230.0) as u8;
                [v, v, v, a]
            } else {
                [r, g, b, a]
            }
        }) {
            material.base_color_texture = Some(images.add(finish(texture, srgb)));
        }
    }
    // Relief: flat where worn.
    if let Some(image) = material.normal_map_texture.as_ref().and_then(|h| images.get(h))
        && let Some(texture) =
            resample(image, DRESSED_SIZE, |x, y, texel| if worn(x, y) { [128, 128, 255, 255] } else { texel })
    {
        material.normal_map_texture = Some(images.add(finish(texture, false)));
    }
    // Roughness (green) high and no metal (blue) where worn.
    if let Some(image) = material.metallic_roughness_texture.as_ref().and_then(|h| images.get(h))
        && let Some(texture) =
            resample(image, DRESSED_SIZE, |x, y, [r, g, b, a]| if worn(x, y) { [r, 235, 0, a] } else { [r, g, b, a] })
    {
        material.metallic_roughness_texture = Some(images.add(finish(texture, false)));
    }
    material
}

/// Which part of the body each texel of a `size`² texture belongs to: the
/// mesh's triangles drawn in its UV space, each in the part most of its
/// corners belong to, then spread a few texels past the seams.
fn paint_mask(mesh: &Mesh, zones: &[Zone], size: u32) -> Option<Vec<u8>> {
    let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0)? {
        VertexAttributeValues::Float32x2(uv) => uv,
        _ => return None,
    };
    let indices: Vec<usize> = mesh.indices()?.iter().collect();
    let n = size as usize;
    let mut mask = vec![Zone::Unpainted as u8; n * n];
    for &[a, b, c] in indices.as_chunks::<3>().0 {
        let (za, zb, zc) = (zones[a], zones[b], zones[c]);
        let zone = if zb == zc { zb } else { za };
        let p = [a, b, c].map(|i| Vec2::from(uvs[i]) * size as f32);
        let lo = p[0].min(p[1]).min(p[2]).floor().max(Vec2::ZERO);
        let hi = p[0].max(p[1]).max(p[2]).ceil().min(Vec2::splat(size as f32 - 1.0));
        let area = (p[1] - p[0]).perp_dot(p[2] - p[0]);
        if area.abs() < 1e-9 {
            continue;
        }
        for y in lo.y as usize..=hi.y as usize {
            for x in lo.x as usize..=hi.x as usize {
                let q = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let w0 = (p[2] - p[1]).perp_dot(q - p[1]) / area;
                let w1 = (p[0] - p[2]).perp_dot(q - p[2]) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 >= -0.01 && w1 >= -0.01 && w2 >= -0.01 {
                    mask[y * n + x] = zone as u8;
                }
            }
        }
    }
    // Past the seams, so that filtering does not bring skin into the cloth.
    for _ in 0..4 {
        let before = mask.clone();
        for y in 0..n {
            for x in 0..n {
                if before[y * n + x] != Zone::Unpainted as u8 {
                    continue;
                }
                let near = [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)]
                    .into_iter()
                    .filter(|&(i, j)| i < n && j < n)
                    .map(|(i, j)| before[j * n + i])
                    .find(|&z| z != Zone::Unpainted as u8);
                if let Some(z) = near {
                    mask[y * n + x] = z;
                }
            }
        }
    }
    Some(mask)
}

/// `image` brought to `size`² (each texel the mean of the source texels it
/// covers), every texel passed through `texel(x, y, rgba)`. `None` if the
/// image is not 8-bit RGBA held on the CPU.
fn resample(image: &Image, size: u32, texel: impl Fn(u32, u32, [u8; 4]) -> [u8; 4]) -> Option<Vec<u8>> {
    let data = image.data.as_ref()?;
    if !matches!(image.texture_descriptor.format, TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb) {
        return None;
    }
    let (w, h) = (image.width(), image.height());
    if w < size || h < size || data.len() < (w * h * 4) as usize {
        return None;
    }
    let (sx, sy) = (w / size, h / size);
    let mut out = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let mut sum = [0u32; 4];
            for j in 0..sy {
                for i in 0..sx {
                    let at = (((y * sy + j) * w + x * sx + i) * 4) as usize;
                    for (k, s) in sum.iter_mut().enumerate() {
                        *s += data[at + k] as u32;
                    }
                }
            }
            let mean = sum.map(|s| (s / (sx * sy)) as u8);
            let at = ((y * size + x) * 4) as usize;
            out[at..at + 4].copy_from_slice(&texel(x, y, mean));
        }
    }
    Some(out)
}

/// A square RGBA texture with its chain of smaller levels (each the mean of
/// four texels of the one above), so that it does not shimmer far off.
fn finish(level0: Vec<u8>, srgb: bool) -> Image {
    let size = ((level0.len() / 4) as f64).sqrt() as u32;
    let mut data = level0.clone();
    let (mut level, mut side, mut count) = (level0, size, 1);
    while side > 1 {
        let half = side / 2;
        let mut next = vec![0u8; (half * half * 4) as usize];
        for y in 0..half {
            for x in 0..half {
                for k in 0..4 {
                    let at = |i: u32, j: u32| level[(((2 * y + j) * side + 2 * x + i) * 4 + k) as usize] as u32;
                    next[((y * half + x) * 4 + k) as usize] = ((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1)) / 4) as u8;
                }
            }
        }
        data.extend_from_slice(&next);
        (level, side, count) = (next, half, count + 1);
    }
    let format = if srgb { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm };
    // Not `Image::new`: it expects the first level alone.
    let mut image = Image::new_uninit(
        Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        TextureDimension::D2,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = count;
    image
}

/// How far cloth stands off the body, m, by what it is.
fn loose(zone: Zone) -> f32 {
    match zone {
        Zone::Top => 0.018,
        Zone::Bottom => 0.014,
        Zone::Skin | Zone::Shoes | Zone::Unpainted => 0.0,
    }
}

/// The body with volume given to what it wears: cloth does not follow
/// every muscle. The worn surface is smoothed (Taubin's λ|μ steps, which
/// do not shrink it), its normals made again from the smoothed surface,
/// and it is pushed out along them by how loose the cloth is. Vertices
/// that share a place (a seam of the texture) move as one, so the mesh
/// does not open.
fn clothe(body: &Mesh, zones: &[Zone]) -> Mesh {
    let mut mesh = body.clone();
    let (Some(VertexAttributeValues::Float32x3(positions)), Some(indices)) =
        (body.attribute(Mesh::ATTRIBUTE_POSITION), body.indices())
    else {
        return mesh;
    };
    let indices: Vec<usize> = indices.iter().collect();
    // Welded: one place per position, a tenth of a millimetre apart.
    let mut places: HashMap<[i32; 3], usize> = HashMap::new();
    let place_of: Vec<usize> = positions
        .iter()
        .map(|p| {
            let key = p.map(|v| (v * 10_000.0).round() as i32);
            let next = places.len();
            *places.entry(key).or_insert(next)
        })
        .collect();
    let n = places.len();
    let mut at = vec![Vec3::ZERO; n];
    let mut worn = vec![Zone::Skin; n];
    for (v, &p) in place_of.iter().enumerate() {
        at[p] = Vec3::from(positions[v]);
        if zones[v] != Zone::Skin && zones[v] != Zone::Unpainted {
            worn[p] = zones[v];
        }
    }
    let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &[a, b, c] in indices.as_chunks::<3>().0 {
        let [a, b, c] = [a, b, c].map(|v| place_of[v]);
        for (x, y) in [(a, b), (b, c), (c, a)] {
            if !neighbours[x].contains(&y) {
                neighbours[x].push(y);
            }
            if !neighbours[y].contains(&x) {
                neighbours[y].push(x);
            }
        }
    }
    let moves = |p: usize| matches!(worn[p], Zone::Top | Zone::Bottom);
    for _ in 0..12 {
        for factor in [0.5f32, -0.53] {
            let before = at.clone();
            for p in (0..n).filter(|&p| moves(p) && !neighbours[p].is_empty()) {
                let mean = neighbours[p].iter().map(|&q| before[q]).sum::<Vec3>() / neighbours[p].len() as f32;
                at[p] = before[p] + factor * (mean - before[p]);
            }
        }
    }
    // Normals of the smoothed surface, by area, per place.
    let mut normal = vec![Vec3::ZERO; n];
    for &[a, b, c] in indices.as_chunks::<3>().0 {
        let [a, b, c] = [a, b, c].map(|v| place_of[v]);
        let face = (at[b] - at[a]).cross(at[c] - at[a]);
        for p in [a, b, c] {
            normal[p] += face;
        }
    }
    let normal: Vec<Vec3> = normal.into_iter().map(|v| v.normalize_or_zero()).collect();
    let new_positions: Vec<[f32; 3]> = (0..positions.len())
        .map(|v| {
            let p = place_of[v];
            if moves(p) { (at[p] + normal[p] * loose(worn[p])).to_array() } else { positions[v] }
        })
        .collect();
    if let Some(VertexAttributeValues::Float32x3(normals)) = body.attribute(Mesh::ATTRIBUTE_NORMAL) {
        let new_normals: Vec<[f32; 3]> = (0..normals.len())
            .map(|v| {
                let p = place_of[v];
                if moves(p) && normal[p] != Vec3::ZERO { normal[p].to_array() } else { normals[v] }
            })
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, new_normals);
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, new_positions);
    mesh
}

/// The body's mesh with its vertex colours set from what each part wears.
fn outfit_mesh(body: &Mesh, zones: &[Zone], outfit: &Outfit) -> Mesh {
    let colour = |zone: Zone| match zone {
        Zone::Top => outfit.top,
        Zone::Bottom => outfit.bottom,
        Zone::Shoes => outfit.shoes,
        Zone::Skin | Zone::Unpainted => Color::WHITE,
    };
    let colours: Vec<[f32; 4]> = zones.iter().map(|&z| colour(z).to_linear().to_f32_array()).collect();
    let mut mesh = body.clone();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
    mesh
}

/// Builds the humans whose models are ready: their skeleton, body, hair.
#[allow(clippy::too_many_arguments)]
fn build(
    mut commands: Commands,
    mut library: ResMut<Library>,
    humans: Query<(Entity, &Human), Without<Skeleton>>,
    gltfs: Res<Assets<Gltf>>,
    nodes: Res<Assets<GltfNode>>,
    meshes_gltf: Res<Assets<GltfMesh>>,
    skins: Res<Assets<GltfSkin>>,
    gltf_materials: Res<Assets<GltfMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if humans.is_empty() {
        return;
    }
    let Library { bodies, hair: hairs, ready, .. } = &mut *library;
    let Some(ready) = ready.as_mut() else { return };
    for (entity, human) in &humans {
        let b = human.body as usize;
        let Some(gltf) = gltfs.get(&bodies[b]) else { continue };
        let Some(armature_node) = gltf.named_nodes.get("Armature").and_then(|h| nodes.get(h)) else { continue };
        // The skeleton: one entity per bone, each a target of the moves.
        let armature = commands
            .spawn((
                Name::new("Armature"),
                armature_node.transform,
                Visibility::default(),
                AnimationPlayer::default(),
                AnimationGraphHandle(ready.graph.clone()),
                AnimationTransitions::new(),
            ))
            .id();
        commands
            .entity(armature)
            .insert((AnimationTargetId::from_names([Name::new("Armature")].iter()), AnimatedBy(armature)));
        commands.entity(entity).add_child(armature);
        let mut by_index: HashMap<usize, Entity> = HashMap::new();
        let mut bones: HashMap<String, Entity> = HashMap::new();
        let mut stack: Vec<(Handle<GltfNode>, Entity, Vec<Name>)> =
            armature_node.children.iter().map(|child| (child.clone(), armature, vec![Name::new("Armature")])).collect();
        let mut skinned: Vec<&GltfNode> = Vec::new();
        while let Some((handle, parent, path)) = stack.pop() {
            let Some(node) = nodes.get(&handle) else { continue };
            if node.mesh.is_some() {
                skinned.push(node);
                continue;
            }
            let mut path = path;
            path.push(Name::new(node.name.clone()));
            let bone = commands
                .spawn((
                    Name::new(node.name.clone()),
                    node.transform,
                    Visibility::default(),
                    AnimationTargetId::from_names(path.iter()),
                    AnimatedBy(armature),
                ))
                .id();
            commands.entity(parent).add_child(bone);
            by_index.insert(node.index, bone);
            bones.insert(node.name.clone(), bone);
            stack.extend(node.children.iter().map(|child| (child.clone(), bone, path.clone())));
        }
        let hair_key = colour_key(human.hair_colour);
        // The body, its eyes and eyebrows, skinned to the skeleton.
        let outfit_key = (human.body, human.outfit.key());
        let dressed_body = ready.dressed.get(&(human.body, human.tone)).cloned();
        let main_body = body_parts(gltf, &nodes, &meshes_gltf, &meshes).map(|(mesh, _, _)| mesh);
        let hair_materials: Vec<&Handle<GltfMaterial>> =
            gltf.named_materials.iter().filter(|(name, _)| name.contains("Hair")).map(|(_, h)| h).collect();
        for node in skinned {
            let (Some(mesh), Some(skin)) =
                (node.mesh.as_ref().and_then(|m| meshes_gltf.get(m)), node.skin.as_ref().and_then(|s| skins.get(s)))
            else {
                continue;
            };
            let joints: Vec<Entity> = skin
                .joints
                .iter()
                .filter_map(|j| nodes.get(j))
                .filter_map(|j| by_index.get(&j.index).copied())
                .collect();
            if joints.len() != skin.joints.len() {
                continue;
            }
            for primitive in &mesh.primitives {
                let is_body = main_body.as_ref() == Some(&primitive.mesh);
                let source = primitive.material.as_ref().and_then(|m| gltf_materials.get(m));
                let (mesh_handle, material) = if is_body {
                    let outfit = match ready.outfits.get(&outfit_key) {
                        Some(handle) => handle.clone(),
                        None => {
                            let Some(body_mesh) = ready.clothed[b].as_ref().or(meshes.get(&primitive.mesh)) else {
                                continue;
                            };
                            let dressed = outfit_mesh(body_mesh, &ready.zones[b], &human.outfit);
                            let made = meshes.add(dressed);
                            ready.outfits.insert(outfit_key, made.clone());
                            made
                        }
                    };
                    let material = dressed_body.clone().unwrap_or_else(|| {
                        materials.add(source.map(standard_material_from_gltf_material).unwrap_or_default())
                    });
                    (outfit, material)
                } else if primitive.material.as_ref().is_some_and(|m| hair_materials.contains(&m)) {
                    // Eyebrows: the hair's colour.
                    let material = tinted(source, human.hair_colour, &mut materials);
                    (primitive.mesh.clone(), material)
                } else {
                    let material = materials.add(source.map(standard_material_from_gltf_material).unwrap_or_default());
                    (primitive.mesh.clone(), material)
                };
                let part = commands
                    .spawn((
                        Mesh3d(mesh_handle),
                        MeshMaterial3d(material),
                        SkinnedMesh { inverse_bindposes: skin.inverse_bind_matrices.clone(), joints: joints.clone() },
                        Transform::IDENTITY,
                    ))
                    .id();
                commands.entity(entity).add_child(part);
            }
        }
        // The hair: rigged to the same skeleton, its bones found by name.
        if let Some(style) = human.hair
            && let Some((_, handle)) = hairs.iter().find(|(h, _)| *h == style)
            && let Some(hair) = gltfs.get(handle)
            && let Some(hair_armature) = hair.named_nodes.get("Armature").and_then(|h| nodes.get(h))
        {
            for child in &hair_armature.children {
                let Some(node) = nodes.get(child) else { continue };
                let (Some(mesh), Some(skin)) = (
                    node.mesh.as_ref().and_then(|m| meshes_gltf.get(m)),
                    node.skin.as_ref().and_then(|s| skins.get(s)),
                ) else {
                    continue;
                };
                let joints: Vec<Entity> = skin
                    .joints
                    .iter()
                    .filter_map(|j| nodes.get(j))
                    .filter_map(|j| bones.get(&j.name).copied())
                    .collect();
                if joints.len() != skin.joints.len() {
                    continue;
                }
                for primitive in &mesh.primitives {
                    let source = primitive.material.as_ref().and_then(|m| gltf_materials.get(m));
                    let material = ready
                        .hair_materials
                        .entry((style, hair_key))
                        .or_insert_with(|| tinted(source, human.hair_colour, &mut materials))
                        .clone();
                    let part = commands
                        .spawn((
                            Mesh3d(primitive.mesh.clone()),
                            MeshMaterial3d(material),
                            SkinnedMesh {
                                inverse_bindposes: skin.inverse_bind_matrices.clone(),
                                joints: joints.clone(),
                            },
                            Transform::IDENTITY,
                        ))
                        .id();
                    commands.entity(entity).add_child(part);
                }
            }
        }
        debug!(?entity, body = ?human.body, act = human.act.0, "osage-humans: built");
        commands.entity(entity).insert(Skeleton { armature, bones, playing: None });
    }
}

fn colour_key(colour: Color) -> [u8; 3] {
    let c = colour.to_srgba();
    [c.red, c.green, c.blue].map(|v| (v * 255.0).round() as u8)
}

/// The model's hair material, its colour texture multiplied by `colour`.
fn tinted(
    source: Option<&GltfMaterial>,
    colour: Color,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    let mut material = source.map(standard_material_from_gltf_material).unwrap_or_default();
    material.base_color = colour;
    materials.add(material)
}

/// Plays each human's move, crossfading when it changes.
fn act(
    library: Res<Library>,
    mut humans: Query<(&Human, &mut Skeleton)>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    clips: Res<Assets<AnimationClip>>,
    graphs: Res<Assets<AnimationGraph>>,
) {
    let Some(ready) = library.ready.as_ref() else { return };
    for (human, mut skeleton) in &mut humans {
        if skeleton.playing == Some(human.act) {
            continue;
        }
        let Some(&node) = ready.nodes.get(human.act.0) else {
            warn!(act = human.act.0, "osage-humans: no such move");
            skeleton.playing = Some(human.act);
            continue;
        };
        let Ok((mut player, mut transitions)) = players.get_mut(skeleton.armature) else { continue };
        let fade = if skeleton.playing.is_some() { CROSSFADE } else { Duration::ZERO };
        let length = graphs
            .get(&ready.graph)
            .and_then(|graph| graph.get(node))
            .and_then(|n| match &n.node_type {
                AnimationNodeType::Clip(clip) => clips.get(clip),
                _ => None,
            })
            .map_or(1.0, |clip| clip.duration());
        transitions.play(&mut player, node, fade).repeat().set_speed(human.pace).seek_to(human.phase * length);
        skeleton.playing = Some(human.act);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bones_wear_what_covers_them() {
        assert_eq!(zone_of("Head"), Zone::Skin);
        assert_eq!(zone_of("neck_01"), Zone::Skin);
        assert_eq!(zone_of("hand_r"), Zone::Skin);
        assert_eq!(zone_of("thumb_02_l"), Zone::Skin);
        assert_eq!(zone_of("upperarm_l"), Zone::Top);
        assert_eq!(zone_of("lowerarm_r"), Zone::Top);
        assert_eq!(zone_of("spine_03"), Zone::Top);
        assert_eq!(zone_of("pelvis"), Zone::Bottom);
        assert_eq!(zone_of("calf_l"), Zone::Bottom);
        assert_eq!(zone_of("ball_r"), Zone::Shoes);
    }

    #[test]
    fn levels_halve_down_to_one_texel() {
        let image = finish(vec![255; 8 * 8 * 4], false);
        assert_eq!(image.texture_descriptor.mip_level_count, 4);
        assert_eq!(image.data.as_ref().map(Vec::len), Some((64 + 16 + 4 + 1) * 4));
    }

    #[test]
    fn outfits_alike_share_a_key() {
        let outfit = Outfit { top: Color::srgb(1.0, 0.5, 0.0), bottom: Color::BLACK, shoes: Color::WHITE };
        assert_eq!(outfit.key(), outfit.key());
        assert_ne!(outfit.key(), Outfit { top: Color::WHITE, ..outfit }.key());
    }
}
