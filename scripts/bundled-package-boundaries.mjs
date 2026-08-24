import { readFile, readdir, realpath } from "node:fs/promises";
import { dirname, isAbsolute, relative, resolve, sep, win32 } from "node:path";
import { pathToFileURL } from "node:url";

const sourceExtensions = new Set([".ts", ".tsx", ".js", ".mjs", ".css"]);
const capabilityProperties = new Map([
  ["workspaceFilesV1", "workspace-files:v1"],
]);

async function sourceFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await sourceFiles(path)));
    } else if (
      entry.isFile() &&
      sourceExtensions.has(entry.name.slice(entry.name.lastIndexOf(".")))
    ) {
      files.push(path);
    }
  }
  return files.sort();
}

function inside(directory, path) {
  const fromDirectory = relative(directory, path);
  return (
    fromDirectory !== ".." &&
    !fromDirectory.startsWith(`..${sep}`) &&
    !isAbsolute(fromDirectory)
  );
}

function importSpecifiers(source) {
  const specifiers = [];
  const pattern =
    /(?:import|export)\s+(?:[^"'()]*?\s+from\s+)?["']([^"']+)["']|import\(\s*["']([^"']+)["']\s*\)/g;
  for (const match of source.matchAll(pattern)) {
    specifiers.push(match[1] ?? match[2]);
  }
  return specifiers;
}

async function resolvedImportPath(path) {
  const candidates = [
    path,
    path.replace(/\.js$/, ".ts"),
    `${path}.ts`,
    `${path}.tsx`,
    resolve(path, "index.ts"),
  ];
  for (const candidate of candidates) {
    try {
      return await realpath(candidate);
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
    }
  }
  return null;
}

async function checkImports(
  packageDirectory,
  sourcePath,
  displayPath,
  source,
  errors,
) {
  for (const specifier of importSpecifiers(source)) {
    if (
      specifier === "@tauri-apps/api" ||
      specifier.startsWith("@tauri-apps/api/")
    ) {
      errors.push(
        `${displayPath}: bundled packages cannot import @tauri-apps/api`,
      );
    }
    if (
      specifier.includes("apps/desktop") ||
      specifier.includes("crates/runtime") ||
      specifier.includes("resonance-runtime") ||
      specifier === "@resonance/desktop"
    ) {
      errors.push(
        `${displayPath}: bundled packages cannot import host internals`,
      );
    }
    if (isAbsolute(specifier) || win32.isAbsolute(specifier)) {
      errors.push(
        `${displayPath}: bundled package imports must not be absolute`,
      );
    }
    if (specifier.startsWith(".")) {
      const imported = resolve(dirname(sourcePath), specifier);
      if (!inside(packageDirectory, imported)) {
        errors.push(
          `${displayPath}: relative import escapes its package: ${specifier}`,
        );
        continue;
      }
      const [realPackageDirectory, realImported] = await Promise.all([
        realpath(packageDirectory),
        resolvedImportPath(imported),
      ]);
      if (realImported && !inside(realPackageDirectory, realImported)) {
        errors.push(
          `${displayPath}: relative import resolves outside its package: ${specifier}`,
        );
      }
    }
  }
}

function checkCapabilities(manifest, path, source, errors) {
  const declared = new Set(manifest.capabilities ?? []);
  for (const match of source.matchAll(
    /\bcapabilities\.([A-Za-z][A-Za-z0-9]*)/g,
  )) {
    const property = match[1];
    const capability = capabilityProperties.get(property);
    if (!capability) {
      errors.push(`${path}: unknown SDK capability property: ${property}`);
    } else if (!declared.has(capability)) {
      errors.push(`${path}: uses undeclared capability ${capability}`);
    }
  }
}

function checkCss(manifest, path, source, errors) {
  if (path.endsWith(".module.css")) return;
  const withoutComments = source.replaceAll(/\/\*[\s\S]*?\*\//g, "");
  const selectorPattern = /([^{}]+)\{/g;
  for (const match of withoutComments.matchAll(selectorPattern)) {
    const selectorGroup = match[1].trim();
    if (
      selectorGroup.startsWith("@") ||
      selectorGroup === "from" ||
      selectorGroup === "to" ||
      /^\d+%$/.test(selectorGroup)
    ) {
      continue;
    }
    for (const selector of selectorGroup
      .split(",")
      .map((value) => value.trim())) {
      if (!selector.startsWith(`[data-package-id="${manifest.id}"]`)) {
        errors.push(`${path}: unscoped package selector: ${selector}`);
      }
    }
  }
}

async function bundledPackages(root) {
  const packagesDirectory = resolve(root, "packages");
  const entries = await readdir(packagesDirectory, { withFileTypes: true });
  const packages = [];
  for (const entry of entries) {
    if (!entry.isDirectory()) continue;
    const directory = resolve(packagesDirectory, entry.name);
    try {
      const manifest = JSON.parse(
        await readFile(resolve(directory, "manifest.json"), "utf8"),
      );
      packages.push({ directory, manifest });
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
    }
  }
  return packages.sort((left, right) =>
    left.manifest.id.localeCompare(right.manifest.id),
  );
}

export async function checkBundledPackageBoundaries({
  root = process.cwd(),
} = {}) {
  const absoluteRoot = resolve(root);
  const errors = [];
  for (const { directory, manifest } of await bundledPackages(absoluteRoot)) {
    const packageJsonPath = resolve(directory, "package.json");
    try {
      const packageJson = JSON.parse(await readFile(packageJsonPath, "utf8"));
      const dependencies = {
        ...packageJson.dependencies,
        ...packageJson.devDependencies,
      };
      for (const dependency of Object.keys(dependencies)) {
        if (
          dependency === "@resonance/desktop" ||
          dependency === "resonance-runtime" ||
          dependency.startsWith("@tauri-apps/api")
        ) {
          errors.push(
            `${relative(absoluteRoot, packageJsonPath)}: forbidden dependency ${dependency}`,
          );
        }
      }
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
    }

    const sourceDirectory = resolve(directory, "src");
    for (const path of await sourceFiles(sourceDirectory)) {
      const source = await readFile(path, "utf8");
      const displayPath = relative(absoluteRoot, path);
      if (path.endsWith(".css")) {
        checkCss(manifest, displayPath, source, errors);
      } else {
        await checkImports(directory, path, displayPath, source, errors);
        checkCapabilities(manifest, displayPath, source, errors);
      }
    }
  }
  if (errors.length > 0) {
    throw new Error(errors.sort().join("\n"));
  }
}

if (
  process.argv[1] &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url
) {
  await checkBundledPackageBoundaries();
}
