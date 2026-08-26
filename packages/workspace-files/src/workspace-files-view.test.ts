import { describe, expect, it } from "vitest";

import type { WorkspaceFilesSnapshot } from "@resonance/package-sdk";

import {
  captureMarkdownDraft,
  childEntries,
  conflictFallbackLabel,
  conflictFallbackSelection,
  conflictLabel,
  conflictRevisionActionLabel,
  createMarkdownEditorSession,
  loadReviewedMarkdownRevision,
  markdownRevisionAwareness,
  returnToMarkdownDraft,
  reviewMarkdownRevision,
  rootStatusMessage,
  treeConflictActionLabel,
  treeConflictPreviewLabel,
} from "./workspace-files-view.js";
import type { ConflictView } from "./workspace-files-types.js";

function snapshot(): WorkspaceFilesSnapshot {
  return {
    root: { state: "healthy" },
    entries: [
      {
        nodeId: "file",
        parentNodeId: "plans",
        name: "roadmap.md",
        kind: "markdown",
        currentRevisionId: "revision",
        editable: true,
      },
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

describe("workspace-files view model", () => {
  it("orders the tree and labels root and conflict states", () => {
    const files = snapshot();
    expect(childEntries(files.entries, null).map(({ name }) => name)).toEqual([
      "plans",
    ]);
    expect(rootStatusMessage("unavailable")).toContain("unavailable");
    expect(conflictLabel("markdown-overlap")).toBe(
      "Overlapping Markdown edits",
    );
    const conflict: ConflictView = {
      recordId: "conflict",
      nodeId: "file",
      kind: "delete-edit",
      competingRevisionIds: ["edited-revision"],
      resolutionCandidateIds: ["edited-revision"],
      reviewableRevisionIds: ["edited-revision"],
      deletionOperationId: "delete-operation",
      treeChoices: [],
    };
    expect(conflictFallbackLabel(conflict.kind)).toBe("Keep deletion");
    expect(conflictFallbackSelection(conflict)).toBe("delete-operation");
    expect(conflictRevisionActionLabel(conflict.kind, "edited-revision")).toBe(
      "Keep edited file edited-r",
    );
  });

  it("labels every tree conflict choice", () => {
    const choices: ConflictView["treeChoices"] = [
      {
        candidateId: "file-operation",
        nodeId: "file-node",
        kind: "file",
        selected: true,
        name: "tree-collision",
        targetLocation: null,
      },
      {
        candidateId: "directory-operation",
        nodeId: "directory-node",
        kind: "directory",
        selected: false,
        name: "tree-collision",
        targetLocation: null,
      },
      {
        candidateId: "move-operation",
        nodeId: "file-node",
        kind: "move",
        selected: false,
        name: "roadmap.md",
        targetLocation: "archive/roadmap.md",
      },
    ];
    expect(choices.map(treeConflictActionLabel)).toEqual([
      "Keep current file: tree-collision",
      "Use competing folder: tree-collision",
      "Use location: archive/roadmap.md",
    ]);
    expect(choices.map(treeConflictPreviewLabel)).toEqual([
      "Preview file",
      "Preview folder",
      "Preview moved file",
    ]);
  });

  it("preserves dirty drafts across unrelated and newer snapshots", () => {
    const revision = {
      nodeId: "file",
      revisionId: "revision",
      markdown: "# Roadmap\n",
    };
    const dirty = captureMarkdownDraft(
      createMarkdownEditorSession(revision, false),
      "# Roadmap\n\nUnsaved idea\n",
    );
    const current = snapshot();
    const unrelated: WorkspaceFilesSnapshot = {
      ...current,
      entries: [
        ...current.entries,
        {
          nodeId: "other",
          parentNodeId: "plans",
          name: "notes.md",
          kind: "markdown",
          currentRevisionId: "other-revision",
          editable: true,
        },
      ],
    };
    expect(markdownRevisionAwareness(dirty, unrelated)).toEqual({
      state: "current",
    });
    const newer: WorkspaceFilesSnapshot = {
      ...unrelated,
      entries: unrelated.entries.map((entry) =>
        entry.nodeId === "file"
          ? { ...entry, currentRevisionId: "next-revision" }
          : entry,
      ),
    };
    expect(markdownRevisionAwareness(dirty, newer)).toEqual({
      state: "stale",
      currentRevisionId: "next-revision",
    });
    expect(dirty.draft).toContain("Unsaved idea");
  });

  it("reports deletion and conflict without changing the draft", () => {
    const dirty = captureMarkdownDraft(
      createMarkdownEditorSession(
        { nodeId: "file", revisionId: "revision", markdown: "base\n" },
        false,
      ),
      "draft\n",
    );
    const current = snapshot();
    const deleted: WorkspaceFilesSnapshot = {
      ...current,
      entries: current.entries.filter(({ nodeId }) => nodeId !== "file"),
    };
    expect(markdownRevisionAwareness(dirty, deleted)).toEqual({
      state: "deleted",
    });
    const conflicted: WorkspaceFilesSnapshot = {
      ...current,
      conflicts: [
        ...current.conflicts,
        {
          recordId: "conflict",
          nodeId: "file",
          kind: "markdown-overlap",
          competingRevisionIds: ["revision", "next"],
          resolutionCandidateIds: ["revision", "next"],
          reviewableRevisionIds: ["revision", "next"],
          deletionOperationId: null,
          treeChoices: [],
        },
      ],
    };
    expect(markdownRevisionAwareness(dirty, conflicted)).toEqual({
      state: "conflicted",
      conflictKind: "markdown-overlap",
    });
    expect(dirty.draft).toBe("draft\n");
  });

  it("reviews, returns, and explicitly loads without an implicit draft reset", () => {
    const dirty = captureMarkdownDraft(
      createMarkdownEditorSession(
        { nodeId: "file", revisionId: "revision", markdown: "base\n" },
        false,
      ),
      "draft\n",
    );
    const latest = {
      nodeId: "file",
      revisionId: "next",
      markdown: "peer\n",
    };
    const reviewing = reviewMarkdownRevision(dirty, latest);
    expect(reviewing.kind).toBe("review");
    expect(reviewing.draft).toBe("draft\n");
    expect(returnToMarkdownDraft(reviewing)).toEqual({
      kind: "draft",
      loadedRevision: dirty.loadedRevision,
      draft: "draft\n",
      readOnly: false,
    });
    expect(loadReviewedMarkdownRevision(reviewing)).toEqual({
      kind: "draft",
      draft: "peer\n",
      loadedRevision: latest,
      readOnly: false,
    });
  });
});
