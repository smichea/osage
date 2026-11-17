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
// x: columns (azimuths), y: rows (elevations).
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var<uniform> shape: vec4<f32>;
// xyz: the photographed sphere's centre, in the world; w: its radius.
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var<uniform> frame: vec4<f32>;
// x, y: cosine and sine of the object's turn about the vertical.
@group(constants::MATERIAL_BIND_GROUP) @binding(103) var<uniform> turn: vec4<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(104) var views: texture_2d_array<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(105) var views_sampler: sampler;

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

// What view (column, row) saw at `q` (the point, from the sphere's centre,
// in the object's frame): its colour, and 1 if it saw the object, else 0.
fn seen(column: f32, row: f32, q: vec3<f32>) -> vec4<f32> {
    let back = view_direction(column, row);
    let right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), back));
    let up = cross(back, right);
    let size = 2.0 * frame.w;
    let uv = vec2<f32>(dot(q, right) / size + 0.5, 0.5 - dot(q, up) / size);
    let layer = i32(row * shape.x + column);
    let c = textureSampleLevel(views, views_sampler, uv, layer, 0.0);
    // How much of the pixel is the magenta ground (at the object's edges,
    // part of it): its coverage, and its colour with the ground taken out.
    let ground = clamp((min(c.r, c.b) - c.g - 0.15) / 0.7, 0.0, 1.0);
    let cover = 1.0 - ground;
    if (cover < 0.05) {
        return vec4<f32>(0.0);
    }
    let unmixed = clamp((c.rgb - ground * vec3<f32>(1.0, 0.0, 1.0)) / cover, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(unmixed, cover);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let p = in.world_position.xyz;
    let eye = into_object(normalize(view.world_position.xyz - p));
    let q = into_object(p - frame.xyz);

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

    let s00 = seen(c0, r0, q);
    let s10 = seen(c1, r0, q);
    let s01 = seen(c0, r1, q);
    let s11 = seen(c1, r1, q);
    let w00 = (1.0 - wa) * (1.0 - we) * s00.a;
    let w10 = wa * (1.0 - we) * s10.a;
    let w01 = (1.0 - wa) * we * s01.a;
    let w11 = wa * we * s11.a;
    let covered = w00 + w10 + w01 + w11;
    let all = (1.0 - wa) * (1.0 - we) + wa * (1.0 - we) + (1.0 - wa) * we + wa * we;
    if (covered < 0.5 * all) {
        discard;
    }
    let colour = (s00.rgb * w00 + s10.rgb * w10 + s01.rgb * w01 + s11.rgb * w11) / covered;

    pbr_input.material.base_color = vec4<f32>(colour, 1.0);
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
