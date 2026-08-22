export type WorkspaceShellView = {
  state:
    | "onboarding"
    | "ready"
    | "joining"
    | "identity-error"
    | "storage-error";
  message: string | null;
  workspace: {
    id: string;
    displayName: string;
    lifecycle: "ready" | "joining";
  } | null;
  localPublicIdentity: string | null;
  members: Array<{
    publicIdentity: string;
    displayName: string;
    role: string;
  }>;
  peers: Array<{
    publicIdentity: string;
    online: boolean;
    connection: "direct" | "relayed" | "unknown";
  }>;
};

function isString(value: unknown): value is string {
  return typeof value === "string";
}

export function isWorkspaceShellView(
  value: unknown,
): value is WorkspaceShellView {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const candidate = value as Record<string, unknown>;
  return (
    [
      "onboarding",
      "ready",
      "joining",
      "identity-error",
      "storage-error",
    ].includes(candidate.state as string) &&
    (candidate.message === null || isString(candidate.message)) &&
    (candidate.localPublicIdentity === null ||
      isString(candidate.localPublicIdentity)) &&
    Array.isArray(candidate.members) &&
    Array.isArray(candidate.peers)
  );
}

export function peerStatus(peer: WorkspaceShellView["peers"][number]): string {
  if (!peer.online) {
    return "Offline";
  }
  return peer.connection === "unknown"
    ? "Online"
    : `Online, ${peer.connection}`;
}
