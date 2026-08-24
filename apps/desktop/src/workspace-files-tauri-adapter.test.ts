import { describe, expect, it, vi } from "vitest";

import type {
  WorkspaceFilesRequest,
  WorkspaceFilesResponse,
  WorkspaceFilesSnapshot,
} from "../../../packages/contracts/src/workspace-files-v1.js";
import { workspaceFilesError } from "../../../packages/sdk/src/workspace-files-v1.js";
import { WorkspaceFilesTauriAdapter } from "./workspace-files-tauri-adapter.js";

function snapshot(
  state: WorkspaceFilesSnapshot["root"]["state"],
): WorkspaceFilesSnapshot {
  return { root: { state }, entries: [], conflicts: [] };
}

function response(
  state: WorkspaceFilesSnapshot["root"]["state"],
): WorkspaceFilesResponse {
  return { operation: "snapshot", snapshot: snapshot(state) };
}

function transport(
  invokeImplementation: (
    request: WorkspaceFilesRequest,
  ) => Promise<unknown> = async () => response("unbound"),
) {
  let eventHandler: ((event: { payload: unknown }) => void) | undefined;
  const unlisten = vi.fn();
  const invoke = vi.fn(
    async (_command: string, arguments_: { request: WorkspaceFilesRequest }) =>
      invokeImplementation(arguments_.request),
  );
  return {
    value: {
      invoke,
      listen: vi.fn(
        async (
          _eventName: string,
          handler: (event: { payload: unknown }) => void,
        ) => {
          eventHandler = handler;
          return unlisten;
        },
      ),
    },
    invoke,
    unlisten,
    emit: (payload: unknown) => eventHandler?.({ payload }),
  };
}

describe("workspace-files Tauri adapter", () => {
  it("uses one dispatcher and maps only contract-safe errors", async () => {
    const requests: WorkspaceFilesRequest[] = [];
    const fake = transport(async (request) => {
      requests.push(request);
      if (request.operation === "open-markdown") {
        throw workspaceFilesError("missing-revision");
      }
      return response("healthy");
    });
    const adapter = new WorkspaceFilesTauriAdapter(fake.value);
    await adapter.ready();

    await expect(adapter.snapshot()).resolves.toEqual(snapshot("healthy"));
    await expect(adapter.openMarkdown("node", "missing")).rejects.toEqual(
      workspaceFilesError("missing-revision"),
    );
    expect(requests).toEqual([
      { operation: "snapshot" },
      { operation: "open-markdown", nodeId: "node", revisionId: "missing" },
    ]);
    expect(fake.invoke).toHaveBeenCalledWith("workspace_files_v1", {
      request: { operation: "snapshot" },
    });
  });

  it("refreshes only for payload-free invalidation", async () => {
    const fake = transport();
    const adapter = new WorkspaceFilesTauriAdapter(fake.value);
    const listener = vi.fn();
    adapter.subscribe(listener);
    await adapter.ready();

    fake.emit({ leaked: "state" });
    await Promise.resolve();
    expect(fake.invoke).not.toHaveBeenCalled();

    fake.emit(null);
    await vi.waitFor(() =>
      expect(listener).toHaveBeenCalledWith(snapshot("unbound")),
    );
    expect(fake.invoke).toHaveBeenCalledOnce();
  });

  it("suppresses stale snapshot responses", async () => {
    const resolvers: Array<(value: unknown) => void> = [];
    const fake = transport(
      () =>
        new Promise((resolve) => {
          resolvers.push(resolve);
        }),
    );
    const adapter = new WorkspaceFilesTauriAdapter(fake.value);
    const listener = vi.fn();
    adapter.subscribe(listener);
    await adapter.ready();

    const first = adapter.snapshot();
    const second = adapter.snapshot();
    await vi.waitFor(() => expect(resolvers).toHaveLength(2));
    resolvers[1]?.(response("healthy"));
    await expect(second).resolves.toEqual(snapshot("healthy"));
    resolvers[0]?.(response("unavailable"));
    await expect(first).resolves.toEqual(snapshot("healthy"));
    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener).toHaveBeenCalledWith(snapshot("healthy"));
  });

  it("turns listener startup failure into an unavailable capability", async () => {
    const fake = transport();
    fake.value.listen = vi.fn(async () => {
      throw new Error("private listener detail");
    });
    const adapter = new WorkspaceFilesTauriAdapter(fake.value);

    await expect(adapter.ready()).rejects.toEqual(
      workspaceFilesError("unavailable-capability"),
    );
    await expect(adapter.snapshot()).rejects.toEqual(
      workspaceFilesError("unavailable-capability"),
    );
  });

  it("rejects unchecked responses and cleans up once", async () => {
    const fake = transport(async () => ({ path: "/private/root" }));
    const adapter = new WorkspaceFilesTauriAdapter(fake.value);
    await adapter.ready();

    await expect(adapter.snapshot()).rejects.toEqual(
      workspaceFilesError("internal"),
    );
    await adapter.dispose();
    await adapter.dispose();
    expect(fake.unlisten).toHaveBeenCalledOnce();
    await expect(adapter.snapshot()).rejects.toEqual(
      workspaceFilesError("unavailable-capability"),
    );
  });
});
