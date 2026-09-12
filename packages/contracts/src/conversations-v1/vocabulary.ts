export const conversationsV1Operations = [
  "snapshot",
  "messages",
  "create-channel",
  "rename-channel",
  "archive-channel",
  "post-message",
  "mark-read",
  "synchronization-state",
] as const;

export type ConversationsV1Operation =
  (typeof conversationsV1Operations)[number];
