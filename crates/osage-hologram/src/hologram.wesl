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

// What view (column, row) saw where the eye's ray meets the object: `q` is
// where the ray crosses the stand-in (from the sphere's centre), `ray` its
// direction, both in the object's frame. The point is found in the view
// along the ray: its depth there read, the point on the ray at that depth
// taken, a few times over. Its colour, and how much of the pixel is the
// object.
fn seen(column: f32, row: f32, q: vec3<f32>, ray: vec3<f32>) -> Seen {
    let back = view_direction(column, row);
    let right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), back));
    let up = cross(back, right);
    let size = vec2<f32>(2.0 * frame.w, 2.0 * frame.w * framing.x);
    let layer = i32(row * shape.x + column);
    let along = dot(ray, back);
    var p = q;
    var uv = vec2<f32>(dot(p, right) / size.x + 0.5, 0.5 - dot(p, up) / size.y);
    // shape.z: 1 to correct the parallax with the views' depth, 0 not to.
    if (shape.z > 0.5 && abs(along) > 0.05) {
        for (var i = 0; i < 4; i++) {
            let depth = depth_at(layer, uv);
            if (depth < -1e8) {
                break;
            }
            let t = (depth - dot(q, back)) / along;
            p = q + ray * t;
            uv = vec2<f32>(dot(p, right) / size.x + 0.5, 0.5 - dot(p, up) / size.y);
        }
    }
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

    let s00 = seen(c0, r0, q, ray);
    let s10 = seen(c1, r0, q, ray);
    let s01 = seen(c0, r1, q, ray);
    let s11 = seen(c1, r1, q, ray);
    let w00 = (1.0 - wa) * (1.0 - we) * s00.colour.a;
    let w10 = wa * (1.0 - we) * s10.colour.a;
    let w01 = (1.0 - wa) * we * s01.colour.a;
    let w11 = wa * we * s11.colour.a;
    let covered = w00 + w10 + w01 + w11;
    let all = (1.0 - wa) * (1.0 - we) + wa * (1.0 - we) + (1.0 - wa) * we + wa * we;
    if (covered < 0.5 * all) {
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
