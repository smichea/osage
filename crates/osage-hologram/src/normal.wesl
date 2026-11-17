// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// A view's normals, for baking a lit hologram (issue #7): the surface's
// direction at each point, in the object's frame (baked at the origin,
// unturned), as colour: n * 0.5 + 0.5; alpha 1 where the object is.

import bevy_pbr::render::forward_io::VertexOutput;

// The model's colour texture, whose alpha cuts leaves out of their cards
// (x: the cutoff; 0: nothing cut).
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var cut_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(3) var cut_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(4) var<uniform> cut: vec4<f32>;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> @location(0) vec4<f32> {
    @if(VERTEX_UVS_A) {
        if (cut.x > 0.0 && textureSample(cut_texture, cut_sampler, in.uv).a < cut.x) {
            discard;
        }
    }
    var n = normalize(in.world_normal);
    if (!is_front) {
        n = -n;
    }
    return vec4<f32>(n * 0.5 + 0.5, 1.0);
}
