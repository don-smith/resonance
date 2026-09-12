import {
  validateConversationsResponse,
  type ConversationChannel,
  type ConversationMessage,
  type ConversationMessagePage,
  type ConversationsError,
  type ConversationsSnapshot,
  type ConversationsV1Operation,
} from "@resonance/contracts";
import { immutableClone } from "../immutable.js";
import {
  conversationsError,
  type ConversationsFailureController,
  type ConversationsInvalidationListener,
  type ConversationsV1,
} from "../conversations-v1.js";

function validated<T>(response: object, value: T): T {
  const result = validateConversationsResponse(response);
  if (result.kind === "invalid") {
    throw new TypeError(
      `Invalid conversations test value: ${result.diagnostics.map(({ path, message }) => `${path} ${message}`).join(", ")}`,
    );
  }
  return immutableClone(value);
}

export class InMemoryConversationsV1
  implements ConversationsV1, ConversationsFailureController
{
  readonly #listeners = new Set<ConversationsInvalidationListener>();
  readonly #failures = new Map<ConversationsV1Operation, ConversationsError>();
  readonly #messages = new Map<string, ConversationMessage[]>();
  #snapshot: ConversationsSnapshot;
  #sequence = 0;

  public constructor(
    snapshot: ConversationsSnapshot,
    messages: readonly ConversationMessage[] = [],
  ) {
    this.#snapshot = validated({ operation: "snapshot", snapshot }, snapshot);
    for (const message of messages) {
      const list = this.#messages.get(message.channelId) ?? [];
      list.push(validated({ operation: "post-message", message }, message));
      this.#messages.set(message.channelId, list);
    }
  }

  public failNext(
    operation: ConversationsV1Operation,
    error: ConversationsError,
  ): void {
    this.#failures.set(operation, error);
  }

  public setSynchronization(
    synchronization: ConversationsSnapshot["synchronization"],
  ): void {
    this.#snapshot = validated(
      {
        operation: "snapshot",
        snapshot: { ...this.#snapshot, synchronization },
      },
      { ...this.#snapshot, synchronization },
    );
    this.invalidate(this.#snapshot.channels[0]?.channelId ?? "general");
  }

  public invalidate(channelId: string, messageId?: string): void {
    const invalidation = immutableClone({
      workspaceId: this.#snapshot.workspaceId,
      channelId,
      ...(messageId ? { messageId } : {}),
    });
    for (const listener of this.#listeners) listener(invalidation);
  }

  public async snapshot(): Promise<ConversationsSnapshot> {
    this.#fail("snapshot");
    return immutableClone(this.#snapshot);
  }

  public async messages(
    channelId: string,
    cursor: string | null = null,
    limit = 50,
  ): Promise<ConversationMessagePage> {
    this.#fail("messages");
    this.#channel(channelId);
    const source = this.#messages.get(channelId) ?? [];
    const cursorIndex =
      cursor === null
        ? -1
        : source.findIndex(({ messageId }) => messageId === cursor);
    if (cursor !== null && cursorIndex < 0) {
      throw conversationsError("invalid-request");
    }
    const start = cursorIndex + 1;
    const messages = source.slice(start, start + limit);
    const nextCursor =
      start + messages.length < source.length
        ? (messages.at(-1)?.messageId ?? null)
        : null;
    const page = { channelId, messages, nextCursor };
    return validated({ operation: "messages", page }, page);
  }

  public async createChannel(name: string): Promise<ConversationChannel> {
    this.#fail("create-channel");
    const channel = validated(
      {
        operation: "create-channel",
        channel: {
          channelId: `memory-channel-${++this.#sequence}`,
          name,
          archived: false,
          unreadCount: 0,
          canManage: true,
        },
      },
      {
        channelId: `memory-channel-${this.#sequence}`,
        name,
        archived: false,
        unreadCount: 0,
        canManage: true,
      },
    );
    this.#snapshot = validated(
      {
        operation: "snapshot",
        snapshot: {
          ...this.#snapshot,
          channels: [...this.#snapshot.channels, channel],
        },
      },
      { ...this.#snapshot, channels: [...this.#snapshot.channels, channel] },
    );
    this.invalidate(channel.channelId);
    return channel;
  }

  public async renameChannel(
    channelId: string,
    name: string,
  ): Promise<ConversationChannel> {
    this.#fail("rename-channel");
    const current = this.#manageable(channelId);
    const channel = validated(
      { operation: "rename-channel", channel: { ...current, name } },
      { ...current, name },
    );
    this.#replaceChannel(channel);
    this.invalidate(channelId);
    return channel;
  }

  public async archiveChannel(channelId: string): Promise<ConversationChannel> {
    this.#fail("archive-channel");
    const current = this.#manageable(channelId);
    const channel = validated(
      { operation: "archive-channel", channel: { ...current, archived: true } },
      { ...current, archived: true },
    );
    this.#replaceChannel(channel);
    this.invalidate(channelId);
    return channel;
  }

  public async postMessage(
    channelId: string,
    markdown: string,
  ): Promise<ConversationMessage> {
    this.#fail("post-message");
    const channel = this.#channel(channelId);
    if (channel.archived) throw conversationsError("archived-channel");
    const message = validated(
      {
        operation: "post-message",
        message: {
          messageId: `memory-message-${++this.#sequence}`,
          channelId,
          author: { publicIdentity: "memory-local", displayName: "You" },
          createdAt: this.#sequence,
          markdown,
        },
      },
      {
        messageId: `memory-message-${this.#sequence}`,
        channelId,
        author: { publicIdentity: "memory-local", displayName: "You" },
        createdAt: this.#sequence,
        markdown,
      },
    );
    this.#messages.set(channelId, [
      ...(this.#messages.get(channelId) ?? []),
      message,
    ]);
    this.invalidate(channelId, message.messageId);
    return message;
  }

  public async markRead(channelId: string, messageId: string): Promise<number> {
    this.#fail("mark-read");
    const channel = this.#channel(channelId);
    if (
      !(this.#messages.get(channelId) ?? []).some(
        (message) => message.messageId === messageId,
      )
    ) {
      throw conversationsError("invalid-request");
    }
    this.#replaceChannel({ ...channel, unreadCount: 0 });
    this.invalidate(channelId, messageId);
    return 0;
  }

  public async synchronizationState(): Promise<
    ConversationsSnapshot["synchronization"]
  > {
    this.#fail("synchronization-state");
    return this.#snapshot.synchronization;
  }

  public subscribe(listener: ConversationsInvalidationListener): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  #channel(channelId: string): ConversationChannel {
    const channel = this.#snapshot.channels.find(
      (candidate) => candidate.channelId === channelId,
    );
    if (!channel) throw conversationsError("missing-channel");
    return channel;
  }

  #manageable(channelId: string): ConversationChannel {
    const channel = this.#channel(channelId);
    if (!channel.canManage) throw conversationsError("unauthorized");
    if (channel.archived) throw conversationsError("archived-channel");
    return channel;
  }

  #replaceChannel(channel: ConversationChannel): void {
    const snapshot = {
      ...this.#snapshot,
      channels: this.#snapshot.channels.map((candidate) =>
        candidate.channelId === channel.channelId ? channel : candidate,
      ),
    };
    this.#snapshot = validated({ operation: "snapshot", snapshot }, snapshot);
  }

  #fail(operation: ConversationsV1Operation): void {
    const error = this.#failures.get(operation);
    if (!error) return;
    this.#failures.delete(operation);
    throw error;
  }
}
