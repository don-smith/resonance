import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

describe("workspace shell", () => {
  it("keeps onboarding and presence around package-neutral content", async () => {
    const [shell, filesPackage] = await Promise.all([
      readFile(resolve("apps/desktop/src/main.ts"), "utf8"),
      readFile(
        resolve("packages/workspace-files/src/workspace-files-package.ts"),
        "utf8",
      ),
    ]);

    expect(shell).toContain("Runtime navigation");
    expect(shell).toContain("PackageHost");
    expect(shell).toContain("bundledPackageCatalog");
    expect(shell).toContain("packageMountRoot");
    expect(shell).toContain("Create a workspace");
    expect(shell).toContain("join_workspace");
    expect(shell).toContain("workspace:changed");
    expect(shell).toContain("isWorkspaceShellView");
    expect(shell).not.toContain("open_markdown_file");
    expect(shell).not.toContain("replace_markdown_file");
    expect(shell).not.toContain("files-panel");
    expect(filesPackage).toContain("Review latest");
    expect(filesPackage).toContain("Return to draft");
    expect(filesPackage).toContain("Load latest and replace draft");
  });
});
