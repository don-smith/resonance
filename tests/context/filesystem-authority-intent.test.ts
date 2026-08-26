import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

describe("filesystem authority intent", () => {
  it("keeps the accepted requirements and decision connected", async () => {
    const [rootRequirements, documentRequirements, decision] = await Promise.all([
      readFile(resolve("context/requirements.md"), "utf8"),
      readFile(resolve("context/02-system/03-documents/requirements.md"), "utf8"),
      readFile(resolve("context/.decisions/0009-filesystem-first-workspace-authority.md"), "utf8"),
    ]);

    expect(rootRequirements).toContain("**RS-R06 Planning workspace files preserve offline work.**");
    expect(documentRequirements).toContain("**RS.SYS.DOC-R01 Workspace files use signed operation authority.**");
    expect(documentRequirements).toContain("`refines: RS.SYS.TRNS-R04, RS.SYS.TRNS-R07`");
    expect(documentRequirements).not.toContain("Each document is a Yjs Y.Doc");
    expect(decision).toContain("Status: accepted");
  });
});
