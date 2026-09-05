import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import {
  validateConversationsError,
  validateConversationsInvalidation,
  validateConversationsRequest,
  validateConversationsResponse,
  type ConversationChannel,
  type ConversationMessage,
  type ConversationMessagePage,
  type ConversationsRequest,
  type ConversationsResponse,
  type ConversationsSnapshot,
} from "@resonance/contracts";
import {
  conversationsError,
  immutableClone,
  type ConversationsInvalidationListener,
  type ConversationsV1,
} from "@resonance/package-sdk";

const INVALIDATION_EVENT = "conversations:changed";

export type ConversationsTransport = {
  invoke(command: string, arguments_: { request: unknown }): Promise<unknown>;
  listen(
    eventName: string,
    handler: (event: { payload: unknown }) => void,
  ): Promise<() => void>;
};

const tauriConversationsTransport: ConversationsTransport = {
  invoke: (command, arguments_) => invoke(command, arguments_),
  listen: (eventName, handler) => listen(eventName, handler),
};

export class ConversationsTauriAdapter implements ConversationsV1 {
  readonly #listeners = new Set<ConversationsInvalidationListener>();
  readonly #transport: ConversationsTransport;
  readonly #ready: Promise<void>;
  readonly #issued = new Map<string, number>();
  readonly #applied = new Map<string, number>();
  readonly #latest = new Map<string, ConversationsResponse>();
  #unlisten: (() => void) | null = null;
  #startupFailed = false;
  #disposed = false;
  #disposePromise: Promise<void> | null = null;

  public constructor(
    transport: ConversationsTransport = tauriConversationsTransport,
  ) {
    this.#transport = transport;
    this.#ready = transport
      .listen(INVALIDATION_EVENT, ({ payload }) => {
        if (this.#disposed) return;
        const result = validateConversationsInvalidation(payload);
        if (result.kind === "invalid") return;
        const invalidation = immutableClone(result.value);
        for (const listener of this.#listeners) {
          try {
            listener(invalidation);
          } catch {
            /* Subscriber isolation is deliberate. */
          }
        }
      })
      .then((unlisten) => {
        if (this.#disposed) unlisten();
        else this.#unlisten = unlisten;
      })
      .catch(() => {
        this.#startupFailed = true;
      });
  }

  public async ready(): Promise<void> {
    await this.#ready;
    if (this.#startupFailed) throw conversationsError("unavailable-capability");
  }

  public subscribe(listener: ConversationsInvalidationListener): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  public async snapshot(): Promise<ConversationsSnapshot> {
    const response = await this.#dispatch(
      { operation: "snapshot" },
      "snapshot",
    );
    if (!("snapshot" in response)) throw conversationsError("internal");
    return response.snapshot;
  }

  public async messages(
    channelId: string,
    cursor: string | null = null,
    limit = 50,
  ): Promise<ConversationMessagePage> {
    const response = await this.#dispatch(
      { operation: "messages", channelId, cursor, limit },
      `messages:${channelId}:${cursor ?? ""}:${limit}`,
    );
    if (!("page" in response)) throw conversationsError("internal");
    return response.page;
  }

  public async createChannel(name: string): Promise<ConversationChannel> {
    return this.#channel(
      await this.#dispatch({ operation: "create-channel", name }),
    );
  }

  public async renameChannel(
    channelId: string,
    name: string,
  ): Promise<ConversationChannel> {
    return this.#channel(
      await this.#dispatch({ operation: "rename-channel", channelId, name }),
    );
  }

  public async archiveChannel(channelId: string): Promise<ConversationChannel> {
    return this.#channel(
      await this.#dispatch({ operation: "archive-channel", channelId }),
    );
  }

  public async postMessage(
    channelId: string,
    markdown: string,
  ): Promise<ConversationMessage> {
    const response = await this.#dispatch({
      operation: "post-message",
      channelId,
      markdown,
    });
    if (!("message" in response)) throw conversationsError("internal");
    return response.message;
  }

  public async markRead(channelId: string, messageId: string): Promise<number> {
    const response = await this.#dispatch({
      operation: "mark-read",
      channelId,
      messageId,
    });
    if (!("unreadCount" in response)) throw conversationsError("internal");
    return response.unreadCount;
  }

  public async synchronizationState(): Promise<
    ConversationsSnapshot["synchronization"]
  > {
    const response = await this.#dispatch(
      { operation: "synchronization-state" },
      "synchronization-state",
    );
    if (!("synchronization" in response)) throw conversationsError("internal");
    return response.synchronization;
  }

  public dispose(): Promise<void> {
    if (this.#disposePromise) return this.#disposePromise;
    this.#disposed = true;
    this.#listeners.clear();
    this.#disposePromise = this.#ready.then(() => {
      this.#unlisten?.();
      this.#unlisten = null;
    });
    return this.#disposePromise;
  }

  async #dispatch(
    request: ConversationsRequest,
    staleKey?: string,
  ): Promise<ConversationsResponse> {
    await this.#ready;
    if (this.#disposed || this.#startupFailed)
      throw conversationsError("unavailable-capability");
    if (validateConversationsRequest(request).kind === "invalid")
      throw conversationsError("invalid-request");
    const sequence = staleKey ? (this.#issued.get(staleKey) ?? 0) + 1 : 0;
    if (staleKey) this.#issued.set(staleKey, sequence);
    let candidate: unknown;
    try {
      candidate = await this.#transport.invoke("conversations_v1", { request });
    } catch (error) {
      const safe = validateConversationsError(error);
      throw safe.kind === "valid" ? safe.value : conversationsError("internal");
    }
    const result = validateConversationsResponse(candidate);
    if (
      result.kind === "invalid" ||
      result.value.operation !== request.operation
    )
      throw conversationsError("internal");
    if (staleKey) {
      const applied = this.#applied.get(staleKey) ?? 0;
      if (sequence < applied)
        return immutableClone(this.#latest.get(staleKey) ?? result.value);
      this.#applied.set(staleKey, sequence);
      this.#latest.set(staleKey, immutableClone(result.value));
    }
    return immutableClone(result.value);
  }

  #channel(response: ConversationsResponse): ConversationChannel {
    if (!("channel" in response)) throw conversationsError("internal");
    return response.channel;
  }
}
