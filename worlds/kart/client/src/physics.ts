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
  /** Height above level ground (the road's on a crest, more in the air) and speed upwards (D-050, D-051). */
  height?: number;
  climb?: number;
  /** 1 once a trick was done in this jump. */
  trick?: number;
  /** Seconds the engine is stalled for, after a start too early. */
  stall?: number;
  /** Seconds in another kart's wake so far. */
  draft?: number;
  /** 1 when the drift button was held at the last step. */
  held?: number;
  /** Seconds the tyres stay oily (D-072): they grip almost nothing. */
  slick?: number;
  /** How the kart is built (D-055), shares of the standard kart's: 1 when absent. */
  top?: number;
  accel?: number;
  handling?: number;
  grip?: number;
  weight?: number;
}

/** The fields of a kart, in the order the WebAssembly physics numbers them. */
const FIELDS = [
  "x", "z", "heading", "speed", "slide", "drift", "charge", "boost", "spin",
  "rocket", "height", "climb", "trick", "stall", "draft", "held", "slick",
] as const;
/** Then the build's, numbered on from the state's; 1 when not given. */
const BUILD = ["top", "accel", "handling", "grip", "weight"] as const;

/** A crest across the road, as in `kart.track/1` (D-051). */
export interface CrestValues {
  along: number;
  rise: number;
  fall: number;
  height: number;
}

/** One step of driving, as in `kart.v1.Step`. */
export interface Drive {
  throttle: number;
  steer: number;
  drift: boolean;
}

export const STEP_SECONDS = 1 / 60;
export const MAX_SPEED = 22;
/** Drift seconds for each level of mini-turbo. */
export const TURBO_CHARGES = [0.6, 1.2, 2.0];
/** Kerbs, beyond the road's edge, and the barriers' face, beyond the verge, metres. */
export const KERB = 1.0;
export const BARRIER = 0.9;

interface Exports {
  track_clear(): void;
  track_point(x: number, z: number): void;
  track_crest(along: number, rise: number, fall: number, height: number): void;
  track_close(halfWidth: number, verge: number): number;
  track_lift(along: number): number;
  track_wet(wet: number): void;
  track_height(height: number): void;
  kart_set(field: number, value: number): void;
  kart_step(throttle: number, steer: number, drift: number): void;
  kart_get(field: number): number;
}

export class Physics {
  private constructor(private readonly wasm: Exports) {}

  static async load(bytes: BufferSource): Promise<Physics> {
    const { instance } = await WebAssembly.instantiate(bytes);
    return new Physics(instance.exports as unknown as Exports);
  }

  /** `points`: x0, z0, x1, z1, … of the closed centre line; `heights`: the road's at each point, over hills (D-077). */
  setTrack(
    points: ArrayLike<number>,
    halfWidth: number,
    verge: number,
    crests: CrestValues[] = [],
    heights: ArrayLike<number> = [],
  ): void {
    this.wasm.track_clear();
    const hilly = heights.length * 2 === points.length;
    for (let i = 0; i + 1 < points.length; i += 2) {
      this.wasm.track_point(points[i], points[i + 1]);
      if (hilly) this.wasm.track_height(heights[i / 2]);
    }
    for (const c of crests) this.wasm.track_crest(c.along, c.rise, c.fall, c.height);
    if (!this.wasm.track_close(halfWidth, verge)) throw new Error("a track needs at least three points");
  }

  /** How wet the road is for the race: 0 dry, 1 in the rain (D-068). A new track is dry. */
  setWet(wet: number): void {
    this.wasm.track_wet(wet);
  }

  /** The road's height `along` metres along the centre line: 0 but over a crest (D-051). */
  roadHeight(along: number): number {
    return this.wasm.track_lift(along);
  }

  step(k: KartValues, d: Drive): KartValues {
    const w = this.wasm;
    FIELDS.forEach((name, i) => w.kart_set(i, k[name] ?? 0));
    BUILD.forEach((name, i) => w.kart_set(FIELDS.length + i, k[name] ?? 1));
    w.kart_step(d.throttle, d.steer, d.drift ? 1 : 0);
    const out = {} as Record<(typeof FIELDS)[number] | (typeof BUILD)[number], number>;
    FIELDS.forEach((name, i) => (out[name] = w.kart_get(i)));
    BUILD.forEach((name, i) => (out[name] = w.kart_get(FIELDS.length + i)));
    return out;
  }
}
