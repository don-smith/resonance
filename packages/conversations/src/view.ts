import type { ConversationSynchronizationState } from "@resonance/package-sdk";

export function synchronizationLabel(
  state: ConversationSynchronizationState,
): string {
  switch (state) {
    case "offline":
      return "Offline";
    case "waiting-to-sync":
      return "Waiting to sync";
    case "current":
      return "Current";
  }
}

export function unreadLabel(count: number): string {
  return count === 0 ? "" : count === 1 ? "1 unread" : `${count} unread`;
}
