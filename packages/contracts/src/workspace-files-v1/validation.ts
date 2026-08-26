import Ajv2020, { type ErrorObject } from "ajv/dist/2020.js";

import schema from "../../schema/workspace-files.v1.json" with { type: "json" };
import type { WorkspaceFilesError } from "./errors.ts";
import type {
  WorkspaceFilesEnvelope,
  WorkspaceFilesRequest,
  WorkspaceFilesResponse,
  WorkspaceFilesSnapshot,
} from "./types.ts";

export type WorkspaceFilesDiagnostic = Readonly<{
  path: string;
  message: string;
}>;

export type WorkspaceFilesValidationResult<T> =
  | Readonly<{ kind: "valid"; value: T }>
  | Readonly<{
      kind: "invalid";
      diagnostics: WorkspaceFilesDiagnostic[];
    }>;

const validator = new Ajv2020({ allErrors: true, strict: true }).compile(
  schema,
);
const utf8 = new TextEncoder();

function schemaDiagnostics(
  errors: ErrorObject[] | null | undefined,
): WorkspaceFilesDiagnostic[] {
  return (errors ?? [])
    .map((error) => ({
      path: error.instancePath || "/",
      message: error.message ?? "is invalid",
    }))
    .sort(compareDiagnostics);
}

function compareDiagnostics(
  left: WorkspaceFilesDiagnostic,
  right: WorkspaceFilesDiagnostic,
): number {
  return `${left.path}:${left.message}`.localeCompare(
    `${right.path}:${right.message}`,
  );
}

function duplicateValues(values: readonly string[]): Set<string> {
  const seen = new Set<string>();
  const duplicates = new Set<string>();
  for (const value of values) {
    if (seen.has(value)) duplicates.add(value);
    seen.add(value);
  }
  return duplicates;
}

function snapshotDiagnostics(
  snapshot: WorkspaceFilesSnapshot,
  path: string,
): WorkspaceFilesDiagnostic[] {
  const diagnostics: WorkspaceFilesDiagnostic[] = [];
  const nodeIds = new Set(snapshot.entries.map(({ nodeId }) => nodeId));
  for (const duplicate of duplicateValues(
    snapshot.entries.map(({ nodeId }) => nodeId),
  )) {
    diagnostics.push({
      path: `${path}/entries`,
      message: `contains duplicate nodeId ${JSON.stringify(duplicate)}`,
    });
  }

  const roots = snapshot.entries.filter(
    ({ parentNodeId }) => parentNodeId === null,
  );
  if (snapshot.entries.length > 0 && roots.length !== 1) {
    diagnostics.push({
      path: `${path}/entries`,
      message: "must contain exactly one top-level entry",
    });
  }

  for (const [index, entry] of snapshot.entries.entries()) {
    if (entry.parentNodeId !== null && !nodeIds.has(entry.parentNodeId)) {
      diagnostics.push({
        path: `${path}/entries/${index}/parentNodeId`,
        message: "must reference an entry in the same snapshot",
      });
    }
  }

  const parents = new Map(
    snapshot.entries.map(({ nodeId, parentNodeId }) => [nodeId, parentNodeId]),
  );
  for (const [index, entry] of snapshot.entries.entries()) {
    const visited = new Set<string>();
    let nodeId: string | null | undefined = entry.nodeId;
    while (nodeId !== null && nodeId !== undefined) {
      if (visited.has(nodeId)) {
        diagnostics.push({
          path: `${path}/entries/${index}/parentNodeId`,
          message: "must not form a parent cycle",
        });
        break;
      }
      visited.add(nodeId);
      nodeId = parents.get(nodeId);
    }
  }

  for (const duplicate of duplicateValues(
    snapshot.conflicts.map(({ recordId }) => recordId),
  )) {
    diagnostics.push({
      path: `${path}/conflicts`,
      message: `contains duplicate recordId ${JSON.stringify(duplicate)}`,
    });
  }

  for (const [index, conflict] of snapshot.conflicts.entries()) {
    const conflictPath = `${path}/conflicts/${index}`;
    if (
      conflict.kind !== "concurrent-create" &&
      !nodeIds.has(conflict.nodeId)
    ) {
      diagnostics.push({
        path: `${conflictPath}/nodeId`,
        message: "must reference an entry in the same snapshot",
      });
    }

    const treeCandidateIds = conflict.treeChoices.map(
      ({ candidateId }) => candidateId,
    );
    if (duplicateValues(treeCandidateIds).size > 0) {
      diagnostics.push({
        path: `${conflictPath}/treeChoices`,
        message: "must use unique candidateId values",
      });
    }

    const knownCandidates = new Set([
      ...conflict.competingRevisionIds,
      ...treeCandidateIds,
      ...(conflict.deletionOperationId === null
        ? []
        : [conflict.deletionOperationId]),
    ]);
    for (const [
      candidateIndex,
      candidateId,
    ] of conflict.resolutionCandidateIds.entries()) {
      if (!knownCandidates.has(candidateId)) {
        diagnostics.push({
          path: `${conflictPath}/resolutionCandidateIds/${candidateIndex}`,
          message:
            "must reference a competing revision, tree choice, or deletion",
        });
      }
    }

    const competingRevisions = new Set(conflict.competingRevisionIds);
    for (const [
      revisionIndex,
      revisionId,
    ] of conflict.reviewableRevisionIds.entries()) {
      if (!competingRevisions.has(revisionId)) {
        diagnostics.push({
          path: `${conflictPath}/reviewableRevisionIds/${revisionIndex}`,
          message: "must reference a competing revision",
        });
      }
    }

    for (const [choiceIndex, choice] of conflict.treeChoices.entries()) {
      if (choice.kind === "move" && !nodeIds.has(choice.nodeId)) {
        diagnostics.push({
          path: `${conflictPath}/treeChoices/${choiceIndex}/nodeId`,
          message: "must reference an entry for a move choice",
        });
      }
    }
  }

  return diagnostics;
}

function semanticDiagnostics(
  envelope: WorkspaceFilesEnvelope,
): WorkspaceFilesDiagnostic[] {
  const diagnostics: WorkspaceFilesDiagnostic[] = [];
  if (envelope.kind === "request") {
    if (
      envelope.value.operation === "create-markdown" ||
      envelope.value.operation === "replace-markdown"
    ) {
      if (utf8.encode(envelope.value.markdown).byteLength > 1_048_576) {
        diagnostics.push({
          path: "/value/markdown",
          message: "exceeds 1048576 UTF-8 bytes",
        });
      }
    }
  } else if (envelope.kind === "response") {
    if ("revision" in envelope.value) {
      if (
        utf8.encode(envelope.value.revision.markdown).byteLength > 1_048_576
      ) {
        diagnostics.push({
          path: "/value/revision/markdown",
          message: "exceeds 1048576 UTF-8 bytes",
        });
      }
    } else if ("preview" in envelope.value) {
      const preview = envelope.value.preview;
      if (
        preview.kind === "image" &&
        preview.byteLength !== preview.bytes.length
      ) {
        diagnostics.push({
          path: "/value/preview/byteLength",
          message: "must equal the number of preview bytes",
        });
      }
    } else {
      diagnostics.push(
        ...snapshotDiagnostics(envelope.value.snapshot, "/value/snapshot"),
      );
    }
  }
  return diagnostics.sort(compareDiagnostics);
}

export function validateWorkspaceFilesEnvelope(
  candidate: unknown,
): WorkspaceFilesValidationResult<WorkspaceFilesEnvelope> {
  if (!validator(candidate)) {
    return {
      kind: "invalid",
      diagnostics: schemaDiagnostics(validator.errors),
    };
  }
  const value = candidate as WorkspaceFilesEnvelope;
  const diagnostics = semanticDiagnostics(value);
  return diagnostics.length > 0
    ? { kind: "invalid", diagnostics }
    : { kind: "valid", value };
}

function validateValue<T>(
  kind: WorkspaceFilesEnvelope["kind"],
  candidate: unknown,
): WorkspaceFilesValidationResult<T> {
  const result = validateWorkspaceFilesEnvelope({ kind, value: candidate });
  return result.kind === "valid"
    ? { kind: "valid", value: result.value.value as T }
    : result;
}

export function validateWorkspaceFilesRequest(
  candidate: unknown,
): WorkspaceFilesValidationResult<WorkspaceFilesRequest> {
  return validateValue("request", candidate);
}

export function validateWorkspaceFilesResponse(
  candidate: unknown,
): WorkspaceFilesValidationResult<WorkspaceFilesResponse> {
  return validateValue("response", candidate);
}

export function validateWorkspaceFilesError(
  candidate: unknown,
): WorkspaceFilesValidationResult<WorkspaceFilesError> {
  return validateValue("error", candidate);
}
