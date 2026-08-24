import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

import { describe, expect, it, vi } from "vitest";

import type { WorkspaceFilesSnapshot } from "@resonance/package-sdk";
import { InMemoryWorkspaceFilesV1 } from "../../sdk/src/testing/in-memory-workspace-files-v1.js";
import { workspaceFilesError } from "../../sdk/src/workspace-files-v1.js";

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
      {
        nodeId: "notes",
        parentNodeId: "plans",
        name: "notes.md",
        kind: "markdown",
        currentRevisionId: "revision-1",
        editable: true,
      },
    ],
    conflicts: [
      {
        recordId: "conflict",
        nodeId: "notes",
        kind: "markdown-overlap",
        competingRevisionIds: ["revision-1", "revision-2"],
        resolutionCandidateIds: ["revision-1", "revision-2"],
        reviewableRevisionIds: ["revision-1", "revision-2"],
        deletionOperationId: null,
        treeChoices: [],
      },
    ],
  };
}

describe("workspace-files package capability behavior", () => {
  it("covers root actions, create, edit, stale protection, and invalidation", async () => {
    const adapter = new InMemoryWorkspaceFilesV1(snapshot());
    const invalidated = vi.fn();
    adapter.subscribe(invalidated);
    await adapter.selectRoot();
    await adapter.repairRoot();
    await adapter.replaceRoot();
    const created = await adapter.createMarkdown(
      "plans",
      "created.md",
      "created\n",
    );
    const replaced = await adapter.replaceMarkdown(
      created.nodeId,
      created.revisionId,
      "updated\n",
    );
    await expect(
      adapter.replaceMarkdown(created.nodeId, created.revisionId, "stale\n"),
    ).rejects.toEqual(workspaceFilesError("stale-revision"));
    await adapter.unbindRoot();

    expect(replaced.markdown).toBe("updated\n");
    expect(invalidated).toHaveBeenCalledTimes(6);
    adapter.setSnapshot({ ...snapshot(), root: { state: "unavailable" } });
    expect(invalidated).toHaveBeenLastCalledWith(
      expect.objectContaining({ root: { state: "unavailable" } }),
    );
  });

  it("opens Markdown, image and unavailable previews, and resolves choices", async () => {
    const adapter = new InMemoryWorkspaceFilesV1(snapshot());
    adapter.setMarkdownRevision({
      nodeId: "notes",
      revisionId: "revision-2",
      markdown: "peer edit\n",
    });
    adapter.setPreview("image", "image-revision", {
      kind: "image",
      mimeType: "image/png",
      bytes: [137, 80],
      byteLength: 2,
    });
    adapter.setPreview("archive", "archive-revision", {
      kind: "unavailable",
      mimeType: "application/octet-stream",
      bytes: [],
      byteLength: 42,
    });

    await expect(
      adapter.openMarkdown("notes", "revision-2"),
    ).resolves.toMatchObject({ markdown: "peer edit\n" });
    await expect(
      adapter.openPreview("image", "image-revision"),
    ).resolves.toMatchObject({ kind: "image" });
    await expect(
      adapter.openPreview("archive", "archive-revision"),
    ).resolves.toMatchObject({ kind: "unavailable" });
    await expect(
      adapter.resolveConflict("conflict", "revision-2"),
    ).resolves.toMatchObject({ conflicts: [] });
  });

  it("owns cleanup and imports no desktop or Tauri implementation", async () => {
    const [entry, source] = await Promise.all([
      readFile(resolve("packages/workspace-files/src/index.ts"), "utf8"),
      readFile(
        resolve("packages/workspace-files/src/workspace-files-package.ts"),
        "utf8",
      ),
    ]);
    const combined = `${entry}\n${source}`;

    expect(entry).toContain("context.capabilities.workspaceFilesV1");
    expect(combined).not.toContain("@tauri-apps/api");
    expect(combined).not.toContain("apps/desktop");
    expect(source).toContain("this.#mountedMarkdown?.instance.destroy()");
    expect(source).toContain("URL.revokeObjectURL");
    expect(source).toContain("this.#unsubscribe?.()");
    expect(source).toContain("clearTimeout");
    expect(source).toContain('removeEventListener("submit"');
    expect(source).toContain('removeEventListener("click"');
  });
});
