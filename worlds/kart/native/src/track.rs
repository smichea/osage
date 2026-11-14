// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The circuit, built from the centre line the world announces: the road and
//! its markings, kerbs, mown verges, barriers, the start line and its grid,
//! and the land around — flat along the track, hills beyond, a lake and a
//! knoll in the room the circuit leaves inside its loop (`osage-kart-land`). The ground is
//! painted through the land's vertex colours: meadows in patches, paler
//! heights, sand at the shore. With the manifest's textures the road and the
//! grass get the grain of real surfaces; without, plain colours.

use bevy::{
    asset::RenderAssetUsages,
    color::ColorToComponents,
    image::ImageSampler,
    light::NotShadowCaster,
    math::Affine2,
    mesh::Indices,
    prelude::*,
    render::render_resource::{Extent3d, PrimitiveTopology, TextureDimension, TextureFormat},
};
pub use osage_kart_land::{BARRIER, Frame, HILLS, LAND, Land, Site, WATER, frames, noise, smooth};

use osage_content::{self as content, GRAIN, Kind, Surface};
use osage_procedural::{Batch, Paint, Shape};
use osage_render::{
    Tier,
    ground::{Ground, GroundMaterial, untextured},
};

use crate::scene::{Course, GRASS, Part};

/// The colour of the ground, linear: meadows in patches, paler on the
/// heights, sand around the lake and under it.
fn colour(land: &Land, at: Vec2, height: f32) -> Vec3 {
    let tone = |r: f32, g: f32, b: f32| Color::srgb(r, g, b).to_linear().to_vec3();
    let (meadow, shade) = (GRASS.to_linear().to_vec3(), tone(0.25, 0.55, 0.17));
    let patches = smooth((noise(at / 38.0 + 40.0) - 0.38) / 0.24);
    let mut colour = meadow.lerp(shade, patches) * (0.9 + 0.2 * noise(at / 9.0 + 11.0));
    colour = colour.lerp(tone(0.56, 0.76, 0.27), (height / HILLS).clamp(0.0, 1.0) * 0.7);
    if let Some(shore) = land.shore(at) {
        colour = colour.lerp(tone(0.93, 0.84, 0.6), smooth((1.24 - shore) / 0.12));
        // Deeper, the bed darkens: the water above it looks deep.
        colour = colour.lerp(tone(0.16, 0.36, 0.42), smooth((0.9 - shore) / 0.4));
    }
    colour.lerp(tone(0.5, 0.47, 0.42), cliff(land, at))
}

/// The colour of a real meadow, linear, under a grain of grass: summer
/// greens, darker and greyer than a toy's, in wide sweeps that hide the
/// grain's repeats; drier patches, yellower heights, sand at the lake.
fn meadow(land: &Land, at: Vec2, height: f32) -> Vec3 {
    let tone = |r: f32, g: f32, b: f32| Color::srgb(r, g, b).to_linear().to_vec3();
    let wide = noise(at / 140.0 + 7.0);
    let middle = noise(at / 27.0 + 3.0);
    let fine = noise(at / 6.0 + 11.0);
    // Meadows are never one green: sweeps of lighter and darker, yellower and bluer.
    let mut colour = tone(0.2, 0.3, 0.11).lerp(tone(0.33, 0.4, 0.16), wide) * (0.75 + 0.35 * middle + 0.25 * fine);
    colour = colour.lerp(tone(0.2, 0.3, 0.16), smooth((middle - 0.6) / 0.3) * 0.5);
    let dry = smooth((noise(at / 38.0 + 40.0) - 0.55) / 0.2) * 0.6;
    colour = colour.lerp(tone(0.45, 0.42, 0.24), dry);
    colour = colour.lerp(tone(0.4, 0.42, 0.2), (height / HILLS).clamp(0.0, 1.0) * 0.5);
    // The clouds' shade is no longer painted here: their shadows drift
    // over the ground on the wind (D-108, [`drift_clouds`]).
    // Further out, farmland: a patchwork of fields, each its own crop.
    let away = smooth((land.distance_to_track(at) - 150.0) / 80.0);
    if away > 0.0 {
        colour = colour.lerp(field(at), away * 0.85);
    }
    if let Some(shore) = land.shore(at) {
        colour = colour.lerp(tone(0.66, 0.58, 0.42), smooth((1.24 - shore) / 0.12));
        colour = colour.lerp(tone(0.2, 0.26, 0.24), smooth((0.9 - shore) / 0.4));
    }
    colour = colour.lerp(tone(0.43, 0.4, 0.35), cliff(land, at));
    colour / GRAIN
}

/// The colour of bare rock where a place has it (D-077), linear: grey
/// granite in mountains, red sandstone in a canyon.
fn rock_of(scenery: &str) -> Option<Vec3> {
    let tone = |r: f32, g: f32, b: f32| Color::srgb(r, g, b).to_linear().to_vec3();
    match scenery {
        "mountain" => Some(tone(0.46, 0.44, 0.41)),
        "canyon" => Some(tone(0.62, 0.36, 0.22)),
        _ => None,
    }
}

/// The land of a track's place (D-077): a mountain's taller hills, a
/// canyon's walls, a gorge under each bridge and a ridge over each tunnel.
pub fn place(land: Land, circuit: &osage_world_kart::Circuit) -> Land {
    let spans = |kind| circuit.spans.iter().filter(|s| s.0 == kind).map(|s| (s.1, s.2)).collect();
    let land = land.with_spans(spans(osage_kart_tracks::SpanKind::Bridge), spans(osage_kart_tracks::SpanKind::Tunnel));
    match circuit.about.scenery.as_str() {
        "mountain" => land.with_relief(2.6),
        "canyon" => land.with_relief(1.4).with_walls(26.0),
        "coast" => land.with_sea(),
        _ => land,
    }
}

/// How much of a point is the cliff's rock: its edge and face, 0 inland.
fn cliff(land: &Land, at: Vec2) -> f32 {
    land.offshore(at).map_or(0.0, |off| smooth((off + 9.0) / 6.0))
}

/// A strip of the plain beyond the land, cut where the sea begins: the
/// part of it inland, if any (`low`, `high` from the land's middle).
fn inland(land: &Land, mut low: Vec2, mut high: Vec2) -> Option<(Vec2, Vec2)> {
    if let Some(sea) = land.sea {
        // Under the cliffs' plateau, short of where the shore wanders to.
        let line = sea.shore - 30.0;
        match (sea.towards.x, sea.towards.y) {
            (x, _) if x > 0.5 => high.x = high.x.min(line),
            (x, _) if x < -0.5 => low.x = low.x.max(-line),
            (_, y) if y > 0.5 => high.y = high.y.min(line),
            _ => low.y = low.y.max(-line),
        }
    }
    (high.x > low.x && high.y > low.y).then_some((low, high))
}

/// The sea, on a coast: deep blue water from under the cliffs to the
/// horizon, its ripples drifting as the lake's.
fn sea(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    land: &Land,
) {
    let Some(sea) = land.sea else { return };
    let water = materials.add(StandardMaterial {
        base_color: Color::srgba(0.05, 0.22, 0.34, 0.93),
        perceptual_roughness: 0.09,
        reflectance: 0.6,
        alpha_mode: AlphaMode::Blend,
        normal_map_texture: Some(images.add(ripples())),
        uv_transform: Affine2::from_scale(Vec2::splat(70.0)),
        ..default()
    });
    let (start, horizon) = (sea.shore - 40.0, 3000.0);
    let middle = land.centre() + sea.towards * (start + horizon) / 2.0;
    let size = if sea.towards.x.abs() > 0.5 {
        Vec2::new(horizon - start, 2.0 * horizon)
    } else {
        Vec2::new(2.0 * horizon, horizon - start)
    };
    let surface = Plane3d::default().mesh().size(size.x, size.y).build();
    commands.spawn((
        Part,
        Mesh3d(meshes.add(with_tangents(surface))),
        MeshMaterial3d(water.clone()),
        Transform::from_xyz(middle.x, osage_kart_land::SEA, middle.y),
        NotShadowCaster,
    ));
    commands.insert_resource(Sea(water));
}

/// The sea's water: its ripples drift too.
#[derive(Resource)]
pub struct Sea(Handle<StandardMaterial>);

/// The colour of the field a point lies in, linear: fields of 60 to 120 m
/// on a grid turned a little, each a crop — pasture, hay, ripe wheat,
/// ploughed earth, young barley — with its own shade.
fn field(at: Vec2) -> Vec3 {
    let tone = |r: f32, g: f32, b: f32| Color::srgb(r, g, b).to_linear().to_vec3();
    let crops = [
        tone(0.24, 0.33, 0.13),
        tone(0.3, 0.37, 0.14),
        tone(0.55, 0.48, 0.25),
        tone(0.66, 0.57, 0.32),
        tone(0.4, 0.31, 0.22),
        tone(0.34, 0.42, 0.18),
    ];
    // A grid turned by 0.3 radians, its cells stretched along one side.
    let turned = Vec2::from_angle(0.3).rotate(at) / Vec2::new(115.0, 70.0);
    let cell = turned.floor();
    let pick = noise(cell * 13.7 + 0.5);
    let crop = crops[(pick * crops.len() as f32) as usize % crops.len()];
    // Rows show as faint stripes along the field.
    let rows = 0.94 + 0.06 * (turned.y * 70.0 * 1.4).sin();
    crop * rows * (0.92 + 0.16 * noise(cell * 5.1 + 2.0))
}

fn linear(r: f32, g: f32, b: f32) -> [f32; 4] {
    Color::srgb(r, g, b).to_linear().to_f32_array()
}

const WHITE: [f32; 4] = [1.0; 4];
/// Cells along each side of the land's grid: small around the circuit, large far away.
const CELLS: usize = 280;

/// A ribbon along the track between two points of its cross-section, each
/// given as (metres right of the centre line, height above the road).
/// `facing` says which way the ribbon faces, in the same terms: (0, 1) up,
/// (-1, 0) towards the left. `colour` paints each segment, or leaves it out.
/// Over a crest the ribbon rises and falls with the road, in short pieces.
fn ribbon(
    frames: &[Frame],
    from: (f32, f32),
    to: (f32, f32),
    facing: (f32, f32),
    colour: impl Fn(usize) -> Option<[f32; 4]>,
) -> Mesh {
    ribbon_shaded(frames, from, to, facing, colour, |_| ([1.0; 3], [1.0; 3]))
}

/// A [`ribbon`] whose colour is multiplied along its `from` edge and its
/// `to` edge by what `shade` gives each segment: grime at the foot of a
/// wall, fading up it.
fn ribbon_shaded(
    frames: &[Frame],
    from: (f32, f32),
    to: (f32, f32),
    facing: (f32, f32),
    colour: impl Fn(usize) -> Option<[f32; 4]>,
    shade: impl Fn(usize) -> ([f32; 3], [f32; 3]),
) -> Mesh {
    let (mut positions, mut normals, mut uvs, mut colours) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let across = Vec2::new(to.0 - from.0, to.1 - from.1).length();
    for (i, a) in frames.iter().enumerate() {
        let Some(colour) = colour(i) else { continue };
        let (low, high) = shade(i);
        let tinted = |by: [f32; 3]| [colour[0] * by[0], colour[1] * by[1], colour[2] * by[2], colour[3]];
        // Corners 0 and 1 lie on the `from` edge, 2 and 3 on the `to` edge.
        let corner_colours = [tinted(low), tinted(low), tinted(high), tinted(high)];
        let b = &frames[(i + 1) % frames.len()];
        let length = a.at.distance(b.at);
        let pieces = a.pieces(length);
        // A share `t` of the way from `a` to `b`: where, which way is right, how high the road.
        let along =
            |t: f32| (a.at.lerp(b.at, t), a.right.lerp(b.right, t).normalize_or(a.right), a.height_towards(t, length));
        for piece in 0..pieces {
            let (t0, t1) = (piece as f32 / pieces as f32, (piece + 1) as f32 / pieces as f32);
            let corner = |t: f32, (lateral, height): (f32, f32)| {
                let (at, right, road) = along(t);
                let p = at + right * lateral;
                Vec3::new(p.x, road + height, p.y)
            };
            let quad = [corner(t0, from), corner(t1, from), corner(t1, to), corner(t0, to)];
            let (v0, v1) = (a.along + t0 * length, a.along + t1 * length);
            let quad_uvs = [[0.0, v0], [0.0, v1], [across, v1], [across, v0]];
            let wanted = Vec3::new(a.right.x * facing.0, facing.1, a.right.y * facing.0);
            let mut normal = (quad[1] - quad[0]).cross(quad[3] - quad[0]).normalize_or(Vec3::Y);
            // Wind the two triangles so that the front face is the wanted one.
            let order = if normal.dot(wanted) >= 0.0 { [0, 1, 3, 3, 1, 2] } else { [0, 3, 1, 1, 3, 2] };
            if normal.dot(wanted) < 0.0 {
                normal = -normal;
            }
            for corner in order {
                positions.push(quad[corner].to_array());
                normals.push(normal.to_array());
                uvs.push(quad_uvs[corner]);
                colours.push(corner_colours[corner]);
            }
        }
    }
    let indices = (0..positions.len() as u32).collect();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
        .with_inserted_indices(Indices::U32(indices))
}

/// The land as a grid, finer around the circuit than far from it, painted
/// with the colour of the ground. `paint` scales that colour for the
/// material that will cover it.
/// `mown` says where the grass is mown (1) or not (0), in the vertex
/// colour's alpha (D-101).
fn land_mesh(land: &Land, paint: impl Fn(Vec2, f32) -> Vec3, mown: impl Fn(Vec2) -> f32, rock: Option<Vec3>) -> Mesh {
    let n = CELLS;
    // Evenly spaced steps, bent so that the middle ones are five times smaller than the last.
    let along = |i: usize| {
        let u = 2.0 * i as f32 / n as f32 - 1.0;
        (0.4 * u + 0.6 * u * u * u) * LAND / 2.0
    };
    let at = |i: usize, j: usize| land.centre() + Vec2::new(along(i), along(j));
    let heights: Vec<f32> =
        (0..=n).flat_map(|j| (0..=n).map(move |i| (i, j))).map(|(i, j)| land.height(at(i, j))).collect();
    let point = |i: usize, j: usize| {
        let p = at(i, j);
        Vec3::new(p.x, heights[j * (n + 1) + i], p.y)
    };
    let (mut positions, mut normals, mut uvs, mut colours) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for j in 0..=n {
        for i in 0..=n {
            let here = point(i, j);
            // The slope, from the neighbours on each side.
            let across = point((i + 1).min(n), j) - point(i.saturating_sub(1), j);
            let down = point(i, (j + 1).min(n)) - point(i, j.saturating_sub(1));
            positions.push(here.to_array());
            normals.push(down.cross(across).normalize_or(Vec3::Y).to_array());
            uvs.push([here.x, here.z]);
            let normal = down.cross(across).normalize_or(Vec3::Y);
            let mut colour = paint(Vec2::new(here.x, here.z), here.y);
            let mut cut = mown(Vec2::new(here.x, here.z));
            // In mountains and canyons the steep ground is bare rock (D-077).
            if let Some(rock) = rock {
                let steep = smooth((1.0 - normal.y - 0.12) / 0.18);
                let streak = 0.85 + 0.3 * noise(Vec2::new(here.x, here.z) / 7.0 + here.y / 9.0);
                colour = colour.lerp(rock * streak, steep);
                cut *= 1.0 - steep;
            }
            colours.push(colour.extend(cut).to_array());
        }
    }
    let mut indices = Vec::with_capacity(n * n * 6);
    for j in 0..n {
        for i in 0..n {
            let corner = (j * (n + 1) + i) as u32;
            let (right, below) = (corner + 1, corner + n as u32 + 1);
            indices.extend([corner, below, right, right, below, below + 1]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
        .with_inserted_indices(Indices::U32(indices))
}

/// A chequered texture, `columns` by `rows` squares, drawn without blur.
pub fn chequers(columns: u32, rows: u32, light: [u8; 3], dark: [u8; 3]) -> Image {
    let squares: Vec<u8> = (0..rows)
        .flat_map(|j| {
            (0..columns).flat_map(move |i| if (i + j) % 2 == 0 { light } else { dark }.into_iter().chain([255]))
        })
        .collect();
    let size = Extent3d { width: columns, height: rows, depth_or_array_layers: 1 };
    let usage = RenderAssetUsages::default();
    let mut image = Image::new(size, TextureDimension::D2, squares, TextureFormat::Rgba8UnormSrgb, usage);
    image.sampler = ImageSampler::nearest();
    image
}

/// The relief of rippled water, as a normal map that repeats: a few waves
/// that each fit a whole number of times in the square.
fn ripples() -> Image {
    const SIDE: usize = 128;
    const WAVES: [(f32, f32, f32, f32); 6] = [
        (1.0, 2.0, 1.0, 0.3),
        (-2.0, 1.0, 0.8, 1.7),
        (3.0, 1.0, 0.5, 4.0),
        (1.0, -4.0, 0.4, 2.2),
        (5.0, 3.0, 0.22, 0.9),
        (-3.0, 6.0, 0.18, 5.1),
    ];
    let pixels = (0..SIDE * SIDE)
        .map(|i| {
            let (x, y) = ((i % SIDE) as f32 / SIDE as f32, (i / SIDE) as f32 / SIDE as f32);
            // The slope of the sum of the waves, by the derivative of each.
            let slope = WAVES.iter().fold(Vec2::ZERO, |slope, &(across, down, height, phase)| {
                let wave = (std::f32::consts::TAU * (across * x + down * y) + phase).cos();
                slope + Vec2::new(across, down) * height * wave
            });
            let normal = Vec3::new(-slope.x * 0.09, -slope.y * 0.09, 1.0).normalize();
            (normal * 0.5 + 0.5).to_array()
        })
        .collect();
    content::tiled(pixels, SIDE, SIDE, Kind::Normal)
}

/// With tangents when the mesh allows, which normal maps need.
fn with_tangents(mesh: Mesh) -> Mesh {
    let plain = mesh.clone();
    mesh.with_generated_tangents().unwrap_or(plain)
}

/// The water of the lake: its ripples drift.
#[derive(Resource)]
pub struct Water(Handle<StandardMaterial>);

pub fn flow(
    time: Res<Time>,
    water: Option<Res<Water>>,
    sea: Option<Res<Sea>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if let Some(mut material) = water.and_then(|water| materials.get_mut(&water.0)) {
        let drift = Vec2::new(0.013, 0.021) * time.elapsed_secs();
        material.uv_transform = Affine2::from_scale_angle_translation(Vec2::splat(9.0), 0.0, drift.fract());
    }
    // The sea's swell runs slower across its wider ripples.
    if let Some(mut material) = sea.and_then(|sea| materials.get_mut(&sea.0)) {
        let drift = Vec2::new(0.004, 0.009) * time.elapsed_secs();
        material.uv_transform = Affine2::from_scale_angle_translation(Vec2::splat(70.0), 0.0, drift.fract());
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build(
    mut commands: Commands,
    course: Res<Course>,
    old: Query<Entity, With<Part>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut grounds: ResMut<Assets<GroundMaterial>>,
    mut images: ResMut<Assets<Image>>,
    tier: Res<Tier>,
) {
    for part in &old {
        commands.entity(part).despawn();
    }
    commands.remove_resource::<Sea>();
    let track = &course.0.track;
    let frames = frames(track);
    // On a coast, the sea beside the land, below its cliffs (D-061); a
    // mountain's heights, a canyon's walls, gorges and ridges (D-077).
    let land = place(Land::new(&frames, track), &course.0);
    crate::spans::build(&mut commands, &mut meshes, &mut materials, &mut images, &course.0, &frames, &land);
    let (half, verge) = (track.half_width, track.verge);
    sea(&mut commands, &mut meshes, &mut materials, &mut images, &land);
    let assets = content::root();
    // The photographic set: the circuit as it would be photographed.
    if let Some(root) = assets.as_deref().filter(|root| root.join("polyhaven/asphalt_02").exists())
        && let Some(set) = Photographed::load(root, &mut images)
    {
        photographed(
            &mut commands,
            &mut meshes,
            &mut materials,
            &mut grounds,
            &mut images,
            &course.0,
            &frames,
            &land,
            set,
            *tier,
        );
        commands.insert_resource(land);
        return;
    }

    // Real surfaces if the manifest's textures are there. The asphalt is a
    // race track's, nearly black: lightened towards the blue-grey of a toy
    // circuit. Of the grass only the grain is kept: the land paints its own
    // colours under it.
    let asphalt = assets.as_deref().and_then(|root| {
        Surface::load(root, "asphalt_track", &mut images, |[r, g, b]| {
            let lift = |c: u8, gain: f32| (f32::from(c) * gain + 8.0).min(255.0) as u8;
            [lift(r, 2.5), lift(g, 2.6), lift(b, 2.8)]
        })
    });
    let grass = assets.as_deref().and_then(|root| Surface::load_grain(root, "sparse_grass", &mut images));
    let plain = |materials: &mut Assets<StandardMaterial>, base_color: Color, perceptual_roughness: f32| {
        materials.add(StandardMaterial { base_color, perceptual_roughness, ..default() })
    };
    let road_material = match &asphalt {
        Some(surface) => materials.add(surface.material(4.0)),
        None => plain(&mut materials, Color::srgb(0.3, 0.32, 0.36), 0.9),
    };
    // Under a grain, colours are painted brighter by as much as it darkens them.
    let (grass_material, paint) = match &grass {
        Some(surface) => (materials.add(surface.material(7.0)), 1.0 / GRAIN),
        None => (plain(&mut materials, Color::WHITE, 1.0), 1.0),
    };
    // What lies flat casts no shadow worth drawing.
    let mut flat = |mesh: Mesh, material: &Handle<StandardMaterial>| {
        commands.spawn((Part, Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone()), NotShadowCaster));
    };

    // The land, and a plain beyond it down to the horizon: four strips
    // around the land, never under it, where the lake goes below the plain.
    let rock = rock_of(&course.0.about.scenery).map(|rock| rock * paint);
    flat(
        with_tangents(land_mesh(&land, |at, height| colour(&land, at, height) * paint, |_| 1.0, rock)),
        &grass_material,
    );
    let plain_material = plain(&mut materials, Color::srgb(0.3, 0.58, 0.18), 1.0);
    let (edge, horizon) = (LAND / 2.0 - 1.0, 3000.0);
    for (low, high) in [
        (Vec2::new(-horizon, edge), Vec2::new(horizon, horizon)),
        (Vec2::new(-horizon, -horizon), Vec2::new(horizon, -edge)),
        (Vec2::new(edge, -edge), Vec2::new(horizon, edge)),
        (Vec2::new(-horizon, -edge), Vec2::new(-edge, edge)),
    ] {
        let Some((low, high)) = inland(&land, low, high) else { continue };
        let (size, middle) = (high - low, land.centre() + (low + high) / 2.0);
        let strip = Plane3d::default().mesh().size(size.x, size.y).build();
        flat(strip.translated_by(Vec3::new(middle.x, -0.4, middle.y)), &plain_material);
    }

    // The road, a little above the land.
    flat(with_tangents(ribbon(&frames, (-half, 0.02), (half, 0.02), (0.0, 1.0), |_| Some(WHITE))), &road_material);

    // The verges, mown in stripes, and a mown apron behind the barriers.
    let mown = |light: bool| {
        let [r, g, b, _] = if light { linear(0.47, 0.76, 0.26) } else { linear(0.33, 0.63, 0.19) };
        [r * paint, g * paint, b * paint, 1.0]
    };
    for side in [-1.0f32, 1.0] {
        let (kerb, wall) = (side * (half + 1.0), side * (half + verge + BARRIER));
        let stripes = |i: usize| Some(mown((i / 2).is_multiple_of(2)));
        flat(with_tangents(ribbon(&frames, (kerb, 0.012), (wall, 0.012), (0.0, 1.0), stripes)), &grass_material);
        let (behind, apron) = (wall + side * 0.7, wall + side * 4.0);
        let stripes = |i: usize| Some(mown(i / 2 % 2 == 1));
        flat(with_tangents(ribbon(&frames, (behind, 0.012), (apron, 0.012), (0.0, 1.0), stripes)), &grass_material);
    }

    // Paint: edge lines, a dashed centre line, the boxes of the starting
    // grid. Pulled towards the eye so that it never flickers against the road.
    let paint_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.93, 0.93, 0.9),
        perceptual_roughness: 0.55,
        depth_bias: 40.0,
        ..default()
    });
    for side in [-1.0, 1.0] {
        let (inner, outer) = (side * (half - 0.75), side * (half - 0.45));
        flat(ribbon(&frames, (inner, 0.03), (outer, 0.03), (0.0, 1.0), |_| Some(WHITE)), &paint_material);
    }
    let dashes = |i: usize| i.is_multiple_of(3).then_some(WHITE);
    flat(ribbon(&frames, (-0.13, 0.03), (0.13, 0.03), (0.0, 1.0), dashes), &paint_material);
    let mut boxes = Batch::default();
    let stroke = |low: Vec3, high: Vec3| {
        let corners = [low, Vec3::new(low.x, 0.0, high.z), high, Vec3::new(high.x, 0.0, low.z)];
        Shape::faces(&[&corners], false)
    };
    // An open box around the front of each slot: a line ahead, two beside.
    let marks = [
        stroke(Vec3::new(-1.3, 0.0, 1.5), Vec3::new(1.3, 0.0, 1.72)),
        stroke(Vec3::new(-1.3, 0.0, 0.2), Vec3::new(-1.08, 0.0, 1.5)),
        stroke(Vec3::new(1.08, 0.0, 0.2), Vec3::new(1.3, 0.0, 1.5)),
    ];
    for slot in 0..8 {
        let kart = course.0.grid_slot(slot);
        let place = Transform::from_xyz(kart.x, 0.032, kart.z).with_rotation(Quat::from_rotation_y(kart.heading));
        for mark in &marks {
            boxes.add(mark, place, Paint::plain(1.0, 1.0, 1.0));
        }
    }
    for mesh in boxes.meshes() {
        flat(mesh, &paint_material);
    }

    // Kerbs: red and white, rising away from the road.
    let coloured = materials.add(StandardMaterial { perceptual_roughness: 0.7, ..default() });
    let stripes = [linear(0.86, 0.12, 0.1), linear(0.95, 0.95, 0.95)];
    for side in [-1.0, 1.0] {
        let kerb =
            ribbon(&frames, (side * half, 0.025), (side * (half + 1.0), 0.11), (0.0, 1.0), |i| Some(stripes[i % 2]));
        flat(kerb, &coloured);
    }

    // Barriers: blocks with an inner face, a top and an outer face. The
    // physics stops a kart's centre at the edge of the verge: the barrier
    // stands a kart's half width further.
    let blocks = [linear(0.1, 0.36, 0.9), linear(0.96, 0.96, 0.98)];
    let block = |i: usize| Some(blocks[i / 2 % 2]);
    for side in [-1.0f32, 1.0] {
        let (inner, outer, top) = (side * (half + verge + BARRIER), side * (half + verge + BARRIER + 0.7), 0.85);
        for wall in [
            ribbon(&frames, (inner, 0.0), (inner, top), (-side, 0.0), block),
            ribbon(&frames, (inner, top), (outer, top), (0.0, 1.0), block),
            ribbon(&frames, (outer, top), (outer, 0.0), (side, 0.0), block),
        ] {
            commands.spawn((Part, Mesh3d(meshes.add(wall)), MeshMaterial3d(coloured.clone())));
        }
    }

    // Start/finish line: a chequered band across the road.
    let (line, heading) = track.at(course.0.start);
    let band = materials.add(StandardMaterial {
        base_color_texture: Some(images.add(chequers(16, 2, [250, 250, 250], [17, 17, 17]))),
        perceptual_roughness: 0.6,
        depth_bias: 60.0,
        ..default()
    });
    commands.spawn((
        Part,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(2.0 * half, 1.8))),
        MeshMaterial3d(band),
        Transform::from_xyz(line[0], 0.035, line[1]).with_rotation(Quat::from_rotation_y(heading)),
        NotShadowCaster,
    ));

    // The lake: water you see the sand through at the shore, the sky in further out.
    if let Some(lake) = land.lake {
        let water = materials.add(StandardMaterial {
            base_color: Color::srgba(0.1, 0.5, 0.74, 0.8),
            perceptual_roughness: 0.07,
            reflectance: 0.7,
            alpha_mode: AlphaMode::Blend,
            normal_map_texture: Some(images.add(ripples())),
            uv_transform: Affine2::from_scale(Vec2::splat(9.0)),
            ..default()
        });
        let surface = Plane3d::default().mesh().size(2.0 * lake.radius, 2.0 * lake.radius).build();
        commands.spawn((
            Part,
            Mesh3d(meshes.add(with_tangents(surface))),
            MeshMaterial3d(water.clone()),
            Transform::from_xyz(lake.centre.x, WATER, lake.centre.y),
            NotShadowCaster,
        ));
        commands.insert_resource(Water(water));
    }

    commands.insert_resource(land);
}

/// Lanes across the road's mesh: enough for the racing line to curve smoothly.
const LANES: usize = 12;

/// Where karts drive, metres right of the centre line, at each frame: a
/// racing line that cuts to the inside of every bend, from the bend's
/// curvature averaged over a dozen frames either side.
fn racing_line(frames: &[Frame], half: f32) -> Vec<f32> {
    let n = frames.len();
    (0..n)
        .map(|i| {
            let bend = (0..25).map(|k| frames[(i + n + k - 12) % n].bend).sum::<f32>() / 25.0;
            // A left bend (positive) draws the line to the left (negative).
            (-bend * 32.0).clamp(-0.55, 0.55) * half
        })
        .collect()
}

/// Skid marks through the bends: two dark tracks a kart's rear width apart,
/// along the racing line, fading in as a bend starts and out as it ends,
/// darker where the bend is sharper. Their darkness is in the vertices' alpha.
fn skid_marks(frames: &[Frame], half: f32) -> Mesh {
    let line = racing_line(frames, half);
    let n = frames.len();
    let (mut positions, mut colours) = (Vec::new(), Vec::new());
    // How hard each frame's bend is, averaged a little so that marks fade.
    let grip: Vec<f32> = (0..n)
        .map(|i| (0..9).map(|k| frames[(i + n + k - 4) % n].bend.abs()).sum::<f32>() / 9.0)
        .map(|bend| ((bend - 0.006) / 0.014).clamp(0.0, 1.0))
        .collect();
    for i in 0..n {
        let j = (i + 1) % n;
        if grip[i] <= 0.0 && grip[j] <= 0.0 {
            continue;
        }
        let (a, b) = (&frames[i], &frames[j]);
        for track in [-0.6f32, 0.6] {
            for wobble in [0.0f32, 0.18] {
                let width = 0.07;
                let at = |f: &Frame, lateral: f32| {
                    let p = f.at + f.right * lateral;
                    [p.x, f.height + 0.025, p.y]
                };
                let (la, lb) = (line[i] + track + wobble, line[j] + track + wobble);
                let quad = [at(a, la - width), at(b, lb - width), at(b, lb + width), at(a, la + width)];
                let alpha = |g: f32| [0.0, 0.0, 0.0, 0.55 * g * if wobble > 0.0 { 0.5 } else { 1.0 }];
                let shade = [alpha(grip[i]), alpha(grip[j]), alpha(grip[j]), alpha(grip[i])];
                for corner in [0, 1, 3, 3, 1, 2] {
                    positions.push(quad[corner]);
                    colours.push(shade[corner]);
                }
            }
        }
    }
    let count = positions.len() as u32;
    let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
        .with_inserted_indices(Indices::U32((0..count).collect()));
    facing_up(mesh)
}

/// The road as a grid, `LANES` across, its texture continuous, its colour
/// darkened where tyres have laid rubber: along the racing line, most at its
/// middle, in patches along the way.
fn road_mesh(frames: &[Frame], half: f32) -> Mesh {
    let line = racing_line(frames, half);
    let n = frames.len();
    let (mut positions, mut normals, mut uvs, mut colours, mut indices) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    // Rows across the road: one at each frame, more over a crest; then a
    // last one that repeats the first, with its texture further on.
    let mut rows = Vec::new();
    for (i, a) in frames.iter().enumerate() {
        let b = &frames[(i + 1) % n];
        let length = a.at.distance(b.at);
        let pieces = a.pieces(length);
        for piece in 0..pieces {
            let t = piece as f32 / pieces as f32;
            let tilt = (a.height_towards(t + 0.01, length) - a.height_towards(t, length)) / (0.01 * length);
            let forward = (b.at - a.at).normalize_or(a.forward);
            let normal = Vec3::new(-forward.x * tilt, 1.0, -forward.y * tilt).normalize();
            let right = a.right.lerp(b.right, t).normalize_or(a.right);
            rows.push((
                a.at.lerp(b.at, t),
                right,
                a.along + t * length,
                a.height_towards(t, length),
                normal,
                line[i] + (line[(i + 1) % n] - line[i]) * t,
            ));
        }
    }
    let total = frames.last().map_or(0.0, |f| f.along + f.at.distance(frames[0].at));
    rows.push(rows[0]);
    rows.last_mut().expect("a row").2 = total;
    let count = rows.len() - 1;
    for &(at, right, along, height, normal, line) in &rows {
        let patchy = 0.75 + 0.5 * noise(Vec2::new(along / 9.0, 3.0));
        for j in 0..=LANES {
            let lateral = -half + 2.0 * half * j as f32 / LANES as f32;
            let p = at + right * lateral;
            positions.push([p.x, height + 0.02, p.y]);
            normals.push(normal.to_array());
            uvs.push([lateral + half, along]);
            let off = (lateral - line) / half;
            let rubber = (0.3 * (-(off / 0.3).powi(2)).exp() + 0.08 * (-(off / 0.7).powi(2)).exp()) * patchy;
            let shade = (1.0 - rubber).clamp(0.5, 1.05);
            colours.push([shade, shade, shade * 1.01, 1.0]);
        }
    }
    let row = (LANES + 1) as u32;
    for i in 0..count as u32 {
        for j in 0..LANES as u32 {
            let (a, b) = (i * row + j, i * row + j + 1);
            let (c, d) = (a + row, b + row);
            // Counter-clockwise seen from above: ahead is +along, right is +lateral.
            indices.extend([a, b, c, b, d, c]);
        }
    }
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
        .with_inserted_indices(Indices::U32(indices));
    facing_up(mesh)
}

/// The mesh with its triangles wound to face up, whichever way it was built.
fn facing_up(mut mesh: Mesh) -> Mesh {
    let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION).cloned()
    else {
        return mesh;
    };
    if let Some(Indices::U32(indices)) = mesh.indices_mut() {
        for triangle in indices.as_chunks_mut::<3>().0 {
            let [a, b, c] = triangle.map(|i| Vec3::from(points[i as usize]));
            if (b - a).cross(c - a).y < 0.0 {
                triangle.swap(1, 2);
            }
        }
    }
    mesh
}

/// The grain for what is built around the photographed circuit: concrete,
/// laid on the scenery's batches.
#[derive(Resource, Clone)]
pub struct Grains {
    pub concrete: Surface,
}

/// The surfaces of a photographed circuit, from the asset manifest's 2k sets.
pub struct Photographed {
    asphalt: Surface,
    grass: Surface,
    gravel: Surface,
    /// Grains, under painted colours.
    paint: Surface,
    concrete: Surface,
    /// Rock for steep ground (D-081), when the content has it.
    rock: Option<Surface>,
}

impl Photographed {
    fn load(root: &std::path::Path, images: &mut Assets<Image>) -> Option<Self> {
        Some(Self {
            asphalt: Surface::load(root, "asphalt_02", images, |p| p)?,
            grass: Surface::load_grain(root, "sparse_grass", images)?,
            gravel: Surface::load(root, "gravel_floor_02", images, |p| p)?,
            // Painted walls and kerbs: the fine grain of cast concrete under
            // the paint (D-092; the peeling-paint photograph's patches read
            // as stains at a distance), else the painted concrete's.
            paint: Surface::load_grain(root, "concrete_wall_008", images)
                .or_else(|| Surface::load_grain(root, "painted_concrete", images))?,
            concrete: Surface::load_grain(root, "concrete_floor_02", images)?,
            rock: Surface::load_grain(root, "rock_face_03", images),
        })
    }
}

/// The circuit as it would be photographed: real asphalt, grass and gravel,
/// kerbs and walls of painted concrete. The same shapes as the toy circuit.
#[allow(clippy::too_many_arguments)]
fn photographed(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    grounds: &mut Assets<GroundMaterial>,
    images: &mut Assets<Image>,
    circuit: &osage_world_kart::Circuit,
    frames: &[Frame],
    land: &Land,
    set: Photographed,
    tier: Tier,
) {
    let track = &circuit.track;
    let (half, verge) = (track.half_width, track.verge);
    commands.insert_resource(Grains { concrete: set.concrete.clone() });
    // Grains sampled without a tile (V1.0: nothing repeats): the road's
    // from T2, the grass and gravel's, which fill most of the picture, from
    // T3 (on an Iris Xe at T2: the road 1.2 ms a frame, all of them 1.7).
    let mut grain = |material: StandardMaterial, tile: f32, ground: Ground, untiled: bool| {
        let (base, colour) = untextured(material);
        let extension = match colour {
            Some(colour) => ground.with_grain(colour, tile, untiled),
            None => ground,
        };
        grounds.add(GroundMaterial { base, extension })
    };
    let (road_untiled, ground_untiled) = (tier >= Tier::T2, tier >= Tier::T3);
    // The road: its history along its length — paving seams, repaired
    // patches, cracks, stains — in stretches of 40 m.
    let road = grain(set.asphalt.material(5.0), 5.0, Ground::asphalt().paved(1.0, 40.0, 2.0 * half), road_untiled);
    // Grass varies in clumps and patches, so that its grain never reads as a carpet;
    // steep ground is rock (D-081), in strata: a canyon's sandstone in
    // bold bands, a mountain's schist in faint ones.
    let land_ground = match set.rock.as_ref().map(Surface::colour) {
        Some(rock) => {
            let (strata, spacing) = if circuit.about.scenery == "canyon" { (0.45, 2.2) } else { (0.18, 1.4) };
            Ground::meadow().with_rock(rock, 5.0, strata, spacing)
        }
        None => Ground::meadow(),
    };
    // Mown in stripes 4.5 m wide, the mower going along the land's long side.
    let grass = grain(set.grass.material(2.6), 2.6, land_ground.mown(0.2, 4.5, Vec2::new(0.6, 0.8)), ground_untiled);
    // Mown grass varies less: clumps only, and faint patches.
    let mown = Ground { variation: Vec4::new(0.3, 0.35, 1.2, 18.0), ..Ground::meadow() };
    let mown_grass = grain(set.grass.material(2.6), 2.6, mown, ground_untiled);
    let gravel = grain(set.gravel.material(2.4), 2.4, Ground::none(), ground_untiled);
    let painted = materials.add(StandardMaterial { perceptual_roughness: 0.75, ..set.paint.material(2.0) });
    let concrete = materials.add(set.concrete.material(3.0));
    // What the rain wets (D-068): the road, the painted kerbs and walls.
    let dry = |m: &StandardMaterial| (m.base_color.to_linear(), m.perceptual_roughness);
    let mut wets = crate::rain::Surfaces::default();
    if let Some(m) = grounds.get(&road) {
        let (colour, rough) = dry(&m.base);
        wets.grounds.push((road.clone(), colour, rough));
    }
    for handle in [&painted, &concrete] {
        if let Some(m) = materials.get(handle) {
            let (colour, rough) = dry(m);
            wets.plain.push((handle.clone(), colour, rough));
        }
    }
    commands.insert_resource(wets);
    let mut flat = |mesh: Mesh, material: &Handle<StandardMaterial>| {
        commands.spawn((Part, Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone()), NotShadowCaster));
    };

    // Laid once `flat` is done with the commands.
    let mut grassed: Vec<(Mesh, &Handle<GroundMaterial>)> = Vec::new();
    let rock = rock_of(&circuit.about.scenery).map(|rock| rock / GRAIN);
    // Near the circuit the grass is mown, in stripes (D-101); a few
    // metres past the barriers no one mows: meadow. Not on a shore or a
    // cliff.
    let fenced = half + verge + BARRIER + 6.0;
    let mown = |at: Vec2| {
        let shore = land.shore(at).map_or(1.0, |shore| smooth((shore - 1.3) / 0.2));
        smooth((fenced - land.distance_to_track(at)) / 3.0) * shore * (1.0 - cliff(land, at))
    };
    grassed.push((with_tangents(land_mesh(land, |at, height| meadow(land, at, height), mown, rock)), &grass));
    // Beyond the land, fields as far as the haze.
    let fields = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 0.25, 0.12),
        perceptual_roughness: 1.0,
        ..default()
    });
    let (edge, horizon) = (LAND / 2.0 - 1.0, 3000.0);
    for (low, high) in [
        (Vec2::new(-horizon, edge), Vec2::new(horizon, horizon)),
        (Vec2::new(-horizon, -horizon), Vec2::new(horizon, -edge)),
        (Vec2::new(edge, -edge), Vec2::new(horizon, edge)),
        (Vec2::new(-horizon, -edge), Vec2::new(-edge, edge)),
    ] {
        let Some((low, high)) = inland(land, low, high) else { continue };
        let (size, middle) = (high - low, land.centre() + (low + high) / 2.0);
        let strip = Plane3d::default().mesh().size(size.x, size.y).build();
        flat(strip.translated_by(Vec3::new(middle.x, -0.4, middle.y)), &fields);
    }

    grassed.push((with_tangents(road_mesh(frames, half)), &road));
    // Skid marks through the bends, dark and half worn away.
    let marks = materials.add(StandardMaterial {
        base_color: Color::srgba(0.02, 0.02, 0.02, 1.0),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.6,
        depth_bias: 30.0,
        ..default()
    });
    flat(skid_marks(frames, half), &marks);

    // Verges of mown grass, in faint stripes; on the outside of a bend, a
    // gravel trap in front of the tyre wall instead.
    let mown = |light: bool| {
        let [r, g, b, _] = if light { linear(0.3, 0.4, 0.16) } else { linear(0.25, 0.35, 0.14) };
        [r / GRAIN, g / GRAIN, b / GRAIN, 1.0]
    };
    for side in [-1.0f32, 1.0] {
        let outer_bend = |i: usize| frames[i].bend.abs() > 0.011 && frames[i].bend.signum() == side;
        let (kerb, wall) = (side * (half + 1.0), side * (half + verge + BARRIER));
        let stripes = |i: usize| (!outer_bend(i)).then(|| mown((i / 2).is_multiple_of(2)));
        grassed.push((with_tangents(ribbon(frames, (kerb, 0.012), (wall, 0.012), (0.0, 1.0), stripes)), &mown_grass));
        let trap = |i: usize| outer_bend(i).then_some(WHITE);
        grassed.push((with_tangents(ribbon(frames, (kerb, 0.015), (wall, 0.015), (0.0, 1.0), trap)), &gravel));
        let (behind, apron) = (wall + side * 0.7, wall + side * 4.0);
        let stripes = |i: usize| Some(mown(i / 2 % 2 == 1));
        grassed
            .push((with_tangents(ribbon(frames, (behind, 0.012), (apron, 0.012), (0.0, 1.0), stripes)), &mown_grass));
    }

    // Road markings, worn into the asphalt: the worn paint's grain, as the
    // kerbs have it, so that no line looks freshly painted.
    let white = Color::srgb(0.8, 0.8, 0.77).to_linear();
    let marking = materials.add(StandardMaterial {
        base_color: Color::linear_rgb(white.red / GRAIN, white.green / GRAIN, white.blue / GRAIN),
        perceptual_roughness: 0.62,
        metallic_roughness_texture: None,
        depth_bias: 40.0,
        ..set.paint.material(1.5)
    });
    for side in [-1.0, 1.0] {
        let (inner, outer) = (side * (half - 0.75), side * (half - 0.45));
        flat(with_tangents(ribbon(frames, (inner, 0.03), (outer, 0.03), (0.0, 1.0), |_| Some(WHITE))), &marking);
    }
    let mut boxes = Batch::default();
    let stroke = |low: Vec3, high: Vec3| {
        let corners = [low, Vec3::new(low.x, 0.0, high.z), high, Vec3::new(high.x, 0.0, low.z)];
        Shape::faces(&[&corners], false)
    };
    let marks = [
        stroke(Vec3::new(-1.3, 0.0, 1.5), Vec3::new(1.3, 0.0, 1.72)),
        stroke(Vec3::new(-1.3, 0.0, 0.2), Vec3::new(-1.08, 0.0, 1.5)),
        stroke(Vec3::new(1.08, 0.0, 0.2), Vec3::new(1.3, 0.0, 1.5)),
    ];
    for slot in 0..8 {
        let kart = circuit.grid_slot(slot);
        let place = Transform::from_xyz(kart.x, 0.032, kart.z).with_rotation(Quat::from_rotation_y(kart.heading));
        for mark in &marks {
            boxes.add(mark, place, Paint::plain(1.0, 1.0, 1.0));
        }
    }
    for mesh in boxes.meshes() {
        flat(with_tangents(mesh), &marking);
    }

    // Kerbs of painted concrete, red and white; colours brighter under a grain.
    let lift = 1.0 / GRAIN;
    let red = linear(0.62, 0.08, 0.06).map(|c| c * lift);
    let white = linear(0.82, 0.82, 0.8).map(|c| c * lift);
    for side in [-1.0, 1.0] {
        let kerb = ribbon(frames, (side * half, 0.025), (side * (half + 1.0), 0.11), (0.0, 1.0), |i| {
            Some(if i % 2 == 0 { red } else { white })
        });
        flat(with_tangents(kerb), &painted);
    }

    // Walls of concrete, painted red and white on the side the karts see.
    let grey = linear(0.55, 0.55, 0.53).map(|c| c * lift);
    for side in [-1.0f32, 1.0] {
        let (inner, outer, top) = (side * (half + verge + BARRIER), side * (half + verge + BARRIER + 0.7), 0.85);
        let face = |i: usize| Some(if (i / 2).is_multiple_of(2) { red } else { white });
        // Grime at the foot of the face the karts see (D-106): the spray
        // and dust of the track, browner and darker at the ground, fading
        // out by a third of the way up; more on some stretches than on
        // others, and most on the outside of the bends, where karts brush it.
        let foot = 0.3;
        let grime = |i: usize| {
            let bend = if frames[i].bend.signum() == side { (frames[i].bend.abs() / 0.02).min(1.0) } else { 0.0 };
            let patch = 0.5 + 0.5 * noise(frames[i].at / 9.0 + side * 13.0);
            let dirt = (0.4 + 0.35 * patch + 0.25 * bend).min(0.9);
            let tone = [1.0 - dirt * 0.66, 1.0 - dirt * 0.7, 1.0 - dirt * 0.76];
            (tone, [1.0 - dirt * 0.12, 1.0 - dirt * 0.13, 1.0 - dirt * 0.15])
        };
        let clean = |i: usize| (grime(i).1, [1.0; 3]);
        for (wall, colours) in [
            (ribbon_shaded(frames, (inner, 0.0), (inner, foot), (-side, 0.0), face, grime), true),
            (ribbon_shaded(frames, (inner, foot), (inner, top), (-side, 0.0), face, clean), true),
            (ribbon(frames, (inner, top), (outer, top), (0.0, 1.0), |_| Some(grey)), false),
            (ribbon(frames, (outer, top), (outer, 0.0), (side, 0.0), |_| Some(grey)), false),
        ] {
            let material = if colours { painted.clone() } else { concrete.clone() };
            commands.spawn((Part, Mesh3d(meshes.add(with_tangents(wall))), MeshMaterial3d(material)));
        }
    }

    // Start/finish line.
    let (line, heading) = track.at(circuit.start);
    let band = materials.add(StandardMaterial {
        base_color_texture: Some(images.add(chequers(16, 2, [225, 225, 220], [20, 20, 20]))),
        perceptual_roughness: 0.65,
        depth_bias: 60.0,
        ..default()
    });
    commands.spawn((
        Part,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(2.0 * half, 1.8))),
        MeshMaterial3d(band),
        Transform::from_xyz(line[0], 0.035, line[1]).with_rotation(Quat::from_rotation_y(heading)),
        NotShadowCaster,
    ));

    // The lake: clear water over the sand, the sky mirrored further out.
    if let Some(lake) = land.lake {
        let water = materials.add(StandardMaterial {
            base_color: Color::srgba(0.05, 0.16, 0.2, 0.75),
            perceptual_roughness: 0.04,
            reflectance: 0.6,
            alpha_mode: AlphaMode::Blend,
            normal_map_texture: Some(images.add(ripples())),
            uv_transform: Affine2::from_scale(Vec2::splat(9.0)),
            ..default()
        });
        let surface = Plane3d::default().mesh().size(2.0 * lake.radius, 2.0 * lake.radius).build();
        commands.spawn((
            Part,
            Mesh3d(meshes.add(with_tangents(surface))),
            MeshMaterial3d(water.clone()),
            Transform::from_xyz(lake.centre.x, WATER, lake.centre.y),
            NotShadowCaster,
        ));
        commands.insert_resource(Water(water));
    }
    for (mesh, material) in grassed {
        commands.spawn((Part, Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone()), NotShadowCaster));
    }
}

/// How far a cloud's shadow darkens the ground on a fair day (D-108).
const CLOUD_SHADE: f32 = 0.3;
/// The wind that carries the clouds, m/s: a light breeze aloft.
const WIND: Vec2 = Vec2::new(4.5, 1.6);

/// The shadows of fair-weather clouds drift over the ground on the wind
/// (D-108): every ground material is told the wind and how dark the
/// shadows are — none at night, and none in the rain, whose overcast sky
/// casts no shadow of its own. A material is touched only when that
/// changes (touching one makes the renderer prepare it again: doing it
/// every frame lost the GPU at T3 on the Iris Xe); the shader moves the
/// shadows by the engine's clock.
pub fn drift_clouds(
    shown: Option<Res<crate::scene::SkyShown>>,
    rain: Option<Res<crate::rain::Rain>>,
    mut grounds: ResMut<Assets<osage_render::ground::GroundMaterial>>,
) {
    let night = shown.is_some_and(|shown| shown.sky == crate::scene::Sky::Night);
    let wet = rain.map_or(0.0, |rain| rain.wet());
    let shade = if night { 0.0 } else { CLOUD_SHADE * (1.0 - wet) };
    let wanted = Vec4::new(shade, 180.0, WIND.x, WIND.y);
    let stale: Vec<_> = grounds
        .iter()
        .filter(|(_, ground)| (ground.extension.clouds - wanted).abs().max_element() > 0.004)
        .map(|(id, _)| id)
        .collect();
    for id in stale {
        if let Some(mut ground) = grounds.get_mut(id) {
            ground.extension.clouds = wanted;
        }
    }
}
