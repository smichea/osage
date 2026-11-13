// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// Client-side prediction of the player's own kart (D-031): every step is
// applied locally at once and sent; when the server's state comes back with
// the last step it applied, the client restarts from it and replays the
// steps the server has not applied yet. The difference with what was shown
// is not snapped but faded out.

import type { Drive, KartValues } from "./physics.js";

export interface Stepper {
  step(kart: KartValues, drive: Drive): KartValues;
}

export interface SentStep extends Drive {
  sequence: bigint;
  /** The item button held at this step (D-084); the world uses the item. */
  useItem?: boolean;
}

/** Steps already sent that go again with the new ones, against datagram loss. */
const REDUNDANCY = 3;
/** At most this many steps in one message. */
const MAX_SENT = 24;
/** Beyond this error, metres, the kart jumps instead of gliding back. */
const SNAP_DISTANCE = 4;

export class Predictor {
  /** The predicted present state; `undefined` until the server's first state. */
  kart: KartValues | undefined;
  /** What is drawn minus `kart`: faded to zero after a correction. */
  readonly error = { x: 0, z: 0, heading: 0 };
  private pending: SentStep[] = [];
  private next = 1n;
  /** Diagnostics: reconciliations that moved the kart, and the largest move, metres. */
  corrections = 0;
  largestCorrection = 0;

  constructor(private readonly physics: Stepper) {}

  /** Applies one step locally. */
  step(drive: Drive & { useItem?: boolean }): void {
    this.pending.push({ ...drive, sequence: this.next++ });
    if (this.kart) this.kart = this.physics.step(this.kart, drive);
  }

  /**
   * What to send after `fresh` new steps, oldest first: those, and a few sent
   * before. One message per frame, however many steps the frame took: a
   * browser may send only about one datagram per frame.
   */
  toSend(fresh: number): SentStep[] {
    return this.pending.slice(-Math.min(MAX_SENT, fresh + REDUNDANCY));
  }

  /** The server's state of this kart, after it applied step `applied`. */
  reconcile(server: KartValues, applied: bigint): void {
    this.pending = this.pending.filter((s) => s.sequence > applied);
    let kart = { ...server };
    for (const step of this.pending) kart = this.physics.step(kart, step);
    if (this.kart) {
      const moved = Math.hypot(this.kart.x - kart.x, this.kart.z - kart.z);
      if (moved > 0.01) this.corrections++;
      this.largestCorrection = Math.max(this.largestCorrection, moved);
      const [dx, dz] = [this.kart.x + this.error.x - kart.x, this.kart.z + this.error.z - kart.z];
      const snap = Math.hypot(dx, dz) > SNAP_DISTANCE;
      this.error.x = snap ? 0 : dx;
      this.error.z = snap ? 0 : dz;
      this.error.heading = snap ? 0 : angle(this.kart.heading + this.error.heading - kart.heading);
    }
    this.kart = kart;
  }

  /** Fades the visual error over `seconds`. */
  fade(seconds: number): void {
    const keep = Math.exp(-seconds * 12);
    this.error.x *= keep;
    this.error.z *= keep;
    this.error.heading *= keep;
  }

  /** Steps not yet applied by the server (for tests and diagnostics). */
  get unacknowledged(): number {
    return this.pending.length;
  }
}

function angle(a: number): number {
  return Math.atan2(Math.sin(a), Math.cos(a));
}
