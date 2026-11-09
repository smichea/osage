// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

import { type DescMessage, type MessageShape, fromBinary } from "@bufbuild/protobuf";
import { ControllerSchema } from "./gen/osage/components/v1/components_pb.js";
import type { ComponentType, ServerMessage, Welcome } from "./gen/osage/v0/protocol_pb.js";

/** Names of the core component types. */
export const types = {
  transform: "osage.transform/1",
  contentRef: "osage.content_ref/1",
  controller: "osage.controller/1",
  moveIntent: "osage.move_intent/1",
  device: "osage.device/1",
  /** Provisional, to be replaced by OpenDeviceDB's contract (opendevicedb#33). */
  measurements: "osage.measurements/1",
  builtinCube: "osage:builtin/cube",
  builtinAvatar: "osage:builtin/avatar",
  builtinDevice: "osage:builtin/device",
} as const;

export type WorldEvent =
  | { kind: "entered"; entity: bigint }
  | { kind: "left"; entity: bigint }
  | { kind: "changed"; entity: bigint; typeName: string };

/** The client's view of the world: entities in scope and their encoded component values, by type name. */
export class World {
  private readonly typesById = new Map<number, ComponentType>();
  private readonly entityMap = new Map<bigint, Map<string, Uint8Array>>();
  tick = 0n;

  constructor(welcome: Welcome) {
    for (const type of welcome.schemas?.types ?? []) this.typesById.set(type.id, type);
  }

  entities(): IterableIterator<bigint> {
    return this.entityMap.keys();
  }

  /** Session id of component type `typeName`, as announced in the Welcome. */
  typeId(typeName: string): number | undefined {
    for (const [id, type] of this.typesById) if (type.name === typeName) return id;
    return undefined;
  }

  /** The entity whose `osage.controller/1` names `perWorldId` — for a player, its avatar. */
  controlledBy(perWorldId: string): bigint | undefined {
    for (const entity of this.entityMap.keys()) {
      if (this.get(entity, types.controller, ControllerSchema)?.perWorldId === perWorldId) return entity;
    }
    return undefined;
  }

  /** Decodes component `typeName` (e.g. `osage.transform/1`) of `entity` with `schema`. */
  get<Desc extends DescMessage>(entity: bigint, typeName: string, schema: Desc): MessageShape<Desc> | undefined {
    const bytes = this.entityMap.get(entity)?.get(typeName);
    return bytes === undefined ? undefined : fromBinary(schema, bytes);
  }

  /** Applies one server message and reports what changed. */
  apply(message: ServerMessage): WorldEvent[] {
    const events: WorldEvent[] = [];
    const m = message.message;
    switch (m.case) {
      case "entityEnter": {
        const components = new Map<string, Uint8Array>();
        for (const c of m.value.components) {
          const type = this.typesById.get(c.type);
          if (type) components.set(type.name, c.value);
        }
        this.entityMap.set(m.value.entity, components);
        events.push({ kind: "entered", entity: m.value.entity });
        break;
      }
      case "entityLeave":
        if (this.entityMap.delete(m.value.entity)) events.push({ kind: "left", entity: m.value.entity });
        break;
      case "componentChanged":
        if (m.value.component) this.set(m.value.entity, m.value.component.type, m.value.component.value, m.value.removed, events);
        break;
      case "state":
        this.tick = m.value.tick;
        for (const v of m.value.values) {
          if (v.component) this.set(v.entity, v.component.type, v.component.value, false, events);
        }
        break;
    }
    return events;
  }

  private set(entity: bigint, typeId: number, value: Uint8Array, removed: boolean, events: WorldEvent[]): void {
    const type = this.typesById.get(typeId);
    const components = this.entityMap.get(entity);
    if (!type || !components) return;
    if (removed) components.delete(type.name);
    else components.set(type.name, value);
    events.push({ kind: "changed", entity, typeName: type.name });
  }
}
