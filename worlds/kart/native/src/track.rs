// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The circuit, built from the centre line the world announces: the road and
//! its markings, kerbs, barriers, the start line, and the land around —
//! flat along the track, hills beyond. With the manifest's textures the road
//! and the grass are real surfaces; without, plain colours.

use bevy::{
    asset::RenderAssetUsages,
    color::ColorToComponents,
    image::ImageSampler,
    mesh::Indices,
    prelude::*,
    render::render_resource::{Extent3d, PrimitiveTopology, TextureDimension, TextureFormat},
};
use osage_kart_physics::Track;

use crate::{
    content::{self, Surface},
    scene::{Course, Part},
};

/// Side of the square of land modelled around the circuit, metres.
const LAND: f32 = 900.0;
/// Size of a cell of the land's grid, metres.
const CELL: f32 = 9.0;
/// Highest hills, metres.
const HILLS: f32 = 26.0;
/// How far beyond the verge the barriers stand, metres.
pub const BARRIER: f32 = 0.9;

/// A point of the centre line with the unit vectors ahead and to the right.
pub struct Frame {
    pub at: Vec2,
    pub forward: Vec2,
    pub right: Vec2,
    /// Distance from the first point, along the centre line.
    pub along: f32,
    /// How sharply the road turns here, radians per metre; positive: to the left.
    pub bend: f32,
}

pub fn frames(track: &Track) -> Vec<Frame> {
    let points = track.points();
    let n = points.len();
    let at = |i: usize| Vec2::from(points[i % n]);
    let mut along = 0.0;
    (0..n)
        .map(|i| {
            let (before, here, after) = (at(i + n - 1), at(i), at(i + 1));
            let forward = (after - before).normalize_or_zero();
            let (arriving, leaving) = ((here - before).normalize_or_zero(), (after - here).normalize_or_zero());
            // Heading grows to the left: with forward (sin h, cos h), a left turn has a negative cross product.
            let bend = -arriving.perp_dot(leaving).asin() / here.distance(after).max(0.1);
            let frame = Frame { at: here, forward, right: Vec2::new(-forward.y, forward.x), along, bend };
            along += here.distance(after);
            frame
        })
        .collect()
}

/// The land's height at a point, and how far the point is from the track.
#[derive(Resource)]
pub struct Land {
    centre: Vec2,
    points: Vec<Vec2>,
    /// Beyond this distance from the centre line, the land may rise.
    flat: f32,
}

impl Land {
    fn new(frames: &[Frame], track: &Track) -> Self {
        let points: Vec<Vec2> = frames.iter().map(|f| f.at).collect();
        let (low, high) = points.iter().fold((Vec2::MAX, Vec2::MIN), |(low, high), p| (low.min(*p), high.max(*p)));
        Self { centre: (low + high) / 2.0, points, flat: track.half_width + track.verge + 5.0 }
    }

    /// Distance from the nearest point of the centre line.
    pub fn distance_to_track(&self, at: Vec2) -> f32 {
        self.points.iter().map(|p| p.distance_squared(at)).fold(f32::MAX, f32::min).sqrt()
    }

    /// Height of the ground: zero along the track, hills beyond, zero again
    /// at the edge of the modelled land.
    pub fn height(&self, at: Vec2) -> f32 {
        let rise = smooth((self.distance_to_track(at) - self.flat) / 70.0);
        let edge = (at - self.centre).abs().max_element();
        let fade = smooth((LAND / 2.0 - edge) / 120.0);
        let hills = 0.5 * noise(at / 150.0) + 0.3 * noise(at / 61.0 + 17.0) + 0.2 * noise(at / 23.0 + 5.0);
        HILLS * rise * fade * hills
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Value noise in 0..1, the same on every machine.
fn noise(p: Vec2) -> f32 {
    let cell = p.floor();
    let t = (p - cell).map(|t| t * t * (3.0 - 2.0 * t));
    let corner = |dx: f32, dy: f32| {
        let (x, y) = ((cell.x + dx) as i32 as u32, (cell.y + dy) as i32 as u32);
        let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
        h = (h ^ (h >> 15)).wrapping_mul(0x2C1B_3C6D);
        ((h ^ (h >> 13)) & 0xffff) as f32 / 65535.0
    };
    let (low, high) = (corner(0.0, 0.0).lerp(corner(1.0, 0.0), t.x), corner(0.0, 1.0).lerp(corner(1.0, 1.0), t.x));
    low.lerp(high, t.y)
}

fn linear(r: f32, g: f32, b: f32) -> [f32; 4] {
    Color::srgb(r, g, b).to_linear().to_f32_array()
}

const WHITE: [f32; 4] = [1.0; 4];

/// A ribbon along the track between two points of its cross-section, each
/// given as (metres right of the centre line, height). `facing` says which
/// way the ribbon faces, in the same terms: (0, 1) up, (-1, 0) towards the
/// left. `colour` paints each segment, or leaves it out.
fn ribbon(
    frames: &[Frame],
    from: (f32, f32),
    to: (f32, f32),
    facing: (f32, f32),
    colour: impl Fn(usize) -> Option<[f32; 4]>,
) -> Mesh {
    let (mut positions, mut normals, mut uvs, mut colours) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let across = Vec2::new(to.0 - from.0, to.1 - from.1).length();
    for (i, a) in frames.iter().enumerate() {
        let Some(colour) = colour(i) else { continue };
        let b = &frames[(i + 1) % frames.len()];
        let ahead = if b.along > a.along { b.along } else { a.along + a.at.distance(b.at) };
        let corner = |f: &Frame, (lateral, height): (f32, f32)| {
            let p = f.at + f.right * lateral;
            Vec3::new(p.x, height, p.y)
        };
        let quad = [corner(a, from), corner(b, from), corner(b, to), corner(a, to)];
        let quad_uvs = [[0.0, a.along], [0.0, ahead], [across, ahead], [across, a.along]];
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
            colours.push(colour);
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

/// The land as a grid: flat along the track, hills beyond.
fn land_mesh(land: &Land) -> Mesh {
    let n = (LAND / CELL) as usize;
    let origin = land.centre - Vec2::splat(LAND / 2.0);
    let at = |i: usize, j: usize| origin + Vec2::new(i as f32, j as f32) * CELL;
    let (mut positions, mut normals, mut uvs) = (Vec::new(), Vec::new(), Vec::new());
    for j in 0..=n {
        for i in 0..=n {
            let p = at(i, j);
            let h = |d: Vec2| land.height(p + d);
            positions.push([p.x, land.height(p), p.y]);
            // The slope, from the heights a little away on each side.
            let slope = Vec3::new(h(Vec2::NEG_X) - h(Vec2::X), 2.0, h(Vec2::NEG_Y) - h(Vec2::Y));
            normals.push(slope.normalize().to_array());
            uvs.push([p.x, p.y]);
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

/// With tangents when the mesh allows, which normal maps need.
fn with_tangents(mesh: Mesh) -> Mesh {
    let plain = mesh.clone();
    mesh.with_generated_tangents().unwrap_or(plain)
}

pub fn build(
    mut commands: Commands,
    course: Res<Course>,
    old: Query<Entity, With<Part>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    for part in &old {
        commands.entity(part).despawn();
    }
    let track = &course.0.track;
    let frames = frames(track);
    let land = Land::new(&frames, track);
    let (half, verge) = (track.half_width, track.verge);
    let assets = content::root();

    // Real surfaces if the manifest's textures are there. The asphalt is a
    // race track's, nearly black: lightened towards the blue-grey of a toy
    // circuit. The grass is sparse on soil: its grain kept, its colour made lush.
    let asphalt = assets.as_deref().and_then(|root| {
        Surface::load(root, "asphalt_track", &mut images, |[r, g, b]| {
            let lift = |c: u8, gain: f32| (f32::from(c) * gain + 8.0).min(255.0) as u8;
            [lift(r, 2.5), lift(g, 2.6), lift(b, 2.8)]
        })
    });
    let grass = assets.as_deref().and_then(|root| {
        Surface::load(root, "sparse_grass", &mut images, |[r, g, b]| {
            let light = (0.3 * f32::from(r) + 0.59 * f32::from(g) + 0.11 * f32::from(b)) / 255.0;
            let shade = 0.55 + 1.5 * light;
            [58.0, 142.0, 40.0].map(|c: f32| (c * shade).min(255.0) as u8)
        })
    });
    let plain = |materials: &mut Assets<StandardMaterial>, base_color: Color, perceptual_roughness: f32| {
        materials.add(StandardMaterial { base_color, perceptual_roughness, ..default() })
    };
    let road_material = match &asphalt {
        Some(surface) => materials.add(surface.material(4.0)),
        None => plain(&mut materials, Color::srgb(0.3, 0.32, 0.36), 0.9),
    };
    let grass_material = match &grass {
        Some(surface) => materials.add(surface.material(7.0)),
        None => plain(&mut materials, Color::srgb(0.23, 0.5, 0.16), 1.0),
    };
    let mut part = |mesh: Mesh, material: &Handle<StandardMaterial>| {
        commands.spawn((Part, Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone())));
    };

    // The land, and a plain beyond it down to the horizon.
    part(with_tangents(land_mesh(&land)), &grass_material);
    let far = Plane3d::default().mesh().size(6000.0, 6000.0).build().translated_by(Vec3::new(0.0, -0.4, 0.0));
    part(far, &plain(&mut materials, Color::srgb(0.2, 0.44, 0.15), 1.0));

    // The road, a little above the land.
    part(with_tangents(ribbon(&frames, (-half, 0.02), (half, 0.02), (0.0, 1.0), |_| Some(WHITE))), &road_material);

    // Paint: edge lines, a dashed centre line. Pulled towards the eye so
    // that it never flickers against the road.
    let paint = materials.add(StandardMaterial {
        base_color: Color::srgb(0.93, 0.93, 0.9),
        perceptual_roughness: 0.55,
        depth_bias: 40.0,
        ..default()
    });
    for side in [-1.0, 1.0] {
        let (inner, outer) = (side * (half - 0.75), side * (half - 0.45));
        part(ribbon(&frames, (inner, 0.03), (outer, 0.03), (0.0, 1.0), |_| Some(WHITE)), &paint);
    }
    part(ribbon(&frames, (-0.13, 0.03), (0.13, 0.03), (0.0, 1.0), |i| (i % 3 == 0).then_some(WHITE)), &paint);

    // Kerbs: red and white, rising away from the road.
    let coloured = materials.add(StandardMaterial { perceptual_roughness: 0.7, ..default() });
    let stripes = [linear(0.86, 0.12, 0.1), linear(0.95, 0.95, 0.95)];
    for side in [-1.0, 1.0] {
        let kerb =
            ribbon(&frames, (side * half, 0.025), (side * (half + 1.0), 0.11), (0.0, 1.0), |i| Some(stripes[i % 2]));
        part(kerb, &coloured);
    }

    // Barriers: blocks with an inner face, a top and an outer face. The
    // physics stops a kart's centre at the edge of the verge: the barrier
    // stands a kart's half width further.
    let blocks = [linear(0.1, 0.36, 0.9), linear(0.96, 0.96, 0.98)];
    let block = |i: usize| Some(blocks[i / 2 % 2]);
    for side in [-1.0f32, 1.0] {
        let (inner, outer, top) = (side * (half + verge + BARRIER), side * (half + verge + BARRIER + 0.7), 0.85);
        part(ribbon(&frames, (inner, 0.0), (inner, top), (-side, 0.0), block), &coloured);
        part(ribbon(&frames, (inner, top), (outer, top), (0.0, 1.0), block), &coloured);
        part(ribbon(&frames, (outer, top), (outer, 0.0), (side, 0.0), block), &coloured);
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
    ));

    commands.insert_resource(land);
}
