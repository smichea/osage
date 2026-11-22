// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// The light coming into a walker's block (D-136), drawn on one face of the
// block from inside. A ray from the eye leaves the block where this face
// is drawn; whatever it would meet out there, the probes of the face's
// tiles saw it arrive. Each probe is an equidistant fisheye over 200
// degrees looking out (osage-world-paris's PROBE_FOV), with how far each of
// its pixels lies. Of the four tiles around where the ray leaves, each is
// asked where the ray first passes behind what its probe saw, marching out
// along the ray and then halving the last step until the place is found.
// Those whose probe saw the ray's point itself there (not something nearer
// hiding it) give its colour, the more the more their look at the point is
// like the eye's.

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
// A trial: probes inside the walker's cube instead of the block's tiles;
// xyz where each stands, w its layer (-1: none).
@group(constants::MATERIAL_BIND_GROUP) @binding(7) var<uniform> probes: array<vec4<f32>, 9>;
// Their pictures' width and height; z 1 when they are used; w how many.
@group(constants::MATERIAL_BIND_GROUP) @binding(8) var<uniform> shape: vec4<f32>;
// A trial (the street's meshes round the block): the mesh's own base colour,
// what colour the probes saw each surface to be, and the base colour's
// factor (w 1: the mesh's own colour is drawn, the probes give the light).
@group(constants::MATERIAL_BIND_GROUP) @binding(9) var base: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(10) var base_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(11) var albedos: texture_2d_array<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(12) var<uniform> tint: vec4<f32>;

const CUBE: f32 = 3.0;
// Half the probes' field, radians (200 degrees).
const HALF_FIELD: f32 = 1.7453293;
const STEPS: i32 = 64;
// Halvings of the step where a ray went behind what a probe saw.
const REFINE: i32 = 8;
// How fast trust falls as a probe's look at a point turns from the eye's,
// radians.
const LIKENESS: f32 = 0.15;
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

// Whether the light comes from probes inside the cube.
fn cubed() -> bool {
    return shape.z > 0.5;
}

// A probe's picture, pixels.
fn size() -> vec2<f32> {
    if (cubed()) {
        return shape.xy;
    }
    return vec2<f32>(high.w);
}

// Where a direction from a probe inside the cube falls on its
// equirectangular picture: the middle column north (-z), east to its
// right, the top row straight up; the whole sphere.
fn equirect(v: vec3<f32>) -> vec3<f32> {
    let n = normalize(v);
    let lon = atan2(n.x, -n.z);
    let lat = asin(clamp(n.y, -1.0, 1.0));
    return vec3<f32>(0.5 + lon / 6.2831853, 0.5 - lat / 3.1415927, 1.0);
}

fn texel(uv: vec2<f32>) -> vec2<i32> {
    let s = size();
    return vec2<i32>(clamp(uv * s, vec2<f32>(0.0), s - 1.0));
}

fn step_distance(k: i32) -> f32 {
    if (k == 0) {
        return 0.0;
    }
    // From a centimetre to 500 m, each step a little farther than the last.
    return 0.01 * pow(50000.0, f32(k - 1) / f32(STEPS - 2));
}

// What a probe saw towards a point.
struct Look {
    uv: vec2<f32>,
    // Whether the point is in the probe's field.
    on: bool,
    // How far the probe saw something that way, m; negative: the sky.
    seen: f32,
    // How far the point is from the probe, m.
    reach: f32,
}

fn look(probe: vec3<f32>, normal: vec3<f32>, up: vec3<f32>, right: vec3<f32>, layer: i32, point: vec3<f32>) -> Look {
    let v = point - probe;
    let p = select(project(v, normal, up, right), equirect(v), cubed());
    var l: Look;
    l.uv = p.xy;
    l.on = p.z > 0.5;
    l.reach = length(v);
    l.seen = -1.0;
    if (l.on) {
        l.seen = textureLoad(distances, texel(p.xy), layer, 0).r;
    }
    return l;
}

// Whether the point lies at or beyond what the probe saw that way.
fn behind(l: Look) -> bool {
    return l.on && l.seen >= 0.0 && l.reach >= l.seen;
}

// Where something nearer hides the ray's point from a probe, what the
// probe saw just beside it and farther than it: the ground behind a
// bollard, the wall beside a door's jamb; the nearer of those, at the
// nearest ring of texels that has one. The thing in front stretched over
// the hole was worse: bollards drawn as stairs. None found: the thing.
fn beside(layer: i32, uv: vec2<f32>, front: f32) -> vec3<f32> {
    let s = size();
    let behind_front = front + 0.3 + 0.05 * front;
    var best = vec2<f32>(-1.0);
    var nearest = 1e9;
    for (var ring = 1; ring <= 5 && best.x < 0.0; ring = ring + 1) {
        let r = vec2<f32>(f32(1 << u32(ring))) / s;
        for (var k = 0; k < 8; k = k + 1) {
            let a = f32(k) * 0.7853982;
            let at = uv + vec2<f32>(cos(a), sin(a)) * r;
            let d = textureLoad(distances, texel(at), layer, 0).r;
            // The sky lies behind everything.
            let depth = select(d, 1e6, d < 0.0);
            if (depth > behind_front && depth < nearest) {
                nearest = depth;
                best = at;
            }
        }
    }
    if (best.x < 0.0) {
        best = uv;
    }
    return radiance(textureSampleLevel(colours, colours_sampler, best, layer, 0.0).rgb);
}

// How far along the ray the last march met what it told of, m (the sky:
// very far).
var<private> met: f32;

// What one probe tells of the ray: its radiance, and how much to trust it.
fn march(probe: vec3<f32>, normal: vec3<f32>, up: vec3<f32>, layer: i32, start: vec3<f32>, ray: vec3<f32>) -> vec4<f32> {
    let right = cross(normal, up);
    // A texel's angle, radians.
    let texel_angle = select(2.0 * HALF_FIELD / high.w, 6.2831853 / shape.x, cubed());
    // Where something nearer hid the ray's point: the least hidden, kept
    // in case no step does better.
    var hidden = vec4<f32>(0.0);
    var previous = 0.0;
    var was_behind = false;
    for (var k = 0; k < STEPS; k = k + 1) {
        let t = step_distance(k);
        let now_behind = behind(look(probe, normal, up, right, layer, start + ray * t));
        if (now_behind && !was_behind) {
            // Between the last step and this one the ray went behind what
            // the probe saw: halve the step to find where.
            var near = previous;
            var far = t;
            for (var i = 0; i < REFINE; i = i + 1) {
                let middle = 0.5 * (near + far);
                if (behind(look(probe, normal, up, right, layer, start + ray * middle))) {
                    far = middle;
                } else {
                    near = middle;
                }
            }
            let point = start + ray * far;
            let l = look(probe, normal, up, right, layer, point);
            let c = radiance(textureSampleLevel(colours, colours_sampler, l.uv, layer, 0.0).rgb);
            // Meeting a surface, the halving ends within about a texel of
            // what the probe saw; passing behind an edge, the whole jump
            // to what stands in front is left.
            let gap = abs(l.reach - l.seen);
            let slack = 0.02 + 4.0 * texel_angle * l.seen + (far - near);
            if (gap <= slack) {
                let angle = acos(clamp(dot(normalize(point - probe), ray), -1.0, 1.0));
                met = far;
                return vec4<f32>(c, exp(-(angle * angle) / (LIKENESS * LIKENESS)) + 1e-3);
            }
            let guess = 1e-4 * exp(-gap);
            if (guess > hidden.a) {
                hidden = vec4<f32>(beside(layer, l.uv, l.seen), guess);
            }
        }
        was_behind = now_behind;
        previous = t;
    }
    met = 1e6;
    if (hidden.a > 0.0) {
        return hidden;
    }
    // It met nothing: the sky, in the ray's own direction.
    let sky = select(project(ray, normal, up, right), equirect(ray), cubed());
    if (sky.z > 0.5 && textureLoad(distances, texel(sky.xy), layer, 0).r < 0.0) {
        let c = textureSampleLevel(colours, colours_sampler, sky.xy, layer, 0.0).rgb;
        return vec4<f32>(radiance(c), 1.0);
    }
    return vec4<f32>(0.0);
}

// A trial: a point of the street's own meshes round the block, lit by the
// probes that saw it. No march: the mesh says where the point is, and a
// probe saw it if what it saw that way lies as far as the point.
fn projected(point: vec3<f32>, own: vec3<f32>) -> vec4<f32> {
    let lo = low.xyz;
    let hi = high.xyz;
    // Inside the block the street's meshes are drawn lit; beyond the ring,
    // the faces.
    if (all(point > lo) && all(point < hi)) {
        discard;
    }
    let middle = 0.5 * (lo + hi);
    if (abs(point.x - middle.x) > scale.z || abs(point.z - middle.z) > scale.z) {
        discard;
    }
    let towards = normalize(point - view.world_position.xyz);
    let texel_angle = 6.2831853 / shape.x;
    var seen = vec3<f32>(0.0);
    var sure = 0.0;
    let count = i32(shape.w);
    for (var k = 0; k < count; k = k + 1) {
        let p = probes[k];
        if (p.w < 0.0) {
            continue;
        }
        let v = point - p.xyz;
        let reach = length(v);
        let on = equirect(v);
        let d = textureLoad(distances, texel(on.xy), i32(p.w), 0).r;
        if (d >= 0.0 && abs(reach - d) <= 0.03 + 0.02 * reach + 6.0 * texel_angle * reach) {
            let angle = acos(clamp(dot(v / reach, towards), -1.0, 1.0));
            let trust = exp(-(angle * angle) / (LIKENESS * LIKENESS)) + 1e-3;
            var told = radiance(textureSampleLevel(colours, colours_sampler, on.xy, i32(p.w), 0.0).rgb);
            if (tint.w > 0.5 && layers[k / 4][k % 4] > 0) {
                // The light apart from the surface's colour as the probe saw
                // it, on the mesh's own colour: sharp as its texture.
                let was = radiance(textureSampleLevel(albedos, colours_sampler, on.xy, i32(p.w), 0.0).rgb);
                if (max(was.r, max(was.g, was.b)) > 0.03) {
                    told = told * clamp(own / max(was, vec3<f32>(0.03)), vec3<f32>(0.25), vec3<f32>(4.0));
                }
            }
            seen = seen + told * trust;
            sure = sure + trust;
        }
    }
    var light = vec3<f32>(1.0, 0.0, 1.0);
    if (sure > 0.0) {
        light = seen / sure;
    } else if (scale.y < 0.5 && count > 0) {
        // No probe saw it: what the centre's saw that way, in front of it.
        let on = equirect(point - probes[0].xyz);
        light = radiance(textureSampleLevel(colours, colours_sampler, on.xy, i32(probes[0].w), 0.0).rgb);
    }
    return vec4<f32>(light * scale.x * view.exposure, 1.0);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    if (scale.w > 0.5) {
        var own = tint.rgb;
        @if(VERTEX_UVS_A) {
            own = own * textureSample(base, base_sampler, in.uv).rgb;
        }
        return projected(in.world_position.xyz, own);
    }
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

    if (cubed()) {
        var seen = vec3<f32>(0.0);
        var sure = 0.0;
        var all_told: array<vec4<f32>, 9>;
        var all_met: array<f32, 9>;
        var nearest = 1e6;
        let count = i32(shape.w);
        for (var k = 0; k < count; k = k + 1) {
            let p = probes[k];
            if (p.w < 0.0) {
                continue;
            }
            let told = march(p.xyz, normal, up, i32(p.w), start, ray);
            seen = seen + told.rgb * told.a;
            sure = sure + told.a;
            all_told[k] = told;
            all_met[k] = met;
            // A thing met, not a guess behind what hid it, nor the sky.
            if (told.a >= 5e-4 && met < nearest) {
                nearest = met;
            }
        }
        // Probes of several cubes (a trial): each sees through its own
        // block, so some see through what stands just outside this one.
        // The nearest thing any of them met is what the eye sees: those
        // that met it tell, the others saw past it.
        if (shape.z > 1.5 && nearest < 1e5) {
            seen = vec3<f32>(0.0);
            sure = 0.0;
            let within = nearest * 1.03 + 0.1;
            // Of those, the ones whose look is most like the eye's: the
            // others, standing metres off, saw it slantwise and blur it.
            var best = 0.0;
            for (var k = 0; k < count; k = k + 1) {
                if (probes[k].w >= 0.0 && all_met[k] <= within) {
                    best = max(best, all_told[k].a);
                }
            }
            for (var k = 0; k < count; k = k + 1) {
                if (probes[k].w >= 0.0 && all_told[k].a >= max(5e-4, 0.6 * best) && all_met[k] <= within) {
                    seen = seen + all_told[k].rgb * all_told[k].a;
                    sure = sure + all_told[k].a;
                }
            }
        }
        var light = vec3<f32>(0.0);
        if (sure > 0.0) {
            light = seen / sure;
        } else if (count > 0) {
            let on = equirect(ray);
            light = radiance(textureSampleLevel(colours, colours_sampler, on.xy, i32(probes[0].w), 0.0).rgb);
        }
        return vec4<f32>(light * scale.x * view.exposure, 1.0);
    }

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
    if (trust > 0.0) {
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
