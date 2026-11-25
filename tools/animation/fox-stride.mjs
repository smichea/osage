// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
// Drive Bevy's foot controller from authored TRS samples and skin both outputs with Three.js.
import {execFileSync} from "node:child_process";
import {mkdir, writeFile} from "node:fs/promises";
import {AnimationMixer, Object3D, Vector3} from "three";

import {footVertices, loadModel} from "./skinned-model.mjs";

const asset = "assets/khronos/Fox.glb";
const dest = "bench/out/ville-paris/npc-animation";
await mkdir(dest, {recursive: true});
const scale = 0.01, hz = 60;
const phase = Number(process.argv[2] ?? 0);
if (!Number.isFinite(phase) || phase < 0 || phase >= 1 || process.argv.length > 3)
    throw Error("usage: node tools/animation/fox-stride.mjs [phase from 0 inclusive to 1 exclusive]");
const suffix = phase === 0 ? "" : "-phase-" + phase;
const bindings = [
    ["FL", "b_LeftUpperArm_09", "b_LeftForeArm_010", "b_LeftHand_011"],
    ["FR", "b_RightUpperArm_06", "b_RightForeArm_07", "b_RightHand_08"],
    ["HL", "b_LeftLeg02_016", "b_LeftFoot01_017", "b_LeftFoot02_018"],
    ["HR", "b_RightLeg02_020", "b_RightFoot01_021", "b_RightFoot02_022"],
];
const model = await loadModel(asset);
const scene = model.scene;
scene.scale.setScalar(scale);
const armature = scene.getObjectByName("root");
const mixer = new AnimationMixer(scene);
const paws = bindings.map(([name, ...path]) => {
    const chain = path.map(n => scene.getObjectByName(n));
    if (chain.some(n => !n) || chain[1].parent !== chain[0] || chain[2].parent !== chain[1])
        throw Error("broken limb");
    const ankle = chain[2], pad = new Object3D();
    ankle.add(pad);
    return {name, chain: [...chain, pad], ankle, pad, vertices: footVertices(scene, ankle)};
});
// A planar triangle can cross the raised edge between two passing vertices. Track
// each edge of each triangle whose three vertices belong to the measured paw.
for (const paw of paws) {
    const groups = new Map();
    for (const [slot, [mesh, id]] of paw.vertices.entries()) {
        if (!groups.has(mesh)) groups.set(mesh, new Map());
        groups.get(mesh).set(id, slot);
    }
    const edges = new Map();
    paw.faces = [];
    for (const [mesh, slots] of groups) {
        const g = mesh.geometry;
        const indices = g.index?.array ?? Array.from({length: g.attributes.position.count}, (_, i) => i);
        for (let i = 0; i < indices.length; i += 3) {
            const tri = [0, 1, 2].map(j => slots.get(indices[i + j]));
            if (tri.includes(undefined)) continue;
            paw.faces.push(tri);
            for (let j = 0; j < 3; j++) {
                const a = tri[j], b = tri[(j + 1) % 3];
                edges.set(Math.min(a, b) + "/" + Math.max(a, b), [a, b]);
            }
        }
    }
    paw.edges = [...edges.values()];
    paw.posed = paw.vertices.map(() => new Vector3());
}
const nodes = [];
armature.traverse(n => nodes.push(n));
const parents = nodes.map(n => nodes.includes(n.parent) ? nodes.indexOf(n.parent) : null);
function setClip(name) {
    mixer.stopAllAction();
    const clip = model.animations.find(a => a.name === name);
    mixer.clipAction(clip).play();
    return clip;
}
function sample(t) {
    mixer.setTime(t);
    scene.updateMatrixWorld(true);
}
const vector = new Vector3();
function vertices(paw, consume) {
    for (const [slot, [mesh, i]] of paw.vertices.entries()) {
        mesh.getVertexPosition(i, vector).applyMatrix4(mesh.matrixWorld);
        consume(vector, mesh, i, slot);
    }
}
const quantile = (a, q) => [...a].sort((x, y) => x - y)[Math.floor((a.length - 1) * q)];
scene.updateMatrixWorld(true);
for (const paw of paws) {
    const points = new Map();
    const skin = new Map();
    const skinSlots = [];
    for (const [mesh, i] of paw.vertices) {
        const {skinIndex, skinWeight, position} = mesh.geometry.attributes;
        const vertex = new Vector3().fromBufferAttribute(position, i).applyMatrix4(mesh.bindMatrix);
        const lanes = paw.chain.map(() => [0, 0, 0, 0]);
        for (let j = 0; j < 4; j++) {
            const weight = skinWeight.getComponent(i, j);
            if (!weight) continue;
            const joint = skinIndex.getComponent(i, j);
            const slot = paw.chain.indexOf(mesh.skeleton.bones[joint]);
            if (slot < 0) throw Error("paw vertex weighted outside its limb");
            const local = vertex.clone().applyMatrix4(mesh.skeleton.boneInverses[joint]).multiplyScalar(scale);
            lanes[slot] = [...local.toArray(), lanes[slot][3] + weight];
        }
        const key = JSON.stringify(lanes);
        if (!skin.has(key)) skin.set(key, {index: skin.size, lanes});
        skinSlots.push(skin.get(key).index);
        let w = 0;
        for (let j = 0; j < 4; j++) {
            if (mesh.skeleton.bones[skinIndex.getComponent(i, j)] === paw.ankle) w += skinWeight.getComponent(i, j);
        }
        if (w < 0.9999) continue;
        const site = paw.ankle.worldToLocal(mesh.getVertexPosition(i, new Vector3()).applyMatrix4(mesh.matrixWorld))
                         .multiplyScalar(scale)
                         .toArray();
        points.set(site.map(v => v.toFixed(6)).join(","), site);
    }
    paw.sole = [...points.values()];
    paw.skin = [...skin.values()].map(s => s.lanes);
    paw.skinFaces = paw.faces.map(f => f.map(i => skinSlots[i]));
    if (!paw.sole.length) throw Error("no rigid sole samples");
}
const walk = setClip("Walk");
for (const paw of paws) {
    const choices = [];
    for (let i = 0; i < 120; i++) {
        sample(walk.duration * i / 120);
        let pos = new Vector3(0, Infinity, 0);
        vertices(paw, p => {
            if (p.y < pos.y) pos.copy(p);
        });
        choices.push({height: pos.y, local: paw.ankle.worldToLocal(pos).toArray()});
    }
    const low = quantile(choices.map(c => c.height), 0.25);
    const support = choices.filter(c => c.height <= low);
    paw.pad.position.fromArray([0, 1, 2].map(j => quantile(support.map(c => c.local[j]), 0.5)));
}
const strip = [1.0, 2.0, 0.08];
const height = (ground, z) => ground && ground[0] <= z && z < ground[1] ? ground[2] : 0;
function localPose(n) {
    if (n.scale.distanceTo(new Vector3(1, 1, 1)) > 1e-5) throw Error("non-rigid joint");
    return [n.position.toArray().map(v => v * scale), n.quaternion.toArray()];
}
const report = {
    scale,
    hz,
    phase,
    bindings: paws.map(p => ({
                           name: p.name,
                           joints: p.chain.slice(0, 3).map(n => n.name),
                           pad_local_asset_units: p.pad.position.toArray(),
                           weighted_vertices: p.vertices.length,
                           rigid_sole_sites: p.sole.length,
                           skinned_sites: p.skin.length,
                           fully_selected_triangles: p.faces.length
                       })),
    clips: {}
};
for (const name of ["Walk", "Run"]) {
    const clip = setClip(name);
    scene.position.set(0, 0, 0);
    const calibration = [];
    for (let i = 0; i < 180; i++) {
        sample(clip.duration * i / 180);
        calibration.push(paws.map(p => p.pad.getWorldPosition(new Vector3())));
    }
    const band = 0.015;
    const speedSamples = [];
    for (let i = 0; i < calibration.length; i++) {
        for (let j = 0; j < paws.length; j++) {
            if (calibration[i][j].y <= band) {
                const before = calibration[(i + 179) % 180][j], after = calibration[(i + 1) % 180][j];
                speedSamples.push(-(after.z - before.z) / (2 * clip.duration / 180));
            }
        }
    }
    if (!speedSamples.length) throw Error("no support samples");
    const speed = quantile(speedSamples, 0.5);
    if (!Number.isFinite(speed) || speed <= 0) throw Error("invalid forward speed");
    const clipReport = {duration_s: clip.duration, speed_m_s: speed, band_m: band, terrains: {}, worst_samples: {}};
    for (const [terrain, ground] of [["flat", null], ["curb", strip]]) {
        const frames = [], rows = [];
        const deepest = {};
        const previousContacts = new Map();
        const count = Math.max(Math.ceil(hz * 3.5 / speed), Math.ceil(hz * 5 * clip.duration));
        let rootY = 0;
        for (let f = 0; f < count; f++) {
            scene.position.set(0, 0, 0);
            sample(f / hz + phase * clip.duration);
            const z = speed * f / hz;
            rootY += (height(ground, z) - rootY) / 6;
            frames.push({root: [0, rootY, z], poses: nodes.map(localPose)});
        }
        const input = {
            parents,
            armature: 0,
            chains: paws.map(p => p.chain.map(n => nodes.indexOf(n))),
            band,
            dt: 1 / hz,
            ground,
            frames
        };
        const solve = (trial) => JSON.parse(execFileSync("target/debug/examples/model_stride", [], {
            input: JSON.stringify(trial),
            encoding: "utf8",
            maxBuffer: 32 * 1024 * 1024,
        }));
        const cycle = {
            start: phase,
            duration: clip.duration,
            travel: [0, speed * clip.duration],
            samples: paws.map((_, j) => calibration.map(frame => frame[j].toArray()))
        };
        const surfaceInput = {...input, skin_soles: paws.map(p => p.skin), faces: paws.map(p => p.skinFaces)};
        const variants = {
            marker: solve(input),
            sole: solve({...input, soles: paws.map(p => p.sole)}),
            skin: solve({...input, skin_soles: paws.map(p => p.skin)}),
            surface: solve(surfaceInput),
            cycle: solve({...surfaceInput, cycle})
        };
        if (Object.values(variants).some(output => output.length !== count)) throw Error("bad replay length");
        for (let f = 0; f < count; f++) {
            const frame = frames[f];
            scene.position.fromArray(frame.root);
            for (const [i, n] of nodes.entries()) {
                n.position.fromArray(frame.poses[i][0]).multiplyScalar(1 / scale);
                n.quaternion.fromArray(frame.poses[i][1]);
            }
            const record = {frame: f, root_y: frame.root[1], body_drop_m: {raw: 0}, feet: {}};
            for (const mode of ["raw", ...Object.keys(variants)]) {
                if (mode !== "raw") {
                    const solved = variants[mode];
                    record.body_drop_m[mode] = frame.poses[input.armature][0][1] - solved[f].armature[1];
                    armature.position.fromArray(solved[f].armature).multiplyScalar(1 / scale);
                    for (const [i, p] of paws.entries())
                        for (let j = 0; j < 3; j++) {
                            p.chain[j].quaternion.fromArray(solved[f].limbs[i][j]);
                        }
                }
                scene.updateMatrixWorld(true);
                for (const paw of paws) {
                    let lowest = Infinity;
                    let first = 0, second = 0, weight = 0;
                    vertices(paw, (p, mesh, i, slot) => {
                        paw.posed[slot].copy(p);
                        const clearance = p.y - height(ground, p.z);
                        if (clearance < lowest) {
                            lowest = clearance;
                            first = second = slot;
                        }
                        if (!deepest[mode] || clearance < deepest[mode].clearance_m) {
                            const {skinIndex, skinWeight} = mesh.geometry.attributes;
                            const influences = {};
                            for (let lane = 0; lane < 4; lane++) {
                                const weight = skinWeight.getComponent(i, lane);
                                if (weight)
                                    influences[mesh.skeleton.bones[skinIndex.getComponent(i, lane)].name] = weight;
                            }
                            deepest[mode] = {
                                frame: f,
                                paw: paw.name,
                                vertex: i,
                                position_m: p.toArray(),
                                clearance_m: clearance,
                                influences
                            };
                        }
                    });
                    const pad = paw.pad.getWorldPosition(new Vector3());
                    if (![lowest, pad.x, pad.y, pad.z].every(Number.isFinite)) throw Error("nonfinite posed foot");
                    record.feet[paw.name] ??= {};
                    let surfaceClearance = lowest;
                    if (ground) {
                        for (const [i, j] of paw.edges) {
                            const a = paw.posed[i], b = paw.posed[j];
                            for (const z of [ground[0], ground[1]]) {
                                if (Math.min(a.z, b.z) < z && z <= Math.max(a.z, b.z)) {
                                    const t = (z - a.z) / (b.z - a.z);
                                    const clearance = a.y + t * (b.y - a.y) - ground[2];
                                    if (clearance < surfaceClearance) {
                                        surfaceClearance = clearance;
                                        first = i;
                                        second = j;
                                        weight = t;
                                    }
                                }
                            }
                        }
                    }
                    // Follow the same material point into the next frame, including an
                    // edge over the curb. The nearest point can switch as the paw rolls.
                    const key = mode + "/" + paw.name;
                    const old = previousContacts.get(key);
                    let materialStep = null;
                    if (old) {
                        const moved = vector.lerpVectors(paw.posed[old.first], paw.posed[old.second], old.weight);
                        materialStep = Math.hypot(moved.x - old.x, moved.z - old.z);
                    }
                    const contact = vector.lerpVectors(paw.posed[first], paw.posed[second], weight);
                    previousContacts.set(key, {first, second, weight, x: contact.x, z: contact.z});
                    record.feet[paw.name][mode] = {
                        previous_contact_travel_m: materialStep,
                        pad: pad.toArray(),
                        clearance: lowest,
                        surface_clearance: surfaceClearance
                    };
                }
            }
            rows.push(record);
        }
        // Restore authored transforms, including joints without animation tracks.
        for (const [i, n] of nodes.entries()) {
            n.position.fromArray(frames[0].poses[i][0]).multiplyScalar(1 / scale);
            n.quaternion.fromArray(frames[0].poses[i][1]);
        }
        const scores = {};
        for (const paw of paws) {
            const score = {};
            for (const mode of ["raw", ...Object.keys(variants)]) {
                const slip = [], contactSlip = [], speeds = [], penetration = [], gap = [], surfaceGap = [], surface = [];
                let previous = null, lastPad = null;
                for (const row of rows) {
                    const p = row.feet[paw.name];
                    penetration.push(Math.max(0, -p[mode].clearance));
                    surface.push(Math.max(0, -p[mode].surface_clearance));
                    const low = p.raw.pad[1] - row.root_y <= band;
                    if (low) {
                        gap.push(Math.max(0, p[mode].clearance));
                        surfaceGap.push(Math.max(0, p[mode].surface_clearance));
                    }
                    if (low && previous) {
                        contactSlip.push(hz * p[mode].previous_contact_travel_m);
                        slip.push(hz * Math.hypot(p[mode].pad[0] - previous[0], p[mode].pad[2] - previous[2]));
                    }
                    const pad = p[mode].pad;
                    if (lastPad) speeds.push(hz * Math.hypot(pad[0] - lastPad[0], pad[2] - lastPad[2]));
                    lastPad = pad;
                    previous = low ? pad : null;
                }
                if (!slip.length) throw Error("no stance pairs for " + name + " " + paw.name);
                score[mode] = {
                    stance_pairs: slip.length,
                    p95_contact_material_slip_m_s: quantile(contactSlip, .95),
                    max_pad_speed_m_s: Math.max(...speeds),
                    max_stance_pad_slip_m_s: Math.max(...slip),
                    max_body_drop_m: Math.max(...rows.map(r => r.body_drop_m[mode])),
                    p95_pad_slip_m_s: quantile(slip, .95),
                    p95_stance_gap_m: quantile(gap, .95),
                    p95_stance_surface_gap_m: quantile(surfaceGap, .95),
                    p95_skin_penetration_m: quantile(penetration, .95),
                    max_skin_penetration_m: Math.max(...penetration),
                    max_surface_penetration_m: Math.max(...surface)
                };
            }
            scores[paw.name] = score;
        }
        clipReport.terrains[terrain] = scores;
        clipReport.worst_samples[terrain] = deepest;
        await writeFile(dest + "/fox-" + name + "-" + terrain + suffix + ".json", JSON.stringify(rows));
    }
    if ([0, 0.25, 0.5, 0.75].includes(phase)) {
        const feet = Object.values(clipReport.terrains.curb);
        const worst = mode => Math.max(...feet.map(f => f[mode].max_skin_penetration_m));
        const gap = Math.max(...feet.map(f => f.sole.p95_stance_gap_m));
        if (worst("sole") >= Math.min(worst("marker"), worst("raw")) ||
            worst("sole") > (name === "Walk" ? 0.031 : 0.009) || gap > 0.025)
            throw Error("curb skin clearance regression for " + name + " at phase " + phase);
        if (worst("skin") > 0.00005 ||
            feet.some(
                f => f.skin.p95_stance_gap_m > 0.025 || f.skin.p95_pad_slip_m_s > f.sole.p95_pad_slip_m_s + 0.001))
            throw Error("weighted vertex clearance regression for " + name + " at phase " + phase);
        if (feet.some(
                f => f.surface.max_surface_penetration_m > 0.00005 ||
                    f.surface.p95_stance_surface_gap_m > 0.025 ||
                    f.surface.p95_pad_slip_m_s > f.sole.p95_pad_slip_m_s + 0.001))
            throw Error("weighted surface clearance regression for " + name + " at phase " + phase);
    }
    for (const feet of Object.values(clipReport.terrains)) {
        const worstFor = (mode, metric) => Math.max(...Object.values(feet).map(f => f[mode][metric]));
        const material = "p95_contact_material_slip_m_s";
        if (worstFor("cycle", material) >= 0.2 * worstFor("surface", material) ||
            worstFor("cycle", "p95_pad_slip_m_s") > 0.002 ||
            worstFor("cycle", "p95_stance_surface_gap_m") > 0.025 ||
            worstFor("cycle", "max_surface_penetration_m") > 0.00005)
            throw Error("planned stance regression for " + name + " at phase " + phase);
    }
    report.clips[name] = clipReport;
}
await writeFile(dest + "/fox-stride" + suffix + ".json", JSON.stringify(report, null, 2) + "\n");
console.log(JSON.stringify(report.clips, null, 2));
