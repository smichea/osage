// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

import { type MessageInitShape, clone, create, fromBinary, toBinary } from "@bufbuild/protobuf";
import {
  ClientMessageSchema,
  CredentialsSchema,
  GoodbyeReason,
  type Hello,
  HelloSchema,
  Role,
  type ServerMessage,
  ServerMessageSchema,
  Tier,
  type Welcome,
} from "./gen/osage/v0/protocol_pb.js";
import { FrameReader, frame } from "./framing.js";

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

/** `hello` presenting an identity credential (D-013) instead of being anonymous. */
export function withIdentity(helloMessage: Hello, credential: string): Hello {
  const copy = clone(HelloSchema, helloMessage);
  const value = new TextEncoder().encode(credential);
  copy.credentials = create(CredentialsSchema, { credential: { case: "verifiablePresentation", value } });
  return copy;
}

/** `hello` for an agent presenting the delegation of its responsible human. */
export function withDelegation(helloMessage: Hello, delegation: string): Hello {
  const copy = clone(HelloSchema, helloMessage);
  const value = new TextEncoder().encode(delegation);
  copy.role = Role.AGENT;
  copy.credentials = create(CredentialsSchema, { credential: { case: "delegation", value } });
  return copy;
}

/** Action asking the world for an agent delegation; the token is in `ActionResult.result`. */
export const DELEGATE_AGENT = "osage.delegate_agent";

/** Where a gateway announces its WebTransport endpoint, on its WebSocket host. */
export const WEBTRANSPORT_DISCOVERY_PATH = "/.well-known/osage/webtransport";

/** How a session reaches its gateway (ADR-0002 §3). */
export type Transport = "webtransport" | "websocket";

/** A gateway's WebTransport endpoint, as announced by `discover`. */
export interface WebTransportEndpoint {
  url: string;
  /** SHA-256 hashes of the gateway's self-signed certificate (development); empty: the web PKI vouches for it. */
  certificateHashes: Uint8Array<ArrayBuffer>[];
}

/** The gateway refused the session: its `Goodbye`. */
export class RefusedError extends Error {
  constructor(
    readonly reason: GoodbyeReason,
    readonly detail: string,
  ) {
    super(`refused by the gateway: ${GoodbyeReason[reason]} ${detail}`);
  }
}

const DISCOVERY_TIMEOUT_MS = 2000;
const WEBTRANSPORT_TIMEOUT_MS = 3000;

type Listener = (message: ServerMessage) => void;

/** One connection to a gateway; delivers each encoded server message to the callback it was opened with. */
interface Link {
  readonly transport: Transport;
  /** Sends one encoded `ClientMessage`; a `volatile` one may travel as a datagram. */
  send(bytes: Uint8Array<ArrayBuffer>, volatile: boolean): void;
  close(): void;
  readonly closed: Promise<void>;
}

/** An open session with an osage gateway. */
export class Session {
  private readonly listeners = new Set<Listener>();
  /** Messages received before the first listener, handed to it. */
  private pending: ServerMessage[] | undefined = [];

  constructor(
    private readonly link: Link,
    readonly welcome: Welcome,
  ) {}

  get transport(): Transport {
    return this.link.transport;
  }

  /** @internal */
  deliver(message: ServerMessage): void {
    if (this.pending) this.pending.push(message);
    for (const listener of this.listeners) listener(message);
  }

  /** Calls `listener` for every server message after the Welcome; returns an unsubscribe function. */
  onMessage(listener: Listener): () => void {
    this.listeners.add(listener);
    const pending = this.pending ?? [];
    this.pending = undefined;
    for (const message of pending) listener(message);
    return () => this.listeners.delete(listener);
  }

  /** Resolves when the connection closes. */
  closed(): Promise<void> {
    return this.link.closed;
  }

  send(message: MessageInitShape<typeof ClientMessageSchema>["message"]): void {
    this.link.send(toBinary(ClientMessageSchema, create(ClientMessageSchema, { message })), false);
  }

  /**
   * Sends an Intent for `entity`: `value` encoded as component type `typeId` (see `World.typeId`). Intents are
   * volatile: over WebTransport they travel as datagrams, and the server keeps the latest sequence.
   */
  sendIntent(sequence: bigint, entity: bigint, typeId: number, value: Uint8Array): void {
    const message = create(ClientMessageSchema, {
      message: { case: "intent", value: { sequence, entity, input: { type: typeId, value } } },
    });
    this.link.send(toBinary(ClientMessageSchema, message), true);
  }

  close(): void {
    this.send({ case: "goodbye", value: { reason: GoodbyeReason.NORMAL } });
    this.link.close();
  }
}

/**
 * Connects to the world at `url` (e.g. `ws://127.0.0.1:7878`), sends `hello` and resolves with the session once
 * welcomed. Where the runtime has WebTransport (browsers), uses the gateway's WebTransport endpoint if it announces
 * one and it answers, WebSocket otherwise; `transport` forces one.
 */
export async function connect(
  url: string,
  helloMessage: Hello,
  options: { transport?: Transport | "auto" } = {},
): Promise<Session> {
  const wanted = options.transport ?? "auto";
  if (wanted !== "websocket" && typeof WebTransport !== "undefined") {
    try {
      const endpoint = url.startsWith("https://")
        ? { url, certificateHashes: [] }
        : await within(discover(url), DISCOVERY_TIMEOUT_MS);
      if (endpoint) {
        const opening = open((deliver) => webTransportLink(endpoint, deliver), helloMessage);
        return await within(opening, WEBTRANSPORT_TIMEOUT_MS, (late) => late.close());
      }
    } catch (error) {
      if (error instanceof RefusedError || wanted === "webtransport") throw error;
      // UDP blocked, endpoint down…: WebSocket.
    }
  }
  if (wanted === "webtransport") throw new Error("WebTransport is not available");
  return open((deliver) => webSocketLink(url, deliver), helloMessage);
}

/** Asks the gateway at `url` (`ws://` or `wss://`) for its WebTransport endpoint; `undefined` if it has none. */
export async function discover(url: string): Promise<WebTransportEndpoint | undefined> {
  const http = url.replace(/^ws(s?):\/\//, "http$1://");
  if (http === url) return undefined;
  const response = await fetch(new URL(WEBTRANSPORT_DISCOVERY_PATH, http), { cache: "no-store" });
  if (!response.ok) return undefined;
  const advert = (await response.json()) as { url: string; certificateHashes?: string[] };
  return { url: advert.url, certificateHashes: (advert.certificateHashes ?? []).map(fromHex) };
}

async function open(makeLink: (deliver: (bytes: Uint8Array) => void) => Promise<Link>, helloMessage: Hello) {
  let session: Session | undefined;
  let settle!: { resolve: (session: Session) => void; reject: (error: Error) => void };
  const welcomed = new Promise<Session>((resolve, reject) => (settle = { resolve, reject }));
  const link: Link = await makeLink((bytes) => {
    const message = fromBinary(ServerMessageSchema, bytes);
    if (session) return session.deliver(message);
    const first = message.message;
    // A datagram may overtake the Welcome on the stream; its state is of no use yet.
    if (first.case === "state") return;
    if (first.case === "welcome") {
      session = new Session(link, first.value);
      return settle.resolve(session);
    }
    link.close();
    settle.reject(
      first.case === "goodbye"
        ? new RefusedError(first.value.reason, first.value.detail)
        : new Error("expected Welcome as the first server message"),
    );
  });
  link.closed.then(() => settle.reject(new Error("connection closed before Welcome")));
  const first = create(ClientMessageSchema, { message: { case: "hello", value: helloMessage } });
  link.send(toBinary(ClientMessageSchema, first), false);
  return welcomed;
}

function webSocketLink(url: string, deliver: (bytes: Uint8Array) => void): Promise<Link> {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    socket.binaryType = "arraybuffer";
    const closed = new Promise<void>((done) => socket.addEventListener("close", () => done(), { once: true }));
    socket.addEventListener("error", () => reject(new Error(`cannot connect to ${url}`)), { once: true });
    socket.addEventListener("message", (event) => deliver(new Uint8Array(event.data as ArrayBuffer)));
    socket.addEventListener(
      "open",
      () =>
        resolve({
          transport: "websocket",
          send: (bytes) => socket.send(bytes),
          close: () => socket.close(),
          closed,
        }),
      { once: true },
    );
  });
}

/**
 * One bidirectional stream carries every reliable message, length-prefixed; server states arrive as datagrams,
 * and intents leave as datagrams when they fit.
 */
async function webTransportLink(endpoint: WebTransportEndpoint, deliver: (bytes: Uint8Array) => void): Promise<Link> {
  const serverCertificateHashes = endpoint.certificateHashes.map((value) => ({ algorithm: "sha-256", value }));
  const transport = new WebTransport(endpoint.url, serverCertificateHashes.length ? { serverCertificateHashes } : {});
  await transport.ready;
  const stream = await transport.createBidirectionalStream();
  const writer = stream.writable.getWriter();
  const datagrams = transport.datagrams.writable.getWriter();
  const closed = transport.closed.then(
    () => undefined,
    () => undefined,
  );

  void (async () => {
    const frames = new FrameReader();
    const reader = stream.readable.getReader();
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      frames.push(value);
      for (let payload = frames.next(); payload; payload = frames.next()) deliver(payload);
    }
  })()
    .catch(() => {})
    .finally(() => transport.close());
  void (async () => {
    const reader = transport.datagrams.readable.getReader();
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      deliver(value);
    }
  })().catch(() => {});

  return {
    transport: "webtransport",
    send(bytes, volatile) {
      if (volatile && bytes.length <= transport.datagrams.maxDatagramSize) {
        datagrams.write(bytes).catch(() => {});
      } else {
        writer.write(frame(bytes)).catch(() => {});
      }
    },
    close: () => transport.close(),
    closed,
  };
}

/** `promise`, or a timeout error after `ms`; `late` receives a result that comes after the timeout. */
function within<T>(promise: Promise<T>, ms: number, late?: (value: T) => void): Promise<T> {
  return new Promise((resolve, reject) => {
    let expired = false;
    const timer = setTimeout(() => {
      expired = true;
      reject(new Error(`no answer within ${ms} ms`));
    }, ms);
    promise.then(
      (value) => {
        clearTimeout(timer);
        if (expired) late?.(value);
        else resolve(value);
      },
      (error) => {
        clearTimeout(timer);
        reject(error);
      },
    );
  });
}

function fromHex(hex: string): Uint8Array<ArrayBuffer> {
  return Uint8Array.from(hex.match(/../g) ?? [], (byte) => Number.parseInt(byte, 16));
}
