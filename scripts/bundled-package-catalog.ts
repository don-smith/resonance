import {
  access,
  mkdir,
  readFile,
  readdir,
  realpath,
  rename,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { constants as fsConstants } from "node:fs";
import { dirname, isAbsolute, relative, resolve, sep, win32 } from "node:path";
import { pathToFileURL } from "node:url";

import prettier from "prettier";

import { validateManifest, type PackageManifest } from "@resonance/contracts";
import { generateManifestBindings } from "../packages/contracts/scripts/generate-manifest-bindings.ts";
type DiscoveredPackage = Readonly<{
  manifest: PackageManifest;
  entryPath: string;
}>;
type CatalogOptions = Readonly<{
  root?: string;
  mode?: "write" | "check";
  typescriptOutput?: string;
  rustOutput?: string;
  renameFile?: typeof rename;
}>;

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

const DEFAULT_TYPESCRIPT_OUTPUT =
  "apps/desktop/src/generated/bundled-package-catalog.ts";
const DEFAULT_RUST_OUTPUT =
  "apps/desktop/src-tauri/generated/bundled-package-manifests.json";

function normalizeManifest(manifest: PackageManifest): PackageManifest {
  return {
    manifestVersion: manifest.manifestVersion,
    source: manifest.source,
    id: manifest.id,
    name: manifest.name,
    description: manifest.description,
    nav: {
      label: manifest.nav.label,
      icon: manifest.nav.icon,
    },
    content: { entry: manifest.content.entry },
    events: {
      emits: [...manifest.events.emits],
      consumes: [...manifest.events.consumes],
    },
    minRole: manifest.minRole,
    ...(manifest.capabilities
      ? { capabilities: [...manifest.capabilities] }
      : {}),
    ...(manifest.agent
      ? {
          agent: {
            systemPrompt: manifest.agent.systemPrompt,
            permissions: [...manifest.agent.permissions],
            contextProviders: [...manifest.agent.contextProviders],
          },
        }
      : {}),
  };
}

function isInside(parent: string, child: string): boolean {
  const pathFromParent = relative(parent, child);
  return (
    pathFromParent !== "" &&
    pathFromParent !== ".." &&
    !pathFromParent.startsWith(`..${sep}`) &&
    !isAbsolute(pathFromParent)
  );
}

function typescriptImportPath(outputPath: string, entryPath: string): string {
  let importPath = relative(dirname(outputPath), entryPath)
    .split(sep)
    .join("/")
    .replace(/\.ts$/, "");
  if (!importPath.startsWith(".")) {
    importPath = `./${importPath}`;
  }
  return importPath;
}

async function validatePackageResource(
  root: string,
  packageDirectory: string,
  packageId: string,
  field: string,
  resource: string,
  extension: string,
): Promise<void> {
  if (!resource.endsWith(extension)) {
    throw new Error(`${packageId} ${field} must be a ${extension} file`);
  }
  const resourcePath = resolve(packageDirectory, resource);
  if (!isInside(packageDirectory, resourcePath)) {
    throw new Error(`${packageId} ${field} escapes its package`);
  }
  let metadata;
  try {
    metadata = await stat(resourcePath);
  } catch {
    throw new Error(
      `${packageId} ${field} does not exist: ${relative(root, resourcePath)}`,
    );
  }
  if (!metadata.isFile()) {
    throw new Error(`${packageId} ${field} is not a file: ${resource}`);
  }
  const [realPackageDirectory, realResourcePath] = await Promise.all([
    realpath(packageDirectory),
    realpath(resourcePath),
  ]);
  if (!isInside(realPackageDirectory, realResourcePath)) {
    throw new Error(`${packageId} ${field} resolves outside its package`);
  }
}

async function discoverPackages(root: string): Promise<DiscoveredPackage[]> {
  const packagesDirectory = resolve(root, "packages");
  const directories = await readdir(packagesDirectory, { withFileTypes: true });
  const discovered: DiscoveredPackage[] = [];

  for (const directory of directories) {
    if (!directory.isDirectory()) continue;

    const packageDirectory = resolve(packagesDirectory, directory.name);
    const manifestPath = resolve(packageDirectory, "manifest.json");
    try {
      await access(manifestPath, fsConstants.R_OK);
    } catch {
      continue;
    }

    let candidate: unknown;
    try {
      candidate = JSON.parse(await readFile(manifestPath, "utf8")) as unknown;
    } catch (error) {
      throw new Error(
        `${relative(root, manifestPath)} is not valid JSON: ${errorMessage(error)}`,
      );
    }

    const validation = validateManifest(candidate);
    if (validation.kind === "invalid") {
      const diagnostics = validation.diagnostics
        .map(({ path, message }) => `${path} ${message}`)
        .join("; ");
      throw new Error(
        `${relative(root, manifestPath)} does not match manifest v2: ${diagnostics}`,
      );
    }

    const manifest = validation.manifest;
    const entry = manifest.content.entry;
    if (
      isAbsolute(entry) ||
      win32.isAbsolute(entry) ||
      entry.includes("\\") ||
      entry.split("/").includes("..")
    ) {
      throw new Error(
        `${manifest.id} content.entry must be a package-relative path without traversal`,
      );
    }

    const entryPath = resolve(packageDirectory, entry);
    if (!isInside(packageDirectory, entryPath)) {
      throw new Error(`${manifest.id} content.entry escapes its package`);
    }

    let entryStats;
    try {
      entryStats = await stat(entryPath);
    } catch {
      throw new Error(`${manifest.id} content.entry does not exist: ${entry}`);
    }
    if (!entryStats.isFile()) {
      throw new Error(`${manifest.id} content.entry is not a file: ${entry}`);
    }

    const [realPackageDirectory, realEntryPath] = await Promise.all([
      realpath(packageDirectory),
      realpath(entryPath),
    ]);
    if (!isInside(realPackageDirectory, realEntryPath)) {
      throw new Error(
        `${manifest.id} content.entry resolves outside its package`,
      );
    }

    if (manifest.agent) {
      await validatePackageResource(
        root,
        packageDirectory,
        manifest.id,
        "agent.systemPrompt",
        manifest.agent.systemPrompt,
        ".md",
      );
    }

    discovered.push({
      manifest: normalizeManifest(manifest),
      entryPath,
    });
  }

  discovered.sort((left, right) =>
    left.manifest.id.localeCompare(right.manifest.id),
  );
  for (let index = 1; index < discovered.length; index += 1) {
    if (discovered[index - 1].manifest.id === discovered[index].manifest.id) {
      throw new Error(
        `duplicate bundled package id: ${discovered[index].manifest.id}`,
      );
    }
  }

  return discovered;
}

async function renderTypescriptCatalog(
  packages: DiscoveredPackage[],
  outputPath: string,
): Promise<string> {
  const entries = packages
    .map(
      ({ manifest, entryPath }) => `  {
    manifest: ${JSON.stringify(manifest, null, 2)},
    load: () => import(${JSON.stringify(
      typescriptImportPath(outputPath, entryPath),
    )}),
  }`,
    )
    .join(",\n");
  const source = `// Generated by scripts/bundled-package-catalog.ts. Do not edit.\n\nexport const bundledPackageCatalog = [\n${entries}\n] as const;\n`;
  return prettier.format(source, { parser: "typescript" });
}

async function assertCurrent(
  path: string,
  expected: string,
  root: string,
): Promise<void> {
  let current;
  try {
    current = await readFile(path, "utf8");
  } catch {
    throw new Error(
      `${relative(root, path)} is missing; run pnpm packages:generate`,
    );
  }
  if (current !== expected) {
    throw new Error(
      `${relative(root, path)} is stale; run pnpm packages:generate`,
    );
  }
}

type CatalogOutput = Readonly<{ path: string; content: string }>;

async function publishMatchedCatalogs(
  outputs: CatalogOutput[],
  renameFile: typeof rename,
): Promise<void> {
  const suffix = `.tmp-${process.pid}-${Date.now()}`;
  const staged = outputs.map((output) => ({
    ...output,
    temporaryPath: `${output.path}${suffix}`,
  }));
  const previous = await Promise.all(
    outputs.map(async ({ path }) => {
      try {
        return await readFile(path, "utf8");
      } catch {
        return null;
      }
    }),
  );
  await Promise.all(
    staged.map(({ temporaryPath, content }) =>
      writeFile(temporaryPath, content, { flush: true }),
    ),
  );

  let promoted = 0;
  try {
    for (const output of staged) {
      await renameFile(output.temporaryPath, output.path);
      promoted += 1;
    }
  } catch (error) {
    for (let index = promoted - 1; index >= 0; index -= 1) {
      const output = outputs[index];
      const oldContent = previous[index];
      if (!output) continue;
      if (oldContent === null || oldContent === undefined) {
        await rm(output.path, { force: true });
      } else {
        const rollbackPath = `${output.path}${suffix}-rollback`;
        await writeFile(rollbackPath, oldContent, { flush: true });
        await renameFile(rollbackPath, output.path);
      }
    }
    throw error;
  } finally {
    await Promise.all(
      staged.map(({ temporaryPath }) => rm(temporaryPath, { force: true })),
    );
  }
}

export async function generateBundledPackageCatalog({
  root = process.cwd(),
  mode = "write",
  typescriptOutput = DEFAULT_TYPESCRIPT_OUTPUT,
  rustOutput = DEFAULT_RUST_OUTPUT,
  renameFile = rename,
}: CatalogOptions = {}): Promise<PackageManifest[]> {
  if (mode !== "write" && mode !== "check") {
    throw new Error(
      `catalog mode must be "write" or "check", received ${mode}`,
    );
  }

  const absoluteRoot = resolve(root);
  await generateManifestBindings({ root: absoluteRoot, mode });
  const typescriptOutputPath = resolve(absoluteRoot, typescriptOutput);
  const rustOutputPath = resolve(absoluteRoot, rustOutput);
  const packages = await discoverPackages(absoluteRoot);
  const typescript = await renderTypescriptCatalog(
    packages,
    typescriptOutputPath,
  );
  const rust = await prettier.format(
    JSON.stringify(packages.map(({ manifest }) => manifest)),
    { parser: "json" },
  );

  if (mode === "check") {
    await Promise.all([
      assertCurrent(typescriptOutputPath, typescript, absoluteRoot),
      assertCurrent(rustOutputPath, rust, absoluteRoot),
    ]);
  } else {
    await Promise.all([
      mkdir(dirname(typescriptOutputPath), { recursive: true }),
      mkdir(dirname(rustOutputPath), { recursive: true }),
    ]);
    await publishMatchedCatalogs(
      [
        { path: typescriptOutputPath, content: typescript },
        { path: rustOutputPath, content: rust },
      ],
      renameFile,
    );
  }

  return packages.map(({ manifest }) => manifest);
}

function parseArguments(values: string[]): CatalogOptions {
  const options: { root?: string; mode?: "write" | "check" } = {};
  for (let index = 0; index < values.length; index += 1) {
    const value = values[index];
    if (value === "write" || value === "check") {
      options.mode = value;
    } else if (value === "--root" && values[index + 1]) {
      options.root = values[index + 1];
      index += 1;
    } else {
      throw new Error(`unknown catalog argument: ${value}`);
    }
  }
  return options;
}

if (
  process.argv[1] &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url
) {
  await generateBundledPackageCatalog(parseArguments(process.argv.slice(2)));
}
