// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

// The engine as WebAssembly, opened on a scene: the same few calls in a page
// and in the AudioWorklet (osage-sound-worklet.js). No bindings: numbers and
// the module's memory (crates/osage-sound-web/src/lib.rs).

export const QUALITIES = ['low', 'medium', 'high', 'reference'];

/** UTF-8 bytes of a string: an AudioWorklet has no TextEncoder. */
export function utf8(text) {
  const bytes = [];
  for (const char of text) {
    const c = char.codePointAt(0);
    if (c < 0x80) bytes.push(c);
    else if (c < 0x800) bytes.push(0xc0 | (c >> 6), 0x80 | (c & 63));
    else if (c < 0x10000) bytes.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
    else bytes.push(0xf0 | (c >> 18), 0x80 | ((c >> 12) & 63), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
  }
  return new Uint8Array(bytes);
}

export class Sound {
  /** `module`: the compiled osage_sound_web.wasm; `scene`: its JSON, as text or UTF-8 bytes. */
  constructor(module, scene, { quality = 'medium', headphones = true, start = 0 } = {}) {
    this.wasm = new WebAssembly.Instance(module, {}).exports;
    this.write(scene);
    const level = Math.max(0, QUALITIES.indexOf(quality));
    if (!this.wasm.sound_open(level, headphones ? 1 : 0, start)) {
      const at = this.wasm.sound_error();
      const bytes = new Uint8Array(this.wasm.memory.buffer, at, this.wasm.sound_error_len());
      throw new Error(`the scene did not open: ${String.fromCharCode(...bytes)}`);
    }
    this.block = this.wasm.sound_block();
  }

  write(text) {
    const bytes = typeof text === 'string' ? utf8(text) : text;
    const at = this.wasm.sound_text(bytes.length);
    new Uint8Array(this.wasm.memory.buffer, at, bytes.length).set(bytes);
    return bytes.length;
  }

  /** The node at `path` (identifiers joined by ' / '), or -1. */
  node(path) {
    return this.wasm.sound_node(this.write(path));
  }

  listener() {
    return this.wasm.sound_listener();
  }

  /** Places `node` in its parent, going on at `velocity` (m/s), from the next block. */
  drive(node, [x, y, z], [qx, qy, qz, qw], [vx, vy, vz]) {
    this.wasm.sound_drive(node, x, y, z, qx, qy, qz, qw, vx, vy, vz);
  }

  /** The next block into `left` and `right` (Float32Arrays of 128). */
  render(left, right) {
    const at = this.wasm.sound_render();
    const out = new Float32Array(this.wasm.memory.buffer, at, 2 * this.block);
    left.set(out.subarray(0, this.block));
    right.set(out.subarray(this.block));
  }
}
