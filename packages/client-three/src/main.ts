// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

// Phase 1 browser client: renders what an osage world replicates.
// Server URL: ?server=ws://host:port (default ws://<this host>:7878).

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

const cube = new THREE.BoxGeometry(1, 1, 1);
const material = new THREE.MeshStandardMaterial({ color: "#3fb950", roughness: 0.4 });
const meshes = new Map<bigint, THREE.Object3D>();

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
  const source = descriptor?.sourceOffer;
  // AGPL section 13: show where the server's Corresponding Source is.
  hud.innerHTML =
    `world <b>${escape(descriptor?.name ?? "")}</b> · you are ${escape(identity?.perWorldId ?? "")}<br>` +
    (source ? `server source: <a href="${escape(source.url)}" target="_blank" rel="noopener">${escape(source.url)}</a> (${escape(source.version)})` : "");

  const world = new World(session.welcome);
  session.onMessage((message) => {
    for (const event of world.apply(message)) {
      if (event.kind === "entered") {
        const content = world.get(event.entity, types.contentRef, components.ContentRefSchema);
        if (content?.uri !== types.builtinCube) continue;
        const mesh = new THREE.Mesh(cube, material);
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
} catch (error) {
  hud.textContent = String(error);
}

renderer.setAnimationLoop(() => renderer.render(scene, camera));
