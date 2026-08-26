import { readFile } from "node:fs/promises";

import { describe, expect, it } from "vitest";

import {
  isWorkspaceShellView,
  localMemberRole,
  peerStatus,
  workspaceViewChanged,
  type WorkspaceShellView,
} from "../apps/desktop/src/workspace-view.js";

function readyView(revision = 1): WorkspaceShellView {
  return {
    revision,
    state: "ready",
    message: null,
    workspace: {
      id: "workspace-id",
      displayName: "Team Resonance",
      lifecycle: "ready",
    },
    localPublicIdentity: "public-id",
    members: [
      {
        publicIdentity: "public-id",
        displayName: "Local member",
        role: "developer",
      },
    ],
    peers: [
      {
        publicIdentity: "peer-id",
        displayName: "Ada",
        online: true,
        connection: "relayed",
      },
    ],
  };
}

describe("workspace shell view", () => {
  it("accepts every strict shared fixture", async () => {
    const fixtures = JSON.parse(
      await readFile(
        "apps/desktop/schema/fixtures/workspace-shell-view.v1.valid.json",
        "utf8",
      ),
    ) as unknown[];
    expect(fixtures.every(isWorkspaceShellView)).toBe(true);
  });

  it("rejects every invalid shared fixture", async () => {
    const fixtures = JSON.parse(
      await readFile(
        "apps/desktop/schema/fixtures/workspace-shell-view.v1.invalid.json",
        "utf8",
      ),
    ) as Array<{ name: string; value: unknown }>;
    for (const fixture of fixtures) {
      expect(isWorkspaceShellView(fixture.value), fixture.name).toBe(false);
    }
  });

  it("accepts only increasing revisions", () => {
    expect(workspaceViewChanged(null, readyView(1))).toBe(true);
    expect(workspaceViewChanged(readyView(2), readyView(2))).toBe(false);
    expect(workspaceViewChanged(readyView(2), readyView(1))).toBe(false);
    expect(workspaceViewChanged(readyView(2), readyView(3))).toBe(true);
  });

  it("finds the canonical local role and fails closed for unknown roles", () => {
    expect(localMemberRole(readyView())).toBe("developer");
    expect(
      localMemberRole({
        ...readyView(),
        members: [{ ...readyView().members[0]!, role: "owner" }],
      }),
    ).toBeNull();
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
