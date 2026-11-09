// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

import { describe, expect, it } from "vitest";
import { FrameReader, MAX_FRAME, frame } from "../src/framing.js";

describe("framing", () => {
  it("reassembles frames however the stream cuts them", () => {
    const first = new Uint8Array([1, 2, 3]);
    const second = new Uint8Array(300).fill(7); // a two-byte length
    const bytes = new Uint8Array([...frame(first), ...frame(second)]);
    expect([...frame(second).slice(0, 2)]).toEqual([0xac, 0x02]); // protobuf varint of 300
    for (let cut = 0; cut <= bytes.length; cut++) {
      const reader = new FrameReader();
      const decoded: Uint8Array[] = [];
      for (const chunk of [bytes.slice(0, cut), bytes.slice(cut)]) {
        reader.push(chunk);
        for (let payload = reader.next(); payload; payload = reader.next()) decoded.push(payload);
      }
      expect(decoded).toEqual([first, second]);
    }
  });

  it("refuses a frame too large before it arrives", () => {
    const reader = new FrameReader();
    reader.push(new Uint8Array([0x81, 0x80, 0x80, 0x02])); // MAX_FRAME + 1
    expect(MAX_FRAME + 1).toBe(1 + 0x02 * 2 ** 21);
    expect(() => reader.next()).toThrow();
  });
});
