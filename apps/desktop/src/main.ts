import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import Editor from "@toast-ui/editor";

import "@toast-ui/editor/dist/toastui-editor.css";
import "@toast-ui/editor/dist/theme/toastui-editor-dark.css";
import "./styles.css";
import {
  isFilePreviewView,
  isMarkdownRevisionView,
  isWorkspaceShellView,
  peerStatus,
  type FileEntryView,
  type FilePreviewView,
  type MarkdownRevisionView,
  type RootState,
  type WorkspaceFilesView,
  type WorkspaceShellView,
  workspaceViewChanged,
} from "./workspace-view.js";
import {
  childEntries,
  conflictFallbackLabel,
  conflictFallbackSelection,
  conflictLabel,
  conflictRevisionActionLabel,
  createMarkdownEditorSession,
  loadReviewedMarkdownRevision,
  markdownRevisionAwareness,
  returnToMarkdownDraft,
  reviewMarkdownRevision,
  rootStatusMessage,
  treeConflictActionLabel,
  treeConflictPreviewLabel,
  type MarkdownEditorSession,
} from "./workspace-files-view.js";
import {
  captureMountedMarkdownDraft,
  editableMarkdownMount,
  viewerMarkdownMount,
  type MountedMarkdown,
} from "./markdown-editor-mount.js";
import { createTemporaryMessage } from "./temporary-message.js";

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) throw new Error("Resonance shell mount point is missing.");
const shell = app;
let actionMessage: string | null = null;
let currentView: WorkspaceShellView | null = null;
let markdownSession: MarkdownEditorSession | null = null;
let mountedMarkdown: MountedMarkdown | null = null;
let previewedEntry: FileEntryView | null = null;
let filePreview: FilePreviewView | null = null;
let filePreviewUrl: string | null = null;
const temporaryMessage = createTemporaryMessage((message) => {
  actionMessage = message;
  if (currentView) render(currentView);
});

function field(label: string, name: string, type = "text"): string {
  return `<label>${label}<input name="${name}" type="${type}" required /></label>`;
}

function render(view: WorkspaceShellView): void {
  captureOpenMarkdownDraft();
  currentView = view;
  mountedMarkdown?.instance.destroy();
  mountedMarkdown = null;
  if (filePreviewUrl) URL.revokeObjectURL(filePreviewUrl);
  filePreviewUrl = null;
  const onboarding = view.state === "onboarding";
  const blocked =
    view.state === "identity-error" || view.state === "storage-error";
  shell.innerHTML = `
    <main class="shell" aria-labelledby="app-title">
      <aside class="navigation" aria-label="Runtime navigation">
        <p class="brand">Resonance</p>
        <nav><a aria-current="page" href="#workspace">Workspace</a></nav>
      </aside>
      <section class="workspace" id="workspace">
        <p class="eyebrow">${onboarding ? "Get started" : "Workspace"}</p>
        <h1 id="app-title"></h1>
        <p class="message" role="status"></p>
        <section class="onboarding" ${onboarding ? "" : "hidden"}>
          <h2>Create a workspace</h2>
          <form data-action="create">
            ${field("Workspace name", "displayName")}
            ${field("Your name", "creatorDisplayName")}
            <label>Relay override, optional<input name="relayOverride" type="url" /></label>
            <button type="submit">Create workspace</button>
          </form>
          <h2>Join with an invite</h2>
          <form data-action="join">
            ${field("Your name", "displayName")}
            ${field("Invite", "invite")}
            <button type="submit">Join workspace</button>
          </form>
        </section>
        <section class="active-workspace" ${onboarding || blocked ? "hidden" : ""}>
          <div class="workspace-actions">
            <button type="button" data-action="invite">Copy invite</button>
            <form class="retry-join" data-action="retry" ${view.state === "joining" ? "" : "hidden"}>
              ${field("Your name", "displayName")}
              <button type="submit">Retry join</button>
            </form>
          </div>
          <section class="files-panel" ${view.files ? "" : "hidden"}>
            <div class="section-heading">
              <div><h2>Files</h2><p class="root-status"></p></div>
              <div class="root-actions"></div>
            </div>
            <div class="document-layout">
              <aside class="file-browser">
                <ul class="file-tree" aria-label="Workspace files"></ul>
                <form data-action="new-markdown" class="new-markdown">
                  <label>Folder<select name="parentNodeId" required></select></label>
                  ${field("File name", "name")}
                  <button type="submit">New Markdown file</button>
                </form>
                <section class="conflicts"><h3>Conflicts</h3><ul></ul></section>
              </aside>
              <section class="editor-panel">
                <p class="editor-placeholder">Choose a workspace entry to inspect or edit.</p>
                <section class="editor-notice" role="status" aria-live="polite" hidden>
                  <p></p>
                  <button type="button" data-action="review-latest" hidden>Review latest</button>
                </section>
                <section class="file-preview" hidden>
                  <h3></h3><p></p><img hidden alt="" /><ul></ul>
                </section>
                <div id="markdown-editor" hidden></div>
                <div class="markdown-review-actions" hidden>
                  <button type="button" data-action="return-to-draft">Return to draft</button>
                  <button type="button" data-action="load-reviewed">Load latest and replace draft</button>
                </div>
                <button type="button" data-action="save-markdown" hidden>Save revision</button>
              </section>
            </div>
          </section>
          <details class="people"><summary>Members and peers</summary>
            <h2>Members</h2><ul class="members"></ul>
            <h2>Peers</h2><ul class="peers"></ul>
          </details>
        </section>
      </section>
    </main>`;

  requiredElement<HTMLHeadingElement>("#app-title").textContent = onboarding
    ? "Create a workspace or join one with an invite."
    : (view.workspace?.displayName ?? "Workspace unavailable");
  requiredElement<HTMLParagraphElement>(".message").textContent =
    actionMessage ?? view.message ?? "";
  renderPeople(view);
  if (view.files) renderFiles(view);

  for (const form of document.querySelectorAll<HTMLFormElement>(
    "form[data-action]",
  )) {
    form.addEventListener("submit", submitForm);
  }
  document
    .querySelector<HTMLButtonElement>('[data-action="invite"]')
    ?.addEventListener("click", copyInvite);
  document
    .querySelector<HTMLButtonElement>('[data-action="save-markdown"]')
    ?.addEventListener("click", saveMarkdown);
}

function renderPeople(view: WorkspaceShellView): void {
  const members = requiredElement<HTMLUListElement>(".members");
  for (const member of view.members) {
    const item = document.createElement("li");
    item.textContent = `${member.displayName} · ${member.role}`;
    members.append(item);
  }
  const peers = requiredElement<HTMLUListElement>(".peers");
  for (const peer of view.peers) {
    const item = document.createElement("li");
    item.textContent = `${peer.displayName} · ${peerStatus(peer)}`;
    peers.append(item);
  }
}

function renderFiles(view: WorkspaceShellView): void {
  const files = view.files;
  if (!files) return;
  requiredElement<HTMLParagraphElement>(".root-status").textContent =
    rootStatusMessage(files.root.state);
  renderRootActions(files.root.state);
  const tree = requiredElement<HTMLUListElement>(".file-tree");
  appendTreeLevel(tree, files.entries, null);

  const folderSelect = requiredElement<HTMLSelectElement>(
    '[name="parentNodeId"]',
  );
  for (const entry of files.entries.filter(
    (entry) => entry.kind === "directory",
  )) {
    const option = document.createElement("option");
    option.value = entry.nodeId;
    option.textContent = entry.name;
    folderSelect.append(option);
  }

  const conflicts = requiredElement<HTMLUListElement>(".conflicts ul");
  if (files.conflicts.length === 0) {
    const item = document.createElement("li");
    item.textContent = "No unresolved conflicts.";
    conflicts.append(item);
  }
  for (const conflict of files.conflicts) {
    const item = document.createElement("li");
    const title = document.createElement("p");
    title.textContent = conflictLabel(conflict.kind);
    item.append(title);
    if (conflict.treeChoices.length > 0) {
      for (const choice of conflict.treeChoices) {
        const preview = document.createElement("button");
        preview.type = "button";
        preview.textContent = treeConflictPreviewLabel(choice);
        preview.addEventListener("click", () => previewEntry(choice.nodeId));
        item.append(preview);
        const choose = document.createElement("button");
        choose.type = "button";
        choose.textContent = treeConflictActionLabel(choice);
        choose.addEventListener("click", () =>
          resolveConflict(conflict.recordId, choice.candidateId),
        );
        item.append(choose);
      }
      conflicts.append(item);
      continue;
    }
    for (const revisionId of conflict.resolutionCandidateIds) {
      if (conflict.reviewableRevisionIds.includes(revisionId)) {
        const review = document.createElement("button");
        review.type = "button";
        review.textContent = `Review ${revisionId.slice(0, 8)}`;
        review.addEventListener("click", () =>
          openMarkdown(conflict.nodeId, revisionId, true),
        );
        item.append(review);
      }
      const use = document.createElement("button");
      use.type = "button";
      use.textContent = conflictRevisionActionLabel(conflict.kind, revisionId);
      use.addEventListener("click", () =>
        resolveConflict(conflict.recordId, revisionId),
      );
      item.append(use);
    }
    const keep = document.createElement("button");
    keep.type = "button";
    keep.textContent = conflictFallbackLabel(conflict.kind);
    const fallbackSelection = conflictFallbackSelection(conflict);
    keep.disabled = conflict.kind === "delete-edit" && !fallbackSelection;
    keep.addEventListener("click", () =>
      resolveConflict(conflict.recordId, fallbackSelection),
    );
    item.append(keep);
    conflicts.append(item);
  }

  if (
    previewedEntry &&
    !files.entries.some((entry) => entry.nodeId === previewedEntry!.nodeId)
  ) {
    previewedEntry = null;
    filePreview = null;
  }
  if (previewedEntry) mountFilePreview(previewedEntry, files.entries);

  if (markdownSession) mountMarkdownSession(markdownSession, files);
}

function renderRootActions(state: RootState): void {
  const actions = requiredElement<HTMLDivElement>(".root-actions");
  const commands =
    state === "unbound"
      ? [["Choose folder", "choose_workspace_root"]]
      : [
          ["Repair", "repair_workspace_root"],
          ["Replace", "replace_workspace_root"],
          ["Unbind", "unbind_workspace_root"],
        ];
  for (const [label, command] of commands) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = label;
    button.addEventListener("click", () => invokeWorkspaceCommand(command));
    actions.append(button);
  }
}

function appendTreeLevel(
  parent: HTMLUListElement,
  entries: FileEntryView[],
  parentNodeId: string | null,
): void {
  for (const entry of childEntries(entries, parentNodeId)) {
    const item = document.createElement("li");
    const label = document.createElement("button");
    label.type = "button";
    label.textContent = entry.name;
    if (
      entry.kind === "markdown" &&
      entry.editable &&
      entry.currentRevisionId
    ) {
      label.addEventListener("click", () =>
        openMarkdown(entry.nodeId, entry.currentRevisionId!, false),
      );
    } else {
      label.addEventListener("click", () => previewEntry(entry.nodeId));
    }
    item.append(label);
    if (entry.kind === "directory") {
      const children = document.createElement("ul");
      appendTreeLevel(children, entries, entry.nodeId);
      item.append(children);
    }
    parent.append(item);
  }
}

function mountFilePreview(
  entry: FileEntryView,
  entries: FileEntryView[],
): void {
  const host = requiredElement<HTMLElement>(".file-preview");
  host.hidden = false;
  requiredElement<HTMLParagraphElement>(".editor-placeholder").hidden = true;
  requiredElement<HTMLHeadingElement>(".file-preview h3").textContent =
    entry.name;
  const message = requiredElement<HTMLParagraphElement>(".file-preview p");
  const image = requiredElement<HTMLImageElement>(".file-preview img");
  const children = requiredElement<HTMLUListElement>(".file-preview ul");
  image.hidden = true;
  children.hidden = true;
  if (entry.kind === "directory") {
    const directChildren = childEntries(entries, entry.nodeId);
    message.textContent =
      directChildren.length === 0
        ? "Empty folder."
        : `${directChildren.length} workspace entries.`;
    children.hidden = false;
    for (const child of directChildren) {
      const item = document.createElement("li");
      item.textContent = `${child.kind === "directory" ? "Folder" : "File"}: ${child.name}`;
      children.append(item);
    }
    return;
  }
  if (filePreview?.kind === "image") {
    const blob = new Blob([new Uint8Array(filePreview.bytes)], {
      type: filePreview.mimeType,
    });
    filePreviewUrl = URL.createObjectURL(blob);
    image.src = filePreviewUrl;
    image.alt = `Preview of ${entry.name}`;
    image.hidden = false;
    message.textContent = `${filePreview.mimeType} · ${filePreview.byteLength} bytes`;
    return;
  }
  message.textContent = filePreview
    ? `Preview unavailable for ${filePreview.mimeType} (${filePreview.byteLength} bytes). Review this file outside Resonance.`
    : "Preview unavailable. Review this file outside Resonance.";
}

function captureOpenMarkdownDraft(): void {
  markdownSession = captureMountedMarkdownDraft(
    markdownSession,
    mountedMarkdown,
  );
}

function mountMarkdownSession(
  session: MarkdownEditorSession,
  files: WorkspaceFilesView,
): void {
  const host = requiredElement<HTMLDivElement>("#markdown-editor");
  host.hidden = false;
  requiredElement<HTMLParagraphElement>(".editor-placeholder").hidden = true;
  renderMarkdownNotice(session, files);

  const save = requiredElement<HTMLButtonElement>(
    '[data-action="save-markdown"]',
  );
  const reviewActions = requiredElement<HTMLDivElement>(
    ".markdown-review-actions",
  );
  const reviewed = session.reviewedRevision;
  if (session.mode === "review" && reviewed) {
    reviewActions.hidden = false;
    requiredElement<HTMLButtonElement>(
      '[data-action="return-to-draft"]',
    ).addEventListener("click", returnToDraft);
    requiredElement<HTMLButtonElement>(
      '[data-action="load-reviewed"]',
    ).addEventListener("click", loadReviewedRevision);
    mountedMarkdown = viewerMarkdownMount(
      Editor.factory({
        el: host,
        viewer: true,
        theme: "dark",
        initialValue: reviewed.markdown,
        usageStatistics: false,
      }),
    );
    return;
  }

  save.hidden = session.readOnly;
  if (session.readOnly) {
    mountedMarkdown = viewerMarkdownMount(
      Editor.factory({
        el: host,
        viewer: true,
        theme: "dark",
        initialValue: session.draft,
        usageStatistics: false,
      }),
    );
    return;
  }
  mountedMarkdown = editableMarkdownMount(
    session.loadedRevision,
    new Editor({
      el: host,
      height: "32rem",
      initialEditType: "wysiwyg",
      initialValue: session.draft,
      hideModeSwitch: true,
      usageStatistics: false,
      toolbarItems: [
        ["heading", "bold", "italic", "strike"],
        ["ul", "ol", "task"],
        ["link", "quote", "code", "codeblock"],
      ],
    }),
  );
}

function renderMarkdownNotice(
  session: MarkdownEditorSession,
  files: WorkspaceFilesView,
): void {
  if (session.readOnly) return;
  const notice = requiredElement<HTMLElement>(".editor-notice");
  const message = requiredElement<HTMLParagraphElement>(".editor-notice p");
  const review = requiredElement<HTMLButtonElement>(
    '[data-action="review-latest"]',
  );
  const awareness = markdownRevisionAwareness(session, files);
  if (awareness.state === "current") return;
  notice.hidden = false;
  if (awareness.state === "deleted") {
    message.textContent =
      "This file was deleted from the workspace. Your draft remains open.";
    return;
  }
  if (awareness.state === "conflicted") {
    message.textContent = `${conflictLabel(awareness.conflictKind)} now affects this file. Your draft remains open.`;
    return;
  }
  if (
    session.mode === "review" &&
    session.reviewedRevision?.revisionId === awareness.currentRevisionId
  ) {
    message.textContent =
      "You are reviewing the latest workspace revision. Your draft remains unchanged.";
    return;
  }
  message.textContent =
    "A newer workspace revision is available. Your draft remains open.";
  review.hidden = false;
  review.addEventListener("click", () =>
    reviewLatestMarkdown(awareness.currentRevisionId),
  );
}

async function submitForm(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  const form = event.currentTarget as HTMLFormElement;
  const values = new FormData(form);
  const action = form.dataset.action;
  temporaryMessage.clear();
  if (action === "new-markdown") {
    await createMarkdown(values);
    return;
  }
  try {
    const result = await invoke<WorkspaceShellView>(
      action === "create"
        ? "create_workspace"
        : action === "join"
          ? "join_workspace"
          : "retry_workspace_join",
      {
        request:
          action === "create"
            ? {
                displayName: String(values.get("displayName") ?? ""),
                creatorDisplayName: String(
                  values.get("creatorDisplayName") ?? "",
                ),
                relayOverride: optionalValue(values.get("relayOverride")),
              }
            : action === "join"
              ? {
                  displayName: String(values.get("displayName") ?? ""),
                  invite: String(values.get("invite") ?? ""),
                }
              : { displayName: String(values.get("displayName") ?? "") },
      },
    );
    if (isWorkspaceShellView(result)) {
      if (action === "retry") {
        actionMessage =
          result.state === "ready"
            ? "Membership is already active."
            : "Join retry sent. Waiting for the inviter.";
      }
      render(result);
    }
  } catch (error) {
    showActionError(error);
  }
}

async function createMarkdown(values: FormData): Promise<void> {
  try {
    const result = await invoke<MarkdownRevisionView>("create_markdown_file", {
      request: {
        parentNodeId: String(values.get("parentNodeId") ?? ""),
        name: String(values.get("name") ?? ""),
        markdown: "",
      },
    });
    if (isMarkdownRevisionView(result)) {
      captureOpenMarkdownDraft();
      previewedEntry = null;
      filePreview = null;
      markdownSession = createMarkdownEditorSession(result, false);
      if (currentView) render(currentView);
    }
  } catch (error) {
    showActionError(error);
  }
}

async function previewEntry(nodeId: string): Promise<void> {
  const entry = currentView?.files?.entries.find(
    (candidate) => candidate.nodeId === nodeId,
  );
  if (!entry) return;
  captureOpenMarkdownDraft();
  previewedEntry = entry;
  if (entry.kind === "directory") {
    markdownSession = null;
    filePreview = null;
    if (currentView) render(currentView);
    return;
  }
  if (!entry.currentRevisionId) return;
  if (entry.kind === "markdown") {
    await openMarkdown(entry.nodeId, entry.currentRevisionId, true);
    return;
  }
  try {
    const result = await invoke<FilePreviewView>("open_file_preview", {
      request: {
        nodeId: entry.nodeId,
        revisionId: entry.currentRevisionId,
      },
    });
    if (isFilePreviewView(result)) {
      markdownSession = null;
      filePreview = result;
      if (currentView) render(currentView);
    }
  } catch (error) {
    showActionError(error);
  }
}

async function openMarkdown(
  nodeId: string,
  revisionId: string,
  readOnly: boolean,
): Promise<void> {
  try {
    const result = await invoke<MarkdownRevisionView>("open_markdown_file", {
      request: { nodeId, revisionId },
    });
    if (isMarkdownRevisionView(result)) {
      captureOpenMarkdownDraft();
      previewedEntry = null;
      filePreview = null;
      markdownSession = createMarkdownEditorSession(result, readOnly);
      if (currentView) render(currentView);
    }
  } catch (error) {
    showActionError(error);
  }
}

async function reviewLatestMarkdown(revisionId: string): Promise<void> {
  captureOpenMarkdownDraft();
  const requestedSession = markdownSession;
  if (!requestedSession || requestedSession.readOnly) return;
  try {
    const result = await invoke<MarkdownRevisionView>("open_markdown_file", {
      request: {
        nodeId: requestedSession.loadedRevision.nodeId,
        revisionId,
      },
    });
    if (
      isMarkdownRevisionView(result) &&
      markdownSession?.loadedRevision.nodeId ===
        requestedSession.loadedRevision.nodeId &&
      markdownSession.loadedRevision.revisionId ===
        requestedSession.loadedRevision.revisionId
    ) {
      markdownSession = reviewMarkdownRevision(markdownSession, result);
      if (currentView) render(currentView);
    }
  } catch (error) {
    showActionError(error);
  }
}

function returnToDraft(): void {
  if (!markdownSession) return;
  markdownSession = returnToMarkdownDraft(markdownSession);
  if (currentView) render(currentView);
}

function loadReviewedRevision(): void {
  if (!markdownSession) return;
  markdownSession = loadReviewedMarkdownRevision(markdownSession);
  if (currentView) render(currentView);
}

async function saveMarkdown(): Promise<void> {
  captureOpenMarkdownDraft();
  const session = markdownSession;
  if (!session || session.readOnly || session.mode !== "draft") return;
  const draft = session.draft;
  try {
    const result = await invoke<MarkdownRevisionView>("replace_markdown_file", {
      request: {
        nodeId: session.loadedRevision.nodeId,
        baseRevisionId: session.loadedRevision.revisionId,
        markdown: draft,
      },
    });
    if (isMarkdownRevisionView(result)) {
      markdownSession = createMarkdownEditorSession(result, false);
      showActionMessage("Markdown revision saved.");
    }
  } catch (error) {
    showActionError(error);
  }
}

async function resolveConflict(
  recordId: string,
  chosenRevisionId: string | null,
): Promise<void> {
  try {
    const result = await invoke<WorkspaceShellView>(
      "resolve_workspace_conflict",
      { request: { recordId, chosenRevisionId } },
    );
    if (isWorkspaceShellView(result)) render(result);
  } catch (error) {
    showActionError(error);
  }
}

async function invokeWorkspaceCommand(command: string): Promise<void> {
  try {
    const result = await invoke<WorkspaceShellView>(command);
    if (isWorkspaceShellView(result)) render(result);
  } catch (error) {
    showActionError(error);
  }
}

async function copyInvite(): Promise<void> {
  try {
    const invite = await invoke<string>("create_workspace_invite");
    await navigator.clipboard.writeText(invite);
    showActionMessage(
      "Invite copied. It grants access only while the inviter is online.",
    );
  } catch (error) {
    showActionError(error);
  }
}

function optionalValue(value: FormDataEntryValue | null): string | null {
  const text = typeof value === "string" ? value.trim() : "";
  return text || null;
}

function showActionMessage(message: string): void {
  temporaryMessage.show(message);
}

function showActionError(error: unknown): void {
  temporaryMessage.clear();
  actionMessage =
    typeof error === "string" ? error : "The request could not be completed.";
  requiredElement<HTMLParagraphElement>(".message").textContent = actionMessage;
}

function requiredElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`Missing shell element: ${selector}`);
  return element;
}

void Promise.all([
  invoke<WorkspaceShellView>("workspace_view"),
  listen<unknown>("workspace:changed", (event) => {
    if (
      isWorkspaceShellView(event.payload) &&
      workspaceViewChanged(currentView, event.payload)
    ) {
      render(event.payload);
    }
  }),
])
  .then(([view]) => {
    if (isWorkspaceShellView(view)) render(view);
  })
  .catch((error: unknown) => {
    shell.textContent =
      typeof error === "string" ? error : "Resonance could not start.";
  });
