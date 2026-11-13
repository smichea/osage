// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

/**
 * The world's leaderboard, as a page (D-091): connected as an observer —
 * no kart, no place on the grid — it shows every track's records
 * (`kart.leaderboard/1`: best laps, best races, best time trials, who drove
 * what, when) and the race being run now, live.
 */

import { World, connect, hello, v0 } from "@osage/sdk";
import {
  type Leaderboard,
  LeaderboardSchema,
  Phase,
  type Race,
  RaceSchema,
  type Racer,
  RacerSchema,
  type Record as KartRecord,
  type Records,
} from "./gen/kart/v1/kart_pb.js";
import { PALETTE } from "./scene.js";

const KART = { leaderboard: "kart.leaderboard/1", race: "kart.race/1", racer: "kart.racer/1" };
const $ = (id: string) => document.getElementById(id)!;
const params = new URLSearchParams(location.search);
const url = params.get("server") ?? `ws://${location.hostname || "127.0.0.1"}:7878`;

const TRACK_NAMES: Record<string, string> = {
  paddock: "The Paddock",
  coast: "Gull Point",
  forest: "Pinewood",
  harbour: "The Harbour",
  yard: "The Yard",
  mountain: "Eagle Ridge",
  canyon: "Red Gorge",
};

const escape = (s: string) =>
  s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);

/** A time as m:ss.cc. */
export function clock(ms: number): string {
  const minutes = Math.floor(ms / 60000);
  const seconds = (ms % 60000) / 1000;
  return `${minutes}:${seconds.toFixed(2).padStart(5, "0")}`;
}

/** One board: the best five, fastest first. */
export function board(title: string, list: KartRecord[]): string {
  if (list.length === 0) return `<h3>${title}</h3><div class="empty">no time yet</div>`;
  const rows = list
    .slice(0, 5)
    .map((r, i) => {
      const when = r.at > 0n ? new Date(Number(r.at) * 1000).toLocaleDateString() : "";
      const what = [r.driver, r.kart].filter(Boolean).join(" · ");
      const fresh = r.fresh ? '<span class="fresh">NEW</span>' : "";
      return (
        `<tr class="p${i + 1}"><td class="place">${i + 1}</td>` +
        `<td class="who">${escape(r.name)}${fresh}<br><small>${escape(what)}${what && when ? " · " : ""}${when}</small></td>` +
        `<td class="time">${clock(r.ms)}</td></tr>`
      );
    })
    .join("");
  return `<h3>${title}</h3><table>${rows}</table>`;
}

/** A track's three boards. */
export function track(records: Records): string {
  const name = TRACK_NAMES[records.track] ?? records.track;
  return (
    `<section><h2>${escape(name)}</h2><div class="grid">` +
    `<div>${board("Best laps", records.laps)}</div>` +
    `<div>${board(`Best races, ${records.raceLaps} laps`, records.races)}</div>` +
    `<div>${board(`Time trials, ${records.raceLaps} laps`, records.trials)}</div>` +
    "</div></section>"
  );
}

const PHASES: Partial<Record<Phase, string>> = {
  [Phase.WAITING]: "waiting for drivers",
  [Phase.READY]: "on the grid",
  [Phase.COUNTDOWN]: "the lights are on",
  [Phase.RACING]: "racing",
  [Phase.FINISHED]: "results",
};

function live(race: Race | undefined, racer: (entity: bigint) => Racer | undefined): string {
  if (!race) return '<h2>Now racing</h2><div class="empty">no race</div>';
  const details = [
    PHASES[race.phase] ?? "",
    race.battle ? "battle" : race.trial ? "time trial" : `${race.laps} laps`,
    race.class ? `${race.class} cc` : "",
    race.bots ? `bots ${race.bots}` : "",
    race.night ? "night" : "",
    race.rain ? "rain" : "",
  ].filter(Boolean);
  const rows = race.order
    .map((entity) => racer(entity))
    .filter((r): r is Racer => r !== undefined)
    .map((r) => {
      const status = r.finishMs > 0n ? clock(Number(r.finishMs)) : race.battle ? `${r.lives} lives` : `lap ${Math.max(r.lap, 1)}`;
      const kind = r.ghost ? " <small>ghost</small>" : r.bot ? " <small>bot</small>" : "";
      return (
        `<tr class="p${r.position}"><td class="place">${r.position}</td>` +
        `<td class="who"><span class="dot" style="background:${PALETTE[r.color % PALETTE.length]}"></span>${escape(r.name)}${kind}</td>` +
        `<td class="time">${status}</td></tr>`
      );
    })
    .join("");
  return `<h2>Now racing</h2><div class="meta">${details.join(" · ")}</div><table>${rows}</table>`;
}

try {
  // An observer: the world shows it everything and gives it no kart.
  const session = await connect(url, hello("osage-kart-leaderboard", "0.1.0", v0.Role.OBSERVER, v0.Tier.T0_HEADLESS));
  const world = new World(session.welcome);
  $("status").textContent = `world ${session.welcome.world?.name ?? ""} · live`;
  const leaderboardOf = (): Leaderboard | undefined => {
    for (const entity of world.entities()) {
      const value = world.get(entity, KART.leaderboard, LeaderboardSchema);
      if (value) return value;
    }
    return undefined;
  };
  const raceOf = (): Race | undefined => {
    for (const entity of world.entities()) {
      const value = world.get(entity, KART.race, RaceSchema);
      if (value) return value;
    }
    return undefined;
  };
  let shown = "";
  const draw = () => {
    const leaderboard = leaderboardOf();
    const race = raceOf();
    const racer = (entity: bigint) => world.get(entity, KART.racer, RacerSchema);
    const tracks = (leaderboard?.tracks ?? []).map(track).join("") || '<section><h2>Records</h2><div class="empty">no record yet: race!</div></section>';
    const html = live(race, racer) + "\n" + tracks;
    if (html !== shown) {
      shown = html;
      $("live").innerHTML = live(race, racer);
      $("tracks").innerHTML = tracks;
    }
  };
  session.onMessage((message) => {
    world.apply(message);
  });
  setInterval(draw, 500);
  session.closed().then(() => ($("status").textContent = "disconnected"));
} catch (error) {
  $("status").textContent = `cannot reach the world at ${url}: ${error}`;
}
