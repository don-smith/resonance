import { describe, expect, it, vi } from "vitest";

import type {
  ConversationsRequest,
  ConversationsResponse,
  ConversationsSnapshot,
} from "@resonance/contracts";
import { conversationsError } from "@resonance/package-sdk";
import { ConversationsTauriAdapter } from "./conversations-tauri-adapter.js";

function snapshot(
  state: ConversationsSnapshot["synchronization"],
): ConversationsSnapshot {
  return {
    workspaceId: "workspace",
    synchronization: state,
    channels: [
      {
        channelId: "general",
        name: "general",
        archived: false,
        unreadCount: 0,
        canManage: true,
      },
    ],
  };
}

function response(
  state: ConversationsSnapshot["synchronization"],
): ConversationsResponse {
  return { operation: "snapshot", snapshot: snapshot(state) };
}

function transport(
  invokeImplementation: (
    request: ConversationsRequest,
  ) => Promise<unknown> = async () => response("current"),
) {
  let eventHandler: ((event: { payload: unknown }) => void) | undefined;
  const unlisten = vi.fn();
  const invoke = vi.fn(
    async (_command: string, arguments_: { request: ConversationsRequest }) =>
      invokeImplementation(arguments_.request),
  );
  return {
    value: {
      invoke,
      listen: vi.fn(
        async (
          _eventName: string,
          handler: (event: { payload: unknown }) => void,
        ) => {
          eventHandler = handler;
          return unlisten;
        },
      ),
    },
    invoke,
    unlisten,
    emit: (payload: unknown) => eventHandler?.({ payload }),
  };
}

describe("conversations Tauri adapter", () => {
  it("validates requests and responses and maps only finite errors", async () => {
    const requests: ConversationsRequest[] = [];
    const fake = transport(async (request) => {
      requests.push(request);
      if (request.operation === "archive-channel")
        throw conversationsError("unauthorized");
      return response("current");
    });
    const adapter = new ConversationsTauriAdapter(fake.value);
    await adapter.ready();
    await expect(adapter.snapshot()).resolves.toEqual(snapshot("current"));
    await expect(adapter.archiveChannel("peer")).rejects.toEqual(
      conversationsError("unauthorized"),
    );
    expect(requests).toEqual([
      { operation: "snapshot" },
      { operation: "archive-channel", channelId: "peer" },
    ]);
    expect(fake.invoke).toHaveBeenCalledWith("conversations_v1", {
      request: { operation: "snapshot" },
    });
  });

  it("delivers only secret-free valid invalidations", async () => {
    const fake = transport();
    const adapter = new ConversationsTauriAdapter(fake.value);
    const listener = vi.fn();
    adapter.subscribe(listener);
    await adapter.ready();
    fake.emit({
      workspaceId: "workspace",
      channelId: "general",
      exactRecord: [1],
    });
    fake.emit({
      workspaceId: "workspace",
      channelId: "general",
      messageId: "message",
    });
    expect(listener).toHaveBeenCalledOnce();
    expect(listener).toHaveBeenCalledWith({
      workspaceId: "workspace",
      channelId: "general",
      messageId: "message",
    });
  });

  it("suppresses stale snapshots", async () => {
    const resolvers: Array<(value: unknown) => void> = [];
    const fake = transport(
      () => new Promise((resolve) => resolvers.push(resolve)),
    );
    const adapter = new ConversationsTauriAdapter(fake.value);
    await adapter.ready();
    const first = adapter.snapshot();
    const second = adapter.snapshot();
    await vi.waitFor(() => expect(resolvers).toHaveLength(2));
    resolvers[1]?.(response("current"));
    await expect(second).resolves.toEqual(snapshot("current"));
    resolvers[0]?.(response("offline"));
    await expect(first).resolves.toEqual(snapshot("current"));
  });

  it("reports listener failure, rejects unsafe response data, and disposes once", async () => {
    const failed = transport();
    failed.value.listen = vi.fn(async () => {
      throw new Error("private detail");
    });
    const unavailable = new ConversationsTauriAdapter(failed.value);
    await expect(unavailable.ready()).rejects.toEqual(
      conversationsError("unavailable-capability"),
    );

    const unsafe = transport(async () => ({
      operation: "snapshot",
      snapshot: { ...snapshot("current"), epochKey: "secret" },
    }));
    const adapter = new ConversationsTauriAdapter(unsafe.value);
    await adapter.ready();
    await expect(adapter.snapshot()).rejects.toEqual(
      conversationsError("internal"),
    );
    await adapter.dispose();
    await adapter.dispose();
    expect(unsafe.unlisten).toHaveBeenCalledOnce();
  });
});
