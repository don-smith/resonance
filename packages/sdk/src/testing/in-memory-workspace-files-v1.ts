import type {
  WorkspaceFilesError,
  WorkspaceFilesMarkdownRevision,
  WorkspaceFilesPreview,
  WorkspaceFilesSnapshot,
  WorkspaceFilesV1Operation,
} from "../../../contracts/src/workspace-files-v1.js";
import {
  workspaceFilesError,
  type WorkspaceFilesFailureController,
  type WorkspaceFilesSnapshotListener,
  type WorkspaceFilesV1,
} from "../workspace-files-v1.js";

export class InMemoryWorkspaceFilesV1
  implements WorkspaceFilesV1, WorkspaceFilesFailureController
{
  readonly #listeners = new Set<WorkspaceFilesSnapshotListener>();
  readonly #failures = new Map<
    WorkspaceFilesV1Operation,
    WorkspaceFilesError
  >();
  readonly #revisions = new Map<string, WorkspaceFilesMarkdownRevision>();
  readonly #previews = new Map<string, WorkspaceFilesPreview>();
  #snapshot: WorkspaceFilesSnapshot;
  #revisionSequence = 0;

  public constructor(snapshot: WorkspaceFilesSnapshot) {
    this.#snapshot = snapshot;
  }

  public setSnapshot(snapshot: WorkspaceFilesSnapshot): void {
    this.#snapshot = snapshot;
    this.#emit();
  }

  public setMarkdownRevision(revision: WorkspaceFilesMarkdownRevision): void {
    this.#revisions.set(
      this.#revisionKey(revision.nodeId, revision.revisionId),
      revision,
    );
  }

  public setPreview(
    nodeId: string,
    revisionId: string,
    preview: WorkspaceFilesPreview,
  ): void {
    this.#previews.set(this.#revisionKey(nodeId, revisionId), preview);
  }

  public failNext(
    operation: WorkspaceFilesV1Operation,
    error: WorkspaceFilesError,
  ): void {
    this.#failures.set(operation, error);
  }

  public async snapshot(): Promise<WorkspaceFilesSnapshot> {
    this.#fail("snapshot");
    return this.#snapshot;
  }

  public subscribe(listener: WorkspaceFilesSnapshotListener): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  public async selectRoot(): Promise<WorkspaceFilesSnapshot> {
    return this.#changeRoot("select-root", "healthy");
  }

  public async replaceRoot(): Promise<WorkspaceFilesSnapshot> {
    return this.#changeRoot("replace-root", "healthy");
  }

  public async repairRoot(): Promise<WorkspaceFilesSnapshot> {
    return this.#changeRoot("repair-root", "healthy");
  }

  public async unbindRoot(): Promise<WorkspaceFilesSnapshot> {
    return this.#changeRoot("unbind-root", "unbound");
  }

  public async openMarkdown(
    nodeId: string,
    revisionId: string,
  ): Promise<WorkspaceFilesMarkdownRevision> {
    this.#fail("open-markdown");
    const revision = this.#revisions.get(this.#revisionKey(nodeId, revisionId));
    if (!revision) throw workspaceFilesError("missing-revision");
    return revision;
  }

  public async openPreview(
    nodeId: string,
    revisionId: string,
  ): Promise<WorkspaceFilesPreview> {
    this.#fail("open-preview");
    const preview = this.#previews.get(this.#revisionKey(nodeId, revisionId));
    if (!preview) throw workspaceFilesError("missing-revision");
    return preview;
  }

  public async createMarkdown(
    parentNodeId: string,
    name: string,
    markdown: string,
  ): Promise<WorkspaceFilesMarkdownRevision> {
    this.#fail("create-markdown");
    this.#validateMarkdown(name, markdown);
    const revision: WorkspaceFilesMarkdownRevision = {
      nodeId: `memory-node-${++this.#revisionSequence}`,
      revisionId: `memory-revision-${this.#revisionSequence}`,
      markdown,
    };
    this.#revisions.set(
      this.#revisionKey(revision.nodeId, revision.revisionId),
      revision,
    );
    this.#snapshot = {
      ...this.#snapshot,
      entries: [
        ...this.#snapshot.entries,
        {
          nodeId: revision.nodeId,
          parentNodeId,
          name,
          kind: "markdown",
          currentRevisionId: revision.revisionId,
          editable: true,
        },
      ],
    };
    this.#emit();
    return revision;
  }

  public async replaceMarkdown(
    nodeId: string,
    baseRevisionId: string,
    markdown: string,
  ): Promise<WorkspaceFilesMarkdownRevision> {
    this.#fail("replace-markdown");
    this.#validateMarkdown("file.md", markdown);
    const entry = this.#snapshot.entries.find(
      (candidate) => candidate.nodeId === nodeId,
    );
    if (!entry) throw workspaceFilesError("missing-revision");
    if (entry.currentRevisionId !== baseRevisionId) {
      throw workspaceFilesError("stale-revision");
    }
    const revision: WorkspaceFilesMarkdownRevision = {
      nodeId,
      revisionId: `memory-revision-${++this.#revisionSequence}`,
      markdown,
    };
    this.#revisions.set(
      this.#revisionKey(nodeId, revision.revisionId),
      revision,
    );
    this.#snapshot = {
      ...this.#snapshot,
      entries: this.#snapshot.entries.map((candidate) =>
        candidate.nodeId === nodeId
          ? { ...candidate, currentRevisionId: revision.revisionId }
          : candidate,
      ),
    };
    this.#emit();
    return revision;
  }

  public async resolveConflict(
    recordId: string,
    chosenCandidateId: string | null,
  ): Promise<WorkspaceFilesSnapshot> {
    this.#fail("resolve-conflict");
    const conflict = this.#snapshot.conflicts.find(
      (candidate) => candidate.recordId === recordId,
    );
    if (!conflict) throw workspaceFilesError("changed-conflict-choice");
    const candidates = new Set([
      ...conflict.resolutionCandidateIds,
      ...conflict.treeChoices.map(({ candidateId }) => candidateId),
    ]);
    if (chosenCandidateId !== null && !candidates.has(chosenCandidateId)) {
      throw workspaceFilesError("changed-conflict-choice");
    }
    this.#snapshot = {
      ...this.#snapshot,
      conflicts: this.#snapshot.conflicts.filter(
        (candidate) => candidate.recordId !== recordId,
      ),
    };
    this.#emit();
    return this.#snapshot;
  }

  async #changeRoot(
    operation: WorkspaceFilesV1Operation,
    state: WorkspaceFilesSnapshot["root"]["state"],
  ): Promise<WorkspaceFilesSnapshot> {
    this.#fail(operation);
    this.#snapshot = { ...this.#snapshot, root: { state } };
    this.#emit();
    return this.#snapshot;
  }

  #validateMarkdown(name: string, markdown: string): void {
    if (!name.toLocaleLowerCase().endsWith(".md")) {
      throw workspaceFilesError("invalid-markdown-name");
    }
    if (new TextEncoder().encode(markdown).byteLength > 1_048_576) {
      throw workspaceFilesError("size-limit");
    }
  }

  #fail(operation: WorkspaceFilesV1Operation): void {
    const error = this.#failures.get(operation);
    if (!error) return;
    this.#failures.delete(operation);
    throw error;
  }

  #emit(): void {
    for (const listener of this.#listeners) listener(this.#snapshot);
  }

  #revisionKey(nodeId: string, revisionId: string): string {
    return `${nodeId}\u0000${revisionId}`;
  }
}
