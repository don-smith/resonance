import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

describe("workspace shell", () => {
  it("renders onboarding and validated workspace presence updates", async () => {
    const source = await readFile(resolve("apps/desktop/src/main.ts"), "utf8");

    expect(source).toContain("Runtime navigation");
    expect(source).toContain("Create a workspace");
    expect(source).toContain("join_workspace");
    expect(source).toContain("workspace:changed");
    expect(source).toContain("isWorkspaceShellView");
    expect(source).not.toContain("agent execution");
    expect(source).not.toContain("conversation");
  });
});
