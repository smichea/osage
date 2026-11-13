// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

/**
 * What the world sends of its paddock, drawn (D-084): crates across the
 * road, bobbing and turning, hidden while broken; tyres rolling where they
 * go; cones standing, or sliding and tumbling when a kart knocked one
 * (D-079); oil spreading; drones flying with their rotors turning. And the
 * rain, when the race is wet (D-068): streaks falling round the camera, the
 * sky and the haze grey.
 */

import * as THREE from "three";
import { Item, type Paddock } from "./gen/kart/v1/kart_pb.js";

/** How fast a rolling tyre slows and a knocked cone slides, m/s² (osage-kart-items). */
const ROLLING = 2.0;
const CONE_SLIDE = 9.0;
const TYRE_RADIUS = 0.35;
const CONE_RADIUS = 0.25;
const OIL_RADIUS = 1.5;
const CONE_LIFE = 20;
const OIL_LIFE = 15;

/** A pool of meshes of one shape: as many shown as there are things of it this frame. */
class Pool {
  private readonly meshes: THREE.Object3D[] = [];
  private used = 0;
  constructor(
    private readonly group: THREE.Group,
    private readonly make: () => THREE.Object3D,
  ) {}
  begin() {
    this.used = 0;
  }
  next(): THREE.Object3D {
    if (this.used === this.meshes.length) {
      const mesh = this.make();
      this.meshes.push(mesh);
      this.group.add(mesh);
    }
    const mesh = this.meshes[this.used++];
    mesh.visible = true;
    return mesh;
  }
  end() {
    for (let i = this.used; i < this.meshes.length; i++) this.meshes[i].visible = false;
  }
}

export class PaddockView {
  readonly group = new THREE.Group();
  private readonly crates: Pool;
  private readonly tyres: Pool;
  private readonly cones: Pool;
  private readonly oils: Pool;
  private readonly drones: Pool;

  constructor() {
    const amber = new THREE.MeshStandardMaterial({
      color: "#ffb02e",
      emissive: "#7a3d00",
      transparent: true,
      opacity: 0.82,
      roughness: 0.25,
    });
    const rubber = new THREE.MeshStandardMaterial({ color: "#1c1c1e", roughness: 0.9 });
    const orange = new THREE.MeshStandardMaterial({ color: "#ff6a13", roughness: 0.6 });
    const white = new THREE.MeshStandardMaterial({ color: "#f2f2f2", roughness: 0.6 });
    const oil = new THREE.MeshStandardMaterial({
      color: "#0b0b10",
      roughness: 0.05,
      metalness: 0.3,
      transparent: true,
      opacity: 0.85,
    });
    const body = new THREE.MeshStandardMaterial({ color: "#30343a", roughness: 0.4, metalness: 0.5 });
    const crate = new THREE.BoxGeometry(1, 1, 1);
    this.crates = new Pool(this.group, () => new THREE.Mesh(crate, amber));
    const tyre = new THREE.TorusGeometry(TYRE_RADIUS * 0.72, TYRE_RADIUS * 0.28, 10, 20);
    this.tyres = new Pool(this.group, () => new THREE.Mesh(tyre, rubber));
    const cone = new THREE.ConeGeometry(CONE_RADIUS, 0.6, 14);
    const band = new THREE.CylinderGeometry(CONE_RADIUS * 0.62, CONE_RADIUS * 0.72, 0.1, 14);
    const foot = new THREE.BoxGeometry(CONE_RADIUS * 2.4, 0.04, CONE_RADIUS * 2.4);
    this.cones = new Pool(this.group, () => {
      const root = new THREE.Group();
      const c = new THREE.Mesh(cone, orange);
      c.position.y = 0.3;
      const b = new THREE.Mesh(band, white);
      b.position.y = 0.3;
      const f = new THREE.Mesh(foot, orange);
      f.position.y = 0.02;
      root.add(c, b, f);
      return root;
    });
    const patch = new THREE.CircleGeometry(OIL_RADIUS, 28).rotateX(-Math.PI / 2);
    this.oils = new Pool(this.group, () => new THREE.Mesh(patch, oil));
    const hull = new THREE.BoxGeometry(0.5, 0.14, 0.5);
    const rotor = new THREE.BoxGeometry(0.42, 0.01, 0.05);
    this.drones = new Pool(this.group, () => {
      const root = new THREE.Group();
      root.add(new THREE.Mesh(hull, body));
      for (const [x, z] of [
        [0.3, 0.3],
        [-0.3, 0.3],
        [0.3, -0.3],
        [-0.3, -0.3],
      ]) {
        const r = new THREE.Mesh(rotor, body);
        r.position.set(x, 0.1, z);
        r.name = "rotor";
        root.add(r);
      }
      return root;
    });
  }

  /**
   * Draws `paddock` as the world sent it, `lead` seconds on (as far as the
   * player's kart is predicted), at `time` seconds; `road` gives the road's
   * height under a point.
   */
  update(paddock: Paddock | undefined, time: number, lead: number, road: (x: number, z: number) => number) {
    for (const pool of [this.crates, this.tyres, this.cones, this.oils, this.drones]) pool.begin();
    for (const crate of paddock?.crates ?? []) {
      if (!crate.standing) continue;
      const mesh = this.crates.next();
      const bob = 0.08 * Math.sin(time * 2.4 + crate.x);
      mesh.position.set(crate.x, road(crate.x, crate.z) + 0.75 + bob, crate.z);
      mesh.rotation.set(0.25, time * 0.9 + crate.z, 0.15);
      mesh.scale.setScalar(0.9);
    }
    for (const thing of paddock?.things ?? []) {
      const speed = Math.hypot(thing.vx, thing.vz);
      // Where it is `lead` on, slowing as it does.
      const ahead = (slowing: number) =>
        speed > 1e-3 ? Math.max(speed * lead - 0.5 * slowing * lead * lead, 0) / speed : 0;
      switch (thing.kind) {
        case Item.TYRE:
        case Item.STACK: {
          const k = ahead(ROLLING);
          const [x, z] = [thing.x + thing.vx * k, thing.z + thing.vz * k];
          const mesh = this.tyres.next();
          mesh.position.set(x, road(x, z) + TYRE_RADIUS, z);
          // Upright, rolling the way it goes.
          mesh.rotation.set(0, Math.atan2(thing.vx, thing.vz) + Math.PI / 2, 0);
          mesh.rotateZ(-(time * speed) / TYRE_RADIUS);
          break;
        }
        case Item.CONE: {
          const k = ahead(CONE_SLIDE);
          const [x, z] = [thing.x + thing.vx * k, thing.z + thing.vz * k];
          const mesh = this.cones.next();
          mesh.position.set(x, road(x, z), z);
          if (speed > 0.05) {
            // Knocked: tumbling over and over as it slides (D-079).
            const axis = new THREE.Vector3(thing.vz, 0, -thing.vx).normalize();
            mesh.quaternion.setFromAxisAngle(axis, time * 2 * Math.min(speed, 12));
            mesh.position.y += 0.25;
          } else {
            mesh.quaternion.identity();
          }
          // Dropped: falls from the kart's tail over a moment.
          const age = CONE_LIFE - thing.life;
          if (age < 0.45) mesh.position.y += 0.8 * (1 - age / 0.45);
          break;
        }
        case Item.OIL: {
          const mesh = this.oils.next();
          const spread = Math.min(Math.max((OIL_LIFE - thing.life) / 0.35, 0.15), 1);
          mesh.position.set(thing.x, road(thing.x, thing.z) + 0.04, thing.z);
          mesh.scale.setScalar(Math.min(Math.max(thing.life, 0.2), 1) * spread);
          break;
        }
        case Item.DRONE: {
          const [x, z] = [thing.x + thing.vx * lead, thing.z + thing.vz * lead];
          const mesh = this.drones.next();
          mesh.position.set(x, road(x, z) + thing.height, z);
          mesh.rotation.set(0.18 + 0.08 * Math.sin(time * 7 + x), Math.atan2(thing.vx, thing.vz), 0);
          for (const child of mesh.children) if (child.name === "rotor") child.rotation.y = time * 60;
          break;
        }
        default:
          break;
      }
    }
    for (const pool of [this.crates, this.tyres, this.cones, this.oils, this.drones]) pool.end();
  }
}

/** Streaks of rain round the camera, falling, and a grey sky while the race is wet. */
export class RainView {
  readonly group = new THREE.Group();
  private readonly streaks: THREE.LineSegments;
  private readonly drops: Float32Array;
  private wet = 0;
  private static readonly COUNT = 1400;
  private static readonly BOX = 30;

  constructor() {
    const geometry = new THREE.BufferGeometry();
    this.drops = new Float32Array(RainView.COUNT * 6);
    for (let i = 0; i < RainView.COUNT; i++) {
      const [x, y, z] = [
        (Math.random() - 0.5) * RainView.BOX,
        Math.random() * 14,
        (Math.random() - 0.5) * RainView.BOX,
      ];
      this.drops.set([x, y, z, x + 0.05, y - 0.6, z], i * 6);
    }
    geometry.setAttribute("position", new THREE.BufferAttribute(this.drops, 3));
    this.streaks = new THREE.LineSegments(
      geometry,
      new THREE.LineBasicMaterial({ color: "#c8d2dc", transparent: true, opacity: 0.35 }),
    );
    this.streaks.frustumCulled = false;
    this.group.add(this.streaks);
    this.group.visible = false;
  }

  /**
   * Follows `rain` (the race is wet), easing in and out over a few seconds;
   * falls round `camera`, and greys `scene`'s sky and haze from `dry`.
   */
  update(rain: boolean, seconds: number, camera: THREE.Camera, scene: THREE.Scene, dry: THREE.Color) {
    this.wet = Math.min(Math.max(this.wet + (rain ? seconds : -seconds) / 4, 0), 1);
    this.group.visible = this.wet > 0.01;
    const grey = new THREE.Color("#8d949b");
    const sky = dry.clone().lerp(grey, this.wet);
    if (scene.background instanceof THREE.Color) scene.background.copy(sky);
    if (scene.fog instanceof THREE.Fog) {
      scene.fog.color.copy(sky);
      scene.fog.far = 420 - 220 * this.wet;
    }
    if (!this.group.visible) return;
    (this.streaks.material as THREE.LineBasicMaterial).opacity = 0.35 * this.wet;
    const fall = 18 * seconds;
    const half = RainView.BOX / 2;
    const at = camera.position;
    for (let i = 0; i < RainView.COUNT; i++) {
      const o = i * 6;
      let [x, y, z] = [this.drops[o], this.drops[o + 1] - fall, this.drops[o + 2]];
      // Round the camera: a drop that falls out of the box comes back above it.
      if (y < at.y - 4) y += 14;
      if (y > at.y + 12) y -= 14;
      if (x < at.x - half) x += RainView.BOX;
      if (x > at.x + half) x -= RainView.BOX;
      if (z < at.z - half) z += RainView.BOX;
      if (z > at.z + half) z -= RainView.BOX;
      this.drops.set([x, y, z, x + 0.05, y - 0.6, z], o);
    }
    this.streaks.geometry.attributes.position.needsUpdate = true;
  }
}
