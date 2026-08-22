import { describe, expect, it } from "vitest";

import {
  isWorkspaceShellView,
  peerStatus,
} from "../apps/desktop/src/workspace-view.js";

describe("workspace shell view", () => {
  it("accepts secret-free peer connection updates", () => {
    expect(
      isWorkspaceShellView({
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
      }),
    ).toBe(true);
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
