// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// The kart world's physics, the same Rust code as the server's, compiled to
// WebAssembly (D-031): what this client predicts is what the server computes.

/** A kart's physical state, as in `kart.state/1`. */
export interface KartValues {
  x: number;
  z: number;
  heading: number;
  speed: number;
  slide: number;
  drift: number;
  charge: number;
  boost: number;
  /** Rate of turn a knock gave, rad/s (D-045). */
  spin: number;
  /** Seconds of rocket left (paddock objects); 0 from a world that does not send it. */
  rocket?: number;
}

/** One step of driving, as in `kart.v1.Step`. */
export interface Drive {
  throttle: number;
  steer: number;
  drift: boolean;
}

export const STEP_SECONDS = 1 / 60;
export const MAX_SPEED = 22;
export const SHORT_TURBO_CHARGE = 0.7;
export const LONG_TURBO_CHARGE = 1.5;
/** Kerbs, beyond the road's edge, and the barriers' face, beyond the verge, metres. */
export const KERB = 1.0;
export const BARRIER = 0.9;

interface Exports {
  track_clear(): void;
  track_point(x: number, z: number): void;
  track_close(halfWidth: number, verge: number): number;
  kart_step(...values: number[]): void;
  kart_get(field: number): number;
}

export class Physics {
  private constructor(private readonly wasm: Exports) {}

  static async load(bytes: BufferSource): Promise<Physics> {
    const { instance } = await WebAssembly.instantiate(bytes);
    return new Physics(instance.exports as unknown as Exports);
  }

  /** `points`: x0, z0, x1, z1, … of the closed centre line. */
  setTrack(points: ArrayLike<number>, halfWidth: number, verge: number): void {
    this.wasm.track_clear();
    for (let i = 0; i + 1 < points.length; i += 2) this.wasm.track_point(points[i], points[i + 1]);
    if (!this.wasm.track_close(halfWidth, verge)) throw new Error("a track needs at least three points");
  }

  step(k: KartValues, d: Drive): KartValues {
    const w = this.wasm;
    w.kart_step(k.x, k.z, k.heading, k.speed, k.slide, k.drift, k.charge, k.boost, k.spin, k.rocket ?? 0, d.throttle, d.steer, d.drift ? 1 : 0);
    return {
      x: w.kart_get(0),
      z: w.kart_get(1),
      heading: w.kart_get(2),
      speed: w.kart_get(3),
      slide: w.kart_get(4),
      drift: w.kart_get(5),
      charge: w.kart_get(6),
      boost: w.kart_get(7),
      spin: w.kart_get(8),
      rocket: w.kart_get(9),
    };
  }
}
