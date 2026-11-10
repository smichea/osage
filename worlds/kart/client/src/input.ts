// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// Keyboard (arrows or WASD, Space or Shift to drift) and gamepad (left stick,
// right/left triggers, a shoulder button or A to drift).

import type { Drive } from "./physics.js";

const DRIVING_KEYS = ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Space"];
/** How fast keyboard steering swings from one side to the other, per second. */
const STEER_RATE = 6;

export class Input {
  private readonly keys = new Set<string>();
  private steer = 0;

  constructor() {
    addEventListener("keydown", (e) => {
      if (DRIVING_KEYS.includes(e.code)) e.preventDefault();
      this.keys.add(e.code);
    });
    addEventListener("keyup", (e) => this.keys.delete(e.code));
    addEventListener("blur", () => this.keys.clear());
  }

  read(seconds: number): Drive {
    const key = (...codes: string[]) => (codes.some((c) => this.keys.has(c)) ? 1 : 0);
    let throttle = key("ArrowUp", "KeyW") - key("ArrowDown", "KeyS");
    const wanted = key("ArrowRight", "KeyD") - key("ArrowLeft", "KeyA");
    let drift = key("Space", "ShiftLeft", "ShiftRight") === 1;
    const swing = STEER_RATE * seconds;
    this.steer += Math.max(-swing, Math.min(swing, wanted - this.steer));

    const pad = navigator.getGamepads?.().find((g) => g?.connected);
    if (pad) {
      const stick = pad.axes[0] ?? 0;
      if (Math.abs(stick) > 0.15) this.steer = Math.max(-1, Math.min(1, stick));
      const pressed = (i: number) => pad.buttons[i]?.value ?? 0;
      const pedal = pressed(7) - pressed(6) || pressed(0) - pressed(1);
      if (pedal) throttle = pedal;
      drift ||= pressed(5) > 0.5 || pressed(4) > 0.5 || pressed(2) > 0.5;
    }
    return { throttle, steer: this.steer, drift };
  }

  /** Nothing pressed, e.g. when the page is hidden. */
  release(): void {
    this.keys.clear();
    this.steer = 0;
  }
}
