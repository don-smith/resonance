import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import {
  validateWorkspaceFilesError,
  validateWorkspaceFilesRequest,
  validateWorkspaceFilesResponse,
  type WorkspaceFilesError,
  type WorkspaceFilesMarkdownRevision,
  type WorkspaceFilesPreview,
  type WorkspaceFilesRequest,
  type WorkspaceFilesResponse,
  type WorkspaceFilesSnapshot,
} from "../../../packages/contracts/src/workspace-files-v1.js";
import {
  workspaceFilesError,
  type WorkspaceFilesSnapshotListener,
  type WorkspaceFilesV1,
} from "../../../packages/sdk/src/workspace-files-v1.js";

const COMMAND = "workspace_files_v1";
const INVALIDATION_EVENT = "workspace-files:changed";

type WorkspaceFilesTransport = {
  invoke(command: string, arguments_: { request: unknown }): Promise<unknown>;
  listen(
    eventName: string,
    handler: (event: { payload: unknown }) => void,
  ): Promise<() => void>;
};

const tauriTransport: WorkspaceFilesTransport = {
  invoke: (command, arguments_) => invoke(command, arguments_),
  listen: (eventName, handler) => listen(eventName, handler),
};

export class WorkspaceFilesTauriAdapter implements WorkspaceFilesV1 {
  readonly #transport: WorkspaceFilesTransport;
  readonly #listeners = new Set<WorkspaceFilesSnapshotListener>();
  readonly #listenerReady: Promise<void>;
  #unlisten: (() => void) | null = null;
  #startupError: WorkspaceFilesError | null = null;
  #disposed = false;
  #issuedSequence = 0;
  #appliedSequence = 0;
  #latestSnapshot: WorkspaceFilesSnapshot | null = null;
  #disposePromise: Promise<void> | null = null;

  public constructor(transport: WorkspaceFilesTransport = tauriTransport) {
    this.#transport = transport;
    this.#listenerReady = this.#transport
      .listen(INVALIDATION_EVENT, ({ payload }) => {
        if (payload !== null || this.#disposed) return;
        void this.snapshot().catch(() => undefined);
      })
      .then((unlisten) => {
        if (this.#disposed) {
          unlisten();
        } else {
          this.#unlisten = unlisten;
        }
      })
      .catch(() => {
        this.#startupError = workspaceFilesError("unavailable-capability");
      });
  }

  public async ready(): Promise<void> {
    await this.#listenerReady;
    if (this.#startupError) throw this.#startupError;
  }

  public subscribe(listener: WorkspaceFilesSnapshotListener): () => void {
    this.#listeners.add(listener);
    if (this.#latestSnapshot) listener(this.#latestSnapshot);
    return () => this.#listeners.delete(listener);
  }

  public async snapshot(): Promise<WorkspaceFilesSnapshot> {
    const response = await this.#dispatch({ operation: "snapshot" });
    return this.#snapshotFrom(response, "snapshot");
  }

  public async selectRoot(): Promise<WorkspaceFilesSnapshot> {
    return this.#snapshotFrom(
      await this.#dispatch({ operation: "select-root" }),
      "select-root",
    );
  }

  public async replaceRoot(): Promise<WorkspaceFilesSnapshot> {
    return this.#snapshotFrom(
      await this.#dispatch({ operation: "replace-root" }),
      "replace-root",
    );
  }

  public async repairRoot(): Promise<WorkspaceFilesSnapshot> {
    return this.#snapshotFrom(
      await this.#dispatch({ operation: "repair-root" }),
      "repair-root",
    );
  }

  public async unbindRoot(): Promise<WorkspaceFilesSnapshot> {
    return this.#snapshotFrom(
      await this.#dispatch({ operation: "unbind-root" }),
      "unbind-root",
    );
  }

  public async openMarkdown(
    nodeId: string,
    revisionId: string,
  ): Promise<WorkspaceFilesMarkdownRevision> {
    const response = await this.#dispatch({
      operation: "open-markdown",
      nodeId,
      revisionId,
    });
    if (response.operation !== "open-markdown" || !("revision" in response)) {
      throw workspaceFilesError("internal");
    }
    return response.revision;
  }

  public async openPreview(
    nodeId: string,
    revisionId: string,
  ): Promise<WorkspaceFilesPreview> {
    const response = await this.#dispatch({
      operation: "open-preview",
      nodeId,
      revisionId,
    });
    if (response.operation !== "open-preview" || !("preview" in response)) {
      throw workspaceFilesError("internal");
    }
    return response.preview;
  }

  public async createMarkdown(
    parentNodeId: string,
    name: string,
    markdown: string,
  ): Promise<WorkspaceFilesMarkdownRevision> {
    const response = await this.#dispatch({
      operation: "create-markdown",
      parentNodeId,
      name,
      markdown,
    });
    if (response.operation !== "create-markdown" || !("revision" in response)) {
      throw workspaceFilesError("internal");
    }
    return response.revision;
  }

  public async replaceMarkdown(
    nodeId: string,
    baseRevisionId: string,
    markdown: string,
  ): Promise<WorkspaceFilesMarkdownRevision> {
    const response = await this.#dispatch({
      operation: "replace-markdown",
      nodeId,
      baseRevisionId,
      markdown,
    });
    if (
      response.operation !== "replace-markdown" ||
      !("revision" in response)
    ) {
      throw workspaceFilesError("internal");
    }
    return response.revision;
  }

  public async resolveConflict(
    recordId: string,
    chosenCandidateId: string | null,
  ): Promise<WorkspaceFilesSnapshot> {
    return this.#snapshotFrom(
      await this.#dispatch({
        operation: "resolve-conflict",
        recordId,
        chosenCandidateId,
      }),
      "resolve-conflict",
    );
  }

  public dispose(): Promise<void> {
    if (this.#disposePromise) return this.#disposePromise;
    this.#disposed = true;
    this.#listeners.clear();
    this.#disposePromise = this.#listenerReady.then(() => {
      this.#unlisten?.();
      this.#unlisten = null;
    });
    return this.#disposePromise;
  }

  async #dispatch(
    request: WorkspaceFilesRequest,
  ): Promise<WorkspaceFilesResponse> {
    await this.#listenerReady;
    if (this.#disposed || this.#startupError) {
      throw workspaceFilesError("unavailable-capability");
    }
    if (validateWorkspaceFilesRequest(request).kind === "invalid") {
      throw workspaceFilesError("invalid-request");
    }
    const sequence = ++this.#issuedSequence;
    let candidate: unknown;
    try {
      candidate = await this.#transport.invoke(COMMAND, { request });
    } catch (error) {
      throw this.#safeError(error);
    }
    const result = validateWorkspaceFilesResponse(candidate);
    if (result.kind === "invalid") throw workspaceFilesError("internal");
    if ("snapshot" in result.value) {
      if (sequence < this.#appliedSequence && this.#latestSnapshot) {
        return { ...result.value, snapshot: this.#latestSnapshot };
      }
      this.#appliedSequence = sequence;
      this.#latestSnapshot = result.value.snapshot;
      for (const listener of this.#listeners) listener(result.value.snapshot);
    }
    return result.value;
  }

  #snapshotFrom(
    response: WorkspaceFilesResponse,
    operation: WorkspaceFilesResponse["operation"],
  ): WorkspaceFilesSnapshot {
    if (response.operation !== operation || !("snapshot" in response)) {
      throw workspaceFilesError("internal");
    }
    return response.snapshot;
  }

  #safeError(candidate: unknown): WorkspaceFilesError {
    const result = validateWorkspaceFilesError(candidate);
    return result.kind === "valid"
      ? result.value
      : workspaceFilesError("internal");
  }
}
