import type {
  ConflictChoiceView,
  ConflictView,
  FileEntryView,
  RootState,
  WorkspaceFilesView,
} from "../workspace-files-types.js";
import {
  childEntries,
  conflictFallbackLabel,
  conflictFallbackSelection,
  conflictLabel,
  conflictRevisionActionLabel,
  treeConflictActionLabel,
  treeConflictPreviewLabel,
} from "../workspace-files-view.js";

export type WorkspaceFilesAction = {
  action: string;
  data?: Record<string, string>;
};

export function renderMarkup(root: HTMLElement): void {
  root.innerHTML = `
    <div class="workspace-files-content">
      <div class="workspace-files-heading">
        <div><h2>Files</h2><p class="workspace-files-root-status"></p></div>
        <div class="workspace-files-root-actions"></div>
      </div>
      <p class="workspace-files-message" role="status"></p>
      <div class="workspace-files-layout">
        <aside class="workspace-files-browser">
          <ul class="workspace-files-tree" aria-label="Workspace files"></ul>
          <form data-action="new-markdown" class="workspace-files-new-markdown">
            <label>Folder<select name="parentNodeId" required></select></label>
            <label>File name<input name="name" required /></label>
            <button type="submit">New Markdown file</button>
          </form>
          <section class="workspace-files-conflicts"><h3>Conflicts</h3><ul></ul></section>
        </aside>
        <section class="workspace-files-editor-panel">
          <p class="workspace-files-placeholder">Choose a workspace entry to inspect or edit.</p>
          <section class="workspace-files-editor-notice" role="status" aria-live="polite" hidden>
            <p></p><button type="button" data-action="review-latest" hidden>Review latest</button>
          </section>
          <section class="workspace-files-preview" hidden>
            <h3></h3><p></p><img hidden alt="" /><ul></ul>
          </section>
          <div class="workspace-files-markdown-editor" hidden></div>
          <div class="workspace-files-review-actions" hidden>
            <button type="button" data-action="return-to-draft">Return to draft</button>
            <button type="button" data-action="load-reviewed">Load latest and replace draft</button>
          </div>
          <button type="button" data-action="save-markdown" hidden>Save revision</button>
        </section>
      </div>
    </div>`;
}

export function required<T extends Element>(
  root: HTMLElement,
  selector: string,
): T {
  const element = root.querySelector<T>(selector);
  if (!element) throw new Error(`Missing workspace-files element: ${selector}`);
  return element;
}

export function renderMessage(root: HTMLElement, message: string | null): void {
  required<HTMLElement>(root, ".workspace-files-message").textContent =
    message ?? "";
}

export function renderRoot(
  root: HTMLElement,
  state: RootState,
  statusMessage: string,
): void {
  required<HTMLElement>(root, ".workspace-files-root-status").textContent =
    statusMessage;
  const actions = required<HTMLElement>(root, ".workspace-files-root-actions");
  actions.replaceChildren();
  const commands: readonly [string, string][] =
    state === "unbound"
      ? [["Choose folder", "select-root"]]
      : [
          ["Repair", "repair-root"],
          ["Replace", "replace-root"],
          ["Unbind", "unbind-root"],
        ];
  for (const [label, action] of commands) {
    actions.append(actionButton(root.ownerDocument, label, action));
  }
}

export function renderTree(
  root: HTMLElement,
  entries: readonly FileEntryView[],
): void {
  const tree = required<HTMLUListElement>(root, ".workspace-files-tree");
  tree.replaceChildren();
  appendTreeLevel(tree, entries, null);
  const select = required<HTMLSelectElement>(root, '[name="parentNodeId"]');
  select.replaceChildren();
  for (const entry of entries.filter(({ kind }) => kind === "directory")) {
    const option = root.ownerDocument.createElement("option");
    option.value = entry.nodeId;
    option.textContent = entry.name;
    select.append(option);
  }
}

function appendTreeLevel(
  parent: HTMLUListElement,
  entries: readonly FileEntryView[],
  parentNodeId: string | null,
): void {
  for (const entry of childEntries(entries, parentNodeId)) {
    const item = parent.ownerDocument.createElement("li");
    const label = actionButton(
      parent.ownerDocument,
      entry.name,
      entry.kind === "markdown" && entry.editable
        ? "open-markdown"
        : "preview-entry",
      { nodeId: entry.nodeId },
    );
    item.append(label);
    if (entry.kind === "directory") {
      const children = parent.ownerDocument.createElement("ul");
      appendTreeLevel(children, entries, entry.nodeId);
      item.append(children);
    }
    parent.append(item);
  }
}

export function renderConflicts(
  root: HTMLElement,
  snapshot: WorkspaceFilesView,
): void {
  const conflicts = required<HTMLUListElement>(
    root,
    ".workspace-files-conflicts ul",
  );
  conflicts.replaceChildren();
  if (snapshot.conflicts.length === 0) {
    const item = root.ownerDocument.createElement("li");
    item.textContent = "No unresolved conflicts.";
    conflicts.append(item);
  }
  for (const conflict of snapshot.conflicts) {
    conflicts.append(renderConflict(root.ownerDocument, conflict));
  }
}

function renderConflict(
  document: Document,
  conflict: ConflictView,
): HTMLLIElement {
  const item = document.createElement("li");
  const title = document.createElement("p");
  title.textContent = conflictLabel(conflict.kind);
  item.append(title);
  if (conflict.treeChoices.length > 0) {
    for (const choice of conflict.treeChoices) {
      item.append(
        actionButton(
          document,
          treeConflictPreviewLabel(choice),
          "preview-entry",
          { nodeId: choice.nodeId },
        ),
        actionButton(
          document,
          treeConflictActionLabel(choice),
          "resolve-conflict",
          { recordId: conflict.recordId, candidateId: choice.candidateId },
        ),
      );
    }
    return item;
  }
  for (const revisionId of conflict.resolutionCandidateIds) {
    if (conflict.reviewableRevisionIds.includes(revisionId)) {
      item.append(
        actionButton(
          document,
          `Review ${revisionId.slice(0, 8)}`,
          "review-conflict",
          { nodeId: conflict.nodeId, revisionId },
        ),
      );
    }
    item.append(
      actionButton(
        document,
        conflictRevisionActionLabel(conflict.kind, revisionId),
        "resolve-conflict",
        { recordId: conflict.recordId, candidateId: revisionId },
      ),
    );
  }
  const fallback = conflictFallbackSelection(conflict);
  const keep = actionButton(
    document,
    conflictFallbackLabel(conflict.kind),
    "resolve-conflict",
    { recordId: conflict.recordId, candidateId: fallback ?? "" },
  );
  keep.dataset.nullCandidate = fallback === null ? "true" : "false";
  keep.disabled = conflict.kind === "delete-edit" && fallback === null;
  item.append(keep);
  return item;
}

function actionButton(
  document: Document,
  label: string,
  action: string,
  data: Record<string, string> = {},
): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = label;
  button.dataset.action = action;
  for (const [key, value] of Object.entries(data)) button.dataset[key] = value;
  return button;
}

export function findEntry(
  snapshot: WorkspaceFilesView | null,
  nodeId: string | undefined,
): FileEntryView | undefined {
  return snapshot?.entries.find((entry) => entry.nodeId === nodeId);
}

export function selectedEntryExists(
  snapshot: WorkspaceFilesView,
  selected: FileEntryView | null,
): boolean {
  return (
    selected === null ||
    snapshot.entries.some(({ nodeId }) => nodeId === selected.nodeId)
  );
}

export function conflictChoiceNodeIds(choice: ConflictChoiceView): string {
  return choice.nodeId;
}
