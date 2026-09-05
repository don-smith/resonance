import type { ConversationsError } from "./errors.ts";

export type ConversationSynchronizationState =
  | "offline"
  | "waiting-to-sync"
  | "current";

export type ConversationChannel = Readonly<{
  channelId: string;
  name: string;
  archived: boolean;
  unreadCount: number;
  canManage: boolean;
}>;

export type ConversationAuthor = Readonly<{
  publicIdentity: string;
  displayName: string;
}>;

export type ConversationMessage = Readonly<{
  messageId: string;
  channelId: string;
  author: ConversationAuthor;
  createdAt: number;
  markdown: string;
}>;

export type ConversationsSnapshot = Readonly<{
  workspaceId: string;
  channels: readonly ConversationChannel[];
  synchronization: ConversationSynchronizationState;
}>;

export type ConversationMessagePage = Readonly<{
  channelId: string;
  messages: readonly ConversationMessage[];
  nextCursor: string | null;
}>;

export type ConversationsInvalidation = Readonly<{
  workspaceId: string;
  channelId: string;
  messageId?: string;
}>;

export type ConversationsRequest =
  | Readonly<{ operation: "snapshot" }>
  | Readonly<{
      operation: "messages";
      channelId: string;
      cursor: string | null;
      limit: number;
    }>
  | Readonly<{ operation: "create-channel"; name: string }>
  | Readonly<{
      operation: "rename-channel";
      channelId: string;
      name: string;
    }>
  | Readonly<{ operation: "archive-channel"; channelId: string }>
  | Readonly<{
      operation: "post-message";
      channelId: string;
      markdown: string;
    }>
  | Readonly<{
      operation: "mark-read";
      channelId: string;
      messageId: string;
    }>
  | Readonly<{ operation: "synchronization-state" }>;

export type ConversationsResponse =
  | Readonly<{ operation: "snapshot"; snapshot: ConversationsSnapshot }>
  | Readonly<{ operation: "messages"; page: ConversationMessagePage }>
  | Readonly<{
      operation: "create-channel" | "rename-channel" | "archive-channel";
      channel: ConversationChannel;
    }>
  | Readonly<{ operation: "post-message"; message: ConversationMessage }>
  | Readonly<{
      operation: "mark-read";
      channelId: string;
      unreadCount: number;
    }>
  | Readonly<{
      operation: "synchronization-state";
      synchronization: ConversationSynchronizationState;
    }>;

export type ConversationsEnvelope =
  | Readonly<{ kind: "request"; value: ConversationsRequest }>
  | Readonly<{ kind: "response"; value: ConversationsResponse }>
  | Readonly<{ kind: "error"; value: ConversationsError }>
  | Readonly<{ kind: "invalidation"; value: ConversationsInvalidation }>;
