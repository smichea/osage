// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// The WebAssembly physics gives the same bits as the server's native build:
// the script and its recorded results are those of
// worlds/kart/physics/tests/golden.rs.

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { Physics } from "../src/physics.js";

const golden = readFileSync(new URL("../../physics/tests/golden.txt", import.meta.url), "utf8");

function bits(value: number): string {
  const view = new DataView(new ArrayBuffer(4));
  view.setFloat32(0, value);
  return view.getUint32(0).toString(16).padStart(8, "0");
}

describe("kart physics in WebAssembly", () => {
  it("matches the native build bit for bit", async () => {
    const physics = await Physics.load(readFileSync(new URL("../public/physics.wasm", import.meta.url)));
    physics.setTrack([0, 0, 0, 200, -100, 200, -100, 0], 6, 3);
    let kart = { x: 0, z: 10, heading: 0, speed: 0, slide: 0, drift: 0, charge: 0, boost: 0 };
    const lines: string[] = [];
    for (let i = 0; i < 1200; i++) {
      const drive = {
        throttle: [1, 1, 1, 0.5, -1][Math.floor(i / 100) % 5],
        steer: ((i % 9) - 4) / 4,
        drift: Math.floor(i / 120) % 2 === 1,
      };
      kart = physics.step(kart, drive);
      if (i % 60 === 59) {
        const fields = [kart.x, kart.z, kart.heading, kart.speed, kart.slide, kart.drift, kart.charge, kart.boost];
        lines.push(`${i + 1} ${fields.map(bits).join(" ")}`);
      }
    }
    expect(lines.join("\n") + "\n").toBe(golden);
  });
});
