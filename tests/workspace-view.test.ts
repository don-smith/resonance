import { describe, expect, it } from "vitest";

import {
  isMarkdownRevisionView,
  isWorkspaceShellView,
  peerStatus,
  workspaceViewChanged,
  type ConflictView,
  type WorkspaceShellView,
} from "../apps/desktop/src/workspace-view.js";
import {
  childEntries,
  conflictFallbackLabel,
  conflictFallbackSelection,
  conflictLabel,
  conflictRevisionActionLabel,
  retainedOpenRevision,
  rootStatusMessage,
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
      reviewableRevisionIds: ["edited-revision"],
      deletionOperationId: "delete-operation",
    };
    expect(conflictFallbackLabel(deleteConflict.kind)).toBe("Keep deletion");
    expect(conflictFallbackSelection(deleteConflict)).toBe("delete-operation");
    expect(
      conflictRevisionActionLabel(deleteConflict.kind, "edited-revision"),
    ).toBe("Keep edited file edited-r");
    expect(conflictFallbackLabel("binary-collision")).toBe(
      "Keep current state",
    );
  });

  it("clears an open revision only when its node leaves the tree", () => {
    const revision = {
      nodeId: "file",
      revisionId: "revision",
      markdown: "# Roadmap\n",
    };
    const entries = readyView().files!.entries;

    expect(retainedOpenRevision(revision, entries)).toBe(revision);
    const newerEntries = structuredClone(entries);
    newerEntries[0].currentRevisionId = "next-revision";
    expect(retainedOpenRevision(revision, newerEntries)).toBe(revision);
    expect(
      retainedOpenRevision(
        revision,
        entries.filter((entry) => entry.nodeId !== revision.nodeId),
      ),
    ).toBeNull();
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
