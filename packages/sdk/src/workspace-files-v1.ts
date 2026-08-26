export type {
  WorkspaceFilesConflict,
  WorkspaceFilesConflictChoice,
  WorkspaceFilesEntry,
  WorkspaceFilesError,
  WorkspaceFilesMarkdownRevision,
  WorkspaceFilesPreview,
  WorkspaceFilesRootState,
  WorkspaceFilesSnapshot,
  WorkspaceFilesV1Operation,
} from "@resonance/contracts";

import {
  workspaceFilesErrorMessages,
  type WorkspaceFilesError,
  type WorkspaceFilesMarkdownRevision,
  type WorkspaceFilesPreview,
  type WorkspaceFilesSnapshot,
  type WorkspaceFilesV1Operation,
} from "@resonance/contracts";

export type WorkspaceFilesSnapshotListener = (
  snapshot: WorkspaceFilesSnapshot,
) => void;

export interface WorkspaceFilesV1 {
  snapshot(): Promise<WorkspaceFilesSnapshot>;
  subscribe(listener: WorkspaceFilesSnapshotListener): () => void;
  selectRoot(): Promise<WorkspaceFilesSnapshot>;
  replaceRoot(): Promise<WorkspaceFilesSnapshot>;
  repairRoot(): Promise<WorkspaceFilesSnapshot>;
  unbindRoot(): Promise<WorkspaceFilesSnapshot>;
  openMarkdown(
    nodeId: string,
    revisionId: string,
  ): Promise<WorkspaceFilesMarkdownRevision>;
  openPreview(
    nodeId: string,
    revisionId: string,
  ): Promise<WorkspaceFilesPreview>;
  createMarkdown(
    parentNodeId: string,
    name: string,
    markdown: string,
  ): Promise<WorkspaceFilesMarkdownRevision>;
  replaceMarkdown(
    nodeId: string,
    baseRevisionId: string,
    markdown: string,
  ): Promise<WorkspaceFilesMarkdownRevision>;
  resolveConflict(
    recordId: string,
    chosenCandidateId: string | null,
  ): Promise<WorkspaceFilesSnapshot>;
}

export function workspaceFilesError(
  code: WorkspaceFilesError["code"],
): WorkspaceFilesError {
  return {
    code,
    message: workspaceFilesErrorMessages[code],
  } as WorkspaceFilesError;
}

export type WorkspaceFilesFailureController = {
  failNext(
    operation: WorkspaceFilesV1Operation,
    error: WorkspaceFilesError,
  ): void;
};
