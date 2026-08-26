import {
  mkdtemp,
  mkdir,
  readFile,
  rename,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import type { PackageManifest } from "@resonance/contracts";
import { generateBundledPackageCatalog } from "./bundled-package-catalog.ts";

const temporaryRoots: string[] = [];

async function temporaryRepository(): Promise<string> {
  const root = await mkdtemp(resolve(tmpdir(), "resonance-catalog-"));
  temporaryRoots.push(root);
  const schemaPath = resolve(
    root,
    "packages/contracts/schema/manifest.v2.json",
  );
  await mkdir(dirname(schemaPath), { recursive: true });
  await writeFile(
    schemaPath,
    await readFile("packages/contracts/schema/manifest.v2.json", "utf8"),
  );
  return root;
}

function manifest(id: string, entry = "src/index.ts"): PackageManifest {
  return {
    manifestVersion: 2,
    source: "bundled",
    id,
    name: id,
    description: `${id} package`,
    nav: { label: id, icon: "box" },
    content: { entry },
    events: { emits: [], consumes: [] },
    minRole: "viewer",
  };
}

async function writePackage(
  root: string,
  directory: string,
  candidate: PackageManifest,
  options: { writeEntry?: boolean } = {},
): Promise<void> {
  const packageDirectory = resolve(root, "packages", directory);
  await mkdir(resolve(packageDirectory, "src"), { recursive: true });
  await writeFile(
    resolve(packageDirectory, "manifest.json"),
    `${JSON.stringify(candidate, null, 2)}\n`,
  );
  if (options.writeEntry !== false) {
    const entryPath = resolve(packageDirectory, candidate.content.entry);
    await mkdir(dirname(entryPath), { recursive: true });
    await writeFile(
      entryPath,
      `export const id = ${JSON.stringify(candidate.id)};\n`,
    );
  }
}

afterEach(async () => {
  await Promise.all(
    temporaryRoots
      .splice(0)
      .map((root) => rm(root, { recursive: true, force: true })),
  );
});

describe("bundled package catalog", () => {
  it("writes sorted TypeScript imports and matching Rust manifests", async () => {
    const root = await temporaryRepository();
    await writePackage(root, "z-package", manifest("resonance.zebra"));
    await writePackage(root, "a-package", manifest("resonance.alpha"));

    const generated = await generateBundledPackageCatalog({ root });
    expect(generated.map(({ id }) => id)).toEqual([
      "resonance.alpha",
      "resonance.zebra",
    ]);

    const typescript = await readFile(
      resolve(root, "apps/desktop/src/generated/bundled-package-catalog.ts"),
      "utf8",
    );
    expect(typescript).toContain(
      'import("../../../../packages/a-package/src/index")',
    );
    expect(typescript.indexOf("resonance.alpha")).toBeLessThan(
      typescript.indexOf("resonance.zebra"),
    );

    const rust = JSON.parse(
      await readFile(
        resolve(
          root,
          "apps/desktop/src-tauri/generated/bundled-package-manifests.json",
        ),
        "utf8",
      ),
    );
    expect(rust).toEqual(generated);
    await expect(
      generateBundledPackageCatalog({ root, mode: "check" }),
    ).resolves.toEqual(generated);
  });

  it("rejects stale generated output", async () => {
    const root = await temporaryRepository();
    await writePackage(root, "reference", manifest("resonance.reference"));
    await generateBundledPackageCatalog({ root });
    await writeFile(
      resolve(root, "apps/desktop/src/generated/bundled-package-catalog.ts"),
      "stale\n",
    );

    await expect(
      generateBundledPackageCatalog({ root, mode: "check" }),
    ).rejects.toThrow("is stale");
  });

  it("restores both catalogs when the second promotion fails", async () => {
    const root = await temporaryRepository();
    await writePackage(root, "first", manifest("resonance.first"));
    await generateBundledPackageCatalog({ root });
    const typescriptPath = resolve(
      root,
      "apps/desktop/src/generated/bundled-package-catalog.ts",
    );
    const rustPath = resolve(
      root,
      "apps/desktop/src-tauri/generated/bundled-package-manifests.json",
    );
    const before = await Promise.all([
      readFile(typescriptPath, "utf8"),
      readFile(rustPath, "utf8"),
    ]);
    await writePackage(root, "second", manifest("resonance.second"));
    let promotions = 0;

    await expect(
      generateBundledPackageCatalog({
        root,
        renameFile: async (source, destination) => {
          promotions += 1;
          if (promotions === 2) throw new Error("injected promotion failure");
          await rename(source, destination);
        },
      }),
    ).rejects.toThrow("injected promotion failure");
    await expect(
      Promise.all([
        readFile(typescriptPath, "utf8"),
        readFile(rustPath, "utf8"),
      ]),
    ).resolves.toEqual(before);
  });

  it("rejects missing declared prompt resources", async () => {
    const root = await temporaryRepository();
    const candidate = manifest("resonance.prompt");
    candidate.agent = {
      systemPrompt: "prompts/missing.md",
      permissions: ["read"],
      contextProviders: [],
    };
    await writePackage(root, "prompt", candidate);

    await expect(generateBundledPackageCatalog({ root })).rejects.toThrow(
      "agent.systemPrompt does not exist",
    );
  });

  it.skipIf(process.platform === "win32")(
    "rejects prompt resources that resolve outside the package",
    async () => {
      const root = await temporaryRepository();
      const candidate = manifest("resonance.prompt-link");
      candidate.agent = {
        systemPrompt: "prompts/reference.md",
        permissions: ["read"],
        contextProviders: [],
      };
      await writePackage(root, "prompt-link", candidate);
      await writeFile(resolve(root, "outside.md"), "outside\n");
      await mkdir(resolve(root, "packages/prompt-link/prompts"), {
        recursive: true,
      });
      await symlink(
        "../../../outside.md",
        resolve(root, "packages/prompt-link/prompts/reference.md"),
      );

      await expect(generateBundledPackageCatalog({ root })).rejects.toThrow(
        "agent.systemPrompt resolves outside its package",
      );
    },
  );

  it("rejects duplicate ids and missing entries", async () => {
    const duplicateRoot = await temporaryRepository();
    await writePackage(duplicateRoot, "first", manifest("resonance.duplicate"));
    await writePackage(
      duplicateRoot,
      "second",
      manifest("resonance.duplicate"),
    );
    await expect(
      generateBundledPackageCatalog({ root: duplicateRoot }),
    ).rejects.toThrow("duplicate bundled package id");

    const missingRoot = await temporaryRepository();
    await writePackage(missingRoot, "missing", manifest("resonance.missing"), {
      writeEntry: false,
    });
    await expect(
      generateBundledPackageCatalog({ root: missingRoot }),
    ).rejects.toThrow("content.entry does not exist");
  });

  it.each(["../outside.ts", "/tmp/outside.ts", "C:\\outside.ts"])(
    "rejects an unbounded entry path %s",
    async (entry) => {
      const root = await temporaryRepository();
      await writePackage(
        root,
        "invalid",
        manifest("resonance.invalid", entry),
        {
          writeEntry: false,
        },
      );

      await expect(generateBundledPackageCatalog({ root })).rejects.toThrow(
        /manifest v2|package-relative/,
      );
    },
  );
});
