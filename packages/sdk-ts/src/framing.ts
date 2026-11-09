// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

/**
 * Framing on WebTransport streams (spec/README.md, "Transport"): each message is preceded by its length as a
 * protobuf varint — protobuf's "length-delimited" format. A datagram carries one message, unframed.
 */

/** Longest frame accepted on a stream; a longer one ends the connection. */
export const MAX_FRAME = 4 << 20;

/** `payload` preceded by its length. */
export function frame(payload: Uint8Array): Uint8Array<ArrayBuffer> {
  const header: number[] = [];
  for (let n = payload.length; ; n = Math.floor(n / 128)) {
    if (n < 128) {
      header.push(n);
      break;
    }
    header.push((n % 128) | 0x80);
  }
  const bytes = new Uint8Array(header.length + payload.length);
  bytes.set(header);
  bytes.set(payload, header.length);
  return bytes;
}

/** Reassembles frames from the chunks a stream delivers, however it cuts them. */
export class FrameReader {
  private buffer = new Uint8Array(0);

  push(chunk: Uint8Array): void {
    const joined = new Uint8Array(this.buffer.length + chunk.length);
    joined.set(this.buffer);
    joined.set(chunk, this.buffer.length);
    this.buffer = joined;
  }

  /** The payload of the next complete frame, if it has arrived; throws on a frame longer than `MAX_FRAME`. */
  next(): Uint8Array | undefined {
    let length = 0;
    for (let i = 0; i < Math.min(this.buffer.length, 10); i++) {
      const byte = this.buffer[i];
      length += (byte & 0x7f) * 2 ** (7 * i);
      if (byte & 0x80) continue;
      if (length > MAX_FRAME) throw new Error(`frame longer than ${MAX_FRAME} bytes`);
      const end = i + 1 + length;
      if (this.buffer.length < end) return undefined;
      const payload = this.buffer.slice(i + 1, end);
      this.buffer = this.buffer.slice(end);
      return payload;
    }
    if (this.buffer.length >= 10) throw new Error("malformed frame length");
    return undefined;
  }
}
