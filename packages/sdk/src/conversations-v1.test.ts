import { describe, expect, it, vi } from "vitest";

import type { ConversationsSnapshot } from "@resonance/contracts";
import { InMemoryConversationsV1 } from "@resonance/package-sdk/testing";
import { conversationsError } from "./conversations-v1.js";

function snapshot(): ConversationsSnapshot {
  return {
    workspaceId: "workspace",
    synchronization: "current",
    channels: [
      {
        channelId: "general",
        name: "#general",
        archived: false,
        unreadCount: 1,
        canManage: true,
      },
      {
        channelId: "peer",
        name: "#peer-channel",
        archived: false,
        unreadCount: 0,
        canManage: false,
      },
    ],
  };
}

describe("in-memory conversations v1", () => {
  it("supports channel lifecycle, attributed Markdown, paging, and local unread updates", async () => {
    const adapter = new InMemoryConversationsV1(snapshot());
    const invalidated = vi.fn();
    const unsubscribe = adapter.subscribe(invalidated);
    const channel = await adapter.createChannel("#planning");
    await adapter.renameChannel(channel.channelId, "#roadmap");
    const first = await adapter.postMessage(channel.channelId, "**one**");
    await adapter.postMessage(channel.channelId, "two");
    const page = await adapter.messages(channel.channelId, null, 1);
    expect(page.messages).toEqual([first]);
    expect(page.nextCursor).toBe(first.messageId);
    await expect(
      adapter.markRead(channel.channelId, first.messageId),
    ).resolves.toBe(0);
    await adapter.archiveChannel(channel.channelId);
    await expect(
      adapter.postMessage(channel.channelId, "late"),
    ).rejects.toEqual(conversationsError("archived-channel"));
    expect(invalidated).toHaveBeenCalled();
    unsubscribe();
  });

  it("enforces creator-only lifecycle behavior and finite synchronization labels", async () => {
    const adapter = new InMemoryConversationsV1(snapshot());
    await expect(adapter.renameChannel("peer", "denied")).rejects.toEqual(
      conversationsError("unauthorized"),
    );
    for (const state of ["offline", "waiting-to-sync", "current"] as const) {
      adapter.setSynchronization(state);
      await expect(adapter.synchronizationState()).resolves.toBe(state);
    }
  });

  it("clones values, disposes subscriptions, and exposes finite failures", async () => {
    const adapter = new InMemoryConversationsV1(snapshot());
    const listener = vi.fn();
    const unsubscribe = adapter.subscribe(listener);
    unsubscribe();
    adapter.invalidate("general");
    expect(listener).not.toHaveBeenCalled();
    const value = await adapter.snapshot();
    expect(Object.isFrozen(value)).toBe(true);
    adapter.failNext("snapshot", conversationsError("internal"));
    await expect(adapter.snapshot()).rejects.toEqual(
      conversationsError("internal"),
    );
  });
});
