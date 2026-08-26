import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { validateManifest } from "../src/index.js";

async function fixture(path: string): Promise<unknown> {
  return JSON.parse(
    await readFile(
      resolve("packages/contracts/fixtures/manifest-v2", path),
      "utf8",
    ),
  ) as unknown;
}

describe("package manifest v2", () => {
  it("returns a tagged valid result for the shared fixture", async () => {
    const result = validateManifest(
      await fixture("valid/reference-manifest.json"),
    );

    expect(result.kind).toBe("valid");
    if (result.kind !== "valid") throw new Error("fixture must be valid");
    expect(result.manifest).toMatchObject({
      manifestVersion: 2,
      source: "bundled",
      id: "resonance.reference",
      content: { entry: "src/index.ts" },
    });
    expect(result.manifest.events.emits).toContain("peer:connection");
    expect(result.manifest.capabilities).toContain("workspace-files:v1");
  });

  it.each([
    ["invalid/placeholder-source.json", "/source"],
    ["invalid/unknown-permission.json", "/agent/permissions/0"],
    ["invalid/traversing-entry.json", "/content/entry"],
    ["invalid/windows-absolute-entry.json", "/content/entry"],
    ["invalid/backslash-traversing-entry.json", "/content/entry"],
  ])("reports an actionable diagnostic for %s", async (path, expectedPath) => {
    const result = validateManifest(await fixture(path));

    expect(result.kind).toBe("invalid");
    if (result.kind !== "invalid") throw new Error("fixture must be invalid");
    expect(result.diagnostics[0]?.path).toBe(expectedPath);
  });
});
