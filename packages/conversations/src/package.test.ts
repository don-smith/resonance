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
        name: "#general",
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
  public form: FakeElement | null = null;
  public readonly ownerDocument: { createElement: () => FakeElement };
  readonly #listeners = new Map<string, (event: Event) => void>();

  public constructor(document?: { createElement: () => FakeElement }) {
    this.ownerDocument = document ?? {
      createElement: () => new FakeElement(this.ownerDocument),
    };
  }

  public addEventListener(
    type: string,
    listener: (event: Event) => void,
  ): void {
    this.#listeners.set(type, listener);
  }
  public removeEventListener(type: string): void {
    this.#listeners.delete(type);
  }
  public dispatch(type: string, target: FakeElement): void {
    this.#listeners.get(type)?.({
      target,
      preventDefault: () => undefined,
    } as unknown as Event);
  }
  public closest<T extends FakeElement>(): T | null {
    return this.dataset.action ? (this as unknown as T) : null;
  }
  public setAttribute(): void {}
  public focus(): void {}
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

function renderedText(element: FakeElement): string[] {
  return [element.textContent, ...element.children.flatMap(renderedText)];
}

function formForAction(element: FakeElement, action: string): FakeElement {
  const result = findFormForAction(element, action);
  if (result) return result;
  throw new Error(`Missing ${action} form`);
}

function findFormForAction(
  element: FakeElement,
  action: string,
): FakeElement | null {
  if (element.dataset.action === action) return element;
  for (const child of element.children) {
    const result = findFormForAction(child, action);
    if (result) return result;
  }
  return null;
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
    const channel = await conversations.createChannel("#planning");
    expect(
      (await conversations.renameChannel(channel.channelId, "#roadmap")).name,
    ).toBe("#roadmap");
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

  it("renders the runtime's empty #general channel exactly once", async () => {
    const conversations = new InMemoryConversationsV1({
      ...snapshot(),
      channels: [{ ...snapshot().channels[0]!, name: "#general" }],
    });
    const root = new FakeElement() as unknown as HTMLElement;
    const packageInstance = new ConversationsPackage(root, conversations);

    await packageInstance.activate();

    const text = renderedText(root as unknown as FakeElement);
    expect(text).toContain("#general");
    expect(text).not.toContain("##general");
    expect(text).toContain("#general is ready for the first message.");
  });

  it("does not replace focused controls for an unchanged invalidation", async () => {
    const conversations = new InMemoryConversationsV1(snapshot());
    const snapshotCall = vi.spyOn(conversations, "snapshot");
    const root = new FakeElement() as unknown as HTMLElement;
    const packageInstance = new ConversationsPackage(root, conversations);

    await packageInstance.activate();
    const heading = (root as unknown as FakeElement).children[0];
    conversations.invalidate("general");

    await vi.waitFor(() => expect(snapshotCall).toHaveBeenCalledTimes(2));
    expect((root as unknown as FakeElement).children[0]).toBe(heading);
  });

  it("keeps the current channel selected when another channel is invalidated", async () => {
    let invalidation: ConversationsInvalidationListener = () => undefined;
    let current = snapshot();
    const messages = vi.fn(async (channelId: string) => ({
      channelId,
      messages: [],
      nextCursor: null,
    }));
    const capability = {
      snapshot: vi.fn(async () => current),
      messages,
      subscribe: vi.fn((listener: ConversationsInvalidationListener) => {
        invalidation = listener;
        return () => undefined;
      }),
    } as unknown as ConversationsV1;
    const root = new FakeElement() as unknown as HTMLElement;
    const packageInstance = new ConversationsPackage(root, capability);

    await packageInstance.activate();
    current = {
      ...snapshot(),
      channels: [
        ...snapshot().channels,
        {
          channelId: "planning",
          name: "#planning",
          archived: false,
          unreadCount: 0,
          canManage: true,
        },
      ],
    };
    invalidation({ workspaceId: "workspace", channelId: "planning" });

    await vi.waitFor(() => expect(messages).toHaveBeenCalledTimes(2));
    expect(messages).toHaveBeenLastCalledWith("general", null, 100);
  });

  it("renders an empty channel selection even when its data matches the current channel", async () => {
    const conversations = new InMemoryConversationsV1({
      ...snapshot(),
      channels: [
        ...snapshot().channels,
        {
          channelId: "planning",
          name: "#planning",
          archived: false,
          unreadCount: 0,
          canManage: true,
        },
      ],
    });
    const root = new FakeElement() as unknown as HTMLElement;
    const packageInstance = new ConversationsPackage(root, conversations);
    const previousElement = globalThis.Element;
    Object.defineProperty(globalThis, "Element", {
      configurable: true,
      value: FakeElement,
    });

    try {
      await packageInstance.activate();
      const heading = (root as unknown as FakeElement).children[0];
      const planning = new FakeElement();
      planning.dataset.action = "select-channel";
      planning.dataset.channelId = "planning";
      (root as unknown as FakeElement).dispatch("click", planning);

      await vi.waitFor(() =>
        expect((root as unknown as FakeElement).children[0]).not.toBe(heading),
      );
    } finally {
      Object.defineProperty(globalThis, "Element", {
        configurable: true,
        value: previousElement,
      });
    }
  });

  it("renders a selection when an invalidation supersedes its refresh", async () => {
    let invalidation: ConversationsInvalidationListener = () => undefined;
    const current = {
      ...snapshot(),
      channels: [
        ...snapshot().channels,
        {
          channelId: "planning",
          name: "#planning",
          archived: false,
          unreadCount: 0,
          canManage: true,
        },
      ],
    };
    const selectionSnapshot = deferred<ConversationsSnapshot>();
    const snapshotCall = vi
      .fn()
      .mockResolvedValueOnce(current)
      .mockReturnValueOnce(selectionSnapshot.promise)
      .mockResolvedValue(current);
    const messages = vi.fn(async (channelId: string) => ({
      channelId,
      messages: [],
      nextCursor: null,
    }));
    const capability = {
      snapshot: snapshotCall,
      messages,
      subscribe: vi.fn((listener: ConversationsInvalidationListener) => {
        invalidation = listener;
        return () => undefined;
      }),
    } as unknown as ConversationsV1;
    const root = new FakeElement() as unknown as HTMLElement;
    const packageInstance = new ConversationsPackage(root, capability);
    const previousElement = globalThis.Element;
    Object.defineProperty(globalThis, "Element", {
      configurable: true,
      value: FakeElement,
    });

    try {
      await packageInstance.activate();
      const heading = (root as unknown as FakeElement).children[0];
      const planning = new FakeElement();
      planning.dataset.action = "select-channel";
      planning.dataset.channelId = "planning";
      (root as unknown as FakeElement).dispatch("click", planning);
      await vi.waitFor(() => expect(snapshotCall).toHaveBeenCalledTimes(2));
      invalidation({ workspaceId: "workspace", channelId: "general" });
      await vi.waitFor(() => expect(messages).toHaveBeenCalledTimes(2));
      selectionSnapshot.resolve(current);

      await vi.waitFor(() =>
        expect((root as unknown as FakeElement).children[0]).not.toBe(heading),
      );
    } finally {
      Object.defineProperty(globalThis, "Element", {
        configurable: true,
        value: previousElement,
      });
    }
  });

  it("clears create and post drafts after successful authoring", async () => {
    const conversations = new InMemoryConversationsV1(snapshot());
    const root = new FakeElement() as unknown as HTMLElement;
    const packageInstance = new ConversationsPackage(root, conversations);
    const previousInput = globalThis.HTMLInputElement;
    const previousTextArea = globalThis.HTMLTextAreaElement;
    const previousFormData = globalThis.FormData;
    const TestFormData = class {
      public constructor(private readonly form: FakeElement) {}

      public get(name: string): string | null {
        return (
          this.form.children.find((control) => control.name === name)?.value ??
          null
        );
      }
    };
    Object.defineProperties(globalThis, {
      HTMLInputElement: { configurable: true, value: FakeElement },
      HTMLTextAreaElement: { configurable: true, value: FakeElement },
      FormData: { configurable: true, value: TestFormData },
    });

    try {
      await packageInstance.activate();
      const create = formForAction(
        root as unknown as FakeElement,
        "create-channel",
      );
      const createInput = create.children[0]!;
      createInput.form = create;
      createInput.value = "planning";
      (root as unknown as FakeElement).dispatch("input", createInput);
      (root as unknown as FakeElement).dispatch("submit", create);

      await vi.waitFor(async () =>
        expect((await conversations.snapshot()).channels).toHaveLength(2),
      );
      await vi.waitFor(() =>
        expect(
          formForAction(root as unknown as FakeElement, "create-channel")
            .children[0]?.value,
        ).toBe(""),
      );

      const post = formForAction(
        root as unknown as FakeElement,
        "post-message",
      );
      const markdown = post.children[0]!;
      markdown.form = post;
      markdown.value = "hello";
      (root as unknown as FakeElement).dispatch("input", markdown);
      (root as unknown as FakeElement).dispatch("submit", post);

      await vi.waitFor(async () => {
        const createdChannelId = (await conversations.snapshot()).channels[1]
          ?.channelId;
        expect(createdChannelId).toBeDefined();
        expect(
          (await conversations.messages(createdChannelId!)).messages,
        ).toHaveLength(1);
      });
      await vi.waitFor(() =>
        expect(
          formForAction(root as unknown as FakeElement, "post-message")
            .children[0]?.value,
        ).toBe(""),
      );
    } finally {
      Object.defineProperties(globalThis, {
        HTMLInputElement: { configurable: true, value: previousInput },
        HTMLTextAreaElement: { configurable: true, value: previousTextArea },
        FormData: { configurable: true, value: previousFormData },
      });
    }
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

  it("renders a finite error when loading an additional page fails", async () => {
    const root = new FakeElement();
    const messages = vi
      .fn()
      .mockResolvedValueOnce({
        channelId: "general",
        messages: [
          {
            messageId: "first",
            author: { displayName: "You" },
            markdown: "first",
            createdAt: 0,
          },
        ],
        nextCursor: "next",
      })
      .mockRejectedValueOnce(conversationsError("unavailable-capability"));
    const capability = {
      snapshot: vi.fn(async () => snapshot()),
      messages,
      subscribe: vi.fn(() => () => undefined),
    } as unknown as ConversationsV1;
    const packageInstance = new ConversationsPackage(
      root as unknown as HTMLElement,
      capability,
    );
    const previousElement = globalThis.Element;
    Object.defineProperty(globalThis, "Element", {
      configurable: true,
      value: FakeElement,
    });

    try {
      await packageInstance.activate();
      const button = new FakeElement();
      button.dataset.action = "load-more";
      root.dispatch("click", button);
      await vi.waitFor(() =>
        expect(renderedText(root)).toContain("Conversations are unavailable."),
      );
    } finally {
      Object.defineProperty(globalThis, "Element", {
        configurable: true,
        value: previousElement,
      });
    }
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
