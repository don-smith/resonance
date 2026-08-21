import { execFile } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { promisify } from "node:util";

import { describe, expect, it } from "vitest";

import { validateManifest } from "./index.js";

async function fixture(path: string): Promise<unknown> {
  return JSON.parse(
    await readFile(resolve("packages/contracts/fixtures", path), "utf8"),
  ) as unknown;
}

const execute = promisify(execFile);

describe("package manifest v1", () => {
  it("accepts the shared valid conformance fixture", async () => {
    const result = validateManifest(
      await fixture("valid/reference-manifest.json"),
    );

    expect(result.diagnostics).toEqual([]);
    expect("manifest" in result && result.manifest.id).toBe(
      "resonance.reference",
    );
  });

  it("generates a manifest that validates through the author adapter", async () => {
    const output = await mkdtemp(resolve(tmpdir(), "resonance-package-"));
    try {
      await execute("node", [
        "packages/contracts/scripts/generate.mjs",
        "--id",
        "resonance.generated",
        "--output",
        output,
      ]);
      expect(
        validateManifest(
          JSON.parse(await readFile(resolve(output, "manifest.json"), "utf8")),
        ),
      ).toMatchObject({
        diagnostics: [],
      });
    } finally {
      await rm(output, { recursive: true, force: true });
    }
  });

  it.each([
    ["invalid/placeholder-source.json", "/source"],
    ["invalid/unknown-permission.json", "/agent/permissions/0"],
  ])("reports an actionable diagnostic for %s", async (path, expectedPath) => {
    const result = validateManifest(await fixture(path));

    expect(result.diagnostics).not.toEqual([]);
    expect(result.diagnostics[0]?.path).toBe(expectedPath);
  });
});
