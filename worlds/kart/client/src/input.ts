// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// Keyboard (arrows or WASD, Space or Shift to drift), gamepad (left stick,
// right/left triggers, a shoulder button or A to drift) and touch screen (the
// left thumb steers on a pad, the right one drifts or brakes; the kart
// accelerates by itself).

import type { Drive } from "./physics.js";

const DRIVING_KEYS = ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Space"];
/** How fast keyboard steering swings from one side to the other, per second. */
const STEER_RATE = 6;

/** Steering within this fraction of the pad's centre is straight ahead. */
const DEAD_ZONE = 0.08;

/** On-screen controls, shown on touch screens. */
class Touch {
  steer = 0;
  brake = false;
  drift = false;
  private readonly knob: HTMLElement;

  constructor() {
    document.body.classList.add("touch");
    const root = document.createElement("div");
    root.id = "touch";
    root.innerHTML = `<div id="pad"><div id="knob"></div></div><div id="brake">BRAKE</div><div id="drift">DRIFT</div>`;
    document.body.append(root);
    const [pad, brake, drift] = ["pad", "brake", "drift"].map((id) => root.querySelector<HTMLElement>(`#${id}`)!);
    this.knob = root.querySelector("#knob")!;

    const steerAt = (e: PointerEvent) => {
      const box = pad.getBoundingClientRect();
      const x = Math.max(-1, Math.min(1, (e.clientX - box.left - box.width / 2) / (box.width / 2 - 32)));
      this.steer = Math.abs(x) < DEAD_ZONE ? 0 : x;
      this.knob.style.transform = `translateX(${x * (box.width / 2 - 32)}px)`;
    };
    this.hold(pad, (down, e) => {
      if (down) steerAt(e);
      else [this.steer, this.knob.style.transform] = [0, ""];
    });
    pad.addEventListener("pointermove", (e) => pad.hasPointerCapture(e.pointerId) && steerAt(e));
    this.hold(brake, (down) => (this.brake = down));
    this.hold(drift, (down) => (this.drift = down));
  }

  /** Calls `change` when a finger lands on `element` and when it leaves. */
  private hold(element: HTMLElement, change: (down: boolean, e: PointerEvent) => void) {
    element.addEventListener("pointerdown", (e) => {
      e.preventDefault();
      element.setPointerCapture(e.pointerId);
      element.classList.add("held");
      change(true, e);
    });
    for (const end of ["pointerup", "pointercancel"] as const) {
      element.addEventListener(end, (e) => {
        element.classList.remove("held");
        change(false, e);
      });
    }
  }
}

export class Input {
  private readonly keys = new Set<string>();
  private steer = 0;
  private touch: Touch | undefined;

  constructor() {
    addEventListener("keydown", (e) => {
      if (DRIVING_KEYS.includes(e.code)) e.preventDefault();
      this.keys.add(e.code);
    });
    addEventListener("keyup", (e) => this.keys.delete(e.code));
    addEventListener("blur", () => this.keys.clear());
    if (matchMedia("(pointer: coarse)").matches) this.touch = new Touch();
    else addEventListener("touchstart", () => (this.touch ??= new Touch()), { once: true, passive: true });
  }

  /** Whether on-screen controls are shown. */
  get touchScreen(): boolean {
    return this.touch !== undefined;
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
    if (this.touch && this.keys.size === 0 && !pad) {
      // Two thumbs are not enough for four controls: the kart accelerates by itself.
      throttle = this.touch.brake ? -1 : 1;
      this.steer = this.touch.steer;
      drift = this.touch.drift;
    }
    return { throttle, steer: this.steer, drift };
  }

  /** Nothing pressed, e.g. when the page is hidden. */
  release(): void {
    this.keys.clear();
    this.steer = 0;
  }
}
