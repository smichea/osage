// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

// Prints what an osage world replicates: `node examples/watch.mjs [ws://127.0.0.1:7878] [seconds]`
// (build the package first: `npm run build`).

import { World, components, connect, hello, types, v0 } from "../dist/index.js";

const url = process.argv[2] ?? "ws://127.0.0.1:7878";
const seconds = Number(process.argv[3] ?? 2);

const session = await connect(url, hello("osage-sdk-ts/watch", "0.1.0", v0.Role.OBSERVER, v0.Tier.T0_HEADLESS));
const { world: descriptor, identity } = session.welcome;
console.log(`welcome: world "${descriptor?.name}" at ${descriptor?.tickRateHz} Hz, you are ${identity?.perWorldId}`);
console.log(`source: ${descriptor?.sourceOffer?.url} (${descriptor?.sourceOffer?.version})`);

const world = new World(session.welcome);
session.onMessage((message) => {
  for (const event of world.apply(message)) {
    if (event.kind === "entered") {
      const content = world.get(event.entity, types.contentRef, components.ContentRefSchema);
      console.log(`entity ${event.entity} entered: ${content?.uri}`);
    } else if (event.kind === "changed" && event.typeName === types.transform && world.tick % 15n === 0n) {
      const t = world.get(event.entity, types.transform, components.TransformSchema)?.translation;
      console.log(`tick ${world.tick}: entity ${event.entity} at (${t?.x.toFixed(2)}, ${t?.y.toFixed(2)}, ${t?.z.toFixed(2)})`);
    }
  }
});

setTimeout(() => session.close(), seconds * 1000);
await session.closed();
