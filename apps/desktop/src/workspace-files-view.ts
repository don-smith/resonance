import type {
  ConflictView,
  FileEntryView,
  MarkdownRevisionView,
  RootState,
} from "./workspace-view.js";

export function childEntries(
  entries: FileEntryView[],
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

export function retainedOpenRevision(
  revision: MarkdownRevisionView | null,
  entries: FileEntryView[],
): MarkdownRevisionView | null {
  if (!revision) return null;
  return entries.some(
    (entry) => entry.nodeId === revision.nodeId && entry.kind === "markdown",
  )
    ? revision
    : null;
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

export function conflictFallbackLabel(kind: ConflictView["kind"]): string {
  return kind === "delete-edit" ? "Keep deletion" : "Keep current state";
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
  return kind === "delete-edit"
    ? `Keep edited file ${shortId}`
    : `Use ${shortId}`;
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
