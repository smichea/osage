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
  ChoiceSchema,
  ControlsSchema,
  KartStateSchema,
  Phase,
  type Race,
  RaceCall,
  RaceSchema,
  type Racer,
  RacerSchema,
  type Record as KartRecord,
  RecordsSchema,
  RosterSchema,
  TrackSchema,
} from "./gen/kart/v1/kart_pb.js";
import { Input } from "./input.js";
import { type KartValues, Physics, STEP_SECONDS, TURBO_CHARGES } from "./physics.js";
import { Predictor } from "./prediction.js";
import { type CircuitView, type KartView, PALETTE, buildCircuit, buildKart, poseKart, sparkColour } from "./scene.js";

const KART = {
  controls: "kart.controls/1",
  state: "kart.state/1",
  racer: "kart.racer/1",
  track: "kart.track/1",
  race: "kart.race/1",
  records: "kart.records/1",
  choice: "kart.choice/1",
  roster: "kart.roster/1",
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
  /** The palette colour it is painted in: its driver's (D-055). */
  color: number;
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
  const sourceLink = source
    ? `<a href="${escape(source.url)}" target="_blank" rel="noopener">${input.touchScreen ? "server source" : escape(source.url)}</a> (${escape(source.version)})`
    : "";
  $("status").innerHTML = input.touchScreen
    ? `${escape(me)} · via ${session.transport} · ${sourceLink}<br>hold DRIFT through bends, release for a turbo (three levels); press it in the air for a trick`
    : `world <b>${escape(session.welcome.world?.name ?? "")}</b> · you are ${escape(me)} · via ${session.transport}<br>` +
      `drive: ↑ ↓ ← → or WASD · drift: Space or Shift (hold through a bend, release for a mini-turbo of up to three levels; press it off a crest for a trick) · throttle as the lights go out for a boost, too early and you stall · gamepad works too<br>` +
      (source ? `server source: ${sourceLink}` : "");

  const race = (): Race | undefined => {
    for (const entity of world.entities()) {
      const value = world.get(entity, KART.race, RaceSchema);
      if (value) return value;
    }
    return undefined;
  };
  const racer = (entity: bigint): Racer | undefined => world.get(entity, KART.racer, RacerSchema);
  /** The world's records (D-053), between races: best laps and races, five each. */
  const records = (myName: string | undefined): string => {
    let value;
    for (const entity of world.entities()) {
      value = world.get(entity, KART.records, RecordsSchema);
      if (value) break;
    }
    if (!value || value.laps.length === 0) return "";
    const rows = (list: KartRecord[]) =>
      list
        .slice(0, 5)
        .map(
          (r, i) =>
            `<div class="${r.name === myName ? "me" : ""}">${i + 1}. ${escape(r.name)} · ${clock(r.ms)}${r.fresh ? " <b style='color:#ffd24d'>NEW!</b>" : ""}</div>`,
        )
        .join("");
    const races = value.races.length ? `<br><small>best races (${value.raceLaps} laps)</small>${rows(value.races)}` : "";
    return `<hr><b>Records</b><br><small>best laps</small>${rows(value.laps)}${races}`;
  };
  const state = (entity: bigint): KartValues | undefined => world.get(entity, KART.state, KartStateSchema);
  // The kart as its own driver built it (D-055): what the predictor steps.
  const built = (entity: bigint, kart: KartValues): KartValues => {
    const b = racer(entity)?.build;
    return b ? { ...kart, top: b.top, accel: b.accel, handling: b.handling, grip: b.grip, weight: b.weight } : kart;
  };
  const finished = (entity: bigint) => (racer(entity)?.finishMs ?? 0n) > 0n;

  session.onMessage((message) => {
    if (message.message.case === "state") serverOffset = Number(message.message.value.serverTimeUs) / 1000 - Date.now();
    for (const event of world.apply(message)) {
      // A track arrives, or the next of a cup (D-056): the circuit is built again.
      const newTrack = event.kind === "entered" || (event.kind === "changed" && event.typeName === KART.track);
      if (newTrack) {
        const track = world.get(event.entity, KART.track, TrackSchema);
        if (track) {
          physics.setTrack(track.points, track.halfWidth, track.verge, track.crests);
          if (circuit) scene.remove(circuit.group);
          // Over a crest the road is as high as the physics has it (D-051).
          circuit = buildCircuit(track.points, track.halfWidth, track.verge, track.start, (along) =>
            physics.roadHeight(along),
          );
          scene.add(circuit.group);
          if (event.kind === "changed") predictor = mine !== undefined ? new Predictor(physics) : undefined;
        }
      }
      if (event.kind === "entered") {
        const who = racer(event.entity);
        const kart = state(event.entity);
        if (who && kart) {
          const shown = { view: buildKart(who.color), color: who.color, x: kart.x, z: kart.z, heading: kart.heading };
          karts.set(event.entity, shown);
          scene.add(shown.view.root);
          if (world.get(event.entity, types.controller, components.ControllerSchema)?.perWorldId === me) {
            mine = event.entity;
            predictor = new Predictor(physics);
            predictor.reconcile(built(event.entity, kart), 0n);
          }
        }
      } else if (event.kind === "left") {
        const shown = karts.get(event.entity);
        if (shown) scene.remove(shown.view.root);
        karts.delete(event.entity);
        if (event.entity === mine) [mine, predictor] = [undefined, undefined];
      } else if (event.kind === "changed" && event.typeName === KART.state && event.entity === mine && predictor) {
        const server = world.get(event.entity, KART.state, KartStateSchema)!;
        predictor.reconcile(built(event.entity, server), server.applied);
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

  /** A call about the race (the host starts it), carried by a few messages: one may be lost. */
  let call = RaceCall.NONE;
  let callsLeft = 0;
  const startRace = () => {
    const now = race();
    if (mine !== undefined && now?.phase === Phase.READY && now.host === mine) [call, callsLeft] = [RaceCall.START, 6];
  };
  addEventListener("keydown", (event) => event.key === "Enter" && startRace());
  $("banner").addEventListener("click", (event) => {
    const id = (event.target as HTMLElement).id;
    if (id === "start") startRace();
    const turns: Record<string, [number, number]> = {
      "driver-back": [-1, 0],
      "driver-on": [1, 0],
      "kart-back": [0, -1],
      "kart-on": [0, 1],
    };
    if (turns[id]) choose(...turns[id]);
    if (id === "event-on") chooseEvent();
  });

  /** The event picked here to race next (D-057), an index into the race's events. */
  let chosenEvent: number | undefined;
  /** The next event, a cup or a track alone: the host's is raced, the others' are votes. */
  function chooseEvent() {
    const now = race();
    const choiceType = world.typeId(KART.choice);
    if (mine === undefined || !now || !now.events.length || choiceType === undefined || !between()) return;
    const at = chosenEvent ?? Math.max(0, now.events.indexOf(now.event));
    chosenEvent = (at + 1) % now.events.length;
    const choice = create(ChoiceSchema, { event: now.events[chosenEvent] });
    session.sendIntent(0n, mine, choiceType, toBinary(ChoiceSchema, choice));
  }
  const TRACK_NAMES: Record<string, string> = {
    paddock: "The Paddock",
    coast: "Gull Point",
    forest: "Pinewood",
    harbour: "Harbour Town",
  };
  const trackName = (id: string) => TRACK_NAMES[id] ?? id;
  const eventName = (event: string, cup?: string) =>
    event.startsWith("cup:") ? `${cup ?? event.slice(4)} (cup)` : `${trackName(event.slice(6))} (one track)`;
  /** The cup and its standings, and what is raced next with a button to change it. */
  const cupPanel = () => {
    const now = race();
    if (!now) return "";
    const myName = mine !== undefined ? racer(mine)?.name : undefined;
    let html = "";
    const cup = now.cup;
    if (cup) {
      const over = cup.race > cup.tracks.length;
      const head = over
        ? `<b>${escape(cup.name)}</b> · final standings`
        : `<b>${escape(cup.name)}</b> · race ${cup.race} of ${cup.tracks.length}: ${escape(trackName(cup.tracks[cup.race - 1]))}`;
      const rows = cup.standings
        .slice(0, 8)
        .map((s, i) => {
          const places = s.places.map((p) => (p === 0 ? "–" : p)).join(" ");
          const podium = over && i < 3 ? [" 🏆", " 🥈", " 🥉"][i] : "";
          return `<div class="${s.name === myName ? "me" : ""}">${i + 1}. ${escape(s.name)} · ${s.points} pts <small>${places}</small>${podium}</div>`;
        })
        .join("");
      html += `<hr>${head}${rows}`;
    }
    if (now.events.length && mine !== undefined) {
      const hosting = now.host === mine || now.host === 0n;
      const shown = chosenEvent !== undefined ? now.events[chosenEvent] : now.event;
      const votes = now.votes.map((v) => `${escape(eventName(v.event, cup?.name))} ${v.drivers}`).join(", ");
      html += hosting
        ? `<hr>next: <b>${escape(eventName(shown, cup?.name))}</b> <button id="event-on">change (T)</button>`
        : `<hr>next: <b>${escape(eventName(now.event, cup?.name))}</b> · your vote: ${escape(eventName(shown, cup?.name))} <button id="event-on">change (T)</button>`;
      if (votes) html += `<br><small>votes: ${votes}</small>`;
    }
    return html;
  };

  /** The driver and kart picked here (D-055), as indices into the world's roster. */
  let chosen: [number, number] | undefined;
  const roster = () => {
    for (const entity of world.entities()) {
      const found = world.get(entity, KART.roster, RosterSchema);
      if (found) return found;
    }
  };
  const between = () => [Phase.WAITING, Phase.READY, Phase.FINISHED].includes(race()?.phase ?? Phase.UNSPECIFIED);
  /** Picks the next or previous driver or kart; on the grid the world changes the kart at once. */
  function choose(driver: number, kart: number) {
    const list = roster();
    const choiceType = world.typeId(KART.choice);
    if (mine === undefined || !list || choiceType === undefined || !between()) return;
    const me = racer(mine);
    const at = (ids: string[], id = "") => Math.max(0, ids.indexOf(id));
    const [d, k] = chosen ?? [at(list.drivers.map((d) => d.id), me?.driver), at(list.karts.map((k) => k.id), me?.kart)];
    const turn = (i: number, by: number, of: number) => (((i + by) % of) + of) % of;
    chosen = [turn(d, driver, list.drivers.length), turn(k, kart, list.karts.length)];
    const choice = create(ChoiceSchema, { driver: list.drivers[chosen[0]].id, kart: list.karts[chosen[1]].id });
    session.sendIntent(0n, mine, choiceType, toBinary(ChoiceSchema, choice));
  }
  addEventListener("keydown", (event) => {
    if (!between() || event.repeat) return;
    const turns: Record<string, [number, number]> = {
      ArrowLeft: [-1, 0],
      ArrowRight: [1, 0],
      ArrowUp: [0, -1],
      ArrowDown: [0, 1],
    };
    if (turns[event.key]) choose(...turns[event.key]);
    if (event.key === "t" || event.key === "T") chooseEvent();
  });
  /** Who drives my kart and in what, with buttons to change it. */
  const pick = () => {
    const list = roster();
    const me = mine !== undefined ? racer(mine) : undefined;
    if (!list || !me) return "";
    const driver = chosen ? list.drivers[chosen[0]] : list.drivers.find((d) => d.id === me.driver);
    const kart = chosen ? list.karts[chosen[1]] : list.karts.find((k) => k.id === me.kart);
    if (!driver || !kart) return "";
    return (
      `<hr><button id="driver-back">◀</button> <b>${driver.name}</b> (${driver.class}) <button id="driver-on">▶</button>` +
      `<br><small>${driver.about}</small><br>` +
      `<button id="kart-back">◀</button> <b>${kart.name}</b> <button id="kart-on">▶</button><br><small>${kart.about}</small>`
    );
  };

  /** The frame's steps to the world, in one message. */
  function send(fresh: number) {
    if (mine === undefined || !predictor || controlsType === undefined || fresh === 0) return;
    const steps = predictor.toSend(fresh).map((step) => ({ ...step }));
    if (callsLeft > 0) {
      callsLeft -= 1;
      Object.assign(steps[steps.length - 1], { call });
    }
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
      // Another driver, another colour: the kart is built again in it.
      const color = racer(entity)?.color;
      if (color !== undefined && color !== shown.color) {
        scene.remove(shown.view.root);
        [shown.view, shown.color] = [buildKart(color), color];
        scene.add(shown.view.root);
      }
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
      const charge = Math.min(1, kart.charge / TURBO_CHARGES[2]);
      const colour = sparkColour(kart.charge);
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
    // Rewritten only when it changes, so that its buttons take a click.
    const show = (html: string) => {
      if (html !== shownBanner) [banner.innerHTML, shownBanner] = [html, html];
    };
    if (now.phase === Phase.FINISHED) {
      const podium = rows.slice(0, 8).join("");
      const myName = mine !== undefined ? racer(mine)?.name : undefined;
      show(`<div class="panel"><b>Results</b> · next race in ${Math.max(0, Math.ceil(remaining / 1000))} s<hr>${podium}${cupPanel()}${pick()}${records(myName)}</div>`);
    } else if (now.phase === Phase.READY) {
      const host = now.host === 0n ? undefined : racer(now.host)?.name;
      show(mine !== undefined && now.host === mine
          ? `<div class="panel"><b>Ready</b> · others can still join<br><button id="start">Start the race</button>${cupPanel()}${pick()}</div>`
          : `<div class="panel">waiting for ${host ?? "the host"} to start the race…${cupPanel()}${pick()}</div>`);
    } else if (now.phase === Phase.WAITING) {
      show(`<div class="panel">waiting for drivers…${cupPanel()}${pick()}</div>`);
    } else {
      show("");
    }
  }

  let shownBanner = "";
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
