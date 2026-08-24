import { execFile } from "node:child_process";
import {
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { promisify } from "node:util";

import { describe, expect, it } from "vitest";

import { validateManifest } from "./index.js";

async function fixture(path: string): Promise<unknown> {
  return JSON.parse(
    await readFile(
      resolve("packages/contracts/fixtures/manifest-v2", path),
      "utf8",
    ),
  ) as unknown;
}

const execute = promisify(execFile);

async function write(path: string, content: string): Promise<void> {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, content);
}

async function filesBelow(directory: string, prefix = ""): Promise<string[]> {
  const entries = await readdir(directory, { withFileTypes: true });
  const files: string[] = [];
  for (const entry of entries.sort((left, right) =>
    left.name.localeCompare(right.name),
  )) {
    const path = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) {
      files.push(...(await filesBelow(resolve(directory, entry.name), path)));
    } else if (entry.isFile()) {
      files.push(path);
    }
  }
  return files;
}

async function snapshotFiles(
  directory: string,
): Promise<Record<string, string>> {
  return Object.fromEntries(
    await Promise.all(
      (await filesBelow(directory)).map(async (path) => [
        path,
        await readFile(resolve(directory, path), "utf8"),
      ]),
    ),
  );
}

describe("package manifest v2", () => {
  it("accepts the shared valid conformance fixture", async () => {
    const result = validateManifest(
      await fixture("valid/reference-manifest.json"),
    );

    expect(result.diagnostics).toEqual([]);
    expect("manifest" in result && result.manifest).toMatchObject({
      manifestVersion: 2,
      source: "bundled",
      id: "resonance.reference",
      content: { entry: "src/index.ts" },
    });
    expect("manifest" in result && result.manifest.events.emits).toContain(
      "peer:connection",
    );
    expect("manifest" in result && result.manifest.capabilities).toContain(
      "workspace-files:v1",
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
        manifest: {
          manifestVersion: 2,
          source: "bundled",
          content: { entry: "src/index.ts" },
        },
      });
    } finally {
      await rm(output, { recursive: true, force: true });
    }
  });

  it("scaffolds deterministic compiling content without Rust edits", async () => {
    const root = await mkdtemp(resolve(tmpdir(), "resonance-scaffold-"));
    try {
      await write(
        resolve(root, "packages/contracts/schema/manifest.v2.json"),
        await readFile("packages/contracts/schema/manifest.v2.json", "utf8"),
      );
      await write(
        resolve(root, "packages/sdk/package.json"),
        JSON.stringify({
          name: "@resonance/package-sdk",
          type: "module",
          exports: { ".": "./src/index.ts" },
        }),
      );
      await write(
        resolve(root, "packages/sdk/src/index.ts"),
        `export type PackageContentModule = { mount(root: HTMLElement, context: { package: { name: string } }): { activate(): void; deactivate(): void; dispose(): void } };\n`,
      );
      const rustSource = "fn main() {}\n";
      await write(
        resolve(root, "apps/desktop/src-tauri/src/main.rs"),
        rustSource,
      );
      await write(
        resolve(root, "tsconfig.json"),
        JSON.stringify({
          compilerOptions: {
            target: "ES2022",
            module: "ESNext",
            moduleResolution: "bundler",
            strict: true,
            noEmit: true,
            baseUrl: ".",
            paths: {
              "@resonance/package-sdk": ["packages/sdk/src/index.ts"],
            },
          },
          include: [
            "packages/generated/src/index.ts",
            "packages/sdk/src/index.ts",
          ],
        }),
      );
      const output = resolve(root, "packages/generated");
      const command = [
        "packages/contracts/scripts/generate.mjs",
        "--id",
        "resonance.generated",
        "--output",
        output,
        "--root",
        root,
      ];
      await execute("node", command);
      const first = await snapshotFiles(root);
      await execute("node", command);
      expect(await snapshotFiles(root)).toEqual(first);

      const manifest = JSON.parse(
        await readFile(resolve(output, "manifest.json"), "utf8"),
      );
      expect(validateManifest(manifest).diagnostics).toEqual([]);
      const typescriptCatalog = await readFile(
        resolve(root, "apps/desktop/src/generated/bundled-package-catalog.ts"),
        "utf8",
      );
      expect(typescriptCatalog).toContain(
        'import("../../../../packages/generated/src/index")',
      );
      const rustCatalog = JSON.parse(
        await readFile(
          resolve(
            root,
            "apps/desktop/src-tauri/generated/bundled-package-manifests.json",
          ),
          "utf8",
        ),
      );
      expect(rustCatalog).toEqual([manifest]);
      expect(
        await readFile(
          resolve(root, "apps/desktop/src-tauri/src/main.rs"),
          "utf8",
        ),
      ).toBe(rustSource);
      expect(
        (await filesBelow(resolve(root, "apps/desktop/src-tauri/src"))).filter(
          (path) => path.endsWith(".rs"),
        ),
      ).toEqual(["main.rs"]);
      await execute("pnpm", [
        "exec",
        "tsc",
        "-p",
        resolve(root, "tsconfig.json"),
      ]);
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  it.each([
    ["invalid/placeholder-source.json", "/source"],
    ["invalid/unknown-permission.json", "/agent/permissions/0"],
    ["invalid/traversing-entry.json", "/content/entry"],
  ])("reports an actionable diagnostic for %s", async (path, expectedPath) => {
    const result = validateManifest(await fixture(path));

    expect(result.diagnostics).not.toEqual([]);
    expect(result.diagnostics[0]?.path).toBe(expectedPath);
  });
});
