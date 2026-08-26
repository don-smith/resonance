import { describe, expect, it, vi } from "vitest";

import { DeclaredPackageEvents, type PackageEventTransport } from "./index.js";

describe("declared package events", () => {
  it("delegates declared emits and subscriptions", async () => {
    const cleanup = vi.fn();
    const transport: PackageEventTransport = {
      emit: vi.fn(async () => undefined),
      listen: vi.fn(async () => cleanup),
    };
    const events = new DeclaredPackageEvents(transport, {
      emits: ["doc:opened"],
      consumes: ["doc:updated"],
    });
    const handler = vi.fn();

    await events.emit("doc:opened", { id: "doc-1" });
    const unlisten = await events.listen("doc:updated", handler);
    unlisten();

    expect(transport.emit).toHaveBeenCalledWith({
      name: "doc:opened",
      payload: { id: "doc-1" },
    });
    expect(transport.listen).toHaveBeenCalledWith("doc:updated", handler);
    expect(cleanup).toHaveBeenCalledOnce();
  });

  it("rejects undeclared event access before transport", async () => {
    const transport: PackageEventTransport = {
      emit: vi.fn(async () => undefined),
      listen: vi.fn(async () => () => undefined),
    };
    const events = new DeclaredPackageEvents(transport, {
      emits: [],
      consumes: [],
    });

    await expect(events.emit("doc:opened", null)).rejects.toThrow(
      "Undeclared package event emit: doc:opened",
    );
    await expect(events.listen("doc:updated", vi.fn())).rejects.toThrow(
      "Undeclared package event subscription: doc:updated",
    );
    expect(transport.emit).not.toHaveBeenCalled();
    expect(transport.listen).not.toHaveBeenCalled();
  });
});
