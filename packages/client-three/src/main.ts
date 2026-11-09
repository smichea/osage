// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

// Phase 1 browser client: renders what an osage world replicates.
// Server URL: ?server=ws://host:port (default ws://<this host>:7878).

import { create, toBinary } from "@bufbuild/protobuf";
import { World, components, connect, hello, types, v0 } from "@osage/sdk";
import * as THREE from "three";

const hud = document.getElementById("hud")!;
const url = new URLSearchParams(location.search).get("server") ?? `ws://${location.hostname || "127.0.0.1"}:7878`;

const renderer = new THREE.WebGLRenderer({ antialias: true });
renderer.setPixelRatio(Math.min(devicePixelRatio, 1.75));
document.body.appendChild(renderer.domElement);

const scene = new THREE.Scene();
scene.background = new THREE.Color("#0d1117");
scene.add(new THREE.HemisphereLight("#cfe3ff", "#20262e", 1.2));
const sun = new THREE.DirectionalLight("#ffffff", 2);
sun.position.set(4, 8, 3);
scene.add(sun, new THREE.GridHelper(12, 12, "#30363d", "#21262d"));

const camera = new THREE.PerspectiveCamera(55, 1, 0.1, 200);
camera.position.set(6, 5, 8);
camera.lookAt(0, 0.5, 0);

function resize() {
  renderer.setSize(innerWidth, innerHeight);
  camera.aspect = innerWidth / innerHeight;
  camera.updateProjectionMatrix();
}
addEventListener("resize", resize);
resize();

const cubeGeometry = new THREE.BoxGeometry(1, 1, 1);
const cubeMaterial = new THREE.MeshStandardMaterial({ color: "#3fb950", roughness: 0.4 });
const avatarGeometry = new THREE.CapsuleGeometry(0.35, 1.1, 4, 12);
const deviceGeometry = new THREE.CylinderGeometry(0.3, 0.3, 0.6, 24);
const deviceMaterial = new THREE.MeshStandardMaterial({ color: "#a371f7", emissive: "#3b1f6b", roughness: 0.3 });
const myMaterial = new THREE.MeshStandardMaterial({ color: "#58a6ff", roughness: 0.5 });
const othersMaterial = new THREE.MeshStandardMaterial({ color: "#d29922", roughness: 0.5 });
const meshes = new Map<bigint, THREE.Object3D>();

/** A built-in shape; avatars stand on their origin, so the capsule is lifted. */
function builtin(uri: string, mine: boolean): THREE.Object3D | undefined {
  if (uri === types.builtinCube) return new THREE.Mesh(cubeGeometry, cubeMaterial);
  if (uri === types.builtinDevice) {
    const device = new THREE.Mesh(deviceGeometry, deviceMaterial);
    device.position.y = 0.3;
    const group = new THREE.Group();
    group.add(device);
    return group;
  }
  if (uri !== types.builtinAvatar) return undefined;
  const group = new THREE.Group();
  const body = new THREE.Mesh(avatarGeometry, mine ? myMaterial : othersMaterial);
  body.position.y = 0.9;
  group.add(body);
  return group;
}

// Keyboard: WASD or arrows, in world axes (W/up = -Z).
const pressed = new Set<string>();
addEventListener("keydown", (e) => pressed.add(e.key.toLowerCase()));
addEventListener("keyup", (e) => pressed.delete(e.key.toLowerCase()));
addEventListener("blur", () => pressed.clear());
function direction(): { x: number; y: number; z: number } {
  const key = (...names: string[]) => (names.some((n) => pressed.has(n)) ? 1 : 0);
  const x = key("d", "arrowright") - key("a", "arrowleft");
  const z = key("s", "arrowdown") - key("w", "arrowup");
  const length = Math.hypot(x, z) || 1;
  return { x: x / length, y: 0, z: z / length };
}

function place(object: THREE.Object3D, transform: components.Transform | undefined) {
  if (!transform) return;
  const { translation: t, rotation: r, scale: s } = transform;
  if (t) object.position.set(t.x, t.y, t.z);
  if (r) object.quaternion.set(r.x, r.y, r.z, r.w);
  if (s) object.scale.set(s.x, s.y, s.z);
}

function escape(text: string): string {
  return text.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);
}

try {
  const session = await connect(url, hello("osage-client-three", "0.1.0", v0.Role.PLAYER, v0.Tier.T2_STANDARD));
  const { world: descriptor, identity } = session.welcome;
  const me = identity?.perWorldId ?? "";
  const source = descriptor?.sourceOffer;
  // AGPL section 13: show where the server's Corresponding Source is.
  hud.innerHTML =
    `world <b>${escape(descriptor?.name ?? "")}</b> · you are ${escape(me)} · move with WASD or arrows<br>` +
    (source ? `server source: <a href="${escape(source.url)}" target="_blank" rel="noopener">${escape(source.url)}</a> (${escape(source.version)})` : "");

  const world = new World(session.welcome);

  // Real devices and their latest readings, with their age: an old value must
  // never look current (design/02 M1).
  const devicesPanel = document.createElement("div");
  hud.appendChild(devicesPanel);
  setInterval(() => {
    const rows: string[] = [];
    for (const entity of world.entities()) {
      const device = world.get(entity, types.device, components.DeviceSchema);
      if (!device) continue;
      const readings = world.get(entity, types.measurements, components.MeasurementsSchema)?.values ?? [];
      const values = readings.map((m) => {
        const age = Math.max(0, Date.now() / 1000 - Number(m.sourceTimeUs) / 1e6);
        return `${escape(m.name)} ${m.value.toFixed(1)} ${escape(m.unit)} <small>(${age.toFixed(0)} s ago)</small>`;
      });
      rows.push(`<b>${escape(device.name)}</b> · owner ${escape(device.ownerPerWorldId)}<br>${values.join(" · ")}`);
    }
    devicesPanel.innerHTML = rows.length ? `<hr>${rows.join("<br>")}` : "";
  }, 500);
  session.onMessage((message) => {
    for (const event of world.apply(message)) {
      if (event.kind === "entered") {
        const content = world.get(event.entity, types.contentRef, components.ContentRefSchema);
        const controller = world.get(event.entity, types.controller, components.ControllerSchema);
        const mesh = content && builtin(content.uri, controller?.perWorldId === me);
        if (!mesh) continue;
        meshes.set(event.entity, mesh);
        scene.add(mesh);
        place(mesh, world.get(event.entity, types.transform, components.TransformSchema));
      } else if (event.kind === "changed" && event.typeName === types.transform) {
        const mesh = meshes.get(event.entity);
        if (mesh) place(mesh, world.get(event.entity, types.transform, components.TransformSchema));
      } else if (event.kind === "left") {
        const mesh = meshes.get(event.entity);
        if (mesh) scene.remove(mesh);
        meshes.delete(event.entity);
      }
    }
  });
  session.closed().then(() => (hud.textContent = "disconnected"));

  // Send movement intents for my avatar at 20 Hz while a key is held, and once
  // more when released so that it stops at once.
  const moveType = world.typeId(types.moveIntent);
  let sequence = 0n;
  let moving = false;
  setInterval(() => {
    const avatar = world.controlledBy(me);
    if (avatar === undefined || moveType === undefined) return;
    const d = direction();
    const now = d.x !== 0 || d.z !== 0;
    if (!now && !moving) return;
    moving = now;
    const value = toBinary(components.MoveIntentSchema, create(components.MoveIntentSchema, { direction: d }));
    session.sendIntent(++sequence, avatar, moveType, value);
  }, 50);
} catch (error) {
  hud.textContent = String(error);
}

renderer.setAnimationLoop(() => renderer.render(scene, camera));
