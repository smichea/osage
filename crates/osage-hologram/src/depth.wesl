// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// A view's depth, for baking a hologram (issue #7): how far each point of
// the object stands towards the camera from the photographed sphere's
// centre, from -radius (0) to +radius (1), in 16 bits over red (high byte)
// and green (low byte); blue 1 where the object is (0 is the ground).

import bevy_pbr::render::forward_io::VertexOutput;

// xyz: the sphere's centre; w: its radius.
@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> frame: vec4<f32>;
// xyz: the direction from the object towards the camera.
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var<uniform> back: vec4<f32>;
// The model's colour texture, whose alpha cuts leaves out of their cards
// (x: the cutoff; 0: nothing cut).
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var cut_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(3) var cut_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(4) var<uniform> cut: vec4<f32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    @if(VERTEX_UVS_A) {
        if (cut.x > 0.0 && textureSample(cut_texture, cut_sampler, in.uv).a < cut.x) {
            discard;
        }
    }
    let d = dot(in.world_position.xyz - frame.xyz, back.xyz);
    let t = clamp((d + frame.w) / (2.0 * frame.w), 0.0, 1.0);
    let v = round(t * 65535.0);
    let high = floor(v / 256.0);
    let low = v - high * 256.0;
    return vec4<f32>(high / 255.0, low / 255.0, 1.0, 1.0);
}
