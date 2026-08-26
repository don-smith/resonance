import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { validateManifest } from "../src/index.js";
import {
  execute,
  filesBelow,
  snapshotFiles,
  write,
} from "./support/temp-repository.js";

describe("package scaffold", () => {
  it("generates a manifest that validates through the author adapter", async () => {
    const output = await mkdtemp(resolve(tmpdir(), "resonance-package-"));
    try {
      await execute("node", [
        "--experimental-strip-types",
        "--no-warnings",
        "packages/contracts/scripts/generate.ts",
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
        kind: "valid",
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
        "--experimental-strip-types",
        "--no-warnings",
        "packages/contracts/scripts/generate.ts",
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
      expect(validateManifest(manifest).kind).toBe("valid");
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
});
