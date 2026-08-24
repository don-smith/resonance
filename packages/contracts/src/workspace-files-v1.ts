import Ajv2020, { type ErrorObject } from "ajv/dist/2020.js";

import schema from "../schema/workspace-files.v1.json";

export const workspaceFilesV1Operations = [
  "snapshot",
  "select-root",
  "replace-root",
  "repair-root",
  "unbind-root",
  "open-markdown",
  "open-preview",
  "create-markdown",
  "replace-markdown",
  "resolve-conflict",
] as const;

export type WorkspaceFilesV1Operation =
  (typeof workspaceFilesV1Operations)[number];
export type WorkspaceFilesRootState =
  | "unbound"
  | "healthy"
  | "unavailable"
  | "unwritable"
  | "unhealthy";
export type WorkspaceFilesEntry = Readonly<{
  nodeId: string;
  parentNodeId: string | null;
  name: string;
  kind: "directory" | "markdown" | "binary";
  currentRevisionId: string | null;
  editable: boolean;
}>;
export type WorkspaceFilesConflictChoice = Readonly<{
  candidateId: string;
  nodeId: string;
  kind: "file" | "directory" | "move";
  selected: boolean;
  name: string;
  targetLocation: string | null;
}>;
export type WorkspaceFilesConflict = Readonly<{
  recordId: string;
  nodeId: string;
  kind:
    | "markdown-overlap"
    | "binary-collision"
    | "delete-edit"
    | "concurrent-create"
    | "competing-move";
  competingRevisionIds: readonly string[];
  resolutionCandidateIds: readonly string[];
  reviewableRevisionIds: readonly string[];
  deletionOperationId: string | null;
  treeChoices: readonly WorkspaceFilesConflictChoice[];
}>;
export type WorkspaceFilesSnapshot = Readonly<{
  root: Readonly<{ state: WorkspaceFilesRootState }>;
  entries: readonly WorkspaceFilesEntry[];
  conflicts: readonly WorkspaceFilesConflict[];
}>;
export type WorkspaceFilesMarkdownRevision = Readonly<{
  nodeId: string;
  revisionId: string;
  markdown: string;
}>;
export type WorkspaceFilesPreview =
  | Readonly<{
      kind: "image";
      mimeType:
        | "image/png"
        | "image/jpeg"
        | "image/gif"
        | "image/webp"
        | "image/bmp";
      bytes: readonly number[];
      byteLength: number;
    }>
  | Readonly<{
      kind: "unavailable";
      mimeType: string;
      bytes: readonly [];
      byteLength: number;
    }>;

export type WorkspaceFilesRequest =
  | Readonly<{ operation: "snapshot" }>
  | Readonly<{ operation: "select-root" }>
  | Readonly<{ operation: "replace-root" }>
  | Readonly<{ operation: "repair-root" }>
  | Readonly<{ operation: "unbind-root" }>
  | Readonly<{
      operation: "open-markdown";
      nodeId: string;
      revisionId: string;
    }>
  | Readonly<{
      operation: "open-preview";
      nodeId: string;
      revisionId: string;
    }>
  | Readonly<{
      operation: "create-markdown";
      parentNodeId: string;
      name: string;
      markdown: string;
    }>
  | Readonly<{
      operation: "replace-markdown";
      nodeId: string;
      baseRevisionId: string;
      markdown: string;
    }>
  | Readonly<{
      operation: "resolve-conflict";
      recordId: string;
      chosenCandidateId: string | null;
    }>;

type SnapshotOperation =
  | "snapshot"
  | "select-root"
  | "replace-root"
  | "repair-root"
  | "unbind-root"
  | "resolve-conflict";
type RevisionOperation =
  | "open-markdown"
  | "create-markdown"
  | "replace-markdown";
export type WorkspaceFilesResponse =
  | Readonly<{
      operation: SnapshotOperation;
      snapshot: WorkspaceFilesSnapshot;
    }>
  | Readonly<{
      operation: RevisionOperation;
      revision: WorkspaceFilesMarkdownRevision;
    }>
  | Readonly<{
      operation: "open-preview";
      preview: WorkspaceFilesPreview;
    }>;

export const workspaceFilesErrorMessages = Object.freeze({
  "unavailable-capability": "Workspace files are unavailable.",
  "invalid-request": "The workspace-files request is invalid.",
  "missing-revision": "That file revision is unavailable.",
  "stale-revision": "The file changed before this edit could be saved.",
  "invalid-markdown-name": "Markdown file names must end in .md.",
  "size-limit": "The workspace-files size limit was exceeded.",
  "unusable-root": "Choose a usable workspace folder.",
  "changed-conflict-choice": "That conflict choice is no longer available.",
  internal: "Workspace files could not complete the request.",
});
export type WorkspaceFilesErrorCode = keyof typeof workspaceFilesErrorMessages;
export type WorkspaceFilesError = Readonly<
  {
    [Code in WorkspaceFilesErrorCode]: {
      code: Code;
      message: (typeof workspaceFilesErrorMessages)[Code];
    };
  }[WorkspaceFilesErrorCode]
>;

export type WorkspaceFilesEnvelope =
  | Readonly<{ kind: "request"; value: WorkspaceFilesRequest }>
  | Readonly<{ kind: "response"; value: WorkspaceFilesResponse }>
  | Readonly<{ kind: "error"; value: WorkspaceFilesError }>;

export type WorkspaceFilesDiagnostic = Readonly<{
  path: string;
  message: string;
}>;

const validator = new Ajv2020({ allErrors: true, strict: true }).compile(
  schema,
);
const utf8 = new TextEncoder();

function diagnostics(
  errors: ErrorObject[] | null | undefined,
): WorkspaceFilesDiagnostic[] {
  return (errors ?? [])
    .map((error) => ({
      path: error.instancePath || "/",
      message: error.message ?? "is invalid",
    }))
    .sort((left, right) =>
      `${left.path}:${left.message}`.localeCompare(
        `${right.path}:${right.message}`,
      ),
    );
}

function markdownByteDiagnostic(
  envelope: WorkspaceFilesEnvelope,
): WorkspaceFilesDiagnostic[] {
  let markdown: string | undefined;
  if (envelope.kind === "request") {
    if (
      envelope.value.operation === "create-markdown" ||
      envelope.value.operation === "replace-markdown"
    ) {
      markdown = envelope.value.markdown;
    }
  } else if (envelope.kind === "response" && "revision" in envelope.value) {
    markdown = envelope.value.revision.markdown;
  }
  return markdown !== undefined && utf8.encode(markdown).byteLength > 1_048_576
    ? [{ path: "/value/markdown", message: "exceeds 1048576 UTF-8 bytes" }]
    : [];
}

export function validateWorkspaceFilesEnvelope(
  candidate: unknown,
):
  | { value: WorkspaceFilesEnvelope; diagnostics: [] }
  | { diagnostics: WorkspaceFilesDiagnostic[] } {
  if (!validator(candidate)) {
    return { diagnostics: diagnostics(validator.errors) };
  }
  const value = candidate as WorkspaceFilesEnvelope;
  const byteDiagnostics = markdownByteDiagnostic(value);
  if (byteDiagnostics.length > 0) return { diagnostics: byteDiagnostics };
  return { value, diagnostics: [] };
}

export function validateWorkspaceFilesRequest(
  candidate: unknown,
):
  | { value: WorkspaceFilesRequest; diagnostics: [] }
  | { diagnostics: WorkspaceFilesDiagnostic[] } {
  const result = validateWorkspaceFilesEnvelope({
    kind: "request",
    value: candidate,
  });
  return "value" in result
    ? { value: result.value.value as WorkspaceFilesRequest, diagnostics: [] }
    : result;
}

export function validateWorkspaceFilesResponse(
  candidate: unknown,
):
  | { value: WorkspaceFilesResponse; diagnostics: [] }
  | { diagnostics: WorkspaceFilesDiagnostic[] } {
  const result = validateWorkspaceFilesEnvelope({
    kind: "response",
    value: candidate,
  });
  return "value" in result
    ? { value: result.value.value as WorkspaceFilesResponse, diagnostics: [] }
    : result;
}

export function validateWorkspaceFilesError(
  candidate: unknown,
):
  | { value: WorkspaceFilesError; diagnostics: [] }
  | { diagnostics: WorkspaceFilesDiagnostic[] } {
  const result = validateWorkspaceFilesEnvelope({
    kind: "error",
    value: candidate,
  });
  return "value" in result
    ? { value: result.value.value as WorkspaceFilesError, diagnostics: [] }
    : result;
}
