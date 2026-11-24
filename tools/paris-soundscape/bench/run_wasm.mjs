// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

// Runs the prototype built as WebAssembly in Node, that is in V8, the engine
// of Chromium: what it costs where a browser game runs.
//
//     cargo build --release --target wasm32-wasip1 -p paris-soundscape
//     taskset -c 2 node tools/paris-soundscape/bench/run_wasm.mjs \
//         target/wasm32-wasip1/release/paris-soundscape.wasm bench --quick
//
// The program's own arguments follow the file's name. It is given /proc, to
// read the processor's time of its own thread as the native program does,
// and the current directory as /here, to write a file (--out /here/x.wav).

import { readFile } from 'node:fs/promises';
import { argv, cwd, exit } from 'node:process';
import { WASI } from 'node:wasi';

const [, , file, ...args] = argv;
if (!file) {
  console.error('which .wasm file?');
  exit(2);
}
const wasi = new WASI({
  version: 'preview1',
  args: ['paris-soundscape', ...args],
  env: {},
  preopens: { '/proc': '/proc', '/here': cwd() },
});
const module = await WebAssembly.compile(await readFile(file));
const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
exit(wasi.start(instance));
