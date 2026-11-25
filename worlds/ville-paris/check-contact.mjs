// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// Independently intersect the replay's flight with the triangle meshes from a standard glTF reader.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { Matrix3, Raycaster, Vector3 } from "three";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";

const [dir, scenePath, replayPath] = process.argv.slice(2);
assert.ok(dir, "node check-contact.mjs RECEIPT_DIR [SCENE.glb REPLAY.json]");
const scene = await readFile(scenePath ?? new URL("./fixtures/bir-hakeim.glb", import.meta.url));
const replayBytes = await readFile(replayPath ?? new URL("./fixtures/descent.json", import.meta.url));
const replay = JSON.parse(replayBytes);
const receipt = JSON.parse(await readFile(join(dir, "receipt.json")));
const sha = bytes => createHash("sha256").update(bytes).digest("hex");
assert.equal(sha(scene), receipt.inputs.scene_sha256);
assert.equal(sha(replayBytes), receipt.inputs.replay_sha256);

const model = await new GLTFLoader().parseAsync(scene.buffer.slice(scene.byteOffset, scene.byteOffset + scene.length), "");
model.scene.updateMatrixWorld(true);
const supports = [];
const ids = new Map();
model.scene.traverse(node => {
  const index = model.parser.associations.get(node)?.nodes;
  const name = model.parser.json.nodes[index]?.name;
  if (!name?.startsWith("support/")) return;
  node.traverse(child => {
    if (child.isMesh) {
      supports.push(child);
      ids.set(child, name.slice("support/".length));
    }
  });
});

// Sum the arithmetic progression of vertical speeds up to the terminal speed.
// This evaluates each tick directly in double precision, independently of the Rust step.
function foot(route, n) {
  const [x, y, z] = route.start_m;
  const [vx, vy, vz] = route.velocity_m_s;
  const hz = replay.tick_hz;
  const g = replay.gravity_m_s2;
  const uncapped = Math.min(n, Math.max(0, Math.floor((vy + replay.terminal_m_s) * hz / g)));
  const fall = uncapped * vy - g / hz * uncapped * (uncapped + 1) / 2 - (n - uncapped) * replay.terminal_m_s;
  return new Vector3(x + n * vx / hz, y + fall / hz, z + n * vz / hz);
}

function landing(route) {
  const ray = new Raycaster();
  for (let tick = 1; tick <= route.ticks; tick++) {
    const a = foot(route, tick - 1);
    const diff = foot(route, tick).sub(a);
    if (diff.y >= 0) continue;
    ray.far = diff.length();
    ray.set(a, diff.normalize());
    const hit = ray.intersectObjects(supports, false).find(hit =>
      hit.face.normal.clone().applyNormalMatrix(new Matrix3().getNormalMatrix(hit.object.matrixWorld)).y > 0.999999);
    if (hit) return { tick, support: ids.get(hit.object), foot: hit.point };
  }
  throw new Error("No mesh landing on " + route.id);
}

const checks = replay.routes.map(route => {
  const measured = receipt.routes.find(r => r.id === route.id);
  const hit = landing(route);
  assert.equal(hit.support, route.expected_support);
  assert.equal(hit.support, measured.support);
  assert.equal(hit.tick, measured.contact_tick);
  const error = hit.foot.distanceTo(new Vector3(...measured.foot_m));
  assert.ok(error < 0.01, "Foot contact differs by " + error + " m on " + route.id);
  return { route: route.id, surface: hit.support, tick: hit.tick, position_error_m: error };
});
console.log(JSON.stringify({ inputs: receipt.inputs, checks }, null, 2));
