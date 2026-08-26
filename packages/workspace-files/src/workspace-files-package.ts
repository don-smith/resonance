import {
  isWorkspaceFilesError,
  type PackageInstance,
  type WorkspaceFilesPreview,
  type WorkspaceFilesSnapshot,
  type WorkspaceFilesV1,
} from "@resonance/package-sdk";

import {
  captureDraft,
  editorIdentity,
  renderEditor,
} from "./workspace-files/markdown-editor.js";
import {
  findEntry,
  renderConflicts,
  renderMarkup,
  renderMessage,
  renderRoot,
  renderTree,
} from "./workspace-files/dom.js";
import {
  clearPreview,
  renderPreview,
  revokePreviewUrl,
} from "./workspace-files/preview.js";
import { toastUiEditorAdapter } from "./toast-ui-editor.js";
import {
  createMarkdownEditorSession,
  loadReviewedMarkdownRevision,
  returnToMarkdownDraft,
  reviewMarkdownRevision,
  rootStatusMessage,
  type MarkdownEditorSession,
} from "./workspace-files-view.js";
import type { FileEntryView } from "./workspace-files-types.js";
import type {
  MarkdownEditorAdapter,
  MountedMarkdown,
} from "./markdown-editor-mount.js";

export type WorkspaceFilesPackageOptions = Readonly<{
  editor?: MarkdownEditorAdapter;
}>;

export class WorkspaceFilesPackage implements PackageInstance {
  readonly #root: HTMLElement;
  readonly #files: WorkspaceFilesV1;
  readonly #editor: MarkdownEditorAdapter;
  readonly #submit = (event: Event) => void this.#handleSubmit(event);
  readonly #click = (event: Event) => void this.#handleClick(event);
  #unsubscribe: (() => void) | null = null;
  #snapshot: WorkspaceFilesSnapshot | null = null;
  #markdownSession: MarkdownEditorSession | null = null;
  #mountedMarkdown: MountedMarkdown | null = null;
  #editorKey: string | null = null;
  #previewedEntry: FileEntryView | null = null;
  #filePreview: WorkspaceFilesPreview | null = null;
  #filePreviewUrl: string | null = null;
  #message: string | null = null;
  #messageTimeout: ReturnType<typeof setTimeout> | null = null;
  #mutationQueue: Promise<void> = Promise.resolve();
  #readVersion = 0;
  #active = false;
  #disposed = false;
  #initialized = false;
  #markupRendered = false;

  public constructor(
    root: HTMLElement,
    files: WorkspaceFilesV1,
    options: WorkspaceFilesPackageOptions = {},
  ) {
    this.#root = root;
    this.#files = files;
    this.#editor = options.editor ?? toastUiEditorAdapter;
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
    this.#readVersion += 1;
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
    if (!this.#markupRendered) {
      renderMarkup(this.#root);
      this.#markupRendered = true;
    }
    this.#captureDraft();
    renderMessage(this.#root, this.#message);
    const snapshot = this.#snapshot;
    if (!snapshot) return;
    renderRoot(
      this.#root,
      snapshot.root.state,
      rootStatusMessage(snapshot.root.state),
    );
    renderTree(this.#root, snapshot.entries);
    renderConflicts(this.#root, snapshot);
    this.#renderSelectedEntry(snapshot);
    this.#renderEditor(snapshot);
  }

  #renderSelectedEntry(snapshot: WorkspaceFilesSnapshot): void {
    if (
      this.#previewedEntry &&
      !snapshot.entries.some(
        ({ nodeId }) => nodeId === this.#previewedEntry?.nodeId,
      )
    ) {
      this.#previewedEntry = null;
      this.#filePreview = null;
    }
    this.#revokePreviewUrl();
    clearPreview(this.#root);
    if (!this.#previewedEntry) return;
    const entry = findEntry(snapshot, this.#previewedEntry.nodeId);
    if (!entry) return;
    this.#filePreviewUrl = renderPreview(
      this.#root,
      {
        entry,
        preview: this.#filePreview,
      },
      snapshot.entries,
    );
  }

  #renderEditor(snapshot: WorkspaceFilesSnapshot): void {
    const key = this.#markdownSession
      ? editorIdentity(this.#markdownSession)
      : null;
    if (key !== this.#editorKey) {
      this.#destroyEditor();
      this.#editorKey = key;
      if (this.#markdownSession) {
        this.#mountedMarkdown = renderEditor(
          this.#root,
          this.#markdownSession,
          snapshot,
          null,
          this.#editor,
        );
      }
      return;
    }
    this.#mountedMarkdown = renderEditor(
      this.#root,
      this.#markdownSession,
      snapshot,
      this.#mountedMarkdown,
      this.#editor,
    );
  }

  async #handleSubmit(event: Event): Promise<void> {
    const form = event.target as HTMLFormElement;
    if (!form.matches('form[data-action="new-markdown"]')) return;
    event.preventDefault();
    const values = new FormData(form);
    await this.#runMutation(async () => {
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
    const data = { ...button.dataset };
    switch (action) {
      case "open-markdown":
      case "preview-entry":
      case "review-conflict":
      case "review-latest":
        await this.#runRead(() => this.#readAction(action, data));
        return;
      default:
        await this.#runMutation(() => this.#mutationAction(action, data));
    }
  }

  async #readAction(
    action: string | undefined,
    data: Record<string, string | undefined>,
  ): Promise<void> {
    switch (action) {
      case "open-markdown": {
        const entry = findEntry(this.#snapshot, data.nodeId);
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
        await this.#previewEntry(data.nodeId ?? "");
        return;
      case "review-conflict":
        await this.#openMarkdown(
          data.nodeId ?? "",
          data.revisionId ?? "",
          true,
        );
        return;
      case "review-latest":
        await this.#reviewLatest(data.revisionId ?? "");
        return;
    }
  }

  async #mutationAction(
    action: string | undefined,
    data: Record<string, string | undefined>,
  ): Promise<void> {
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
      case "resolve-conflict":
        this.#snapshot = await this.#files.resolveConflict(
          data.recordId ?? "",
          data.nullCandidate === "true" ? null : (data.candidateId ?? null),
        );
        break;
      case "return-to-draft":
        if (this.#markdownSession) {
          this.#markdownSession = returnToMarkdownDraft(this.#markdownSession);
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
  }

  async #previewEntry(nodeId: string): Promise<void> {
    const entry = findEntry(this.#snapshot, nodeId);
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
    const version = this.#readVersion;
    this.#filePreview = await this.#files.openPreview(
      entry.nodeId,
      entry.currentRevisionId,
    );
    if (version !== this.#readVersion || this.#disposed) return;
    this.#markdownSession = null;
    this.#render();
  }

  async #openMarkdown(
    nodeId: string,
    revisionId: string,
    readOnly: boolean,
  ): Promise<void> {
    const version = this.#readVersion;
    const revision = await this.#files.openMarkdown(nodeId, revisionId);
    if (version !== this.#readVersion || this.#disposed) return;
    this.#captureDraft();
    this.#previewedEntry = null;
    this.#filePreview = null;
    this.#markdownSession = createMarkdownEditorSession(revision, readOnly);
    this.#render();
  }

  async #reviewLatest(revisionId: string): Promise<void> {
    this.#captureDraft();
    const requested = this.#markdownSession;
    if (!requested || requested.readOnly || requested.kind !== "draft") return;
    const version = this.#readVersion;
    const revision = await this.#files.openMarkdown(
      requested.loadedRevision.nodeId,
      revisionId,
    );
    if (
      version !== this.#readVersion ||
      this.#disposed ||
      this.#markdownSession?.loadedRevision.nodeId !==
        requested.loadedRevision.nodeId ||
      this.#markdownSession.loadedRevision.revisionId !==
        requested.loadedRevision.revisionId
    ) {
      return;
    }
    this.#markdownSession = reviewMarkdownRevision(requested, revision);
    this.#render();
  }

  async #saveMarkdown(): Promise<void> {
    this.#captureDraft();
    const session = this.#markdownSession;
    if (!session || session.readOnly || session.kind !== "draft") return;
    const revision = await this.#files.replaceMarkdown(
      session.loadedRevision.nodeId,
      session.loadedRevision.revisionId,
      session.draft,
    );
    this.#markdownSession = createMarkdownEditorSession(revision, false);
    this.#snapshot = await this.#files.snapshot();
    this.#showMessage("Markdown revision saved.");
  }

  #runMutation(action: () => Promise<void>): Promise<void> {
    this.#readVersion += 1;
    const next = this.#mutationQueue
      .catch(() => undefined)
      .then(action)
      .catch((error) => this.#showError(error));
    this.#mutationQueue = next;
    return next;
  }

  #runRead(action: () => Promise<void>): Promise<void> {
    const version = ++this.#readVersion;
    return action().catch((error) => {
      if (version === this.#readVersion) this.#showError(error);
    });
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
    this.#message = isWorkspaceFilesError(error)
      ? error.message
      : "The workspace-files request could not be completed.";
    this.#render();
  }

  #captureDraft(): void {
    this.#markdownSession = captureDraft(
      this.#markdownSession,
      this.#mountedMarkdown,
    );
  }

  #destroyEditor(): void {
    this.#mountedMarkdown?.instance.destroy();
    this.#mountedMarkdown = null;
  }

  #revokePreviewUrl(): void {
    revokePreviewUrl(this.#filePreviewUrl);
    this.#filePreviewUrl = null;
  }
}
