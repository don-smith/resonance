import { describe, expect, it, vi } from "vitest";

import type { WorkspaceFilesSnapshot } from "../../contracts/src/workspace-files-v1.js";
import { InMemoryWorkspaceFilesV1 } from "./testing/in-memory-workspace-files-v1.js";
import { workspaceFilesError } from "./workspace-files-v1.js";

function snapshot(): WorkspaceFilesSnapshot {
  return {
    root: { state: "unbound" },
    entries: [
      {
        nodeId: "plans",
        parentNodeId: null,
        name: "plans",
        kind: "directory",
        currentRevisionId: null,
        editable: true,
      },
    ],
    conflicts: [],
  };
}

describe("in-memory workspace-files v1", () => {
  it("publishes root, create, and replace snapshots", async () => {
    const adapter = new InMemoryWorkspaceFilesV1(snapshot());
    const listener = vi.fn();
    const unsubscribe = adapter.subscribe(listener);

    await expect(adapter.selectRoot()).resolves.toMatchObject({
      root: { state: "healthy" },
    });
    const created = await adapter.createMarkdown("plans", "notes.md", "one\n");
    const replaced = await adapter.replaceMarkdown(
      created.nodeId,
      created.revisionId,
      "two\n",
    );

    expect(replaced.markdown).toBe("two\n");
    expect((await adapter.snapshot()).entries).toContainEqual(
      expect.objectContaining({
        nodeId: created.nodeId,
        currentRevisionId: replaced.revisionId,
      }),
    );
    expect(listener).toHaveBeenCalledTimes(3);
    unsubscribe();
    await adapter.unbindRoot();
    expect(listener).toHaveBeenCalledTimes(3);
  });

  it("opens controlled revisions and previews", async () => {
    const adapter = new InMemoryWorkspaceFilesV1(snapshot());
    adapter.setMarkdownRevision({
      nodeId: "notes",
      revisionId: "revision-1",
      markdown: "# Notes\n",
    });
    adapter.setPreview("image", "revision-2", {
      kind: "image",
      mimeType: "image/png",
      bytes: [137, 80],
      byteLength: 2,
    });

    await expect(
      adapter.openMarkdown("notes", "revision-1"),
    ).resolves.toMatchObject({ markdown: "# Notes\n" });
    await expect(
      adapter.openPreview("image", "revision-2"),
    ).resolves.toMatchObject({
      kind: "image",
    });
  });

  it("provides controllable finite failures", async () => {
    const adapter = new InMemoryWorkspaceFilesV1(snapshot());
    adapter.failNext("snapshot", workspaceFilesError("internal"));

    await expect(adapter.snapshot()).rejects.toEqual(
      workspaceFilesError("internal"),
    );
    await expect(adapter.snapshot()).resolves.toEqual(snapshot());
    await expect(
      adapter.createMarkdown("plans", "not-markdown.txt", ""),
    ).rejects.toEqual(workspaceFilesError("invalid-markdown-name"));
  });
});
