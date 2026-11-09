// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

import { type MessageInitShape, create, fromBinary, toBinary } from "@bufbuild/protobuf";
import {
  ClientMessageSchema,
  GoodbyeReason,
  type Hello,
  HelloSchema,
  Role,
  type ServerMessage,
  ServerMessageSchema,
  Tier,
  type Welcome,
} from "./gen/osage/v0/protocol_pb.js";

/** Protocol version spoken by this SDK. */
export const PROTOCOL_VERSION = "0.1";

/** A `Hello` for an anonymous client of the given role and tier. */
export function hello(implementation: string, version: string, role: Role, tier: Tier): Hello {
  return create(HelloSchema, {
    protocolVersions: [PROTOCOL_VERSION],
    client: { implementation, version },
    role,
    capabilities: { tier, contentKinds: ["builtin"] },
    credentials: { credential: { case: "anonymous", value: {} } },
  });
}

type Listener = (message: ServerMessage) => void;

/** An open session with an osage gateway. */
export class Session {
  private readonly listeners = new Set<Listener>();

  constructor(
    private readonly socket: WebSocket,
    readonly welcome: Welcome,
  ) {
    socket.addEventListener("message", (event) => {
      const message = fromBinary(ServerMessageSchema, new Uint8Array(event.data as ArrayBuffer));
      for (const listener of this.listeners) listener(message);
    });
  }

  /** Calls `listener` for every server message after the Welcome; returns an unsubscribe function. */
  onMessage(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Resolves when the connection closes. */
  closed(): Promise<void> {
    if (this.socket.readyState === WebSocket.CLOSED) return Promise.resolve();
    return new Promise((resolve) => this.socket.addEventListener("close", () => resolve(), { once: true }));
  }

  send(message: MessageInitShape<typeof ClientMessageSchema>["message"]): void {
    this.socket.send(toBinary(ClientMessageSchema, create(ClientMessageSchema, { message })));
  }

  /** Sends an Intent for `entity`: `value` encoded as component type `typeId` (see `World.typeId`). */
  sendIntent(sequence: bigint, entity: bigint, typeId: number, value: Uint8Array): void {
    this.send({ case: "intent", value: { sequence, entity, input: { type: typeId, value } } });
  }

  close(): void {
    this.send({ case: "goodbye", value: { reason: GoodbyeReason.NORMAL } });
    this.socket.close();
  }
}

/** Connects to `url` (e.g. `ws://127.0.0.1:7878`), sends `hello` and resolves with the session once welcomed. */
export function connect(url: string, helloMessage: Hello): Promise<Session> {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    socket.binaryType = "arraybuffer";
    socket.addEventListener("error", () => reject(new Error(`cannot connect to ${url}`)), { once: true });
    socket.addEventListener("open", () => {
      const first = create(ClientMessageSchema, { message: { case: "hello", value: helloMessage } });
      socket.send(toBinary(ClientMessageSchema, first));
    });
    const onFirst = (event: MessageEvent) => {
      socket.removeEventListener("message", onFirst);
      const message = fromBinary(ServerMessageSchema, new Uint8Array(event.data as ArrayBuffer)).message;
      if (message.case === "welcome") {
        resolve(new Session(socket, message.value));
      } else if (message.case === "goodbye") {
        socket.close();
        reject(new Error(`refused by the gateway: ${message.value.reason} ${message.value.detail}`));
      } else {
        socket.close();
        reject(new Error("expected Welcome as the first server message"));
      }
    };
    socket.addEventListener("message", onFirst);
  });
}
