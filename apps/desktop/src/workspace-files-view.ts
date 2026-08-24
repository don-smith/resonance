import type {
  ConflictChoiceView,
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

export function treeConflictActionLabel(choice: ConflictChoiceView): string {
  if (choice.kind === "move") {
    const target = choice.targetPath ?? choice.name;
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
