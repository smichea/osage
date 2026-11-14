// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

// The traced light's grain filtered out (D-095): an edge-avoiding à-trous
// wavelet filter (Dammertz et al. 2010), weighted as SVGF does (Schied et
// al. 2017), over the picture divided by each surface's colour — the
// light alone, so that the filter blurs light, never a texture. Three
// passes, taps 1, 2 then 4 pixels apart: a 15-pixel reach for 27 taps.

import bevy_render::view::View;
import bevy_pbr::deferred::types::unpack_24bit_normal;
import bevy_pbr::render::rgb9e5::rgb9e5_to_vec3_;
import bevy_render::utils::octahedral_decode;

@group(0) @binding(0) var input: texture_2d<f32>;
@group(0) @binding(1) var output: texture_storage_2d<rgba16float, write>;
@group(0) @binding(2) var gbuffer: texture_2d<u32>;
@group(0) @binding(3) var depth_buffer: texture_depth_2d;
@group(0) @binding(4) var<uniform> view: View;

/// A surface's colour is never taken darker than this when the light is
/// divided by it: a black surface has no light to tell.
const DARKEST = 0.03;
/// How far, in standard deviations of the light round a pixel, a
/// neighbour's light may differ and still count.
const SIGMA_LIGHT = 4.0;
/// How sharply a neighbour of another facing counts less.
const SIGMA_NORMAL = 32.0;

struct Pixel {
    /// Lit by the traced light: a surface of the G-buffer, not glowing.
    lit: bool,
    light: vec3<f32>,
    colour: vec3<f32>,
    normal: vec3<f32>,
    position: vec3<f32>,
}

fn pixel(at: vec2<i32>) -> Pixel {
    var p: Pixel;
    p.lit = false;
    let size = vec2<i32>(view.main_pass_viewport.zw);
    if any(at < vec2(0)) || any(at >= size) {
        return p;
    }
    let depth = textureLoad(depth_buffer, at, 0);
    let g = textureLoad(gbuffer, at, 0);
    // The sky (no depth), a mesh drawn forward (no G-buffer), a glowing
    // surface (its glow is no light to filter): left as drawn.
    if depth == 0.0 || g.a == 0u || any(rgb9e5_to_vec3_(g.g) > vec3(0.0)) {
        return p;
    }
    p.lit = true;
    p.colour = max(pow(unpack4x8unorm(g.r).rgb, vec3(2.2)), vec3(DARKEST));
    p.light = textureLoad(input, at, 0).rgb / p.colour;
    p.normal = octahedral_decode(unpack_24bit_normal(g.a));
    let uv = (vec2<f32>(at) + 0.5) / view.main_pass_viewport.zw;
    let ndc = (uv - vec2(0.5)) * vec2(2.0, -2.0);
    let world = view.world_from_clip * vec4(ndc, depth, 1.0);
    p.position = world.xyz / world.w;
    return p;
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3(0.2126, 0.7152, 0.0722));
}

/// The pixels a workgroup of 8 × 8 reads, decoded once: its own and, round
/// them, as many as its taps reach (4 at most): 16 × 16.
var<workgroup> tile_light: array<vec4<f32>, 256>;
var<workgroup> tile_normal: array<vec3<f32>, 256>;
var<workgroup> tile_position: array<vec3<f32>, 256>;

fn denoise_at(group: vec2<u32>, local: vec2<u32>, index: u32, step: i32) {
    let side = 8 + 2 * step;
    let origin = vec2<i32>(group * 8u) - vec2(step);
    for (var k = i32(index); k < side * side; k += 64) {
        let p = pixel(origin + vec2(k % side, k / side));
        tile_light[k] = vec4(p.light, select(0.0, 1.0, p.lit));
        tile_normal[k] = p.normal;
        tile_position[k] = p.position;
    }
    workgroupBarrier();

    let at = vec2<i32>(group * 8u + local);
    if any(at >= vec2<i32>(view.main_pass_viewport.zw)) {
        return;
    }
    let mine = (i32(local.y) + step) * side + i32(local.x) + step;
    if tile_light[mine].w == 0.0 {
        textureStore(output, at, textureLoad(input, at, 0));
        return;
    }
    let colour = max(pow(unpack4x8unorm(textureLoad(gbuffer, at, 0).r).rgb, vec3(2.2)), vec3(DARKEST));
    let normal = tile_normal[mine];
    let position = tile_position[mine];
    let l0 = luminance(tile_light[mine].rgb);
    // The light's spread round the pixel, its own scale for the weights.
    var mean = 0.0;
    var square = 0.0;
    var counted = 0.0;
    for (var i = 0; i < 9; i++) {
        let q = mine + ((i / 3 - 1) * side + (i % 3 - 1)) * step;
        if tile_light[q].w > 0.0 {
            let l = luminance(tile_light[q].rgb);
            mean += l;
            square += l * l;
            counted += 1.0;
        }
    }
    mean /= counted;
    let deviation = sqrt(max(square / counted - mean * mean, 0.0));
    let distance = length(position - view.world_position);
    // A neighbour off the pixel's plane by more than this, m, is another
    // surface: tighter near the camera, looser with the taps' reach.
    let plane = 0.004 * distance * f32(step) + 0.01;
    var sum = vec3(0.0);
    var weights = 0.0;
    for (var i = 0; i < 9; i++) {
        let q = mine + ((i / 3 - 1) * side + (i % 3 - 1)) * step;
        let light = tile_light[q];
        if light.w == 0.0 {
            continue;
        }
        let kernel = select(0.5, 1.0, i % 3 == 1) * select(0.5, 1.0, i / 3 == 1);
        let w_normal = pow(max(dot(normal, tile_normal[q]), 0.0), SIGMA_NORMAL);
        let w_plane = exp(-abs(dot(normal, tile_position[q] - position)) / plane);
        let w_light = exp(-abs(luminance(light.rgb) - l0) / (SIGMA_LIGHT * deviation + 1e-4));
        let w = kernel * w_normal * w_plane * w_light;
        sum += light.rgb * w;
        weights += w;
    }
    textureStore(output, at, vec4(sum / max(weights, 1e-6) * colour, 1.0));
}

@compute @workgroup_size(8, 8, 1)
fn step1(@builtin(workgroup_id) group: vec3<u32>, @builtin(local_invocation_id) local: vec3<u32>, @builtin(local_invocation_index) index: u32) {
    denoise_at(group.xy, local.xy, index, 1);
}

@compute @workgroup_size(8, 8, 1)
fn step2(@builtin(workgroup_id) group: vec3<u32>, @builtin(local_invocation_id) local: vec3<u32>, @builtin(local_invocation_index) index: u32) {
    denoise_at(group.xy, local.xy, index, 2);
}

@compute @workgroup_size(8, 8, 1)
fn step4(@builtin(workgroup_id) group: vec3<u32>, @builtin(local_invocation_id) local: vec3<u32>, @builtin(local_invocation_index) index: u32) {
    denoise_at(group.xy, local.xy, index, 4);
}
