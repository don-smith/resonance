import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { MANIFEST_VERSION, type PackageInput, type RepositoryConfig } from './package-contract.ts';

const MANIFEST_NAME = '.theview.json';
const DEFAULT_HOME: PackageInput = { source: 'README.md' };
const DEFAULT_DOCS: PackageInput = { extensions: ['.md', '.markdown'], ignoredDirectories: ['.git', 'node_modules'] };

function toPath(value: string | URL): string {
  return value instanceof URL ? fileURLToPath(value) : value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function defaultPackageInputs(): Record<string, PackageInput> {
  return {
    home: { ...DEFAULT_HOME },
    docs: {
      extensions: [...(DEFAULT_DOCS.extensions as string[])],
      ignoredDirectories: [...(DEFAULT_DOCS.ignoredDirectories as string[])],
    },
  };
}

function validatePackageInputs(packages: unknown, source: string): Record<string, PackageInput> {
  if (packages === undefined) return defaultPackageInputs();
  if (!isRecord(packages)) throw new Error(`${source}: packages must be an object.`);

  const result: Record<string, PackageInput> = {};
  for (const [id, input] of Object.entries(packages)) {
    if (!isRecord(input)) throw new Error(`${source}: package ${id} inputs must be an object.`);
    result[id] = { ...input };
  }
  const defaults = defaultPackageInputs();
  if (!result.home) result.home = defaults.home;
  if (!result.docs) result.docs = defaults.docs;
  return result;
}

export function defaultRepositoryConfig(): RepositoryConfig {
  return { version: MANIFEST_VERSION, packages: defaultPackageInputs() };
}

export function validateRepositoryConfig(value: unknown, source = MANIFEST_NAME): RepositoryConfig {
  if (!isRecord(value)) throw new Error(`${source}: manifest must be an object.`);
  if (value.version !== MANIFEST_VERSION) throw new Error(`${source}: version must be ${MANIFEST_VERSION}.`);
  return { version: MANIFEST_VERSION, packages: validatePackageInputs(value.packages, source) };
}

export async function loadRepositoryConfig(root: string | URL): Promise<RepositoryConfig> {
  const filename = path.join(toPath(root), MANIFEST_NAME);
  let contents: string;
  try {
    contents = await readFile(filename, 'utf8');
  } catch (error) {
    if (error.code === 'ENOENT') return defaultRepositoryConfig();
    throw error;
  }

  let value: unknown;
  try {
    value = JSON.parse(contents);
  } catch {
    throw new Error(`${filename}: manifest is not valid JSON.`);
  }
  return validateRepositoryConfig(value, filename);
}
