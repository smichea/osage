// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// Browser client of the osage kart world. Everything it knows about the
// circuit and the race comes from the world; its own kart is predicted with
// the world's physics (D-031), the others are drawn from the server's states.
// Server URL: ?server=ws://host:port (default ws://<this host>:7878).

import { create, toBinary } from "@bufbuild/protobuf";
import { World, components, connect, hello, types, v0 } from "@osage/sdk";
import * as THREE from "three";
import {
  ControlsSchema,
  KartStateSchema,
  Phase,
  type Race,
  RaceSchema,
  type Racer,
  RacerSchema,
  TrackSchema,
} from "./gen/kart/v1/kart_pb.js";
import { Input } from "./input.js";
import { type KartValues, LONG_TURBO_CHARGE, Physics, STEP_SECONDS, SHORT_TURBO_CHARGE } from "./physics.js";
import { Predictor } from "./prediction.js";
import { type CircuitView, type KartView, PALETTE, buildCircuit, buildKart, poseKart } from "./scene.js";

const KART = {
  controls: "kart.controls/1",
  state: "kart.state/1",
  racer: "kart.racer/1",
  track: "kart.track/1",
  race: "kart.race/1",
};

const $ = (id: string) => document.getElementById(id)!;
const params = new URLSearchParams(location.search);
const url = params.get("server") ?? `ws://${location.hostname || "127.0.0.1"}:7878`;
/** `?debug`: prediction diagnostics on screen and in `window.kartDebug()`. */
const debug = params.has("debug");

const renderer = new THREE.WebGLRenderer({ antialias: true });
renderer.setPixelRatio(Math.min(devicePixelRatio, 1.75));
document.body.prepend(renderer.domElement);
const scene = new THREE.Scene();
scene.background = new THREE.Color("#87b5e0");
scene.fog = new THREE.Fog("#87b5e0", 120, 420);
scene.add(new THREE.HemisphereLight("#dbeaff", "#4f6b3a", 1.4));
const sun = new THREE.DirectionalLight("#ffffff", 2.2);
sun.position.set(60, 120, 40);
scene.add(sun);
const camera = new THREE.PerspectiveCamera(65, 1, 0.1, 900);
camera.position.set(0, 120, -160);
camera.lookAt(-60, 0, 0);
function resize() {
  renderer.setSize(innerWidth, innerHeight);
  camera.aspect = innerWidth / innerHeight;
  camera.updateProjectionMatrix();
}
addEventListener("resize", resize);
resize();

const escape = (text: string) => text.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);
const clock = (ms: number) => `${Math.floor(ms / 60000)}:${((ms / 1000) % 60).toFixed(2).padStart(5, "0")}`;

interface Shown {
  view: KartView;
  /** Where it is drawn, gliding towards the server's latest state. */
  x: number;
  z: number;
  heading: number;
}

try {
  const physics = await Physics.load(await (await fetch("/physics.wasm")).arrayBuffer());
  const session = await connect(url, hello("osage-kart-client", "0.1.0", v0.Role.PLAYER, v0.Tier.T2_STANDARD));
  const world = new World(session.welcome);
  const me = session.welcome.identity?.perWorldId ?? "";
  const controlsType = world.typeId(KART.controls);
  const input = new Input();
  const karts = new Map<bigint, Shown>();
  let circuit: CircuitView | undefined;
  let mine: bigint | undefined;
  let predictor: Predictor | undefined;
  /** Server clock minus ours, milliseconds, from the latest state datagram. */
  let serverOffset = 0;
  const serverNow = () => Date.now() + serverOffset;

  const source = session.welcome.world?.sourceOffer;
  // AGPL section 13: show where the server's Corresponding Source is.
  $("status").innerHTML =
    `world <b>${escape(session.welcome.world?.name ?? "")}</b> · you are ${escape(me)} · via ${session.transport}<br>` +
    `drive: ↑ ↓ ← → or WASD · drift: Space or Shift (hold through a bend, release for a mini-turbo) · gamepad works too<br>` +
    (source ? `server source: <a href="${escape(source.url)}" target="_blank" rel="noopener">${escape(source.url)}</a> (${escape(source.version)})` : "");

  const race = (): Race | undefined => {
    for (const entity of world.entities()) {
      const value = world.get(entity, KART.race, RaceSchema);
      if (value) return value;
    }
    return undefined;
  };
  const racer = (entity: bigint): Racer | undefined => world.get(entity, KART.racer, RacerSchema);
  const state = (entity: bigint): KartValues | undefined => world.get(entity, KART.state, KartStateSchema);
  const finished = (entity: bigint) => (racer(entity)?.finishMs ?? 0n) > 0n;

  session.onMessage((message) => {
    if (message.message.case === "state") serverOffset = Number(message.message.value.serverTimeUs) / 1000 - Date.now();
    for (const event of world.apply(message)) {
      if (event.kind === "entered") {
        const track = world.get(event.entity, KART.track, TrackSchema);
        if (track) {
          physics.setTrack(track.points, track.halfWidth, track.verge);
          if (circuit) scene.remove(circuit.group);
          circuit = buildCircuit(track.points, track.halfWidth, track.verge, track.start);
          scene.add(circuit.group);
        }
        const who = racer(event.entity);
        const kart = state(event.entity);
        if (who && kart) {
          const shown = { view: buildKart(who.color), x: kart.x, z: kart.z, heading: kart.heading };
          karts.set(event.entity, shown);
          scene.add(shown.view.root);
          if (world.get(event.entity, types.controller, components.ControllerSchema)?.perWorldId === me) {
            mine = event.entity;
            predictor = new Predictor(physics);
            predictor.reconcile(kart, 0n);
          }
        }
      } else if (event.kind === "left") {
        const shown = karts.get(event.entity);
        if (shown) scene.remove(shown.view.root);
        karts.delete(event.entity);
        if (event.entity === mine) [mine, predictor] = [undefined, undefined];
      } else if (event.kind === "changed" && event.typeName === KART.state && event.entity === mine && predictor) {
        const server = world.get(event.entity, KART.state, KartStateSchema)!;
        predictor.reconcile(server, server.applied);
      }
    }
  });
  session.closed().then(() => ($("race").textContent = "disconnected"));
  addEventListener("visibilitychange", () => document.hidden && input.release());

  /** One physics step of my kart, predicted here; false if I am not driving. */
  function drive(): boolean {
    if (mine === undefined || !predictor || finished(mine)) return false;
    const wanted = input.read(STEP_SECONDS);
    // The lights hold the karts: the world ignores the throttle until the start.
    if (race()?.phase !== Phase.RACING) wanted.throttle = 0;
    predictor.step(wanted);
    return true;
  }

  /** The frame's steps to the world, in one message. */
  function send(fresh: number) {
    if (mine === undefined || !predictor || controlsType === undefined || fresh === 0) return;
    const steps = predictor.toSend(fresh);
    const controls = toBinary(ControlsSchema, create(ControlsSchema, { steps }));
    session.sendIntent(steps[steps.length - 1].sequence, mine, controlsType, controls);
  }

  const look = new THREE.Vector3();
  function render(seconds: number) {
    predictor?.fade(seconds);
    const glide = 1 - Math.exp(-seconds * 18);
    for (const [entity, shown] of karts) {
      const server = state(entity);
      if (!server) continue;
      let target = { x: server.x, z: server.z, heading: server.heading };
      let effects: KartValues = server;
      if (entity === mine && predictor?.kart && !finished(entity)) {
        const { kart, error } = predictor;
        target = { x: kart.x + error.x, z: kart.z + error.z, heading: kart.heading + error.heading };
        [shown.x, shown.z, shown.heading] = [target.x, target.z, target.heading];
        effects = kart;
      } else {
        shown.x += (target.x - shown.x) * glide;
        shown.z += (target.z - shown.z) * glide;
        shown.heading += Math.atan2(Math.sin(target.heading - shown.heading), Math.cos(target.heading - shown.heading)) * glide;
      }
      poseKart(shown.view, shown.x, shown.z, shown.heading, effects, seconds);
    }

    // Chase camera behind my kart, or the leader's when watching.
    const leader = race()?.order[0];
    const followed = karts.get(mine ?? leader ?? -1n);
    if (followed) {
      const [fx, fz] = [Math.sin(followed.heading), Math.cos(followed.heading)];
      const behind = new THREE.Vector3(followed.x - fx * 6.5, 2.9, followed.z - fz * 6.5);
      camera.position.lerp(behind, 1 - Math.exp(-seconds * 7));
      look.lerp(new THREE.Vector3(followed.x + fx * 4, 1, followed.z + fz * 4), 1 - Math.exp(-seconds * 10));
      camera.lookAt(look);
    }
    renderer.render(scene, camera);
  }

  function hud() {
    const now = race();
    if (!now) return;
    const remaining = Number(now.phaseEndsUs) / 1000 - serverNow();
    const lights = $("lights");
    if (now.phase === Phase.COUNTDOWN) lights.textContent = String(Math.max(1, Math.ceil(remaining / 1000)));
    else if (now.phase === Phase.RACING && serverNow() - Number(now.startedUs) / 1000 < 1000) lights.textContent = "GO!";
    else lights.textContent = "";

    const elapsed = now.startedUs > 0n ? serverNow() - Number(now.startedUs) / 1000 : 0;
    const my = mine === undefined ? undefined : racer(mine);
    $("race").innerHTML = my
      ? `<b>P${my.position}</b>/${now.order.length || karts.size} · lap ${Math.min(Math.max(my.lap, 1), now.laps)}/${now.laps}<br>` +
        (my.finishMs > 0n ? `finished in ${clock(Number(my.finishMs))}` : now.phase === Phase.RACING ? clock(elapsed) : "")
      : "watching · you will be on the grid of the next race";

    const rows = now.order
      .map((entity) => [entity, racer(entity)] as const)
      .filter((row): row is readonly [bigint, Racer] => row[1] !== undefined)
      .map(([entity, r]) => {
        const status = r.finishMs > 0n ? clock(Number(r.finishMs)) : `lap ${Math.min(Math.max(r.lap, 1), now.laps)}`;
        const kind = r.bot ? " <small>bot</small>" : "";
        return `<div class="${entity === mine ? "me" : ""}">${r.position}. <span class="dot" style="background:${PALETTE[r.color % PALETTE.length]}"></span>${escape(r.name)}${kind} · ${status}</div>`;
      });
    $("board").innerHTML = rows.join("") || "";

    const kart = mine !== undefined ? (predictor?.kart ?? state(mine)) : undefined;
    if (kart) {
      const charge = Math.min(1, kart.charge / LONG_TURBO_CHARGE);
      const colour = kart.charge >= LONG_TURBO_CHARGE ? "#ff8c1a" : kart.charge >= SHORT_TURBO_CHARGE ? "#3fa9ff" : "#fff6d5";
      $("speed").innerHTML =
        `<b>${Math.round(Math.abs(kart.speed) * 3.6)}</b> km/h${kart.boost > 0 ? " · <b style='color:#ff9a3c'>TURBO</b>" : ""}<br>` +
        `mini-turbo <span class="gauge"><i style="width:${(charge * 100).toFixed(0)}%;background:${colour}"></i></span>`;
    }

    if (debug && mine !== undefined && predictor) {
      const server = world.get(mine, KART.state, KartStateSchema);
      const diagnostics = {
        unacknowledged: predictor.unacknowledged,
        aheadOfServer: server && predictor.kart ? Math.hypot(predictor.kart.x - server.x, predictor.kart.z - server.z) : 0,
        corrections: predictor.corrections,
        largestCorrection: predictor.largestCorrection,
        shownError: Math.hypot(predictor.error.x, predictor.error.z),
        applied: Number(server?.applied ?? 0n),
      };
      (window as unknown as { kartDebug: () => object }).kartDebug = () => diagnostics;
      $("status").dataset.debug = JSON.stringify(diagnostics);
      $("speed").innerHTML += `<br><small>${Object.entries(diagnostics).map(([k, v]) => `${k} ${typeof v === "number" ? +v.toFixed(3) : v}`).join(" · ")}</small>`;
    }

    const banner = $("banner");
    if (now.phase === Phase.FINISHED) {
      const podium = rows.slice(0, 8).join("");
      banner.innerHTML = `<div class="panel"><b>Results</b> · next race in ${Math.max(0, Math.ceil(remaining / 1000))} s<hr>${podium}</div>`;
    } else if (now.phase === Phase.WAITING) {
      banner.innerHTML = `<div class="panel">waiting for drivers…</div>`;
    } else {
      banner.innerHTML = "";
    }
  }

  let last = performance.now();
  let pending = 0;
  let sinceHud = 0;
  renderer.setAnimationLoop((time) => {
    const seconds = Math.min(0.25, (time - last) / 1000);
    last = time;
    // Fixed steps of 1/60 s, like the world's ticks.
    pending += seconds;
    let steps = 0;
    let driven = 0;
    while (pending >= STEP_SECONDS && steps < 8) {
      if (drive()) driven++;
      pending -= STEP_SECONDS;
      steps++;
    }
    if (steps === 8) pending = 0;
    send(driven);
    render(seconds);
    sinceHud += seconds;
    if (sinceHud > 0.1) {
      hud();
      sinceHud = 0;
    }
  });
} catch (error) {
  $("race").textContent = String(error);
}
