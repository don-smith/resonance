export type WorkspaceShellView = {
  state:
    | "onboarding"
    | "initializing"
    | "ready"
    | "joining"
    | "identity-error"
    | "storage-error";
  message: string | null;
  workspace: {
    id: string;
    displayName: string;
    lifecycle: "initializing" | "ready" | "joining";
  } | null;
  localPublicIdentity: string | null;
  members: Array<{
    publicIdentity: string;
    displayName: string;
    role: string;
  }>;
  peers: Array<{
    publicIdentity: string;
    displayName: string;
    online: boolean;
    connection: "direct" | "relayed" | "unknown";
  }>;
};

function isString(value: unknown): value is string {
  return typeof value === "string";
}

function isNullableString(value: unknown): value is string | null {
  return value === null || isString(value);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function hasPrivateContractKey(value: unknown): boolean {
  if (Array.isArray(value)) return value.some(hasPrivateContractKey);
  if (!isRecord(value)) return false;
  const forbidden = new Set([
    "path",
    "token",
    "privateKey",
    "blobLocation",
    "watcherState",
    "iroh",
  ]);
  return (
    Object.keys(value).some((key) => forbidden.has(key)) ||
    Object.values(value).some(hasPrivateContractKey)
  );
}

export function isWorkspaceShellView(
  value: unknown,
): value is WorkspaceShellView {
  if (!isRecord(value) || hasPrivateContractKey(value)) return false;
  return (
    [
      "onboarding",
      "initializing",
      "ready",
      "joining",
      "identity-error",
      "storage-error",
    ].includes(value.state as string) &&
    isNullableString(value.message) &&
    isNullableString(value.localPublicIdentity) &&
    Array.isArray(value.members) &&
    Array.isArray(value.peers) &&
    !("files" in value)
  );
}

export function workspaceViewChanged(
  current: WorkspaceShellView | null,
  incoming: WorkspaceShellView,
): boolean {
  return (
    current === null || JSON.stringify(current) !== JSON.stringify(incoming)
  );
}

export function peerStatus(peer: WorkspaceShellView["peers"][number]): string {
  if (!peer.online) return "Offline";
  return peer.connection === "unknown"
    ? "Online"
    : `Online, ${peer.connection}`;
}
