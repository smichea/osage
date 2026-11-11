// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

// Ground that varies: the standard material, its colour multiplied by
// clumps (two octaves of noise, metres across) and patches (tens of
// metres: drier and lusher), in world space, so that a tiled grain never
// reads as a carpet. Octaves smaller than a few pixels fade out: no
// shimmer in the distance.

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
