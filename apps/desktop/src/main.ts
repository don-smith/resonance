import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import Editor from "@toast-ui/editor";

import "@toast-ui/editor/dist/toastui-editor.css";
import "./styles.css";
import {
  isMarkdownRevisionView,
  isWorkspaceShellView,
  peerStatus,
  type FileEntryView,
  type MarkdownRevisionView,
  type RootState,
  type WorkspaceShellView,
  workspaceViewChanged,
} from "./workspace-view.js";
import {
  childEntries,
  conflictFallbackLabel,
  conflictFallbackSelection,
  conflictLabel,
  conflictRevisionActionLabel,
  retainedOpenRevision,
  rootStatusMessage,
} from "./workspace-files-view.js";
import { createTemporaryMessage } from "./temporary-message.js";

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) throw new Error("Resonance shell mount point is missing.");
const shell = app;
let actionMessage: string | null = null;
let currentView: WorkspaceShellView | null = null;
let openRevision: MarkdownRevisionView | null = null;
let openRevisionReadOnly = false;
let markdownEditor: Editor | null = null;
const temporaryMessage = createTemporaryMessage((message) => {
  actionMessage = message;
  if (currentView) render(currentView);
});

function field(label: string, name: string, type = "text"): string {
  return `<label>${label}<input name="${name}" type="${type}" required /></label>`;
}

function render(view: WorkspaceShellView): void {
  currentView = view;
  markdownEditor?.destroy();
  markdownEditor = null;
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
                <p class="editor-placeholder">Choose a Markdown file to edit.</p>
                <div id="markdown-editor" hidden></div>
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
    for (const revisionId of conflict.competingRevisionIds) {
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

  const retainedRevision = retainedOpenRevision(openRevision, files.entries);
  if (openRevision && !retainedRevision) {
    openRevision = null;
    openRevisionReadOnly = false;
    requiredElement<HTMLParagraphElement>(".editor-placeholder").textContent =
      "The open Markdown file was deleted.";
  } else if (retainedRevision) {
    mountMarkdownEditor(retainedRevision);
  }
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
    const label = document.createElement(
      entry.kind === "markdown" && entry.editable ? "button" : "span",
    );
    label.textContent = entry.name;
    if (label instanceof HTMLButtonElement && entry.currentRevisionId) {
      label.type = "button";
      label.addEventListener("click", () =>
        openMarkdown(entry.nodeId, entry.currentRevisionId!, false),
      );
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

function mountMarkdownEditor(revision: MarkdownRevisionView): void {
  const host = requiredElement<HTMLDivElement>("#markdown-editor");
  host.hidden = false;
  requiredElement<HTMLParagraphElement>(".editor-placeholder").hidden = true;
  requiredElement<HTMLButtonElement>('[data-action="save-markdown"]').hidden =
    openRevisionReadOnly;
  markdownEditor = new Editor({
    el: host,
    height: "32rem",
    initialEditType: "wysiwyg",
    initialValue: revision.markdown,
    hideModeSwitch: true,
    usageStatistics: false,
    toolbarItems: [
      ["heading", "bold", "italic", "strike"],
      ["ul", "ol", "task"],
      ["link", "quote", "code", "codeblock"],
    ],
  });
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
      openRevision = result;
      openRevisionReadOnly = false;
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
      openRevision = result;
      openRevisionReadOnly = readOnly;
      if (currentView) render(currentView);
    }
  } catch (error) {
    showActionError(error);
  }
}

async function saveMarkdown(): Promise<void> {
  if (!openRevision || !markdownEditor) return;
  try {
    const result = await invoke<MarkdownRevisionView>("replace_markdown_file", {
      request: {
        nodeId: openRevision.nodeId,
        baseRevisionId: openRevision.revisionId,
        markdown: markdownEditor.getMarkdown(),
      },
    });
    if (isMarkdownRevisionView(result)) {
      openRevision = result;
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
