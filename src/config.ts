import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { MANIFEST_VERSION, type PackageConfig, type PackageInput, type RepositoryConfig } from './package-contract.ts';

export const MANIFEST_NAME = '.resonance/config.json';
const DEFAULT_HOME: PackageInput = { source: 'README.md' };
const DEFAULT_DOCS: PackageInput = { extensions: ['.md', '.markdown'], ignoredDirectories: ['.git', 'node_modules'] };
const DEFAULT_MODULES = {
  shell: 'src/packages/shell/index.ts',
  home: 'src/packages/home/index.ts',
  docs: 'src/packages/docs/index.ts',
} as const;

function toPath(value: string | URL): string { return value instanceof URL ? fileURLToPath(value) : value; }
function isRecord(value: unknown): value is Record<string, unknown> { return Boolean(value) && typeof value === 'object' && !Array.isArray(value); }

export function createRepositoryConfig({ home = false, docs = false }: { home?: boolean; docs?: boolean } = {}): RepositoryConfig {
  const packages: Record<string, PackageConfig> = { shell: { module: DEFAULT_MODULES.shell } };
  if (home) packages.home = { module: DEFAULT_MODULES.home, ...DEFAULT_HOME };
  if (docs) packages.docs = { module: DEFAULT_MODULES.docs, extensions: [...(DEFAULT_DOCS.extensions as string[])], ignoredDirectories: [...(DEFAULT_DOCS.ignoredDirectories as string[])] };
  return { version: MANIFEST_VERSION, packages };
}

function validatePackageInputs(packages: unknown, source: string): Record<string, PackageConfig> {
  if (!isRecord(packages)) throw new Error(`${source}: packages must be an object.`);
  const result: Record<string, PackageConfig> = {};
  for (const [id, input] of Object.entries(packages)) {
    if (!isRecord(input)) throw new Error(`${source}: package ${id} inputs must be an object.`);
    if ('enabled' in input && typeof input.enabled !== 'boolean') throw new Error(`${source}: package ${id} enabled must be a boolean.`);
    if ('module' in input && typeof input.module !== 'string') throw new Error(`${source}: package ${id} module must be a string.`);
    result[id] = { ...input } as PackageConfig;
  }
  return result;
}

export function defaultRepositoryConfig(): RepositoryConfig { return createRepositoryConfig(); }
export function validateRepositoryConfig(value: unknown, source = MANIFEST_NAME): RepositoryConfig {
  if (!isRecord(value)) throw new Error(`${source}: manifest must be an object.`);
  if (value.version !== MANIFEST_VERSION) throw new Error(`${source}: version must be ${MANIFEST_VERSION}.`);
  return { version: MANIFEST_VERSION, packages: validatePackageInputs(value.packages, source) };
}

export async function writeRepositoryConfig(root: string | URL, config = defaultRepositoryConfig()): Promise<RepositoryConfig> {
  const filename = path.join(toPath(root), MANIFEST_NAME);
  const validated = validateRepositoryConfig(config, filename);
  await mkdir(path.dirname(filename), { recursive: true });
  await writeFile(filename, `${JSON.stringify(validated, null, 2)}\n`, 'utf8');
  return validated;
}

export async function repositoryConfigExists(root: string | URL): Promise<boolean> {
  try { await readFile(path.join(toPath(root), MANIFEST_NAME), 'utf8'); return true; }
  catch (error) { if (error.code === 'ENOENT') return false; throw error; }
}

export async function loadRepositoryConfig(root: string | URL): Promise<RepositoryConfig> {
  const filename = path.join(toPath(root), MANIFEST_NAME);
  const contents = await readFile(filename, 'utf8');
  let value: unknown;
  try { value = JSON.parse(contents); }
  catch { throw new Error(`${filename}: manifest is not valid JSON.`); }
  return validateRepositoryConfig(value, filename);
}
