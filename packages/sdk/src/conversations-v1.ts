export type {
  ConversationAuthor,
  ConversationChannel,
  ConversationMessage,
  ConversationMessagePage,
  ConversationSynchronizationState,
  ConversationsError,
  ConversationsInvalidation,
  ConversationsSnapshot,
  ConversationsV1Operation,
} from "@resonance/contracts";

import {
  conversationsErrorMessages,
  validateConversationsError,
  type ConversationChannel,
  type ConversationMessage,
  type ConversationMessagePage,
  type ConversationsError,
  type ConversationsInvalidation,
  type ConversationsSnapshot,
  type ConversationsV1Operation,
} from "@resonance/contracts";

export type ConversationsInvalidationListener = (
  invalidation: ConversationsInvalidation,
) => void;

export interface ConversationsV1 {
  snapshot(): Promise<ConversationsSnapshot>;
  messages(
    channelId: string,
    cursor?: string | null,
    limit?: number,
  ): Promise<ConversationMessagePage>;
  createChannel(name: string): Promise<ConversationChannel>;
  renameChannel(channelId: string, name: string): Promise<ConversationChannel>;
  archiveChannel(channelId: string): Promise<ConversationChannel>;
  postMessage(
    channelId: string,
    markdown: string,
  ): Promise<ConversationMessage>;
  markRead(channelId: string, messageId: string): Promise<number>;
  synchronizationState(): Promise<ConversationsSnapshot["synchronization"]>;
  subscribe(listener: ConversationsInvalidationListener): () => void;
}

export function conversationsError(
  code: ConversationsError["code"],
): ConversationsError {
  return {
    code,
    message: conversationsErrorMessages[code],
  } as ConversationsError;
}

export function isConversationsError(
  candidate: unknown,
): candidate is ConversationsError {
  return validateConversationsError(candidate).kind === "valid";
}

export type ConversationsFailureController = {
  failNext(
    operation: ConversationsV1Operation,
    error: ConversationsError,
  ): void;
};
