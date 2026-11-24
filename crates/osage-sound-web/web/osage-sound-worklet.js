// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

// The engine in an AudioWorklet: its block is the render quantum, so each
// call of process() makes one block, with every pose told until then.
//
//     await context.audioWorklet.addModule('osage-sound-worklet.js');
//     const node = new AudioWorkletNode(context, 'osage-sound', {
//       numberOfInputs: 0, outputChannelCount: [2],
//       processorOptions: { module, scene, quality: 'medium', headphones: true },
//     });
//     node.port.postMessage({ drive: [nodeIndex, [x, y, z], [qx, qy, qz, qw], [vx, vy, vz]] });
//     node.port.postMessage({ cost: true });  // answered with what the blocks cost
//
// `module` is the compiled osage_sound_web.wasm (a WebAssembly.Module can be
// sent to a worklet); the context must run at 48 kHz, the engine's rate.

import { Sound } from './osage-sound.js';

class OsageSound extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const { module, scene, quality, headphones, start } = options.processorOptions;
    this.sound = new Sound(module, scene, { quality, headphones, start });
    // What each block costs, on the best clock the worklet has: a block
    // over its budget (128 frames at the context's rate) is one the output
    // may have had to do without.
    this.clock = globalThis.performance ? () => performance.now() : () => Date.now();
    this.cost = { clock: globalThis.performance ? 'performance.now' : 'Date.now', blocks: 0, ms: 0, worst: 0, over: 0 };
    this.budget = (1000 * 128) / sampleRate;
    // Date.now counts whole milliseconds: a block is over its budget for
    // certain only when it reads a millisecond more.
    this.over = this.budget + (globalThis.performance ? 0 : 1);
    this.port.onmessage = ({ data }) => {
      if (data.drive) this.sound.drive(...data.drive);
      if (data.node) this.port.postMessage({ node: data.node, index: this.sound.node(data.node) });
      if (data.cost) this.port.postMessage({ cost: { ...this.cost, budget: this.budget } });
    };
    this.port.postMessage({ ready: true, listener: this.sound.listener() });
  }

  process(_inputs, outputs) {
    const [left, right] = outputs[0];
    const began = this.clock();
    this.sound.render(left, right);
    const ms = this.clock() - began;
    this.cost.blocks++;
    this.cost.ms += ms;
    this.cost.worst = Math.max(this.cost.worst, ms);
    if (ms > this.over) this.cost.over++;
    return true;
  }
}

registerProcessor('osage-sound', OsageSound);
