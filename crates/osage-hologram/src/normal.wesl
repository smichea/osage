// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// A view's normals, for baking a lit hologram (issue #7): the surface's
// direction at each point, in the object's frame (baked at the origin,
// unturned), as colour: n * 0.5 + 0.5; alpha 1 where the object is.

import bevy_pbr::render::forward_io::VertexOutput;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> @location(0) vec4<f32> {
    var n = normalize(in.world_normal);
    if (!is_front) {
        n = -n;
    }
    return vec4<f32>(n * 0.5 + 0.5, 1.0);
}
