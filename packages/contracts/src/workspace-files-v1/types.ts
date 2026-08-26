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

export type WorkspaceFilesEnvelope =
  | Readonly<{ kind: "request"; value: WorkspaceFilesRequest }>
  | Readonly<{ kind: "response"; value: WorkspaceFilesResponse }>
  | Readonly<{
      kind: "error";
      value: import("./errors.js").WorkspaceFilesError;
    }>;
