import { mkdtemp, mkdir, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { checkBundledPackageBoundaries } from "./bundled-package-boundaries.ts";

const roots: string[] = [];

type FixtureOptions = Readonly<{
  id?: string;
  capabilities?: string[];
  dependencies?: Record<string, string>;
  source?: string;
  css?: string;
}>;

async function write(path: string, content: string): Promise<void> {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, content);
}

async function packageFixture(
  root: string,
  name: string,
  options: FixtureOptions = {},
): Promise<void> {
  const directory = resolve(root, "packages", name);
  const id = options.id ?? `resonance.${name}`;
  await write(
    resolve(directory, "manifest.json"),
    JSON.stringify({
      manifestVersion: 2,
      source: "bundled",
      id,
      name,
      description: `${name} package`,
      nav: { label: name, icon: "box" },
      content: { entry: "src/index.ts" },
      events: { emits: [], consumes: [] },
      minRole: "viewer",
      ...(options.capabilities ? { capabilities: options.capabilities } : {}),
    }),
  );
  await write(
    resolve(directory, "package.json"),
    JSON.stringify({
      name: `@resonance/${name}`,
      type: "module",
      dependencies: options.dependencies ?? {
        "@resonance/package-sdk": "workspace:*",
      },
    }),
  );
  await write(
    resolve(directory, "src/index.ts"),
    options.source ??
      'import type { PackageContentModule } from "@resonance/package-sdk";\nexport const mount: PackageContentModule["mount"] = () => ({ activate() {}, deactivate() {}, dispose() {} });\n',
  );
  await write(
    resolve(directory, "src/styles.css"),
    options.css ?? `[data-package-id="${id}"] { color: inherit; }\n`,
  );
}

afterEach(async () => {
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true, force: true })),
  );
});

describe("bundled package boundaries", () => {
  it("accepts SDK-only imports, declared capabilities, and scoped CSS", async () => {
    const root = await mkdtemp(resolve(tmpdir(), "resonance-boundary-"));
    roots.push(root);
    await packageFixture(root, "valid", {
      capabilities: ["workspace-files:v1"],
      source:
        'import type { PackageContentModule } from "@resonance/package-sdk";\nexport const mount: PackageContentModule["mount"] = (_root, context) => { void context.capabilities.workspaceFilesV1; return { activate() {}, deactivate() {}, dispose() {} }; };\n',
    });

    await expect(
      checkBundledPackageBoundaries({ root }),
    ).resolves.toBeUndefined();
  });

  it.skipIf(process.platform === "win32")(
    "rejects an import that escapes through a symlink",
    async () => {
      const root = await mkdtemp(resolve(tmpdir(), "resonance-boundary-"));
      roots.push(root);
      await packageFixture(root, "symlinked", {
        source: 'import "./linked.js";\nexport const value = true;\n',
      });
      await write(
        resolve(root, "outside.ts"),
        "export const outside = true;\n",
      );
      await symlink(
        "../../../outside.ts",
        resolve(root, "packages/symlinked/src/linked.ts"),
      );

      await expect(checkBundledPackageBoundaries({ root })).rejects.toThrow(
        "relative import resolves outside its package",
      );
    },
  );

  it("rejects transport, host, escaping, capability, dependency, and CSS violations", async () => {
    const root = await mkdtemp(resolve(tmpdir(), "resonance-boundary-"));
    roots.push(root);
    await packageFixture(root, "invalid", {
      dependencies: { "@tauri-apps/api": "2.5.0" },
      source: [
        'import { invoke } from "@tauri-apps/api/core";',
        'import "../../../apps/desktop/src/main.js";',
        'import "../../outside.js";',
        "export function mount(_root, context) { void invoke; void context.capabilities.workspaceFilesV1; return { activate() {}, deactivate() {}, dispose() {} }; }",
      ].join("\n"),
      css: "body, .unscoped { color: red; }\n",
    });

    await expect(checkBundledPackageBoundaries({ root })).rejects.toThrow(
      /forbidden dependency[\s\S]*@tauri-apps\/api[\s\S]*host internals[\s\S]*relative import escapes[\s\S]*undeclared capability[\s\S]*unscoped package selector/,
    );
  });
});
