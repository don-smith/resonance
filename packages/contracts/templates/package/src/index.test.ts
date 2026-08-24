import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

describe("__PACKAGE_ID__ lifecycle", () => {
  it("declares the retained mount lifecycle", async () => {
    const source = await readFile(
      resolve("__PACKAGE_OUTPUT__/src/index.ts"),
      "utf8",
    );
    expect(source).toContain("mount");
    expect(source).toContain("activate");
    expect(source).toContain("deactivate");
    expect(source).toContain("dispose");
  });
});
