import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import {
  validateWorkspaceFilesEnvelope,
  validateWorkspaceFilesError,
  validateWorkspaceFilesRequest,
  validateWorkspaceFilesResponse,
  workspaceFilesErrorMessages,
  workspaceFilesV1Operations,
  type WorkspaceFilesEnvelope,
} from "./workspace-files-v1.js";

async function fixture<T>(kind: "valid" | "invalid"): Promise<T> {
  return JSON.parse(
    await readFile(
      resolve(
        `packages/contracts/fixtures/workspace-files-v1/${kind}/corpus.json`,
      ),
      "utf8",
    ),
  ) as T;
}

describe("workspace-files v1 contract", () => {
  it("accepts every shared operation, result, enum, and safe error fixture", async () => {
    const corpus = await fixture<WorkspaceFilesEnvelope[]>("valid");
    for (const value of corpus) {
      expect(
        validateWorkspaceFilesEnvelope(value).diagnostics,
        JSON.stringify(value),
      ).toEqual([]);
    }

    const requests = new Set(
      corpus
        .filter((value) => value.kind === "request")
        .map((value) => value.value.operation),
    );
    const responses = new Set(
      corpus
        .filter((value) => value.kind === "response")
        .map((value) => value.value.operation),
    );
    expect(requests).toEqual(new Set(workspaceFilesV1Operations));
    expect(responses).toEqual(new Set(workspaceFilesV1Operations));
    expect(
      corpus
        .filter((value) => value.kind === "error")
        .map((value) => value.value.code)
        .sort(),
    ).toEqual(Object.keys(workspaceFilesErrorMessages).sort());
  });

  it("rejects every shared invalid and private-field fixture", async () => {
    const corpus =
      await fixture<Array<{ name: string; value: unknown }>>("invalid");
    for (const { name, value } of corpus) {
      expect(
        validateWorkspaceFilesEnvelope(value).diagnostics,
        name,
      ).not.toEqual([]);
    }
    expect(corpus.map(({ name }) => name)).toEqual(
      expect.arrayContaining([
        "path",
        "token",
        "private key",
        "blob location",
        "watcher state",
        "persistence details",
        "sql details",
        "signed operation",
        "iroh data",
      ]),
    );
  });

  it("enforces UTF-8 byte and collection limits", () => {
    expect(
      validateWorkspaceFilesRequest({
        operation: "create-markdown",
        parentNodeId: "plans",
        name: "large.md",
        markdown: "é".repeat(524_289),
      }).diagnostics,
    ).not.toEqual([]);

    expect(
      validateWorkspaceFilesResponse({
        operation: "snapshot",
        snapshot: {
          root: { state: "unbound" },
          entries: Array.from({ length: 10_001 }, (_, index) => ({
            nodeId: `node-${index}`,
            parentNodeId: null,
            name: "entry",
            kind: "directory",
            currentRevisionId: null,
            editable: true,
          })),
          conflicts: [],
        },
      }).diagnostics,
    ).not.toEqual([]);
  });

  it("validates request, response, and error values independently", () => {
    expect(
      validateWorkspaceFilesRequest({ operation: "snapshot" }).diagnostics,
    ).toEqual([]);
    expect(
      validateWorkspaceFilesResponse({
        operation: "snapshot",
        snapshot: { root: { state: "unbound" }, entries: [], conflicts: [] },
      }).diagnostics,
    ).toEqual([]);
    expect(
      validateWorkspaceFilesError({
        code: "internal",
        message: workspaceFilesErrorMessages.internal,
      }).diagnostics,
    ).toEqual([]);
  });
});
