import { readFile } from "node:fs/promises";

import { describe, expect, it, vi } from "vitest";

import {
  conversationsError,
  type ConversationsInvalidationListener,
  type ConversationsSnapshot,
  type ConversationsV1,
} from "@resonance/package-sdk";
import { InMemoryConversationsV1 } from "@resonance/package-sdk/testing";
import { ConversationsPackage } from "./package.js";
import { synchronizationLabel, unreadLabel } from "./view.js";

function snapshot(): ConversationsSnapshot {
  return {
    workspaceId: "workspace",
    synchronization: "current",
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

class FakeElement {
  public readonly dataset: Record<string, string | undefined> = {};
  public readonly children: FakeElement[] = [];
  public className = "";
  public textContent = "";
  public type = "";
  public name = "";
  public value = "";
  public placeholder = "";
  public required = false;
  public disabled = false;
  public maxLength = 0;
  public readonly ownerDocument: { createElement: () => FakeElement };

  public constructor(document?: { createElement: () => FakeElement }) {
    this.ownerDocument = document ?? {
      createElement: () => new FakeElement(this.ownerDocument),
    };
  }

  public addEventListener(): void {}
  public removeEventListener(): void {}
  public setAttribute(): void {}
  public contains(): boolean {
    return true;
  }
  public append(...children: FakeElement[]): void {
    this.children.push(...children);
  }
  public replaceChildren(...children: FakeElement[]): void {
    this.children.splice(0, this.children.length, ...children);
  }
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe("bundled conversations package", () => {
  it("starts with empty #general and supports public channel lifecycle and attributed Markdown", async () => {
    const conversations = new InMemoryConversationsV1(snapshot());
    expect((await conversations.messages("general")).messages).toEqual([]);
    const channel = await conversations.createChannel("planning");
    expect(
      (await conversations.renameChannel(channel.channelId, "roadmap")).name,
    ).toBe("roadmap");
    const message = await conversations.postMessage(
      channel.channelId,
      "**Milestone** ready",
    );
    expect(message).toMatchObject({
      author: { displayName: "You" },
      markdown: "**Milestone** ready",
    });
    await conversations.archiveChannel(channel.channelId);
    await expect(
      conversations.postMessage(channel.channelId, "late"),
    ).rejects.toEqual(conversationsError("archived-channel"));
  });

  it("updates local unread state and refreshes from secret-free invalidations", async () => {
    const conversations = new InMemoryConversationsV1({
      ...snapshot(),
      channels: [{ ...snapshot().channels[0]!, unreadCount: 2 }],
    });
    const invalidation = vi.fn();
    const dispose = conversations.subscribe(invalidation);
    const message = await conversations.postMessage("general", "new");
    await conversations.markRead("general", message.messageId);
    expect((await conversations.snapshot()).channels[0]?.unreadCount).toBe(0);
    expect(invalidation).toHaveBeenCalledWith({
      workspaceId: "workspace",
      channelId: "general",
      messageId: message.messageId,
    });
    dispose();
    conversations.invalidate("general");
    expect(invalidation).toHaveBeenCalledTimes(2);
  });

  it("refreshes invalidations, suppresses stale activation, and disposes its listener", async () => {
    let invalidation: ConversationsInvalidationListener = () => undefined;
    const unsubscribe = vi.fn();
    const first = deferred<ConversationsSnapshot>();
    const snapshotCall = vi
      .fn()
      .mockReturnValueOnce(first.promise)
      .mockResolvedValue(snapshot());
    const capability = {
      snapshot: snapshotCall,
      messages: vi.fn(async (channelId: string) => ({
        channelId,
        messages: [],
        nextCursor: null,
      })),
      subscribe: vi.fn((listener: ConversationsInvalidationListener) => {
        invalidation = listener;
        return unsubscribe;
      }),
    } as unknown as ConversationsV1;
    const root = new FakeElement() as unknown as HTMLElement;
    const packageInstance = new ConversationsPackage(root, capability);

    const stale = packageInstance.activate();
    await packageInstance.activate();
    first.resolve({ ...snapshot(), synchronization: "offline" });
    await stale;
    expect(snapshotCall).toHaveBeenCalledTimes(2);

    invalidation({ workspaceId: "workspace", channelId: "general" });
    await vi.waitFor(() => expect(snapshotCall).toHaveBeenCalledTimes(3));
    packageInstance.dispose();
    invalidation({ workspaceId: "workspace", channelId: "general" });
    await Promise.resolve();
    expect(snapshotCall).toHaveBeenCalledTimes(3);
    expect(unsubscribe).toHaveBeenCalledOnce();
  });

  it("uses only the three finite synchronization labels", () => {
    expect(
      (["offline", "waiting-to-sync", "current"] as const).map(
        synchronizationLabel,
      ),
    ).toEqual(["Offline", "Waiting to sync", "Current"]);
    expect([unreadLabel(0), unreadLabel(1), unreadLabel(2)]).toEqual([
      "",
      "1 unread",
      "2 unread",
    ]);
  });

  it("does not offer deferred conversation or membership controls", async () => {
    const source = await readFile(
      "packages/conversations/src/package.ts",
      "utf8",
    );
    for (const excluded of [
      "read-receipt",
      "reply-message",
      "thread-message",
      "edit-message",
      "delete-message",
      "approve-removal",
      "private-channel",
      "direct-message",
      "attach-file",
    ]) {
      expect(source).not.toContain(excluded);
    }
  });
});
