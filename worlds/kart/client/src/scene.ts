// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// What the kart world looks like: the circuit built from `kart.track/1`
// (road, kerbs, walls, start line, trees) and the karts — all procedural.

import * as THREE from "three";
import { BARRIER, KERB, TURBO_CHARGES } from "./physics.js";

/** The colour of drift sparks by the level of mini-turbo charged: none, blue, orange, violet. */
export function sparkColour(charge: number): string {
  const level = TURBO_CHARGES.filter((c) => charge >= c).length;
  return ["#fff6d5", "#3fa9ff", "#ff8c1a", "#bf4dff"][level];
}

export const PALETTE = ["#e5534b", "#539bf5", "#57ab5a", "#c69026", "#b083f0", "#39c5cf", "#e0823d", "#f778ba"];

/** A point of the centre line with its unit direction and right vector, and its distance along. */
interface Frame {
  x: number;
  z: number;
  fx: number;
  fz: number;
  rx: number;
  rz: number;
  along: number;
}

function frames(points: ArrayLike<number>): Frame[] {
  const n = points.length / 2;
  const at = (i: number) => [points[2 * ((i + n) % n)], points[2 * ((i + n) % n) + 1]];
  let along = 0;
  return Array.from({ length: n }, (_, i) => {
    const [[ax, az], [x, z], [bx, bz]] = [at(i - 1), at(i), at(i + 1)];
    const length = Math.hypot(bx - ax, bz - az) || 1;
    const [fx, fz] = [(bx - ax) / length, (bz - az) / length];
    // Right of a kart facing (fx, fz) is (-fz, fx).
    const frame = { x, z, fx, fz, rx: -fz, rz: fx, along };
    const [nx, nz] = at(i + 1);
    along += Math.hypot(nx - x, nz - z);
    return frame;
  });
}

/** The road's height along the centre line: 0, but over a crest. */
type Lift = (along: number) => number;

/**
 * A strip from `inner` to `outer` metres right of the centre line (negative:
 * left), coloured per segment, `y` above the road; with a `height`, a wall
 * from the ground up to that far above the road. Over a crest (D-051) it
 * rises and falls with the road, in half-metre pieces.
 */
function strip(
  f: Frame[],
  inner: number,
  outer: number,
  y: number,
  colour: (i: number) => THREE.Color,
  height = 0,
  lift: Lift = () => 0,
) {
  const positions: number[] = [];
  const colours: number[] = [];
  for (let i = 0; i < f.length; i++) {
    const [a, b] = [f[i], f[(i + 1) % f.length]];
    const length = Math.hypot(b.x - a.x, b.z - a.z);
    const level = [0, 0.25, 0.5, 0.75, 1].every((t) => lift(a.along + t * length) === 0);
    const pieces = level ? 1 : Math.max(1, Math.ceil(length / 0.5));
    const c = colour(i);
    for (let k = 0; k < pieces; k++) {
      const corner = (t: number, d: number, up: number, base = false) => {
        const [x, z] = [a.x + t * (b.x - a.x), a.z + t * (b.z - a.z)];
        const [rx, rz] = [a.rx + t * (b.rx - a.rx), a.rz + t * (b.rz - a.rz)];
        const road = lift(a.along + t * length);
        return [x + rx * d, base ? 0 : road + y + up, z + rz * d];
      };
      const [t0, t1] = [k / pieces, (k + 1) / pieces];
      const quad = height
        ? [corner(t0, inner, 0, true), corner(t1, inner, 0, true), corner(t1, inner, height), corner(t0, inner, height)]
        : [corner(t0, inner, 0), corner(t1, inner, 0), corner(t1, outer, 0), corner(t0, outer, 0)];
      for (const index of [0, 1, 2, 0, 2, 3]) positions.push(...quad[index]);
      for (let v = 0; v < 6; v++) colours.push(c.r, c.g, c.b);
    }
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colours, 3));
  geometry.computeVertexNormals();
  return new THREE.Mesh(
    geometry,
    new THREE.MeshStandardMaterial({ vertexColors: true, roughness: 0.9, side: THREE.DoubleSide }),
  );
}

export interface CircuitView {
  group: THREE.Group;
  /** Centre line point and heading at `along` metres from the first point. */
  at(along: number): { x: number; z: number; heading: number };
}

export function buildCircuit(
  points: ArrayLike<number>,
  halfWidth: number,
  verge: number,
  start: number,
  lift: Lift = () => 0,
): CircuitView {
  const f = frames(points);
  const group = new THREE.Group();

  const ground = new THREE.Mesh(
    new THREE.CircleGeometry(600, 64).rotateX(-Math.PI / 2),
    new THREE.MeshStandardMaterial({ color: "#4f8a3c", roughness: 1 }),
  );
  group.add(ground);

  const asphalt = new THREE.Color("#3b4048");
  const asphaltLight = new THREE.Color("#41474f");
  group.add(strip(f, -halfWidth, halfWidth, 0.02, (i) => (i % 2 ? asphalt : asphaltLight), 0, lift));
  const [red, white] = [new THREE.Color("#d73a3a"), new THREE.Color("#f0f0f0")];
  const kerb = (i: number) => (Math.floor(i / 2) % 2 ? red : white);
  group.add(
    strip(f, halfWidth, halfWidth + KERB, 0.03, kerb, 0, lift),
    strip(f, -halfWidth - KERB, -halfWidth, 0.03, kerb, 0, lift),
  );
  // The barriers' face, where a kart's outline stops (D-045).
  const face = halfWidth + verge + BARRIER;
  // The verges, which rise with the road over a crest.
  const grass = new THREE.Color("#4f8a3c");
  group.add(
    strip(f, halfWidth + KERB, face, 0.01, () => grass, 0, lift),
    strip(f, -face, -halfWidth - KERB, 0.01, () => grass, 0, lift),
  );
  const [blue, light] = [new THREE.Color("#2f6fd6"), new THREE.Color("#e8eef7")];
  const wall = (i: number) => (i % 2 ? blue : light);
  group.add(strip(f, face, 0, 0, wall, 0.9, lift), strip(f, -face, 0, 0, wall, 0.9, lift));

  // Distances along the centre line, to place things by distance.
  const distances = [0];
  for (let i = 1; i <= f.length; i++) {
    const [a, b] = [f[i - 1], f[i % f.length]];
    distances.push(distances[i - 1] + Math.hypot(b.x - a.x, b.z - a.z));
  }
  const length = distances[f.length];
  const at = (along: number) => {
    const d = ((along % length) + length) % length;
    let i = 0;
    while (distances[i + 1] <= d) i++;
    const [a, b] = [f[i], f[(i + 1) % f.length]];
    const t = (d - distances[i]) / (distances[i + 1] - distances[i] || 1);
    return { x: a.x + t * (b.x - a.x), z: a.z + t * (b.z - a.z), heading: Math.atan2(b.x - a.x, b.z - a.z) };
  };

  // Start/finish line: a chequered band across the road.
  const canvas = document.createElement("canvas");
  [canvas.width, canvas.height] = [16, 2];
  const paint = canvas.getContext("2d")!;
  for (let i = 0; i < 16; i++) {
    for (let j = 0; j < 2; j++) {
      paint.fillStyle = (i + j) % 2 ? "#111" : "#fafafa";
      paint.fillRect(i, j, 1, 1);
    }
  }
  const texture = new THREE.CanvasTexture(canvas);
  texture.magFilter = THREE.NearestFilter;
  const line = new THREE.Mesh(
    new THREE.PlaneGeometry(2 * halfWidth, 1.6).rotateX(-Math.PI / 2),
    new THREE.MeshStandardMaterial({ map: texture, roughness: 0.8 }),
  );
  const s = at(start);
  line.position.set(s.x, 0.04, s.z);
  line.rotation.y = s.heading;
  group.add(line);

  // Trees outside the walls, placed the same way on every client.
  let seed = 7;
  const random = () => ((seed = (seed * 16807) % 2147483647) / 2147483647);
  const trunk = new THREE.CylinderGeometry(0.25, 0.35, 1.6, 6);
  const crown = new THREE.ConeGeometry(1.6, 3.6, 7);
  const [bark, leaves] = [
    new THREE.MeshStandardMaterial({ color: "#6b4a2b" }),
    new THREE.MeshStandardMaterial({ color: "#2f6b34", roughness: 0.8 }),
  ];
  for (let i = 0; i < f.length; i += 3) {
    const side = random() < 0.5 ? -1 : 1;
    const away = halfWidth + verge + 5 + random() * 25;
    const [x, z] = [f[i].x + f[i].rx * away * side, f[i].z + f[i].rz * away * side];
    if (f.some((p) => Math.hypot(p.x - x, p.z - z) < halfWidth + verge + 3)) continue;
    const tree = new THREE.Group();
    const scale = 0.8 + random() * 0.7;
    tree.add(new THREE.Mesh(trunk, bark).translateY(0.8), new THREE.Mesh(crown, leaves).translateY(3.2));
    tree.scale.setScalar(scale);
    tree.position.set(x, 0, z);
    group.add(tree);
  }
  return { group, at };
}

export interface KartView {
  root: THREE.Group;
  body: THREE.Group;
  wheels: THREE.Mesh[];
  sparks: THREE.Mesh[];
  flame: THREE.Mesh;
  /** How far through a trick's roll, 0 to 1. */
  trick: number;
}

const shared = {
  chassis: new THREE.BoxGeometry(1.3, 0.35, 2.0),
  nose: new THREE.BoxGeometry(0.9, 0.22, 0.6),
  wing: new THREE.BoxGeometry(1.4, 0.08, 0.35),
  wheel: new THREE.CylinderGeometry(0.32, 0.32, 0.3, 14).rotateZ(Math.PI / 2),
  head: new THREE.SphereGeometry(0.3, 14, 10),
  spark: new THREE.SphereGeometry(0.14, 6, 4),
  flame: new THREE.ConeGeometry(0.22, 0.9, 10).rotateX(-Math.PI / 2),
  rubber: new THREE.MeshStandardMaterial({ color: "#1c1c1c", roughness: 0.7 }),
  helmet: new THREE.MeshStandardMaterial({ color: "#f5f5f5", roughness: 0.3 }),
  dark: new THREE.MeshStandardMaterial({ color: "#22272e", roughness: 0.5 }),
};

/** What a ghost is made of (D-062): pale, seen through. */
const ghostly = new THREE.MeshStandardMaterial({ color: "#b8dcff", roughness: 0.6, transparent: true, opacity: 0.35, depthWrite: false });

/** A kart facing +Z, its origin on the ground; a ghost's is the same all over, seen through. */
export function buildKart(colour: number, ghost = false): KartView {
  const paint = ghost
    ? ghostly
    : new THREE.MeshStandardMaterial({ color: PALETTE[colour % PALETTE.length], roughness: 0.35, metalness: 0.2 });
  const [dark, helmet, rubber] = ghost ? [ghostly, ghostly, ghostly] : [shared.dark, shared.helmet, shared.rubber];
  const body = new THREE.Group();
  body.add(
    new THREE.Mesh(shared.chassis, paint).translateY(0.4),
    new THREE.Mesh(shared.nose, paint).translateY(0.35).translateZ(1.25),
    new THREE.Mesh(shared.wing, dark).translateY(0.85).translateZ(-1.0),
    new THREE.Mesh(shared.head, helmet).translateY(1.0).translateZ(-0.2),
  );
  const wheels = [-1, 1].flatMap((side) =>
    [-1, 1].map((end) => {
      const wheel = new THREE.Mesh(shared.wheel, rubber);
      wheel.position.set(0.75 * side, 0.32, 0.72 * end);
      return wheel;
    }),
  );
  body.add(...wheels);
  const sparks = [-1, 1].map((side) => {
    const spark = new THREE.Mesh(shared.spark, new THREE.MeshBasicMaterial({ color: "#ffffff" }));
    spark.position.set(0.75 * side, 0.15, -0.95);
    spark.visible = false;
    return spark;
  });
  const flame = new THREE.Mesh(shared.flame, new THREE.MeshBasicMaterial({ color: "#ff9a3c", transparent: true, opacity: 0.85 }));
  flame.position.set(0, 0.45, -1.45);
  flame.visible = false;
  body.add(...sparks, flame);
  const root = new THREE.Group();
  // Heading, then the nose's pitch, then a trick's roll.
  root.rotation.order = "YXZ";
  root.add(body);
  return { root, body, wheels, sparks, flame, trick: 0 };
}

/** Pose and effects of a kart from its state. */
export function poseKart(
  view: KartView,
  x: number,
  z: number,
  heading: number,
  state: { speed: number; drift: number; charge: number; boost: number; height?: number; climb?: number; trick?: number },
  seconds: number,
) {
  view.root.position.set(x, state.height ?? 0, z);
  view.root.rotation.y = heading;
  // Up a crest and in the air the nose follows the path; a trick is a roll.
  view.root.rotation.x = -Math.atan((state.climb ?? 0) / Math.max(4, Math.abs(state.speed)));
  view.trick = (state.trick ?? 0) > 0 ? Math.min(1, view.trick + seconds / 0.45) : 0;
  view.root.rotation.z = 2 * Math.PI * view.trick * view.trick * (3 - 2 * view.trick);
  // A drifting kart is drawn turned into the slide.
  const slip = -state.drift * 0.35;
  view.body.rotation.y += (slip - view.body.rotation.y) * Math.min(1, seconds * 10);
  for (const wheel of view.wheels) wheel.rotation.x += (state.speed * seconds) / 0.32;
  const colour = sparkColour(state.charge);
  for (const spark of view.sparks) {
    spark.visible = state.drift !== 0 && Math.random() < 0.8;
    (spark.material as THREE.MeshBasicMaterial).color.set(colour);
    spark.scale.setScalar(0.6 + Math.random() * 0.9);
  }
  view.flame.visible = state.boost > 0;
  view.flame.scale.set(1, 1, 0.7 + Math.random() * 0.6);
}
