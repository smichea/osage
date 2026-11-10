// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The circuit, built from the centre line the world announces: the road and
//! its markings, kerbs, mown verges, barriers, the start line and its grid,
//! and the land around — flat along the track, hills beyond, a lake and a
//! knoll in the room the circuit leaves inside its loop. The ground is
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
use osage_kart_physics::Track;

use crate::{
    content::{self, GRAIN, Kind, Surface},
    scene::{Course, GRASS, Part},
    sculpt::{Batch, Paint, Shape},
};

/// Side of the square of land modelled around the circuit, metres.
const LAND: f32 = 900.0;
/// Cells along each side of the land's grid: small around the circuit, large far away.
const CELLS: usize = 200;
/// Highest hills, metres.
const HILLS: f32 = 26.0;
/// Height of the knoll inside the circuit, metres.
const KNOLL: f32 = 7.0;
/// Depth of the lake's bed, and the level of its water, metres.
const LAKE_BED: f32 = 2.4;
pub const WATER: f32 = -0.45;
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

/// Room clear of the track: a disc.
#[derive(Clone, Copy, Debug)]
pub struct Site {
    pub centre: Vec2,
    pub radius: f32,
}

/// The land: its height and its colour at any point, how far a point is from
/// the track, and the two places the circuit leaves room for inside its loop.
#[derive(Resource)]
pub struct Land {
    centre: Vec2,
    points: Vec<Vec2>,
    /// Beyond this distance from the centre line, the land may rise.
    flat: f32,
    /// A knoll in the widest place inside the circuit: where a landmark stands.
    pub knoll: Option<Site>,
    /// A lake in the next widest place.
    pub lake: Option<Site>,
}

impl Land {
    fn new(frames: &[Frame], track: &Track) -> Self {
        let points: Vec<Vec2> = frames.iter().map(|f| f.at).collect();
        let (low, high) = points.iter().fold((Vec2::MAX, Vec2::MIN), |(low, high), p| (low.min(*p), high.max(*p)));
        let flat = track.half_width + track.verge + 5.0;
        let mut land = Self { centre: (low + high) / 2.0, points, flat, knoll: None, lake: None };

        // The rooms inside the loop, widest first: every few metres, how far the track is.
        let mut rooms = Vec::new();
        let (columns, rows) = (((high.x - low.x) / 4.0) as usize, ((high.y - low.y) / 4.0) as usize);
        for j in 0..=rows {
            for i in 0..=columns {
                let centre = low + Vec2::new(i as f32, j as f32) * 4.0;
                if land.inside(centre) {
                    rooms.push(Site { centre, radius: land.distance_to_track(centre) });
                }
            }
        }
        rooms.sort_by(|a, b| b.radius.total_cmp(&a.radius));
        // What is built there keeps clear of the flat land along the track.
        let clear = |room: &Site, most: f32| Site { centre: room.centre, radius: (room.radius - flat - 6.0).min(most) };
        if let Some(widest) = rooms.first().filter(|room| room.radius > flat + 22.0) {
            land.knoll = Some(clear(widest, 30.0));
            let apart = |room: &&Site| room.centre.distance(widest.centre) > widest.radius + 0.8 * room.radius;
            land.lake = rooms.iter().find(apart).filter(|room| room.radius > flat + 18.0).map(|room| clear(room, 26.0));
        }
        land
    }

    /// Distance from the nearest point of the centre line.
    pub fn distance_to_track(&self, at: Vec2) -> f32 {
        self.points.iter().map(|p| p.distance_squared(at)).fold(f32::MAX, f32::min).sqrt()
    }

    /// Whether a point is inside the loop of the circuit.
    pub fn inside(&self, at: Vec2) -> bool {
        let n = self.points.len();
        let crossings = (0..n).filter(|&i| {
            let (a, b) = (self.points[i], self.points[(i + 1) % n]);
            (a.y > at.y) != (b.y > at.y) && at.x < a.x + (b.x - a.x) * (at.y - a.y) / (b.y - a.y)
        });
        crossings.count() % 2 == 1
    }

    /// Where a point is relative to the lake's shore: under 1 inside it, 1 on
    /// the edge of its basin, above 1 outside. The shore is not a circle.
    pub fn shore(&self, at: Vec2) -> Option<f32> {
        let lake = self.lake?;
        let from = at - lake.centre;
        let angle = from.y.atan2(from.x);
        let outline = 0.84 + 0.11 * (2.0 * angle + 0.9).cos() + 0.05 * (5.0 * angle + 2.0).cos();
        Some(from.length() / (lake.radius * outline))
    }

    /// Height of the ground: zero along the track, hills beyond, zero again
    /// at the edge of the modelled land; a knoll and a lake inside the loop.
    pub fn height(&self, at: Vec2) -> f32 {
        let rise = smooth((self.distance_to_track(at) - self.flat) / 70.0);
        let edge = (at - self.centre).abs().max_element();
        let fade = smooth((LAND / 2.0 - edge) / 120.0);
        let hills = 0.5 * noise(at / 150.0) + 0.3 * noise(at / 61.0 + 17.0) + 0.2 * noise(at / 23.0 + 5.0);
        let mut height = HILLS * rise * fade * hills;
        if let Some(knoll) = self.knoll {
            height += KNOLL * smooth(1.0 - at.distance(knoll.centre) / knoll.radius);
        }
        if let Some(shore) = self.shore(at) {
            // No hill stands in the lake: the land comes down to the shore,
            // then goes under the water.
            height *= smooth((shore - 1.0) / 0.6);
            height -= LAKE_BED * smooth((1.0 - shore) / 0.5);
        }
        height
    }

    /// Whether a point is in the lake or on its beach.
    pub fn wet(&self, at: Vec2) -> bool {
        self.shore(at).is_some_and(|shore| shore < 1.3)
    }

    /// The colour of the ground, linear: meadows in patches, paler on the
    /// heights, sand around the lake and under it.
    fn colour(&self, at: Vec2, height: f32) -> Vec3 {
        let tone = |r: f32, g: f32, b: f32| Color::srgb(r, g, b).to_linear().to_vec3();
        let (meadow, shade) = (GRASS.to_linear().to_vec3(), tone(0.25, 0.55, 0.17));
        let patches = smooth((noise(at / 38.0 + 40.0) - 0.38) / 0.24);
        let mut colour = meadow.lerp(shade, patches) * (0.9 + 0.2 * noise(at / 9.0 + 11.0));
        colour = colour.lerp(tone(0.56, 0.76, 0.27), (height / HILLS).clamp(0.0, 1.0) * 0.7);
        if let Some(shore) = self.shore(at) {
            colour = colour.lerp(tone(0.93, 0.84, 0.6), smooth((1.24 - shore) / 0.12));
            // Deeper, the bed darkens: the water above it looks deep.
            colour = colour.lerp(tone(0.16, 0.36, 0.42), smooth((0.9 - shore) / 0.4));
        }
        colour
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Value noise in 0..1, the same on every machine.
pub fn noise(p: Vec2) -> f32 {
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

/// The land as a grid, finer around the circuit than far from it, painted
/// with the colour of the ground. `paint` scales that colour for the
/// material that will cover it.
fn land_mesh(land: &Land, paint: f32) -> Mesh {
    let n = CELLS;
    // Evenly spaced steps, bent so that the middle ones are five times smaller than the last.
    let along = |i: usize| {
        let u = 2.0 * i as f32 / n as f32 - 1.0;
        (0.4 * u + 0.6 * u * u * u) * LAND / 2.0
    };
    let at = |i: usize, j: usize| land.centre + Vec2::new(along(i), along(j));
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
            colours.push((land.colour(Vec2::new(here.x, here.z), here.y) * paint).extend(1.0).to_array());
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

pub fn flow(time: Res<Time>, water: Option<Res<Water>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    if let Some(mut material) = water.and_then(|water| materials.get_mut(&water.0)) {
        let drift = Vec2::new(0.013, 0.021) * time.elapsed_secs();
        material.uv_transform = Affine2::from_scale_angle_translation(Vec2::splat(9.0), 0.0, drift.fract());
    }
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
    flat(with_tangents(land_mesh(&land, paint)), &grass_material);
    let plain_material = plain(&mut materials, Color::srgb(0.3, 0.58, 0.18), 1.0);
    let (edge, horizon) = (LAND / 2.0 - 1.0, 3000.0);
    for (low, high) in [
        (Vec2::new(-horizon, edge), Vec2::new(horizon, horizon)),
        (Vec2::new(-horizon, -horizon), Vec2::new(horizon, -edge)),
        (Vec2::new(edge, -edge), Vec2::new(horizon, edge)),
        (Vec2::new(-horizon, -edge), Vec2::new(-edge, edge)),
    ] {
        let (size, middle) = (high - low, land.centre + (low + high) / 2.0);
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

#[cfg(test)]
mod tests {
    use osage_world_kart::Circuit;

    use super::*;

    fn land() -> (Land, Circuit) {
        let circuit = Circuit::new();
        let frames = frames(&circuit.track);
        (Land::new(&frames, &circuit.track), circuit)
    }

    #[test]
    fn the_land_is_flat_where_karts_drive() {
        let (land, circuit) = land();
        let reach = circuit.track.half_width + circuit.track.verge + BARRIER + 0.7;
        for frame in frames(&circuit.track) {
            for lateral in [-reach, 0.0, reach] {
                let at = frame.at + frame.right * lateral;
                assert!(land.height(at).abs() < 1e-3, "{} m high at {at}", land.height(at));
            }
        }
    }

    #[test]
    fn the_circuit_leaves_room_for_a_knoll_and_a_lake() {
        let (land, circuit) = land();
        let (knoll, lake) = (land.knoll.expect("a knoll"), land.lake.expect("a lake"));
        let barrier = circuit.track.half_width + circuit.track.verge + BARRIER + 0.7;
        for site in [knoll, lake] {
            assert!(land.inside(site.centre), "{site:?} is inside the loop");
            assert!(land.distance_to_track(site.centre) - site.radius > barrier + 3.0, "{site:?} keeps clear");
        }
        assert!(knoll.centre.distance(lake.centre) > knoll.radius + lake.radius, "they do not overlap");
        assert!(!land.inside(Vec2::new(500.0, 0.0)));

        // The knoll stands above the track, the lake's bed lies under its water,
        // and its shore is above it all the way round.
        assert!(land.height(knoll.centre) >= KNOLL - 1e-3);
        assert!(land.height(lake.centre) < WATER - 1.0);
        for step in 0..48 {
            let around = Vec2::from_angle(step as f32 / 48.0 * std::f32::consts::TAU);
            let edge = lake.centre + around * lake.radius * 1.02;
            assert!(land.height(edge) > WATER, "the water is held in at {edge}");
            assert!(land.shore(edge).unwrap() >= 1.0);
        }
    }
}
