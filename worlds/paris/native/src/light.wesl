// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// The light coming into a walker's block (D-136), drawn on one face of the
// block from inside. A ray from the eye leaves the block where this face
// is drawn; whatever it would meet out there, the probes of the face's
// tiles saw it arrive. Each probe is an equidistant fisheye over 200
// degrees looking out (osage-world-paris's PROBE_FOV), with how far each of
// its pixels lies. Of the four
// tiles around where the ray leaves, each is asked where the ray first
// passes behind what its probe saw — marching out along the ray — and
// those that agree that the ray meets what they saw there give its colour.

import bevy_pbr::render::{
    forward_io::VertexOutput,
    mesh_view_bindings::view,
};

// xyz: the block's lowest corner; w: which face this is (0 -x, 1 +x,
// 2 -y, 3 +y, 4 -z, 5 +z).
@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> low: vec4<f32>;
// xyz: the block's highest corner; w: a probe's side, pixels.
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var<uniform> high: vec4<f32>;
// The layer of each of the face's tiles in the arrays, tile (i, j) at
// i * (tiles along b) + j; -1 where it has not come.
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var<uniform> layers: array<vec4<i32>, 3>;
// Radiance through its curve, 10 bits a band.
@group(constants::MATERIAL_BIND_GROUP) @binding(3) var colours: texture_2d_array<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(4) var colours_sampler: sampler;
// Distance, m; negative where the sky is.
@group(constants::MATERIAL_BIND_GROUP) @binding(5) var distances: texture_2d_array<f32>;
// x: radiance to luminance (the renderer's units to cd/m²).
@group(constants::MATERIAL_BIND_GROUP) @binding(6) var<uniform> scale: vec4<f32>;

const CUBE: f32 = 3.0;
// Half the probes' field, radians (200 degrees).
const HALF_FIELD: f32 = 1.7453293;
const STEPS: i32 = 40;
// The curve the radiance travels through (osage-hologram-stream's light):
// ln(1 + 64 * 64).
const CURVE_K: f32 = 64.0;
const CURVE_LN: f32 = 8.3180;

fn along(v: vec3<f32>, axis: i32) -> f32 {
    if (axis == 0) {
        return v.x;
    }
    if (axis == 1) {
        return v.y;
    }
    return v.z;
}

fn unit(axis: i32) -> vec3<f32> {
    if (axis == 0) {
        return vec3<f32>(1.0, 0.0, 0.0);
    }
    if (axis == 1) {
        return vec3<f32>(0.0, 1.0, 0.0);
    }
    return vec3<f32>(0.0, 0.0, 1.0);
}

fn layer_of(slot: i32) -> i32 {
    let v = layers[slot / 4];
    let k = slot % 4;
    if (k == 0) {
        return v.x;
    }
    if (k == 1) {
        return v.y;
    }
    if (k == 2) {
        return v.z;
    }
    return v.w;
}

fn radiance(c: vec3<f32>) -> vec3<f32> {
    return (exp(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)) * CURVE_LN) - 1.0) / CURVE_K;
}

// Where a direction from a probe falls on its fisheye (0..1, rows from the
// top), and whether it is on it (z).
fn project(v: vec3<f32>, normal: vec3<f32>, up: vec3<f32>, right: vec3<f32>) -> vec3<f32> {
    let z = dot(v, normal);
    let x = dot(v, right);
    let y = dot(v, up);
    let theta = atan2(length(vec2<f32>(x, y)), z);
    let r = theta / HALF_FIELD;
    let phi = atan2(y, x);
    let uv = vec2<f32>(0.5 + 0.5 * r * cos(phi), 0.5 - 0.5 * r * sin(phi));
    // Only the half sphere out of the block counts.
    return vec3<f32>(uv, select(0.0, 1.0, z > 0.0 && r <= 1.0));
}

fn texel(uv: vec2<f32>) -> vec2<i32> {
    let side = high.w;
    return vec2<i32>(clamp(uv * side, vec2<f32>(0.0), vec2<f32>(side - 1.0)));
}

fn step_distance(k: i32) -> f32 {
    if (k == 0) {
        return 0.0;
    }
    // From a centimetre to 500 m, each step a little farther than the last.
    return 0.01 * pow(50000.0, f32(k - 1) / f32(STEPS - 2));
}

// What one probe tells of the ray: its radiance, and how much to trust it.
fn march(probe: vec3<f32>, normal: vec3<f32>, up: vec3<f32>, layer: i32, start: vec3<f32>, ray: vec3<f32>) -> vec4<f32> {
    let right = cross(normal, up);
    var previous = 0.0;
    for (var k = 0; k < STEPS; k = k + 1) {
        let t = step_distance(k);
        let point = start + ray * t;
        let v = point - probe;
        let on = project(v, normal, up, right);
        if (on.z > 0.5) {
            let d = textureLoad(distances, texel(on.xy), layer, 0).r;
            let reach = length(v);
            if (d >= 0.0 && reach >= d - 0.01) {
                // The ray has passed behind what the probe saw: is it what
                // the ray meets, or something in front hiding it?
                let seen = probe + normalize(v) * d;
                let gap = distance(point, seen);
                let tolerance = (t - previous) + 0.02 * t + 0.03;
                if (gap > 2.0 * tolerance) {
                    return vec4<f32>(0.0);
                }
                let c = textureSampleLevel(colours, colours_sampler, on.xy, layer, 0.0).rgb;
                let closeness = exp(-(gap / tolerance) * (gap / tolerance));
                return vec4<f32>(radiance(c), closeness);
            }
        }
        previous = t;
    }
    // It met nothing: the sky, in the ray's own direction.
    let sky = project(ray, normal, up, right);
    if (sky.z > 0.5 && textureLoad(distances, texel(sky.xy), layer, 0).r < 0.0) {
        let c = textureSampleLevel(colours, colours_sampler, sky.xy, layer, 0.0).rgb;
        return vec4<f32>(radiance(c), 1.0);
    }
    return vec4<f32>(0.0);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let face = i32(low.w);
    let axis = face / 2;
    let positive = (face % 2) == 1;
    var a = 1;
    var b = 2;
    if (axis == 1) {
        a = 0;
    } else if (axis == 2) {
        a = 0;
        b = 1;
    }
    let lo = low.xyz;
    let hi = high.xyz;
    var normal = unit(axis);
    if (!positive) {
        normal = -normal;
    }
    var up = vec3<f32>(0.0, 1.0, 0.0);
    if (axis == 1) {
        up = vec3<f32>(0.0, 0.0, -1.0);
    }
    var plane = along(lo, axis);
    if (positive) {
        plane = along(hi, axis);
    }

    let eye = view.world_position.xyz;
    let start = in.world_position.xyz;
    let ray = normalize(start - eye);

    // The tiles around where the ray leaves: their centres on a lattice of
    // 3 m, the ray's place between them.
    let na = i32(round((along(hi, a) - along(lo, a)) / CUBE));
    let nb = i32(round((along(hi, b) - along(lo, b)) / CUBE));
    let fa = clamp((along(start, a) - along(lo, a)) / CUBE - 0.5, 0.0, f32(na - 1));
    let fb = clamp((along(start, b) - along(lo, b)) / CUBE - 0.5, 0.0, f32(nb - 1));
    let i0 = min(i32(floor(fa)), max(na - 2, 0));
    let j0 = min(i32(floor(fb)), max(nb - 2, 0));
    var ta = 0.0;
    var tb = 0.0;
    if (na > 1) {
        ta = fa - f32(i0);
    }
    if (nb > 1) {
        tb = fb - f32(j0);
    }

    var total = vec3<f32>(0.0);
    var trust = 0.0;
    for (var corner = 0; corner < 4; corner = corner + 1) {
        let di = corner % 2;
        let dj = corner / 2;
        var w = select(1.0 - ta, ta, di == 1) * select(1.0 - tb, tb, dj == 1);
        let i = i0 + di;
        let j = j0 + dj;
        if (w <= 0.0 || i >= na || j >= nb) {
            continue;
        }
        let layer = layer_of(i * nb + j);
        if (layer < 0) {
            continue;
        }
        var probe = unit(axis) * plane;
        probe = probe + unit(a) * (along(lo, a) + (f32(i) + 0.5) * CUBE);
        probe = probe + unit(b) * (along(lo, b) + (f32(j) + 0.5) * CUBE);
        let told = march(probe, normal, up, layer, start, ray);
        total = total + told.rgb * w * told.a;
        trust = trust + w * told.a;
    }
    var light = vec3<f32>(0.0);
    if (trust > 1e-4) {
        light = total / trust;
    } else {
        // No probe agreed: the nearest one's colour in the ray's direction.
        let i = clamp(i32(round(fa)), 0, na - 1);
        let j = clamp(i32(round(fb)), 0, nb - 1);
        let layer = layer_of(i * nb + j);
        if (layer >= 0) {
            let right = cross(normal, up);
            let on = project(ray, normal, up, right);
            light = radiance(textureSampleLevel(colours, colours_sampler, on.xy, layer, 0.0).rgb);
        }
    }
    return vec4<f32>(light * scale.x * view.exposure, 1.0);
}
