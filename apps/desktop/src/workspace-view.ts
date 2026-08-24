export type RootState =
  | "unbound"
  | "healthy"
  | "unavailable"
  | "unwritable"
  | "unhealthy";

export type FileEntryView = {
  nodeId: string;
  parentNodeId: string | null;
  name: string;
  kind: "directory" | "markdown" | "binary";
  currentRevisionId: string | null;
  editable: boolean;
};

export type ConflictChoiceView = {
  candidateId: string;
  nodeId: string;
  kind: "file" | "directory" | "move";
  selected: boolean;
  name: string;
  targetPath: string | null;
};

export type ConflictView = {
  recordId: string;
  nodeId: string;
  kind:
    | "markdown-overlap"
    | "binary-collision"
    | "delete-edit"
    | "concurrent-create"
    | "competing-move";
  competingRevisionIds: string[];
  resolutionCandidateIds: string[];
  reviewableRevisionIds: string[];
  deletionOperationId: string | null;
  treeChoices: ConflictChoiceView[];
};

export type FilePreviewView = {
  kind: "image" | "unavailable";
  mimeType: string;
  bytes: number[];
  byteLength: number;
};

export type WorkspaceFilesView = {
  root: { state: RootState };
  entries: FileEntryView[];
  conflicts: ConflictView[];
};

export type MarkdownRevisionView = {
  nodeId: string;
  revisionId: string;
  markdown: string;
};

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
  files: WorkspaceFilesView | null;
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

function isFileEntry(value: unknown): value is FileEntryView {
  if (!isRecord(value)) return false;
  return (
    isString(value.nodeId) &&
    isNullableString(value.parentNodeId) &&
    isString(value.name) &&
    ["directory", "markdown", "binary"].includes(value.kind as string) &&
    isNullableString(value.currentRevisionId) &&
    typeof value.editable === "boolean"
  );
}

function isConflictChoice(value: unknown): value is ConflictChoiceView {
  if (!isRecord(value)) return false;
  return (
    isString(value.candidateId) &&
    isString(value.nodeId) &&
    ["file", "directory", "move"].includes(value.kind as string) &&
    typeof value.selected === "boolean" &&
    isString(value.name) &&
    isNullableString(value.targetPath)
  );
}

function isConflict(value: unknown): value is ConflictView {
  if (!isRecord(value)) return false;
  return (
    isString(value.recordId) &&
    isString(value.nodeId) &&
    [
      "markdown-overlap",
      "binary-collision",
      "delete-edit",
      "concurrent-create",
      "competing-move",
    ].includes(value.kind as string) &&
    Array.isArray(value.competingRevisionIds) &&
    value.competingRevisionIds.every(isString) &&
    Array.isArray(value.resolutionCandidateIds) &&
    value.resolutionCandidateIds.every(isString) &&
    Array.isArray(value.reviewableRevisionIds) &&
    value.reviewableRevisionIds.every(isString) &&
    isNullableString(value.deletionOperationId) &&
    Array.isArray(value.treeChoices) &&
    value.treeChoices.every(isConflictChoice)
  );
}

function isFilesView(value: unknown): value is WorkspaceFilesView {
  if (!isRecord(value) || !isRecord(value.root)) return false;
  return (
    ["unbound", "healthy", "unavailable", "unwritable", "unhealthy"].includes(
      value.root.state as string,
    ) &&
    Array.isArray(value.entries) &&
    value.entries.every(isFileEntry) &&
    Array.isArray(value.conflicts) &&
    value.conflicts.every(isConflict)
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
    (value.files === null || isFilesView(value.files))
  );
}

export function isFilePreviewView(value: unknown): value is FilePreviewView {
  if (!isRecord(value)) return false;
  return (
    ["image", "unavailable"].includes(value.kind as string) &&
    isString(value.mimeType) &&
    Array.isArray(value.bytes) &&
    value.bytes.every(
      (byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255,
    ) &&
    typeof value.byteLength === "number" &&
    Number.isSafeInteger(value.byteLength) &&
    value.byteLength >= 0
  );
}

export function isMarkdownRevisionView(
  value: unknown,
): value is MarkdownRevisionView {
  return (
    isRecord(value) &&
    isString(value.nodeId) &&
    isString(value.revisionId) &&
    isString(value.markdown)
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
  if (!peer.online) {
    return "Offline";
  }
  return peer.connection === "unknown"
    ? "Online"
    : `Online, ${peer.connection}`;
}
