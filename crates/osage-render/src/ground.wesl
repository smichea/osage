// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

// Ground that varies and does not repeat: the standard material, its
// colour multiplied by clumps (two octaves of noise, metres across) and
// patches (tens of metres: drier and lusher), in world space; by its grain
// sampled by hexagonal cells shifted and turned by where they are; and on
// a paved path by its history (seams, patches, cracks, stains) from the
// texture coordinates in metres. Octaves smaller than a few pixels fade
// out: no shimmer in the distance.

import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
};

// x: strength of the clumps, y: strength of the patches, z: size of a
// clump in metres, w: size of a patch in metres.
@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> variation: vec4<f32>;
// The colour a dry patch turns to and a lush one, as multipliers.
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var<uniform> dry: vec4<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var<uniform> lush: vec4<f32>;
// The grain: x 1 untiled, 0 a plain repeat; y its repeat in metres; z how
// far a cell turns; w how sharp the blend between cells.
@group(constants::MATERIAL_BIND_GROUP) @binding(103) var<uniform> tiling: vec4<f32>;
// A paved path: x strength, y a stretch's length, z the road's width (m),
// w fresh asphalt's colour multiplier.
@group(constants::MATERIAL_BIND_GROUP) @binding(104) var<uniform> path: vec4<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(105) var grain_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(106) var grain_sampler: sampler;

// Hexagonal cells per repeat of the grain (ground.rs: HEX_CELLS).
const HEX_CELLS: f32 = 1.6;
const TAU: f32 = 6.2831853;
// A cell weighing less is not sampled (ground.rs: SKIP).
const SKIP: f32 = 0.03;

fn hash(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(0.1031, 0.1030));
    let r = q + dot(q, q.yx + 33.33);
    return fract((r.x + r.y) * r.x);
}

// Value noise in 0..1, smooth between lattice points.
fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash(i);
    let b = hash(i + vec2<f32>(1.0, 0.0));
    let c = hash(i + vec2<f32>(0.0, 1.0));
    let d = hash(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// The grain at `uv` (repeats) without a tile: the three hexagonal cells
// around it, each shifted and turned by a hash of its centre, blended by
// nearness raised to `tiling.w`. ground.rs's `hex_tiles` is the same, step
// by step: change both.
fn untiled(uv: vec2<f32>) -> vec4<f32> {
    let dx = dpdx(uv);
    let dy = dpdy(uv);
    let p = uv * HEX_CELLS;
    let skewed = vec2<f32>(p.x - p.y * 0.5773503, p.y * 1.1547005);
    let base = floor(skewed);
    let f = fract(skewed);
    let z = 1.0 - f.x - f.y;
    var w: vec3<f32>;
    var c0: vec2<f32>;
    var c1: vec2<f32>;
    var c2: vec2<f32>;
    if (z > 0.0) {
        w = vec3<f32>(z, f.y, f.x);
        c0 = base;
        c1 = base + vec2<f32>(0.0, 1.0);
        c2 = base + vec2<f32>(1.0, 0.0);
    } else {
        w = vec3<f32>(-z, 1.0 - f.y, 1.0 - f.x);
        c0 = base + vec2<f32>(1.0, 1.0);
        c1 = base + vec2<f32>(1.0, 0.0);
        c2 = base + vec2<f32>(0.0, 1.0);
    }
    w = pow(max(w, vec3<f32>(0.0)), vec3<f32>(tiling.w));
    w = w / (w.x + w.y + w.z);
    // Most pixels lie well inside one cell: a cell that weighs almost
    // nothing is not sampled (explicit gradients: no uniformity needed).
    w = select(w, vec3<f32>(0.0), w < vec3<f32>(SKIP));
    var sum = vec4<f32>(0.0);
    if (w.x > 0.0) {
        sum = sum + w.x * cell_sample(uv, dx, dy, c0);
    }
    if (w.y > 0.0) {
        sum = sum + w.y * cell_sample(uv, dx, dy, c1);
    }
    if (w.z > 0.0) {
        sum = sum + w.z * cell_sample(uv, dx, dy, c2);
    }
    return sum / (w.x + w.y + w.z);
}

fn cell_sample(uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, corner: vec2<f32>) -> vec4<f32> {
    let angle = hash(corner + 0.5) * TAU * tiling.z;
    let shift = vec2<f32>(hash(corner + vec2<f32>(17.3, 3.1)), hash(corner + vec2<f32>(5.7, 29.9)));
    let c = cos(angle);
    let s = sin(angle);
    let turn = mat2x2<f32>(c, s, -s, c);
    return textureSampleGrad(grain_texture, grain_sampler, turn * uv + shift, turn * dx, turn * dy);
}

// A line of `width` metres at distance `d`, faded as pixels outgrow it.
fn line(d: f32, width: f32, footprint: f32) -> f32 {
    let w = max(width, footprint);
    return (1.0 - smoothstep(0.5 * w, w, abs(d))) * (width / w);
}

// What a paved road has lived through, at `across` and `along` metres:
// a multiplier of its colour (rgb) and of its roughness (a).
fn paved(across: f32, along: f32, footprint: f32) -> vec4<f32> {
    let strength = path.x;
    let stretch = max(path.y, 1.0);
    let width = path.z;
    var tint = 1.0;
    var rough = 1.0;

    // Paving seams along the road: where two passes of the paver met,
    // sealed with bitumen, a little off the middle, wandering slowly.
    let seam_at = width * (0.5 + 0.08 * (noise(vec2<f32>(along / 37.0, 3.0)) - 0.5)) + 0.35 * (hash(vec2<f32>(floor(along / 120.0), 1.0)) - 0.5);
    let seam = line(across - seam_at, 0.05, footprint);
    tint = tint * (1.0 - 0.35 * seam);
    rough = rough * (1.0 - 0.4 * seam);

    // Each stretch: maybe a repaired patch of fresh asphalt, its edges
    // sealed, its corners not quite square.
    let k = floor(along / stretch);
    let h = vec2<f32>(k, 7.0);
    if (hash(h) < 0.55) {
        let start = (k + 0.1 + 0.5 * hash(h + 1.3)) * stretch;
        let long = (0.12 + 0.25 * hash(h + 2.7)) * stretch;
        let left = width * 0.75 * hash(h + 4.1);
        let span = min(width - left, 1.5 + 0.35 * width * hash(h + 5.9));
        let wobble = 0.06 * (noise(vec2<f32>(along, across) * 1.7) - 0.5);
        let du = min(along - start, start + long - along) + wobble;
        let dv = min(across - left, left + span - across) + wobble;
        let inside = step(0.0, du) * step(0.0, dv);
        let fresh = mix(1.0, path.w, 0.6 + 0.4 * hash(h + 8.8));
        tint = tint * mix(1.0, fresh, inside);
        rough = rough * mix(1.0, 0.92, inside);
        // The sealed edge: where the nearer side is.
        let edge = line(min(du, dv), 0.04, footprint);
        tint = tint * (1.0 - 0.3 * edge);
    }

    // Cracks, in the older stretches: the ridges of a noise, thin and dark,
    // branching where two ridges meet.
    let old = smoothstep(0.55, 0.8, noise(vec2<f32>(along / 23.0, across / 9.0) + 13.0));
    let ridge = abs(noise(vec2<f32>(along, across) * 0.9 + 31.0) - 0.5);
    let ridge2 = abs(noise(vec2<f32>(along * 1.9 + 7.0, across * 2.3)) - 0.5);
    let crack = max(line(ridge, 0.012, footprint * 0.9), 0.6 * line(ridge2, 0.01, footprint * 1.9)) * old;
    tint = tint * (1.0 - 0.45 * crack);

    // Stains: oil and coolant where cars stood or broke down, darker and
    // smoother, rare.
    let blot = smoothstep(0.78, 0.86, noise(vec2<f32>(along, across) / 1.3 + 51.0)) * smoothstep(0.6, 0.75, noise(vec2<f32>(along / 17.0, 5.0)));
    tint = tint * (1.0 - 0.25 * blot);
    rough = rough * (1.0 - 0.3 * blot);

    // The road's tone wanders along its length.
    tint = tint * (1.0 + 0.06 * (noise(vec2<f32>(along / 45.0, 9.0)) - 0.5));

    return vec4<f32>(vec3<f32>(mix(1.0, tint, strength)), mix(1.0, rough, strength));
}

// How much an octave of `size` metres shows when a pixel covers
// `footprint` metres: whole above eight pixels, gone below two.
fn shows(size: f32, footprint: f32) -> f32 {
    return 1.0 - smoothstep(0.125, 0.5, footprint / size);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let at = in.world_position.xz;
    // A pixel's size on the ground: the mean of its two sides, which a
    // grazing view stretches very unequally (the longer side alone would
    // flatten every slope seen from the road).
    let footprint = sqrt(length(dpdx(at)) * length(dpdy(at)));
    let clump_size = variation.z;
    let patch_size = variation.w;

    // Clumps: two octaves, the second turned and smaller, centred on zero.
    let turned = mat2x2<f32>(0.8, 0.6, -0.6, 0.8) * at;
    let near = (noise(at / clump_size) - 0.5) * shows(clump_size, footprint)
        + 0.6 * (noise(turned / (clump_size * 0.37) + 17.0) - 0.5) * shows(clump_size * 0.37, footprint);
    var tint = vec3<f32>(1.0 + variation.x * near * 1.6);

    // Patches: drier where one noise is high, lusher where another is low.
    let drier = smoothstep(0.55, 0.85, noise(at / patch_size + 41.0)) * variation.y * shows(patch_size, footprint);
    let lusher = (1.0 - smoothstep(0.15, 0.45, noise(turned / (patch_size * 0.6) + 7.0))) * variation.y * shows(patch_size, footprint);
    tint = tint * mix(vec3<f32>(1.0), dry.rgb, drier) * mix(vec3<f32>(1.0), lush.rgb, lusher);

    // The grain, untiled or repeated; a paved path's history, from the
    // texture coordinates in metres.
    @if(VERTEX_UVS_A) {
        let grain_uv = in.uv / tiling.y;
        var grain: vec4<f32>;
        if (tiling.x > 0.5) {
            grain = untiled(grain_uv);
        } else {
            grain = textureSample(grain_texture, grain_sampler, grain_uv);
        }
        tint = tint * grain.rgb;
        if (path.x > 0.0) {
            let worn = paved(in.uv.x, in.uv.y, footprint);
            tint = tint * worn.rgb;
            pbr_input.material.perceptual_roughness = pbr_input.material.perceptual_roughness * worn.a;
        }
    }

    pbr_input.material.base_color = vec4<f32>(pbr_input.material.base_color.rgb * tint, pbr_input.material.base_color.a);
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color
    );

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
