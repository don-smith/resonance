import type {
  ConflictChoiceView,
  ConflictView,
  FileEntryView,
  MarkdownRevisionView,
  RootState,
  WorkspaceFilesView,
} from "./workspace-files-types.js";

export type MarkdownEditorSession = {
  loadedRevision: MarkdownRevisionView;
  draft: string;
  readOnly: boolean;
  mode: "draft" | "review";
  reviewedRevision: MarkdownRevisionView | null;
};

export type MarkdownRevisionAwareness =
  | { state: "current" }
  | { state: "stale"; currentRevisionId: string }
  | { state: "deleted" }
  | { state: "conflicted"; conflictKind: ConflictView["kind"] };

export function childEntries(
  entries: readonly FileEntryView[],
  parentNodeId: string | null,
): FileEntryView[] {
  return entries
    .filter((entry) => entry.parentNodeId === parentNodeId)
    .sort((left, right) => {
      if (left.kind === "directory" && right.kind !== "directory") return -1;
      if (left.kind !== "directory" && right.kind === "directory") return 1;
      return left.name.localeCompare(right.name);
    });
}

export function createMarkdownEditorSession(
  revision: MarkdownRevisionView,
  readOnly: boolean,
): MarkdownEditorSession {
  return {
    loadedRevision: revision,
    draft: revision.markdown,
    readOnly,
    mode: "draft",
    reviewedRevision: null,
  };
}

export function captureMarkdownDraft(
  session: MarkdownEditorSession,
  draft: string,
): MarkdownEditorSession {
  return { ...session, draft };
}

export function markdownRevisionAwareness(
  session: MarkdownEditorSession,
  files: WorkspaceFilesView,
): MarkdownRevisionAwareness {
  const conflict = files.conflicts.find(
    (candidate) => candidate.nodeId === session.loadedRevision.nodeId,
  );
  if (conflict) {
    return { state: "conflicted", conflictKind: conflict.kind };
  }
  const entry = files.entries.find(
    (candidate) => candidate.nodeId === session.loadedRevision.nodeId,
  );
  if (!entry || entry.kind !== "markdown") return { state: "deleted" };
  if (
    entry.currentRevisionId &&
    entry.currentRevisionId !== session.loadedRevision.revisionId
  ) {
    return {
      state: "stale",
      currentRevisionId: entry.currentRevisionId,
    };
  }
  return { state: "current" };
}

export function reviewMarkdownRevision(
  session: MarkdownEditorSession,
  revision: MarkdownRevisionView,
): MarkdownEditorSession {
  if (revision.nodeId !== session.loadedRevision.nodeId) return session;
  return { ...session, mode: "review", reviewedRevision: revision };
}

export function returnToMarkdownDraft(
  session: MarkdownEditorSession,
): MarkdownEditorSession {
  return { ...session, mode: "draft" };
}

export function loadReviewedMarkdownRevision(
  session: MarkdownEditorSession,
): MarkdownEditorSession {
  const revision = session.reviewedRevision;
  if (!revision) return session;
  return createMarkdownEditorSession(revision, session.readOnly);
}

export function rootStatusMessage(state: RootState): string {
  switch (state) {
    case "unbound":
      return "Choose a new or empty folder to materialize workspace files.";
    case "healthy":
      return "The private workspace folder is available.";
    case "unavailable":
      return "The private workspace folder is unavailable.";
    case "unwritable":
      return "The private workspace folder is not writable.";
    case "unhealthy":
      return "The private workspace folder needs repair.";
  }
}

export function treeConflictActionLabel(choice: ConflictChoiceView): string {
  if (choice.kind === "move") {
    const target = choice.targetLocation ?? choice.name;
    return choice.selected
      ? `Keep current location: ${target}`
      : `Use location: ${target}`;
  }
  const kind = choice.kind === "directory" ? "folder" : "file";
  return choice.selected
    ? `Keep current ${kind}: ${choice.name}`
    : `Use competing ${kind}: ${choice.name}`;
}

export function treeConflictPreviewLabel(choice: ConflictChoiceView): string {
  if (choice.kind === "move") return "Preview moved file";
  return choice.kind === "directory" ? "Preview folder" : "Preview file";
}

export function conflictFallbackLabel(kind: ConflictView["kind"]): string {
  switch (kind) {
    case "delete-edit":
      return "Keep deletion";
    case "concurrent-create":
      return "Keep current entry";
    case "competing-move":
      return "Keep current location";
    default:
      return "Keep current state";
  }
}

export function conflictFallbackSelection(
  conflict: ConflictView,
): string | null {
  return conflict.kind === "delete-edit" ? conflict.deletionOperationId : null;
}

export function conflictRevisionActionLabel(
  kind: ConflictView["kind"],
  revisionId: string,
): string {
  const shortId = revisionId.slice(0, 8);
  switch (kind) {
    case "delete-edit":
      return `Keep edited file ${shortId}`;
    case "concurrent-create":
      return `Use competing entry ${shortId}`;
    case "competing-move":
      return `Use move ${shortId}`;
    default:
      return `Use ${shortId}`;
  }
}

export function conflictLabel(kind: ConflictView["kind"]): string {
  switch (kind) {
    case "markdown-overlap":
      return "Overlapping Markdown edits";
    case "binary-collision":
      return "Competing binary revisions";
    case "delete-edit":
      return "Delete and edit conflict";
    case "concurrent-create":
      return "Competing file creation";
    case "competing-move":
      return "Competing file moves";
  }
}
