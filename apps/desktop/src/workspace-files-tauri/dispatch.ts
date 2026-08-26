import {
  validateWorkspaceFilesError,
  validateWorkspaceFilesRequest,
  validateWorkspaceFilesResponse,
  type WorkspaceFilesRequest,
  type WorkspaceFilesResponse,
} from "@resonance/contracts";
import { workspaceFilesError } from "@resonance/package-sdk";

import type { WorkspaceFilesTransport } from "./transport.js";

export type WorkspaceFilesOperation = WorkspaceFilesRequest["operation"];
export type RequestFor<O extends WorkspaceFilesOperation> = Extract<
  WorkspaceFilesRequest,
  { operation: O }
>;
export type ResponseFor<O extends WorkspaceFilesOperation> = O extends
  | "snapshot"
  | "select-root"
  | "replace-root"
  | "repair-root"
  | "unbind-root"
  | "resolve-conflict"
  ? Extract<WorkspaceFilesResponse, { snapshot: unknown }>
  : O extends "open-preview"
    ? Extract<WorkspaceFilesResponse, { preview: unknown }>
    : Extract<WorkspaceFilesResponse, { revision: unknown }>;

export async function dispatchWorkspaceFiles<O extends WorkspaceFilesOperation>(
  transport: WorkspaceFilesTransport,
  request: RequestFor<O>,
): Promise<ResponseFor<O>> {
  if (validateWorkspaceFilesRequest(request).kind === "invalid") {
    throw workspaceFilesError("invalid-request");
  }
  let candidate: unknown;
  try {
    candidate = await transport.invoke("workspace_files_v1", { request });
  } catch (error) {
    throw safeWorkspaceFilesError(error);
  }
  const result = validateWorkspaceFilesResponse(candidate);
  if (
    result.kind === "invalid" ||
    result.value.operation !== request.operation
  ) {
    throw workspaceFilesError("internal");
  }
  return result.value as ResponseFor<O>;
}

export function safeWorkspaceFilesError(candidate: unknown) {
  const result = validateWorkspaceFilesError(candidate);
  return result.kind === "valid"
    ? result.value
    : workspaceFilesError("internal");
}
