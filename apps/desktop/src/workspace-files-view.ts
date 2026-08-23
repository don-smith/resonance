import type {
  ConflictView,
  FileEntryView,
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
