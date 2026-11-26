// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// A hologram (issue #7): an object drawn from photographs of it taken from
// a grid of directions, on a simple stand-in surface. For each pixel, the
// direction from the surface to the eye picks the four views of the grid
// around it; the point is found in each view as that view's orthographic
// camera saw it, sampled, and the four blended by how near their direction
// is. Where a view saw nothing (its magenta ground), it adds nothing; where
// none saw anything, nothing is drawn.

import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    mesh_view_bindings::view,
};

// x: first azimuth, y: azimuth step, z: first elevation, w: elevation step
// (degrees).
@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> grid: vec4<f32>;
// x: columns (azimuths), y: rows (elevations), z: 1 to find each point
// along the eye's ray with the views' depth (0: on the stand-in).
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var<uniform> shape: vec4<f32>;
// xyz: the photographed sphere's centre, in the world; w: its radius.
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var<uniform> frame: vec4<f32>;
// x, y: cosine and sine of the object's turn about the vertical; z, w: the
// distances over which it fades in (from z, whole at w; w 0: always whole),
// where a model gives way to it.
@group(constants::MATERIAL_BIND_GROUP) @binding(103) var<uniform> turn: vec4<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(104) var views: texture_2d_array<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(105) var views_sampler: sampler;
// Each view's depth (16 bits over red and green, blue where the object is),
// read texel by texel: filtering would mix the two bytes.
@group(constants::MATERIAL_BIND_GROUP) @binding(106) var depths: texture_2d_array<f32>;
// Each view's normals, in the object's frame (n * 0.5 + 0.5): with them and
// views of the colour without light (shape.w 1), the hologram is lit where
// it is drawn, by the world's lights.
@group(constants::MATERIAL_BIND_GROUP) @binding(107) var normals: texture_2d_array<f32>;
// x: the views' height to width (their camera framed 2r wide, 2r·x high);
// y: 1 where the object is told by the depth pictures (views baked on
// black), 0 by the magenta ground of the views.
@group(constants::MATERIAL_BIND_GROUP) @binding(108) var<uniform> framing: vec4<f32>;
// The window of views on the GPU (issue #8): first column, first row,
// columns, rows of the grid that the arrays hold; z 0: the whole grid. A
// view outside it is taken as the nearest inside, its point found by depth.
// A view's layer wraps round the window's size (row modulo its rows,
// column modulo its columns): a window slid by a column changes only that
// column's layers.
@group(constants::MATERIAL_BIND_GROUP) @binding(109) var<uniform> window: vec4<f32>;

// A view's layer in the arrays.
fn layer_of(column: f32, row: f32) -> i32 {
    if (window.z < 0.5) {
        return i32(row * shape.x + column);
    }
    let c = clamp(column, window.x, window.x + window.z - 1.0);
    let r = clamp(row, window.y, window.y + window.w - 1.0);
    let wrapped_r = r - window.w * floor(r / window.w);
    let wrapped_c = c - window.z * floor(c / window.z);
    return i32(wrapped_r * window.z + wrapped_c);
}

const DEGREES: f32 = 0.017453292;

// A world direction in the object's own frame (its front +Z).
fn into_object(d: vec3<f32>) -> vec3<f32> {
    let c = turn.x;
    let s = turn.y;
    return vec3<f32>(c * d.x - s * d.z, d.y, s * d.x + c * d.z);
}

// The direction towards a view's camera, in the object's frame.
fn view_direction(column: f32, row: f32) -> vec3<f32> {
    let a = (grid.x + column * grid.y) * DEGREES;
    let e = (grid.z + row * grid.w) * DEGREES;
    return vec3<f32>(sin(a) * cos(e), sin(e), cos(a) * cos(e));
}

// A view's depth at `uv`: how far the surface stands towards the view's
// camera from the sphere's centre; -1e9 where the view saw nothing.
fn depth_at(layer: i32, uv: vec2<f32>) -> f32 {
    let size = vec2<f32>(textureDimensions(depths));
    let texel = vec2<i32>(clamp(uv * size, vec2<f32>(0.0), size - 1.0));
    let d = textureLoad(depths, texel, layer, 0);
    if (d.b < 0.5) {
        return -1e9;
    }
    let t = (round(d.r * 255.0) * 256.0 + round(d.g * 255.0)) / 65535.0;
    return (t * 2.0 - 1.0) * frame.w;
}

// What a view saw: its colour (alpha: how much of the pixel is the object)
// and the surface's direction there, in the object's frame.
struct Seen {
    colour: vec4<f32>,
    normal: vec3<f32>,
}

// Where a point of the object's frame (from the sphere's centre) falls in
// a view's picture.
fn uv_in(column: f32, row: f32, p: vec3<f32>) -> vec2<f32> {
    let back = view_direction(column, row);
    let right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), back));
    let up = cross(back, right);
    let size = vec2<f32>(2.0 * frame.w, 2.0 * frame.w * framing.x);
    return vec2<f32>(dot(p, right) / size.x + 0.5, 0.5 - dot(p, up) / size.y);
}

// Where the eye's ray first meets the object as one view saw it (w: 1 if
// it does): the ray marched through the photographed sphere, the view's
// depth read at each step, until the ray passes behind the surface the
// view saw there; then the crossing narrowed down. A deep, sparse object
// (a stand of trees) is mostly empty where the ray crosses the stand-in,
// so the point cannot be found from there alone.
fn march(column: f32, row: f32, q: vec3<f32>, ray: vec3<f32>) -> vec4<f32> {
    let back = view_direction(column, row);
    let layer = layer_of(column, row);
    // The ray's span inside the sphere: |q + ray t| = r.
    let b = dot(q, ray);
    let c = dot(q, q) - frame.w * frame.w;
    let h = b * b - c;
    if (h <= 0.0) {
        return vec4<f32>(q, 0.0);
    }
    let root = sqrt(h);
    // Never behind the eye: seen from within the sphere, what stands at
    // its back is not there.
    let t0 = max(-b - root, -distance(into_object(view.world_position.xyz - frame.xyz), q));
    let t1 = -b + root;
    let steps = 24;
    let dt = (t1 - t0) / f32(steps);
    var before = t0;
    for (var i = 0; i <= steps; i++) {
        let t = t0 + f32(i) * dt;
        let p = q + ray * t;
        let uv = uv_in(column, row, p);
        if (all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0))) {
            let surface = depth_at(layer, uv);
            // Behind the surface seen there, and not by more than a step
            // (else the ray went behind something it never touched).
            let gap = surface - dot(p, back);
            if (surface > -1e8 && gap >= 0.0 && gap <= abs(dt * dot(ray, back)) * 1.5 + 0.02 * frame.w) {
                // Narrowed down between the last step in front and this one.
                var lo = before;
                var hi = t;
                for (var k = 0; k < 5; k++) {
                    let m = 0.5 * (lo + hi);
                    let pm = q + ray * m;
                    let sm = depth_at(layer, uv_in(column, row, pm));
                    if (sm > -1e8 && sm >= dot(pm, back)) {
                        hi = m;
                    } else {
                        lo = m;
                    }
                }
                return vec4<f32>(q + ray * hi, 1.0);
            }
        }
        before = t;
    }
    return vec4<f32>(q, 0.0);
}

// Where a view sees the eye's ray, from where it crosses the stand-in:
// the view's depth read there, the point on the ray at that depth taken, a
// few times over. Right for a surface the stand-in lies on (a facade),
// where the march finds nothing its depth tells apart.
fn settle(column: f32, row: f32, q: vec3<f32>, ray: vec3<f32>) -> vec3<f32> {
    let back = view_direction(column, row);
    let layer = layer_of(column, row);
    let along = dot(ray, back);
    var p = q;
    if (abs(along) > 0.05) {
        for (var i = 0; i < 4; i++) {
            let depth = depth_at(layer, uv_in(column, row, p));
            if (depth < -1e8) {
                break;
            }
            p = q + ray * ((depth - dot(q, back)) / along);
        }
    }
    return p;
}

// What view (column, row) saw at a point `p` of the object's frame: its
// colour (alpha: how much of the pixel is the object) and the surface's
// direction there, in the object's frame.
fn seen(column: f32, row: f32, p: vec3<f32>) -> Seen {
    let layer = layer_of(column, row);
    let uv = uv_in(column, row, p);
    let c = textureSampleLevel(views, views_sampler, uv, layer, 0.0);
    var ground = 0.0;
    if (framing.y > 0.5) {
        // The object is where the depth picture has it (blue).
        let dims = vec2<f32>(textureDimensions(depths));
        let texel = vec2<i32>(clamp(uv * dims, vec2<f32>(0.0), dims - 1.0));
        ground = 1.0 - textureLoad(depths, texel, layer, 0).b;
    } else {
        // How much of the pixel is the magenta ground (at the object's
        // edges, part of it): its coverage, its colour with the ground out.
        ground = clamp((min(c.r, c.b) - c.g - 0.15) / 0.7, 0.0, 1.0);
    }
    let cover = 1.0 - ground;
    var out: Seen;
    out.normal = textureSampleLevel(normals, views_sampler, uv, layer, 0.0).rgb * 2.0 - 1.0;
    if (cover < 0.05) {
        out.colour = vec4<f32>(0.0);
        return out;
    }
    let key = select(vec3<f32>(1.0, 0.0, 1.0), vec3<f32>(0.0), framing.y > 0.5);
    let unmixed = clamp((c.rgb - ground * key) / cover, vec3<f32>(0.0), vec3<f32>(1.0));
    out.colour = vec4<f32>(unmixed, cover);
    return out;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let p = in.world_position.xyz;
    let eye = into_object(normalize(view.world_position.xyz - p));
    let q = into_object(p - frame.xyz);
    let ray = -eye;

    // Where the eye's direction falls in the grid.
    let azimuth = atan2(eye.x, eye.z) / DEGREES;
    let elevation = asin(clamp(eye.y, -1.0, 1.0)) / DEGREES;
    let fa = clamp((azimuth - grid.x) / grid.y, 0.0, shape.x - 1.0);
    let fe = clamp((elevation - grid.z) / grid.w, 0.0, shape.y - 1.0);
    let c0 = floor(fa);
    let r0 = floor(fe);
    let c1 = min(c0 + 1.0, shape.x - 1.0);
    let r1 = min(r0 + 1.0, shape.y - 1.0);
    let wa = fa - c0;
    let we = fe - r0;

    // The point the eye sees: for an object told by its depth pictures (a
    // stand of trees, baked on black), found along its ray in the nearest
    // view; else, or failing that, each view's point settled from where the
    // ray crosses the stand-in (enough for a facade, and cheaper).
    var hit = vec4<f32>(q, 0.0);
    if (shape.z > 0.5 && framing.y > 0.5) {
        hit = march(round(fa), round(fe), q, ray);
    }
    let found = hit.w > 0.5;
    let depthy = shape.z > 0.5 && !found;
    let s00 = seen(c0, r0, select(select(q, settle(c0, r0, q, ray), depthy), hit.xyz, found));
    let s10 = seen(c1, r0, select(select(q, settle(c1, r0, q, ray), depthy), hit.xyz, found));
    let s01 = seen(c0, r1, select(select(q, settle(c0, r1, q, ray), depthy), hit.xyz, found));
    let s11 = seen(c1, r1, select(select(q, settle(c1, r1, q, ray), depthy), hit.xyz, found));
    let w00 = (1.0 - wa) * (1.0 - we) * s00.colour.a;
    let w10 = wa * (1.0 - we) * s10.colour.a;
    let w01 = (1.0 - wa) * we * s01.colour.a;
    let w11 = wa * we * s11.colour.a;
    let covered = w00 + w10 + w01 + w11;
    let all = (1.0 - wa) * (1.0 - we) + wa * (1.0 - we) + (1.0 - wa) * we + wa * we;
    // Half the views must see the object for it to be drawn, unless the
    // march found the point: then it is the object's, and any view seeing
    // it gives its colour.
    let needed = select(0.5 * all, 1e-4, found);
    if (covered < needed) {
        discard;
    }
    let colour = (s00.colour.rgb * w00 + s10.colour.rgb * w10 + s01.colour.rgb * w01 + s11.colour.rgb * w11) / covered;

    var fade = 1.0;
    if (turn.w > 0.0) {
        fade = smoothstep(turn.z, turn.w, distance(view.world_position.xyz, p));
        if (fade <= 0.0) {
            discard;
        }
    }
    pbr_input.material.base_color = vec4<f32>(colour, fade);
    if (shape.w > 0.5) {
        // Lit here: the views' colour is the surface's own, their normals
        // turned from the object's frame into the world's.
        let n = normalize(s00.normal * w00 + s10.normal * w10 + s01.normal * w01 + s11.normal * w11);
        let c = turn.x;
        let s = turn.y;
        let world = normalize(vec3<f32>(c * n.x + s * n.z, n.y, -s * n.x + c * n.z));
        pbr_input.N = world;
        pbr_input.world_normal = world;
    }
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
