import { describe, expect, it } from "vitest";

import {
  isFilePreviewView,
  isMarkdownRevisionView,
  isWorkspaceShellView,
  peerStatus,
  workspaceViewChanged,
  type ConflictView,
  type WorkspaceShellView,
} from "../apps/desktop/src/workspace-view.js";
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
} from "../apps/desktop/src/workspace-files-view.js";

function readyView(): WorkspaceShellView {
  return {
    state: "ready",
    message: null,
    workspace: null,
    localPublicIdentity: "public-id",
    members: [],
    peers: [
      {
        publicIdentity: "peer-id",
        displayName: "Ada",
        online: true,
        connection: "relayed",
      },
    ],
    files: {
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
    },
  };
}

describe("workspace shell view", () => {
  it("accepts bounded file, root, and peer state", () => {
    expect(isWorkspaceShellView(readyView())).toBe(true);
    expect(
      isMarkdownRevisionView({
        nodeId: "file",
        revisionId: "revision",
        markdown: "# Roadmap\n",
      }),
    ).toBe(true);
  });

  it("rejects malformed private-root and file state", () => {
    const view = readyView() as unknown as Record<string, unknown>;
    view.files = {
      root: { state: "healthy", path: "/private/root" },
      entries: [{ nodeId: "missing-fields" }],
      conflicts: [],
    };
    expect(isWorkspaceShellView(view)).toBe(false);
  });

  it("orders directories before files and describes root health", () => {
    const files = readyView().files;
    expect(files).not.toBeNull();
    expect(
      childEntries(files!.entries, null).map((entry) => entry.name),
    ).toEqual(["plans"]);
    expect(rootStatusMessage("unavailable")).toContain("unavailable");
    expect(conflictLabel("markdown-overlap")).toBe(
      "Overlapping Markdown edits",
    );
    const deleteConflict: ConflictView = {
      recordId: "conflict",
      nodeId: "file",
      kind: "delete-edit",
      competingRevisionIds: ["edited-revision"],
      resolutionCandidateIds: ["edited-revision"],
      reviewableRevisionIds: ["edited-revision"],
      deletionOperationId: "delete-operation",
      treeChoices: [],
    };
    expect(conflictFallbackLabel(deleteConflict.kind)).toBe("Keep deletion");
    expect(conflictFallbackSelection(deleteConflict)).toBe("delete-operation");
    expect(
      conflictRevisionActionLabel(deleteConflict.kind, "edited-revision"),
    ).toBe("Keep edited file edited-r");
    expect(conflictFallbackLabel("binary-collision")).toBe(
      "Keep current state",
    );
    expect(conflictFallbackLabel("competing-move")).toBe(
      "Keep current location",
    );
    expect(
      conflictRevisionActionLabel("competing-move", "move-operation"),
    ).toBe("Use move move-ope");
    const createChoices: ConflictView["treeChoices"] = [
      {
        candidateId: "file-operation",
        nodeId: "file-node",
        kind: "file",
        selected: true,
        name: "tree-collision",
        targetPath: null,
      },
      {
        candidateId: "directory-operation",
        nodeId: "directory-node",
        kind: "directory",
        selected: false,
        name: "tree-collision",
        targetPath: null,
      },
    ];
    expect(createChoices.map(treeConflictActionLabel)).toEqual([
      "Keep current file: tree-collision",
      "Use competing folder: tree-collision",
    ]);
    expect(createChoices.map(treeConflictPreviewLabel)).toEqual([
      "Preview file",
      "Preview folder",
    ]);
    expect(createChoices).toHaveLength(2);
    expect(
      isFilePreviewView({
        kind: "image",
        mimeType: "image/png",
        bytes: [137, 80, 78, 71],
        byteLength: 4,
      }),
    ).toBe(true);
  });

  it("preserves clean and dirty editor sessions across shell updates", () => {
    const revision = {
      nodeId: "file",
      revisionId: "revision",
      markdown: "# Roadmap\n",
    };
    const files = readyView().files!;
    const clean = createMarkdownEditorSession(revision, false);

    expect(clean.draft).toBe(revision.markdown);
    expect(markdownRevisionAwareness(clean, files)).toEqual({
      state: "current",
    });

    const dirty = captureMarkdownDraft(clean, "# Roadmap\n\nUnsaved idea\n");
    const unrelatedUpdate = structuredClone(files);
    unrelatedUpdate.entries.push({
      nodeId: "other-file",
      parentNodeId: "plans",
      name: "notes.md",
      kind: "markdown",
      currentRevisionId: "other-revision",
      editable: true,
    });

    expect(dirty.draft).toBe("# Roadmap\n\nUnsaved idea\n");
    expect(markdownRevisionAwareness(dirty, unrelatedUpdate)).toEqual({
      state: "current",
    });
  });

  it("reports newer, deleted, and conflicted authority state", () => {
    const clean = createMarkdownEditorSession(
      {
        nodeId: "file",
        revisionId: "revision",
        markdown: "# Roadmap\n",
      },
      false,
    );
    const session = captureMarkdownDraft(clean, "# Roadmap\n\nUnsaved idea\n");
    const newer = structuredClone(readyView().files!);
    newer.entries[0].currentRevisionId = "next-revision";
    const stale = {
      state: "stale" as const,
      currentRevisionId: "next-revision",
    };
    expect(markdownRevisionAwareness(clean, newer)).toEqual(stale);
    expect(markdownRevisionAwareness(session, newer)).toEqual(stale);
    expect(session.draft).toBe("# Roadmap\n\nUnsaved idea\n");

    const deleted = structuredClone(newer);
    deleted.entries = deleted.entries.filter(
      (entry) => entry.nodeId !== session.loadedRevision.nodeId,
    );
    expect(markdownRevisionAwareness(session, deleted)).toEqual({
      state: "deleted",
    });

    const conflicted = structuredClone(newer);
    conflicted.conflicts.push({
      recordId: "conflict",
      nodeId: "file",
      kind: "markdown-overlap",
      competingRevisionIds: ["revision", "next-revision"],
      resolutionCandidateIds: ["revision", "next-revision"],
      reviewableRevisionIds: ["revision", "next-revision"],
      deletionOperationId: null,
      treeChoices: [],
    });
    expect(markdownRevisionAwareness(session, conflicted)).toEqual({
      state: "conflicted",
      conflictKind: "markdown-overlap",
    });
    expect(session.draft).toBe("# Roadmap\n\nUnsaved idea\n");
  });

  it("reviews and explicitly loads a revision without changing the draft", () => {
    const dirty = captureMarkdownDraft(
      createMarkdownEditorSession(
        {
          nodeId: "file",
          revisionId: "revision",
          markdown: "# Roadmap\n",
        },
        false,
      ),
      "# Roadmap\n\nUnsaved idea\n",
    );
    const reviewed = {
      nodeId: "file",
      revisionId: "next-revision",
      markdown: "# Roadmap\n\nPeer edit\n",
    };

    const reviewing = reviewMarkdownRevision(dirty, reviewed);
    expect(reviewing.mode).toBe("review");
    expect(reviewing.draft).toBe(dirty.draft);
    expect(reviewing.loadedRevision).toBe(dirty.loadedRevision);
    expect(reviewing.reviewedRevision).toBe(reviewed);

    const returned = returnToMarkdownDraft(reviewing);
    expect(returned.mode).toBe("draft");
    expect(returned.draft).toBe(dirty.draft);
    expect(returned.loadedRevision).toBe(dirty.loadedRevision);

    const loaded = loadReviewedMarkdownRevision(reviewing);
    expect(loaded.mode).toBe("draft");
    expect(loaded.draft).toBe(reviewed.markdown);
    expect(loaded.loadedRevision).toBe(reviewed);
    expect(loaded.reviewedRevision).toBeNull();
  });

  it("does not replace interactive UI for an identical transport view", () => {
    const current = readyView();
    const identical = structuredClone(current);
    expect(workspaceViewChanged(current, identical)).toBe(false);

    identical.files!.entries[0].currentRevisionId = "next-revision";
    expect(workspaceViewChanged(current, identical)).toBe(true);
  });

  it("renders an offline peer without a connection claim", () => {
    expect(
      peerStatus({
        publicIdentity: "peer-id",
        displayName: "Ada",
        online: false,
        connection: "direct",
      }),
    ).toBe("Offline");
  });
});
