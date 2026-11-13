// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

import { describe, expect, it } from "vitest";
import type { Drive, KartValues } from "../src/physics.js";
import { Predictor } from "../src/prediction.js";

/** A one-dimensional stand-in for the physics: x moves by the throttle. */
const line = { step: (k: KartValues, d: Drive): KartValues => ({ ...k, x: k.x + d.throttle }) };
const still: KartValues = { x: 0, z: 0, heading: 0, speed: 0, slide: 0, drift: 0, charge: 0, boost: 0, spin: 0 };
const go = { throttle: 1, steer: 0, drift: false };

describe("Predictor", () => {
  it("sends the item button with each step, for the world to use the item (D-084)", async () => {
    const { create, toBinary, fromBinary } = await import("@bufbuild/protobuf");
    const { ControlsSchema } = await import("../src/gen/kart/v1/kart_pb.js");
    const predictor = new Predictor(line);
    predictor.reconcile(still, 0n);
    predictor.step({ ...go, useItem: false });
    predictor.step({ ...go, useItem: true });
    const steps = predictor.toSend(2).map((step) => ({ ...step }));
    expect(steps.map((s) => s.useItem)).toEqual([false, true]);
    // As main.ts sends them: the field reaches the wire.
    const wire = toBinary(ControlsSchema, create(ControlsSchema, { steps }));
    expect(fromBinary(ControlsSchema, wire).steps.map((s) => s.useItem)).toEqual([false, true]);
  });

  it("replays the steps the server has not applied yet", () => {
    const predictor = new Predictor(line);
    predictor.reconcile(still, 0n);
    for (let i = 0; i < 5; i++) predictor.step(go);
    expect(predictor.kart?.x).toBe(5);

    // The server applied steps 1 to 3 the same way: nothing to correct.
    predictor.reconcile({ ...still, x: 3 }, 3n);
    expect(predictor.kart?.x).toBe(5);
    expect(predictor.unacknowledged).toBe(2);
    expect(predictor.error.x).toBe(0);

    // The server was pushed back by another kart: the correction is faded in.
    predictor.reconcile({ ...still, x: 3.5 }, 4n);
    expect(predictor.kart?.x).toBe(4.5);
    expect(predictor.error.x).toBeCloseTo(0.5);
    predictor.fade(1);
    expect(Math.abs(predictor.error.x)).toBeLessThan(0.001);
  });

  it("sends a frame's steps with the few before them", () => {
    const predictor = new Predictor(line);
    for (let i = 0; i < 8; i++) predictor.step(go);
    expect(predictor.toSend(1).map((s) => s.sequence)).toEqual([5n, 6n, 7n, 8n]);
    expect(predictor.toSend(3).map((s) => s.sequence)).toEqual([3n, 4n, 5n, 6n, 7n, 8n]);
  });
});
