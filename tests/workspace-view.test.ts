import { describe, expect, it } from "vitest";

import {
  isWorkspaceShellView,
  peerStatus,
  workspaceViewChanged,
  type WorkspaceShellView,
} from "../apps/desktop/src/workspace-view.js";

function readyView(): WorkspaceShellView {
  return {
    state: "ready",
    message: null,
    workspace: {
      id: "workspace-id",
      displayName: "Team Resonance",
      lifecycle: "ready",
    },
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
  };
}

describe("workspace shell view", () => {
  it("accepts package-neutral workspace and peer state", () => {
    expect(isWorkspaceShellView(readyView())).toBe(true);
  });

  it("rejects legacy file state and private values", () => {
    expect(isWorkspaceShellView({ ...readyView(), files: {} })).toBe(false);
    expect(
      isWorkspaceShellView({ ...readyView(), path: "/private/root" }),
    ).toBe(false);
  });

  it("does not replace interactive UI for an identical transport view", () => {
    const current = readyView();
    const identical = structuredClone(current);
    expect(workspaceViewChanged(current, identical)).toBe(false);
    identical.peers[0].online = false;
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
