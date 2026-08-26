import { readFile } from "node:fs/promises";

import { describe, expect, it } from "vitest";

async function jsonVersion(path: string): Promise<string> {
  const value = JSON.parse(await readFile(path, "utf8")) as {
    version?: unknown;
  };
  expect(value.version, `${path} must declare a version`).toEqual(
    expect.any(String),
  );
  return value.version as string;
}

function cargoWorkspaceVersion(source: string): string | undefined {
  return source.match(
    /\[workspace\.package\][\s\S]*?\nversion\s*=\s*"([^"]+)"/,
  )?.[1];
}

describe("product version", () => {
  it("uses the root package version in every shipped manifest", async () => {
    const authoritative = await jsonVersion("package.json");
    const copies = await Promise.all([
      jsonVersion("apps/desktop/package.json"),
      jsonVersion("apps/desktop/src-tauri/tauri.conf.json"),
      jsonVersion("packages/contracts/package.json"),
      jsonVersion("packages/sdk/package.json"),
      jsonVersion("packages/reference-package/package.json"),
      jsonVersion("packages/workspace-files/package.json"),
    ]);
    const cargo = cargoWorkspaceVersion(await readFile("Cargo.toml", "utf8"));

    expect(authoritative).toMatch(/^\d+\.\d+\.\d+$/);
    expect(cargo).toBe(authoritative);
    expect(copies).toEqual(copies.map(() => authoritative));
  });
});
