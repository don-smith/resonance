import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

describe("runtime shell baseline", () => {
  it("exposes only navigation and default-workspace status", async () => {
    const source = await readFile(resolve("apps/desktop/src/main.ts"), "utf8");

    expect(source).toContain("Runtime navigation");
    expect(source).toContain("Local workspace ready");
    expect(source).not.toContain("agent execution");
    expect(source).not.toContain("conversation");
  });
});
