// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

import { create, toBinary } from "@bufbuild/protobuf";
import { describe, expect, it } from "vitest";
import { ControllerSchema, TransformSchema } from "../src/gen/osage/components/v1/components_pb.js";
import { ServerMessageSchema, WelcomeSchema } from "../src/gen/osage/v0/protocol_pb.js";
import { World, types } from "../src/world.js";

const welcome = create(WelcomeSchema, {
  protocolVersion: "0.1",
  schemas: {
    types: [
      { id: 1, name: types.transform, messageType: "osage.components.v1.Transform", volatile: true },
      { id: 3, name: types.controller, messageType: "osage.components.v1.Controller" },
    ],
  },
});

const transform = (x: number) => toBinary(TransformSchema, create(TransformSchema, { translation: { x, y: 1, z: 0 } }));

describe("World", () => {
  it("tracks entities and decodes their components", () => {
    const world = new World(welcome);
    const entered = world.apply(
      create(ServerMessageSchema, {
        message: { case: "entityEnter", value: { entity: 7n, components: [{ type: 1, value: transform(1) }] } },
      }),
    );
    expect(entered).toEqual([{ kind: "entered", entity: 7n }]);

    const changed = world.apply(
      create(ServerMessageSchema, {
        message: { case: "state", value: { tick: 3n, values: [{ entity: 7n, component: { type: 1, value: transform(2) } }] } },
      }),
    );
    expect(changed).toEqual([{ kind: "changed", entity: 7n, typeName: types.transform }]);
    expect(world.tick).toBe(3n);
    expect(world.get(7n, types.transform, TransformSchema)?.translation?.x).toBe(2);

    world.apply(create(ServerMessageSchema, { message: { case: "entityLeave", value: { entity: 7n } } }));
    expect([...world.entities()]).toEqual([]);
  });

  it("ignores component types it was not told about", () => {
    const world = new World(welcome);
    world.apply(
      create(ServerMessageSchema, {
        message: { case: "entityEnter", value: { entity: 1n, components: [{ type: 99, value: new Uint8Array([1]) }] } },
      }),
    );
    expect(world.get(1n, "unknown/1", TransformSchema)).toBeUndefined();
  });

  it("finds the entity a player controls", () => {
    const world = new World(welcome);
    const controller = toBinary(ControllerSchema, create(ControllerSchema, { kind: 1, perWorldId: "guest-2" }));
    world.apply(
      create(ServerMessageSchema, {
        message: { case: "entityEnter", value: { entity: 5n, components: [{ type: 3, value: controller }] } },
      }),
    );
    expect(world.typeId(types.controller)).toBe(3);
    expect(world.controlledBy("guest-2")).toBe(5n);
    expect(world.controlledBy("guest-9")).toBeUndefined();
  });

  it("never lets a late datagram replace a newer value", () => {
    const world = new World(welcome);
    for (const entity of [1n, 2n]) {
      world.apply(
        create(ServerMessageSchema, {
          message: { case: "entityEnter", value: { entity, components: [{ type: 1, value: transform(0) }] } },
        }),
      );
    }
    const state = (tick: bigint, entity: bigint, x: number) =>
      create(ServerMessageSchema, {
        message: { case: "state", value: { tick, values: [{ entity, component: { type: 1, value: transform(x) } }] } },
      });
    world.apply(state(10n, 1n, 10));
    world.apply(state(9n, 1n, 9)); // late
    world.apply(state(9n, 2n, 9)); // late, but nothing newer for entity 2
    expect(world.get(1n, types.transform, TransformSchema)?.translation?.x).toBe(10);
    expect(world.get(2n, types.transform, TransformSchema)?.translation?.x).toBe(9);
    expect(world.tick).toBe(10n);
  });
});
