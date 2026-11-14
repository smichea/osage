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

fn denoise_at(id: vec2<u32>, step: i32) {
    let size = vec2<u32>(view.main_pass_viewport.zw);
    if any(id >= size) {
        return;
    }
    let at = vec2<i32>(id);
    let centre = pixel(at);
    if !centre.lit {
        textureStore(output, at, textureLoad(input, at, 0));
        return;
    }
    // The light's spread round the pixel, its own scale for the weights.
    var taps: array<Pixel, 9>;
    var mean = 0.0;
    var square = 0.0;
    var counted = 0.0;
    for (var i = 0; i < 9; i++) {
        let offset = vec2(i % 3 - 1, i / 3 - 1) * step;
        taps[i] = pixel(at + offset);
        if taps[i].lit {
            let l = luminance(taps[i].light);
            mean += l;
            square += l * l;
            counted += 1.0;
        }
    }
    mean /= counted;
    let deviation = sqrt(max(square / counted - mean * mean, 0.0));
    let l0 = luminance(centre.light);
    let distance = length(centre.position - view.world_position);
    // A neighbour off the pixel's plane by more than this, m, is another
    // surface: tighter near the camera, looser with the taps' reach.
    let plane = 0.004 * distance * f32(step) + 0.01;
    var sum = vec3(0.0);
    var weights = 0.0;
    for (var i = 0; i < 9; i++) {
        let q = taps[i];
        if !q.lit {
            continue;
        }
        let kernel = select(0.5, 1.0, i % 3 == 1) * select(0.5, 1.0, i / 3 == 1);
        let w_normal = pow(max(dot(centre.normal, q.normal), 0.0), SIGMA_NORMAL);
        let w_plane = exp(-abs(dot(centre.normal, q.position - centre.position)) / plane);
        let w_light = exp(-abs(luminance(q.light) - l0) / (SIGMA_LIGHT * deviation + 1e-4));
        let w = kernel * w_normal * w_plane * w_light;
        sum += q.light * w;
        weights += w;
    }
    let light = sum / max(weights, 1e-6);
    textureStore(output, at, vec4(light * centre.colour, 1.0));
}

@compute @workgroup_size(8, 8, 1)
fn step1(@builtin(global_invocation_id) id: vec3<u32>) {
    denoise_at(id.xy, 1);
}

@compute @workgroup_size(8, 8, 1)
fn step2(@builtin(global_invocation_id) id: vec3<u32>) {
    denoise_at(id.xy, 2);
}

@compute @workgroup_size(8, 8, 1)
fn step4(@builtin(global_invocation_id) id: vec3<u32>) {
    denoise_at(id.xy, 4);
}
