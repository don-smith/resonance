/// <reference path="./toast-ui-editor.d.ts" />

import Editor from "@toast-ui/editor";
import type {
  PackageInstance,
  WorkspaceFilesError,
  WorkspaceFilesPreview,
  WorkspaceFilesSnapshot,
  WorkspaceFilesV1,
} from "@resonance/package-sdk";

import {
  captureMountedMarkdownDraft,
  editableMarkdownMount,
  viewerMarkdownMount,
  type MountedMarkdown,
} from "./markdown-editor-mount.js";
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
import type { FileEntryView } from "./workspace-files-types.js";

export class WorkspaceFilesPackage implements PackageInstance {
  readonly #root: HTMLElement;
  readonly #files: WorkspaceFilesV1;
  readonly #submit = (event: Event) => void this.#handleSubmit(event);
  readonly #click = (event: Event) => void this.#handleClick(event);
  #unsubscribe: (() => void) | null = null;
  #snapshot: WorkspaceFilesSnapshot | null = null;
  #markdownSession: MarkdownEditorSession | null = null;
  #mountedMarkdown: MountedMarkdown | null = null;
  #previewedEntry: FileEntryView | null = null;
  #filePreview: WorkspaceFilesPreview | null = null;
  #filePreviewUrl: string | null = null;
  #message: string | null = null;
  #messageTimeout: ReturnType<typeof setTimeout> | null = null;
  #active = false;
  #disposed = false;
  #initialized = false;

  public constructor(root: HTMLElement, files: WorkspaceFilesV1) {
    this.#root = root;
    this.#files = files;
    this.#root.addEventListener("submit", this.#submit);
    this.#root.addEventListener("click", this.#click);
    this.#unsubscribe = files.subscribe((snapshot) => {
      if (this.#disposed) return;
      this.#snapshot = snapshot;
      if (this.#active) this.#render();
    });
  }

  public async activate(): Promise<void> {
    if (this.#disposed) return;
    this.#active = true;
    if (!this.#initialized) {
      this.#snapshot = await this.#files.snapshot();
      this.#initialized = true;
    }
    this.#render();
  }

  public deactivate(): void {
    this.#active = false;
  }

  public dispose(): void {
    if (this.#disposed) return;
    this.#disposed = true;
    this.#active = false;
    this.#captureDraft();
    this.#destroyEditor();
    this.#revokePreviewUrl();
    this.#unsubscribe?.();
    this.#unsubscribe = null;
    if (this.#messageTimeout !== null) clearTimeout(this.#messageTimeout);
    this.#messageTimeout = null;
    this.#root.removeEventListener("submit", this.#submit);
    this.#root.removeEventListener("click", this.#click);
    this.#root.replaceChildren();
  }

  #render(): void {
    if (this.#disposed || !this.#active) return;
    this.#captureDraft();
    this.#destroyEditor();
    this.#revokePreviewUrl();
    const snapshot = this.#snapshot;
    this.#root.innerHTML = `
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
    this.#required<HTMLElement>(".workspace-files-message").textContent =
      this.#message ?? "";
    if (!snapshot) return;
    this.#required<HTMLElement>(".workspace-files-root-status").textContent =
      rootStatusMessage(snapshot.root.state);
    this.#renderRootActions(snapshot.root.state);
    this.#appendTreeLevel(
      this.#required<HTMLUListElement>(".workspace-files-tree"),
      snapshot.entries,
      null,
    );
    const select = this.#required<HTMLSelectElement>('[name="parentNodeId"]');
    for (const entry of snapshot.entries.filter(
      (entry) => entry.kind === "directory",
    )) {
      const option = this.#document.createElement("option");
      option.value = entry.nodeId;
      option.textContent = entry.name;
      select.append(option);
    }
    this.#renderConflicts(snapshot);
    if (
      this.#previewedEntry &&
      !snapshot.entries.some(
        (entry) => entry.nodeId === this.#previewedEntry?.nodeId,
      )
    ) {
      this.#previewedEntry = null;
      this.#filePreview = null;
    }
    if (this.#previewedEntry) {
      this.#mountFilePreview(this.#previewedEntry, snapshot.entries);
    }
    if (this.#markdownSession) {
      this.#mountMarkdownSession(this.#markdownSession, snapshot);
    }
  }

  #renderRootActions(state: WorkspaceFilesSnapshot["root"]["state"]): void {
    const actions = this.#required<HTMLElement>(
      ".workspace-files-root-actions",
    );
    const commands =
      state === "unbound"
        ? [["Choose folder", "select-root"]]
        : [
            ["Repair", "repair-root"],
            ["Replace", "replace-root"],
            ["Unbind", "unbind-root"],
          ];
    for (const [label, action] of commands) {
      const button = this.#document.createElement("button");
      button.type = "button";
      button.textContent = label;
      button.dataset.action = action;
      actions.append(button);
    }
  }

  #appendTreeLevel(
    parent: HTMLUListElement,
    entries: readonly FileEntryView[],
    parentNodeId: string | null,
  ): void {
    for (const entry of childEntries(entries, parentNodeId)) {
      const item = this.#document.createElement("li");
      const label = this.#document.createElement("button");
      label.type = "button";
      label.textContent = entry.name;
      label.dataset.nodeId = entry.nodeId;
      label.dataset.action =
        entry.kind === "markdown" && entry.editable
          ? "open-markdown"
          : "preview-entry";
      item.append(label);
      if (entry.kind === "directory") {
        const children = this.#document.createElement("ul");
        this.#appendTreeLevel(children, entries, entry.nodeId);
        item.append(children);
      }
      parent.append(item);
    }
  }

  #renderConflicts(snapshot: WorkspaceFilesSnapshot): void {
    const conflicts = this.#required<HTMLUListElement>(
      ".workspace-files-conflicts ul",
    );
    if (snapshot.conflicts.length === 0) {
      const item = this.#document.createElement("li");
      item.textContent = "No unresolved conflicts.";
      conflicts.append(item);
    }
    for (const conflict of snapshot.conflicts) {
      const item = this.#document.createElement("li");
      const title = this.#document.createElement("p");
      title.textContent = conflictLabel(conflict.kind);
      item.append(title);
      if (conflict.treeChoices.length > 0) {
        for (const choice of conflict.treeChoices) {
          item.append(
            this.#actionButton(
              treeConflictPreviewLabel(choice),
              "preview-entry",
              {
                nodeId: choice.nodeId,
              },
            ),
            this.#actionButton(
              treeConflictActionLabel(choice),
              "resolve-conflict",
              {
                recordId: conflict.recordId,
                candidateId: choice.candidateId,
              },
            ),
          );
        }
        conflicts.append(item);
        continue;
      }
      for (const revisionId of conflict.resolutionCandidateIds) {
        if (conflict.reviewableRevisionIds.includes(revisionId)) {
          item.append(
            this.#actionButton(
              `Review ${revisionId.slice(0, 8)}`,
              "review-conflict",
              {
                nodeId: conflict.nodeId,
                revisionId,
              },
            ),
          );
        }
        item.append(
          this.#actionButton(
            conflictRevisionActionLabel(conflict.kind, revisionId),
            "resolve-conflict",
            { recordId: conflict.recordId, candidateId: revisionId },
          ),
        );
      }
      const fallback = conflictFallbackSelection(conflict);
      const keep = this.#actionButton(
        conflictFallbackLabel(conflict.kind),
        "resolve-conflict",
        { recordId: conflict.recordId, candidateId: fallback ?? "" },
      );
      keep.dataset.nullCandidate = fallback === null ? "true" : "false";
      keep.disabled = conflict.kind === "delete-edit" && fallback === null;
      item.append(keep);
      conflicts.append(item);
    }
  }

  #actionButton(
    label: string,
    action: string,
    data: Record<string, string>,
  ): HTMLButtonElement {
    const button = this.#document.createElement("button");
    button.type = "button";
    button.textContent = label;
    button.dataset.action = action;
    for (const [key, value] of Object.entries(data))
      button.dataset[key] = value;
    return button;
  }

  #mountFilePreview(
    entry: FileEntryView,
    entries: readonly FileEntryView[],
  ): void {
    const host = this.#required<HTMLElement>(".workspace-files-preview");
    host.hidden = false;
    this.#required<HTMLElement>(".workspace-files-placeholder").hidden = true;
    this.#required<HTMLElement>(".workspace-files-preview h3").textContent =
      entry.name;
    const message = this.#required<HTMLElement>(".workspace-files-preview p");
    const image = this.#required<HTMLImageElement>(
      ".workspace-files-preview img",
    );
    const children = this.#required<HTMLUListElement>(
      ".workspace-files-preview ul",
    );
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
        const item = this.#document.createElement("li");
        item.textContent = `${child.kind === "directory" ? "Folder" : "File"}: ${child.name}`;
        children.append(item);
      }
      return;
    }
    if (this.#filePreview?.kind === "image") {
      const blob = new Blob([new Uint8Array(this.#filePreview.bytes)], {
        type: this.#filePreview.mimeType,
      });
      this.#filePreviewUrl = URL.createObjectURL(blob);
      image.src = this.#filePreviewUrl;
      image.alt = `Preview of ${entry.name}`;
      image.hidden = false;
      message.textContent = `${this.#filePreview.mimeType} · ${this.#filePreview.byteLength} bytes`;
      return;
    }
    message.textContent = this.#filePreview
      ? `Preview unavailable for ${this.#filePreview.mimeType} (${this.#filePreview.byteLength} bytes). Review this file outside Resonance.`
      : "Preview unavailable. Review this file outside Resonance.";
  }

  #mountMarkdownSession(
    session: MarkdownEditorSession,
    snapshot: WorkspaceFilesSnapshot,
  ): void {
    const host = this.#required<HTMLElement>(
      ".workspace-files-markdown-editor",
    );
    host.hidden = false;
    this.#required<HTMLElement>(".workspace-files-placeholder").hidden = true;
    this.#renderMarkdownNotice(session, snapshot);
    const save = this.#required<HTMLButtonElement>(
      '[data-action="save-markdown"]',
    );
    const reviewActions = this.#required<HTMLElement>(
      ".workspace-files-review-actions",
    );
    const reviewed = session.reviewedRevision;
    if (session.mode === "review" && reviewed) {
      reviewActions.hidden = false;
      this.#mountedMarkdown = viewerMarkdownMount(
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
      this.#mountedMarkdown = viewerMarkdownMount(
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
    this.#mountedMarkdown = editableMarkdownMount(
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

  #renderMarkdownNotice(
    session: MarkdownEditorSession,
    snapshot: WorkspaceFilesSnapshot,
  ): void {
    if (session.readOnly) return;
    const notice = this.#required<HTMLElement>(
      ".workspace-files-editor-notice",
    );
    const message = this.#required<HTMLElement>(
      ".workspace-files-editor-notice p",
    );
    const review = this.#required<HTMLButtonElement>(
      '[data-action="review-latest"]',
    );
    const awareness = markdownRevisionAwareness(session, snapshot);
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
    review.dataset.revisionId = awareness.currentRevisionId;
  }

  async #handleSubmit(event: Event): Promise<void> {
    const form = event.target as HTMLFormElement;
    if (!form.matches('form[data-action="new-markdown"]')) return;
    event.preventDefault();
    const values = new FormData(form);
    await this.#run(async () => {
      const revision = await this.#files.createMarkdown(
        String(values.get("parentNodeId") ?? ""),
        String(values.get("name") ?? ""),
        "",
      );
      this.#previewedEntry = null;
      this.#filePreview = null;
      this.#markdownSession = createMarkdownEditorSession(revision, false);
      this.#snapshot = await this.#files.snapshot();
      this.#render();
    });
  }

  async #handleClick(event: Event): Promise<void> {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const button = target.closest<HTMLButtonElement>("button[data-action]");
    if (!button || !this.#root.contains(button)) return;
    const action = button.dataset.action;
    await this.#run(async () => {
      switch (action) {
        case "select-root":
          this.#snapshot = await this.#files.selectRoot();
          break;
        case "replace-root":
          this.#snapshot = await this.#files.replaceRoot();
          break;
        case "repair-root":
          this.#snapshot = await this.#files.repairRoot();
          break;
        case "unbind-root":
          this.#snapshot = await this.#files.unbindRoot();
          break;
        case "open-markdown": {
          const entry = this.#entry(button.dataset.nodeId);
          if (entry?.currentRevisionId) {
            await this.#openMarkdown(
              entry.nodeId,
              entry.currentRevisionId,
              false,
            );
          }
          return;
        }
        case "preview-entry":
          await this.#previewEntry(button.dataset.nodeId ?? "");
          return;
        case "review-conflict":
          await this.#openMarkdown(
            button.dataset.nodeId ?? "",
            button.dataset.revisionId ?? "",
            true,
          );
          return;
        case "resolve-conflict":
          this.#snapshot = await this.#files.resolveConflict(
            button.dataset.recordId ?? "",
            button.dataset.nullCandidate === "true"
              ? null
              : (button.dataset.candidateId ?? null),
          );
          break;
        case "review-latest":
          await this.#reviewLatest(button.dataset.revisionId ?? "");
          return;
        case "return-to-draft":
          if (this.#markdownSession) {
            this.#markdownSession = returnToMarkdownDraft(
              this.#markdownSession,
            );
          }
          break;
        case "load-reviewed":
          if (this.#markdownSession) {
            this.#markdownSession = loadReviewedMarkdownRevision(
              this.#markdownSession,
            );
          }
          break;
        case "save-markdown":
          await this.#saveMarkdown();
          return;
        default:
          return;
      }
      this.#render();
    });
  }

  async #previewEntry(nodeId: string): Promise<void> {
    const entry = this.#entry(nodeId);
    if (!entry) return;
    this.#captureDraft();
    this.#previewedEntry = entry;
    if (entry.kind === "directory") {
      this.#markdownSession = null;
      this.#filePreview = null;
      this.#render();
      return;
    }
    if (!entry.currentRevisionId) return;
    if (entry.kind === "markdown") {
      await this.#openMarkdown(entry.nodeId, entry.currentRevisionId, true);
      return;
    }
    this.#filePreview = await this.#files.openPreview(
      entry.nodeId,
      entry.currentRevisionId,
    );
    this.#markdownSession = null;
    this.#render();
  }

  async #openMarkdown(
    nodeId: string,
    revisionId: string,
    readOnly: boolean,
  ): Promise<void> {
    const revision = await this.#files.openMarkdown(nodeId, revisionId);
    this.#captureDraft();
    this.#previewedEntry = null;
    this.#filePreview = null;
    this.#markdownSession = createMarkdownEditorSession(revision, readOnly);
    this.#render();
  }

  async #reviewLatest(revisionId: string): Promise<void> {
    this.#captureDraft();
    const requested = this.#markdownSession;
    if (!requested || requested.readOnly) return;
    const revision = await this.#files.openMarkdown(
      requested.loadedRevision.nodeId,
      revisionId,
    );
    if (
      this.#markdownSession?.loadedRevision.nodeId ===
        requested.loadedRevision.nodeId &&
      this.#markdownSession.loadedRevision.revisionId ===
        requested.loadedRevision.revisionId
    ) {
      this.#markdownSession = reviewMarkdownRevision(
        this.#markdownSession,
        revision,
      );
      this.#render();
    }
  }

  async #saveMarkdown(): Promise<void> {
    this.#captureDraft();
    const session = this.#markdownSession;
    if (!session || session.readOnly || session.mode !== "draft") return;
    const revision = await this.#files.replaceMarkdown(
      session.loadedRevision.nodeId,
      session.loadedRevision.revisionId,
      session.draft,
    );
    this.#markdownSession = createMarkdownEditorSession(revision, false);
    this.#snapshot = await this.#files.snapshot();
    this.#showMessage("Markdown revision saved.");
  }

  async #run(action: () => Promise<void>): Promise<void> {
    if (this.#disposed) return;
    try {
      await action();
    } catch (error) {
      this.#showError(error);
    }
  }

  #showMessage(message: string): void {
    if (this.#messageTimeout !== null) clearTimeout(this.#messageTimeout);
    this.#message = message;
    this.#render();
    this.#messageTimeout = setTimeout(() => {
      this.#messageTimeout = null;
      this.#message = null;
      this.#render();
    }, 5_000);
  }

  #showError(error: unknown): void {
    this.#message = this.#isWorkspaceFilesError(error)
      ? error.message
      : "The workspace-files request could not be completed.";
    this.#render();
  }

  #isWorkspaceFilesError(error: unknown): error is WorkspaceFilesError {
    return (
      typeof error === "object" &&
      error !== null &&
      "code" in error &&
      "message" in error &&
      typeof error.code === "string" &&
      typeof error.message === "string"
    );
  }

  #entry(nodeId: string | undefined): FileEntryView | undefined {
    return this.#snapshot?.entries.find((entry) => entry.nodeId === nodeId);
  }

  #captureDraft(): void {
    this.#markdownSession = captureMountedMarkdownDraft(
      this.#markdownSession,
      this.#mountedMarkdown,
    );
  }

  #destroyEditor(): void {
    this.#mountedMarkdown?.instance.destroy();
    this.#mountedMarkdown = null;
  }

  #revokePreviewUrl(): void {
    if (this.#filePreviewUrl) URL.revokeObjectURL(this.#filePreviewUrl);
    this.#filePreviewUrl = null;
  }

  #required<T extends Element>(selector: string): T {
    const element = this.#root.querySelector<T>(selector);
    if (!element)
      throw new Error(`Missing workspace-files element: ${selector}`);
    return element;
  }

  get #document(): Document {
    return this.#root.ownerDocument;
  }
}
