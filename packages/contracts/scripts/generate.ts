import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, relative, resolve, sep } from "node:path";

import { generateBundledPackageCatalog } from "../../../scripts/bundled-package-catalog.ts";

function parseArguments(values: string[]): Map<string, string> {
  const argumentsByName = new Map<string, string>();
  for (let index = 0; index < values.length; index += 1) {
    const value = values[index];
    if (!value.startsWith("--") || !values[index + 1]) {
      throw new Error(`Unknown scaffold argument: ${value}`);
    }
    argumentsByName.set(value.slice(2), values[index + 1]);
    index += 1;
  }
  return argumentsByName;
}

async function templateFiles(
  directory: string,
  prefix = "",
): Promise<string[]> {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries.sort((left, right) =>
    left.name.localeCompare(right.name),
  )) {
    const relativePath = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) {
      files.push(
        ...(await templateFiles(resolve(directory, entry.name), relativePath)),
      );
    } else if (entry.isFile()) {
      files.push(relativePath);
    }
  }
  return files;
}

const argumentsByName = parseArguments(process.argv.slice(2));
const id = argumentsByName.get("id");
const output = argumentsByName.get("output");
const root = resolve(
  argumentsByName.get("root") ?? resolve(import.meta.dirname, "../../.."),
);

if (
  !id ||
  !output ||
  !/^(resonance|[a-z][a-z0-9-]*)\.[a-z][a-z0-9-]*$/.test(id)
) {
  throw new Error(
    "Usage: pnpm generate -- --id <namespace.name> --output <directory> [--root <repository>]",
  );
}

const [namespace, packageSlug] = id.split(".");
const packageName = packageSlug
  .split("-")
  .map((word) => `${word[0]?.toUpperCase() ?? ""}${word.slice(1)}`)
  .join(" ");
const npmPackageName = `@${namespace}/${packageSlug}`;
const destination = resolve(output);
const outputFromRoot = relative(root, destination).split(sep).join("/");
const packageOutput =
  !isAbsolute(outputFromRoot) && !outputFromRoot.startsWith("../")
    ? outputFromRoot
    : destination.split(sep).join("/");
const replacements = new Map([
  ["__PACKAGE_ID__", id],
  ["__PACKAGE_NAME__", packageName],
  ["__NPM_PACKAGE_NAME__", npmPackageName],
  ["__PACKAGE_OUTPUT__", packageOutput],
]);
const templateRoot = resolve(import.meta.dirname, "../templates/package");

const renderedFiles = await Promise.all(
  (await templateFiles(templateRoot)).map(async (path) => {
    let content = await readFile(resolve(templateRoot, path), "utf8");
    for (const [placeholder, value] of replacements) {
      content = content.replaceAll(placeholder, value);
    }
    return { path, content, outputPath: resolve(destination, path) };
  }),
);
const changedFiles: string[] = [];
const missingFiles: typeof renderedFiles = [];
for (const file of renderedFiles) {
  try {
    const existing = await readFile(file.outputPath, "utf8");
    if (existing !== file.content) changedFiles.push(file.path);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      missingFiles.push(file);
    } else {
      throw error;
    }
  }
}
if (changedFiles.length > 0) {
  throw new Error(
    `Scaffold would overwrite changed files:\n${changedFiles
      .sort()
      .map((path) => `- ${path}`)
      .join("\n")}`,
  );
}
for (const file of missingFiles) {
  await mkdir(dirname(file.outputPath), { recursive: true });
  await writeFile(file.outputPath, file.content);
}

const packagesDirectory = resolve(root, "packages");
const packageRelativePath = relative(packagesDirectory, destination);
const isDirectPackage =
  packageRelativePath !== "" &&
  packageRelativePath !== ".." &&
  !packageRelativePath.startsWith(`..${sep}`) &&
  !packageRelativePath.includes(sep);
if (isDirectPackage) {
  await generateBundledPackageCatalog({ root });
}
