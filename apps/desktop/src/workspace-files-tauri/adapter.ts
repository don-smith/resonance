import {
  type WorkspaceFilesError,
  type WorkspaceFilesMarkdownRevision,
  type WorkspaceFilesPreview,
  type WorkspaceFilesRequest,
  type WorkspaceFilesResponse,
  type WorkspaceFilesSnapshot,
} from "@resonance/contracts";
import {
  immutableClone,
  workspaceFilesError,
  type WorkspaceFilesSnapshotListener,
  type WorkspaceFilesV1,
} from "@resonance/package-sdk";

import {
  dispatchWorkspaceFiles,
  type RequestFor,
  type ResponseFor,
  safeWorkspaceFilesError,
} from "./dispatch.js";
import { SnapshotPublisher } from "./snapshots.js";
import {
  tauriWorkspaceFilesTransport,
  type WorkspaceFilesTransport,
} from "./transport.js";

const INVALIDATION_EVENT = "workspace-files:changed";
type SnapshotOperation =
  | "snapshot"
  | "select-root"
  | "replace-root"
  | "repair-root"
  | "unbind-root"
  | "resolve-conflict";

type AdapterOptions = Readonly<{
  onSubscriberError?: (error: unknown) => void;
}>;

export class WorkspaceFilesTauriAdapter implements WorkspaceFilesV1 {
  readonly #transport: WorkspaceFilesTransport;
  readonly #publisher: SnapshotPublisher;
  readonly #listenerReady: Promise<void>;
  #unlisten: (() => void) | null = null;
  #startupError: WorkspaceFilesError | null = null;
  #disposed = false;
  #issuedSequence = 0;
  #appliedSequence = 0;
  #disposePromise: Promise<void> | null = null;

  public constructor(
    transport: WorkspaceFilesTransport = tauriWorkspaceFilesTransport,
    options: AdapterOptions = {},
  ) {
    this.#transport = transport;
    this.#publisher = new SnapshotPublisher(options.onSubscriberError);
    this.#listenerReady = this.#transport
      .listen(INVALIDATION_EVENT, ({ payload }) => {
        if (payload !== null || this.#disposed) return;
        void this.snapshot().catch(() => undefined);
      })
      .then((unlisten) => {
        if (this.#disposed) unlisten();
        else this.#unlisten = unlisten;
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
    return this.#publisher.subscribe(listener);
  }

  public async snapshot(): Promise<WorkspaceFilesSnapshot> {
    return this.#snapshotFrom(
      "snapshot",
      await this.#dispatch({ operation: "snapshot" }),
    );
  }

  public async selectRoot(): Promise<WorkspaceFilesSnapshot> {
    return this.#snapshotFrom(
      "select-root",
      await this.#dispatch({ operation: "select-root" }),
    );
  }

  public async replaceRoot(): Promise<WorkspaceFilesSnapshot> {
    return this.#snapshotFrom(
      "replace-root",
      await this.#dispatch({ operation: "replace-root" }),
    );
  }

  public async repairRoot(): Promise<WorkspaceFilesSnapshot> {
    return this.#snapshotFrom(
      "repair-root",
      await this.#dispatch({ operation: "repair-root" }),
    );
  }

  public async unbindRoot(): Promise<WorkspaceFilesSnapshot> {
    return this.#snapshotFrom(
      "unbind-root",
      await this.#dispatch({ operation: "unbind-root" }),
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
    return immutableClone(response.revision);
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
    return immutableClone(response.preview);
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
    return immutableClone(response.revision);
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
    return immutableClone(response.revision);
  }

  public async resolveConflict(
    recordId: string,
    chosenCandidateId: string | null,
  ): Promise<WorkspaceFilesSnapshot> {
    return this.#snapshotFrom(
      "resolve-conflict",
      await this.#dispatch({
        operation: "resolve-conflict",
        recordId,
        chosenCandidateId,
      }),
    );
  }

  public dispose(): Promise<void> {
    if (this.#disposePromise) return this.#disposePromise;
    this.#disposed = true;
    this.#publisher.clear();
    this.#disposePromise = this.#listenerReady.then(() => {
      this.#unlisten?.();
      this.#unlisten = null;
    });
    return this.#disposePromise;
  }

  async #dispatch<O extends WorkspaceFilesRequest["operation"]>(
    request: RequestFor<O>,
  ): Promise<ResponseFor<O>> {
    await this.#listenerReady;
    if (this.#disposed || this.#startupError) {
      throw workspaceFilesError("unavailable-capability");
    }
    const sequence = ++this.#issuedSequence;
    let response: ResponseFor<O>;
    try {
      response = await dispatchWorkspaceFiles(this.#transport, request);
    } catch (error) {
      throw safeWorkspaceFilesError(error);
    }
    const candidate = response as WorkspaceFilesResponse;
    if (!isSnapshotResponse(candidate)) return immutableClone(response);
    if (sequence < this.#appliedSequence && this.#publisher.latest) {
      return {
        ...candidate,
        snapshot: this.#publisher.latest,
      } as ResponseFor<O>;
    }
    this.#appliedSequence = sequence;
    const snapshot = this.#publisher.publish(candidate.snapshot);
    return { ...candidate, snapshot } as ResponseFor<O>;
  }

  #snapshotFrom<O extends SnapshotOperation>(
    operation: O,
    response: ResponseFor<O>,
  ): WorkspaceFilesSnapshot {
    const candidate = response as WorkspaceFilesResponse;
    if (!isSnapshotResponse(candidate) || candidate.operation !== operation) {
      throw workspaceFilesError("internal");
    }
    return candidate.snapshot;
  }
}

function isSnapshotResponse(
  response: WorkspaceFilesResponse,
): response is Extract<
  WorkspaceFilesResponse,
  { snapshot: WorkspaceFilesSnapshot }
> {
  return "snapshot" in response;
}
