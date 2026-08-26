import type {
  WorkspaceFilesSnapshot,
  WorkspaceFilesSnapshotListener,
} from "@resonance/package-sdk";
import { immutableClone } from "@resonance/package-sdk";

export type SnapshotDiagnostics = (error: unknown) => void;

export class SnapshotPublisher {
  readonly #listeners = new Set<WorkspaceFilesSnapshotListener>();
  readonly #onListenerError: SnapshotDiagnostics;
  #latest: WorkspaceFilesSnapshot | null = null;

  public constructor(onListenerError: SnapshotDiagnostics = () => undefined) {
    this.#onListenerError = onListenerError;
  }

  public get latest(): WorkspaceFilesSnapshot | null {
    return this.#latest;
  }

  public subscribe(listener: WorkspaceFilesSnapshotListener): () => void {
    this.#listeners.add(listener);
    if (this.#latest) this.#notify(listener, this.#latest);
    return () => this.#listeners.delete(listener);
  }

  public publish(snapshot: WorkspaceFilesSnapshot): WorkspaceFilesSnapshot {
    this.#latest = immutableClone(snapshot);
    for (const listener of this.#listeners)
      this.#notify(listener, this.#latest);
    return this.#latest;
  }

  public clear(): void {
    this.#listeners.clear();
    this.#latest = null;
  }

  #notify(
    listener: WorkspaceFilesSnapshotListener,
    snapshot: WorkspaceFilesSnapshot,
  ): void {
    try {
      listener(snapshot);
    } catch (error) {
      this.#onListenerError(error);
    }
  }
}
