import { builtinModules } from "node:module";
import { readFile, readdir, realpath } from "node:fs/promises";
import {
  dirname,
  extname,
  isAbsolute,
  relative,
  resolve,
  sep,
  win32,
} from "node:path";
import { pathToFileURL } from "node:url";

import postcss from "postcss";
import ts from "typescript";

import {
  semanticCapabilityProperties,
  type PackageManifest,
  type SemanticCapability,
} from "@resonance/contracts";

type BundledPackage = Readonly<{
  directory: string;
  manifest: PackageManifest;
}>;

type PackageJson = Readonly<{
  dependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
}>;

type ImportReference = Readonly<{
  specifier: string;
  line: number;
  column: number;
}>;

const sourceExtensions = new Set([".ts", ".tsx", ".js", ".mjs", ".css"]);
const capabilityProperties = new Map<string, SemanticCapability>(
  Object.entries(semanticCapabilityProperties).map(([capability, property]) => [
    property,
    capability as SemanticCapability,
  ]),
);
const nodeBuiltins = new Set([
  ...builtinModules,
  ...builtinModules.map((name) => `node:${name}`),
]);

function errorCode(error: unknown): string | undefined {
  return (error as NodeJS.ErrnoException).code;
}

function sourceLocation(
  sourceFile: ts.SourceFile,
  node: ts.Node,
): { line: number; column: number } {
  const location = sourceFile.getLineAndCharacterOfPosition(node.getStart());
  return { line: location.line + 1, column: location.character + 1 };
}

function diagnostic(
  path: string,
  line: number,
  column: number,
  message: string,
): string {
  return `${path}:${line}:${column}: ${message}`;
}

async function sourceFiles(directory: string): Promise<string[]> {
  const entries = await readdir(directory, { withFileTypes: true });
  const files: string[] = [];
  for (const entry of entries) {
    if (entry.name === "node_modules" || entry.name === "dist") continue;
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await sourceFiles(path)));
    } else if (entry.isFile() && sourceExtensions.has(extname(entry.name))) {
      files.push(path);
    }
  }
  return files.sort();
}

function inside(directory: string, path: string): boolean {
  const fromDirectory = relative(directory, path);
  return (
    fromDirectory !== ".." &&
    !fromDirectory.startsWith(`..${sep}`) &&
    !isAbsolute(fromDirectory)
  );
}

function packageRoot(specifier: string): string {
  const parts = specifier.split("/");
  return specifier.startsWith("@") ? parts.slice(0, 2).join("/") : parts[0]!;
}

function scriptKind(path: string): ts.ScriptKind {
  return path.endsWith(".tsx")
    ? ts.ScriptKind.TSX
    : path.endsWith(".js") || path.endsWith(".mjs")
      ? ts.ScriptKind.JS
      : ts.ScriptKind.TS;
}

function inspectTypeScript(
  path: string,
  displayPath: string,
  source: string,
  manifest: PackageManifest,
  errors: string[],
): ImportReference[] {
  const sourceFile = ts.createSourceFile(
    path,
    source,
    ts.ScriptTarget.Latest,
    true,
    scriptKind(path),
  );
  const imports: ImportReference[] = [];
  const declared = new Set(manifest.capabilities ?? []);

  const addImport = (literal: ts.StringLiteralLike): void => {
    imports.push({
      specifier: literal.text,
      ...sourceLocation(sourceFile, literal),
    });
  };
  const checkCapability = (node: ts.Node, property: string | null): void => {
    const location = sourceLocation(sourceFile, node);
    if (property === null) {
      errors.push(
        diagnostic(
          displayPath,
          location.line,
          location.column,
          "capability access must use a literal property",
        ),
      );
      return;
    }
    const capability = capabilityProperties.get(property);
    if (!capability) {
      errors.push(
        diagnostic(
          displayPath,
          location.line,
          location.column,
          `unknown SDK capability property: ${property}`,
        ),
      );
    } else if (!declared.has(capability)) {
      errors.push(
        diagnostic(
          displayPath,
          location.line,
          location.column,
          `uses undeclared capability ${capability}`,
        ),
      );
    }
  };

  const visit = (node: ts.Node): void => {
    if (
      (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
      node.moduleSpecifier &&
      ts.isStringLiteralLike(node.moduleSpecifier)
    ) {
      addImport(node.moduleSpecifier);
    } else if (
      ts.isCallExpression(node) &&
      node.expression.kind === ts.SyntaxKind.ImportKeyword
    ) {
      const argument = node.arguments[0];
      if (argument && ts.isStringLiteralLike(argument)) {
        addImport(argument);
      } else {
        const location = sourceLocation(sourceFile, node);
        errors.push(
          diagnostic(
            displayPath,
            location.line,
            location.column,
            "bundled production imports must use a string literal",
          ),
        );
      }
    }

    if (
      ts.isPropertyAccessExpression(node) &&
      ts.isPropertyAccessExpression(node.expression) &&
      node.expression.name.text === "capabilities"
    ) {
      checkCapability(node, node.name.text);
    } else if (
      ts.isElementAccessExpression(node) &&
      ts.isPropertyAccessExpression(node.expression) &&
      node.expression.name.text === "capabilities"
    ) {
      checkCapability(
        node,
        node.argumentExpression &&
          ts.isStringLiteralLike(node.argumentExpression)
          ? node.argumentExpression.text
          : null,
      );
    }
    ts.forEachChild(node, visit);
  };
  visit(sourceFile);
  return imports;
}

function checkCss(
  manifest: PackageManifest,
  path: string,
  source: string,
  errors: string[],
): void {
  if (path.endsWith(".module.css")) return;
  const rootSelector = `[data-package-id="${manifest.id}"]`;
  let root: postcss.Root;
  try {
    root = postcss.parse(source, { from: path });
  } catch (error) {
    errors.push(
      `${path}: ${error instanceof Error ? error.message : String(error)}`,
    );
    return;
  }
  root.walkRules((rule) => {
    if (
      rule.parent?.type === "atrule" &&
      /keyframes$/i.test(rule.parent.name)
    ) {
      return;
    }
    const location = rule.source?.start ?? { line: 1, column: 1 };
    for (const selector of postcss.list.comma(rule.selector)) {
      if (selector.includes(":global")) {
        errors.push(
          diagnostic(
            path,
            location.line,
            location.column,
            `CSS Modules global escape is not allowed: ${selector}`,
          ),
        );
      } else if (!selector.trim().startsWith(rootSelector)) {
        errors.push(
          diagnostic(
            path,
            location.line,
            location.column,
            `unscoped package selector: ${selector.trim()}`,
          ),
        );
      }
    }
  });
  root.walkAtRules("scope", (rule) => {
    const location = rule.source?.start ?? { line: 1, column: 1 };
    if (!rule.params.trim().startsWith(`(${rootSelector})`)) {
      errors.push(
        diagnostic(
          path,
          location.line,
          location.column,
          `@scope must begin at ${rootSelector}`,
        ),
      );
    }
  });
}

async function resolvedImportPath(path: string): Promise<string | null> {
  const candidates = [
    path,
    path.replace(/\.js$/, ".ts"),
    `${path}.ts`,
    `${path}.tsx`,
    `${path}.js`,
    `${path}.css`,
    resolve(path, "index.ts"),
  ];
  for (const candidate of candidates) {
    try {
      return await realpath(candidate);
    } catch (error) {
      if (errorCode(error) !== "ENOENT") throw error;
    }
  }
  return null;
}

async function inspectModule(
  packageDirectory: string,
  manifest: PackageManifest,
  path: string,
  absoluteRoot: string,
  errors: string[],
): Promise<ImportReference[]> {
  const source = await readFile(path, "utf8");
  const displayPath = relative(absoluteRoot, path);
  if (path.endsWith(".css")) {
    checkCss(manifest, displayPath, source, errors);
    return [];
  }
  const imports = inspectTypeScript(
    path,
    displayPath,
    source,
    manifest,
    errors,
  );
  for (const reference of imports) {
    const specifier = reference.specifier;
    if (
      specifier === "@tauri-apps/api" ||
      specifier.startsWith("@tauri-apps/api/")
    ) {
      errors.push(
        diagnostic(
          displayPath,
          reference.line,
          reference.column,
          "bundled packages cannot import @tauri-apps/api",
        ),
      );
    }
    if (
      specifier.includes("apps/desktop") ||
      specifier.includes("crates/runtime") ||
      specifier.includes("resonance-runtime") ||
      specifier === "@resonance/desktop"
    ) {
      errors.push(
        diagnostic(
          displayPath,
          reference.line,
          reference.column,
          "bundled packages cannot import host internals",
        ),
      );
    }
    if (isAbsolute(specifier) || win32.isAbsolute(specifier)) {
      errors.push(
        diagnostic(
          displayPath,
          reference.line,
          reference.column,
          "bundled package imports must not be absolute",
        ),
      );
    }
    if (specifier.startsWith(".")) {
      const imported = resolve(dirname(path), specifier);
      if (!inside(packageDirectory, imported)) {
        errors.push(
          diagnostic(
            displayPath,
            reference.line,
            reference.column,
            `relative import escapes its package: ${specifier}`,
          ),
        );
      }
    }
  }
  return imports;
}

async function productionGraph(
  packageDirectory: string,
  manifest: PackageManifest,
  absoluteRoot: string,
  errors: string[],
): Promise<{ files: Set<string>; dependencies: Set<string> }> {
  const realPackageDirectory = await realpath(packageDirectory);
  const entryPath = await resolvedImportPath(
    resolve(packageDirectory, manifest.content.entry),
  );
  if (!entryPath || !inside(realPackageDirectory, entryPath)) {
    errors.push(
      `${manifest.id}: content.entry cannot be resolved inside its package`,
    );
    return { files: new Set(), dependencies: new Set() };
  }

  const files = new Set<string>();
  const dependencies = new Set<string>();
  const pending = [entryPath];
  while (pending.length > 0) {
    const path = pending.pop()!;
    if (files.has(path)) continue;
    files.add(path);
    const references = await inspectModule(
      packageDirectory,
      manifest,
      path,
      absoluteRoot,
      errors,
    );
    for (const reference of references) {
      if (!reference.specifier.startsWith(".")) {
        if (!nodeBuiltins.has(reference.specifier)) {
          dependencies.add(packageRoot(reference.specifier));
        }
        continue;
      }
      const imported = await resolvedImportPath(
        resolve(dirname(path), reference.specifier),
      );
      if (!imported) {
        errors.push(
          diagnostic(
            relative(absoluteRoot, path),
            reference.line,
            reference.column,
            `local import cannot be resolved: ${reference.specifier}`,
          ),
        );
      } else if (!inside(realPackageDirectory, imported)) {
        errors.push(
          diagnostic(
            relative(absoluteRoot, path),
            reference.line,
            reference.column,
            `relative import resolves outside its package: ${reference.specifier}`,
          ),
        );
      } else {
        pending.push(imported);
      }
    }
  }
  return { files, dependencies };
}

async function bundledPackages(root: string): Promise<BundledPackage[]> {
  const packagesDirectory = resolve(root, "packages");
  const entries = await readdir(packagesDirectory, { withFileTypes: true });
  const packages: BundledPackage[] = [];
  for (const entry of entries) {
    if (!entry.isDirectory()) continue;
    const directory = resolve(packagesDirectory, entry.name);
    try {
      const manifest = JSON.parse(
        await readFile(resolve(directory, "manifest.json"), "utf8"),
      ) as PackageManifest;
      packages.push({ directory, manifest });
    } catch (error) {
      if (errorCode(error) !== "ENOENT") throw error;
    }
  }
  return packages.sort((left, right) =>
    left.manifest.id.localeCompare(right.manifest.id),
  );
}

export async function checkBundledPackageBoundaries({
  root = process.cwd(),
}: { root?: string } = {}): Promise<void> {
  const absoluteRoot = resolve(root);
  const errors: string[] = [];
  for (const { directory, manifest } of await bundledPackages(absoluteRoot)) {
    const packageJsonPath = resolve(directory, "package.json");
    let packageJson: PackageJson = {};
    try {
      packageJson = JSON.parse(
        await readFile(packageJsonPath, "utf8"),
      ) as PackageJson;
    } catch (error) {
      if (errorCode(error) !== "ENOENT") throw error;
    }
    const productionDeclarations = new Set(
      Object.keys(packageJson.dependencies ?? {}),
    );
    const testDeclarations = new Set([
      ...productionDeclarations,
      ...Object.keys(packageJson.devDependencies ?? {}),
    ]);
    for (const dependency of testDeclarations) {
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

    const production = await productionGraph(
      directory,
      manifest,
      absoluteRoot,
      errors,
    );
    for (const dependency of production.dependencies) {
      if (!productionDeclarations.has(dependency)) {
        errors.push(
          `${relative(absoluteRoot, packageJsonPath)}: production import requires dependency ${dependency}`,
        );
      }
    }

    const allDependencies = new Set(production.dependencies);
    for (const path of await sourceFiles(directory)) {
      if (production.files.has(await realpath(path))) continue;
      const references = await inspectModule(
        directory,
        manifest,
        path,
        absoluteRoot,
        errors,
      );
      for (const reference of references) {
        if (reference.specifier.startsWith(".")) continue;
        if (nodeBuiltins.has(reference.specifier)) continue;
        const dependency = packageRoot(reference.specifier);
        allDependencies.add(dependency);
        if (!testDeclarations.has(dependency)) {
          errors.push(
            diagnostic(
              relative(absoluteRoot, path),
              reference.line,
              reference.column,
              `test or tooling import requires dependency ${dependency}`,
            ),
          );
        }
      }
    }
    for (const dependency of testDeclarations) {
      if (!allDependencies.has(dependency)) {
        errors.push(
          `${relative(absoluteRoot, packageJsonPath)}: unused dependency ${dependency}`,
        );
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
