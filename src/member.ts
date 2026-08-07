import { mkdir, readFile, realpath, rename, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { MANIFEST_VERSION, type PackageConfig, type PackageInput } from './package-contract.ts';

export const MEMBER_CONFIG_NAME = '.resonance/member-config.json';
export const MEMBER_MANIFEST_NAME = 'member-packages.json';
export const MEMBER_MANIFEST_NAMES = [MEMBER_MANIFEST_NAME, '.resonance/member-packages.json', '.resonance/config.json'] as const;

type RecordValue = Record<string, unknown>;
export type MemberConfig = { version: typeof MANIFEST_VERSION; source: string; packages: Record<string, PackageInput> };
export type MemberManifest = { version: typeof MANIFEST_VERSION; packages: Record<string, PackageConfig> };

function isRecord(value: unknown): value is RecordValue { return Boolean(value) && typeof value === 'object' && !Array.isArray(value); }
function toPath(value: string | URL): string { return value instanceof URL ? fileURLToPath(value) : value; }
function absolutePath(value: string, source: string): string {
  if (!value || !path.isAbsolute(value)) throw new Error(`${source}: source must be an absolute path.`);
  return path.resolve(value);
}
function validatePackages(value: unknown, source: string, allowModule: boolean): Record<string, PackageConfig> {
  if (!isRecord(value)) throw new Error(`${source}: packages must be an object.`);
  const packages: Record<string, PackageConfig> = {};
  for (const [id, input] of Object.entries(value)) {
    if (!isRecord(input)) throw new Error(`${source}: package ${id} inputs must be an object.`);
    if (allowModule && typeof input.module !== 'string') throw new Error(`${source}: package ${id} module must be a string.`);
    if (!allowModule && 'module' in input) throw new Error(`${source}: member package ${id} must not duplicate its module path.`);
    packages[id] = { ...input } as PackageConfig;
  }
  return packages;
}

export function validateMemberConfig(value: unknown, source = MEMBER_CONFIG_NAME): MemberConfig {
  if (!isRecord(value)) throw new Error(`${source}: member config must be an object.`);
  if (value.version !== MANIFEST_VERSION) throw new Error(`${source}: version must be ${MANIFEST_VERSION}.`);
  if (typeof value.source !== 'string') throw new Error(`${source}: source must be a string.`);
  return { version: MANIFEST_VERSION, source: absolutePath(value.source, source), packages: validatePackages(value.packages, source, false) };
}

export function validateMemberManifest(value: unknown, source = 'member-packages.json'): MemberManifest {
  if (!isRecord(value)) throw new Error(`${source}: manifest must be an object.`);
  if (value.version !== MANIFEST_VERSION) throw new Error(`${source}: version must be ${MANIFEST_VERSION}.`);
  return { version: MANIFEST_VERSION, packages: validatePackages(value.packages, source, true) };
}

async function readJson(filename: string): Promise<unknown> {
  let contents: string;
  try { contents = await readFile(filename, 'utf8'); }
  catch (error) { throw new Error(`${filename}: unable to read member manifest.`, { cause: error }); }
  try { return JSON.parse(contents); }
  catch { throw new Error(`${filename}: manifest is not valid JSON.`); }
}

export async function loadMemberManifest(root: string | URL): Promise<MemberManifest> {
  const directory = toPath(root);
  let lastError: unknown;
  for (const relative of MEMBER_MANIFEST_NAMES) {
    const filename = path.join(directory, relative);
    try { return validateMemberManifest(await readJson(filename), filename); }
    catch (error) { lastError = error; }
  }
  throw lastError instanceof Error ? lastError : new Error(`${directory}: member manifest not found.`);
}

export async function loadMemberConfig(root: string | URL): Promise<MemberConfig | null> {
  const filename = path.join(toPath(root), MEMBER_CONFIG_NAME);
  let contents: string;
  try { contents = await readFile(filename, 'utf8'); }
  catch (error) { if (error?.code === 'ENOENT') return null; throw error; }
  let value: unknown;
  try { value = JSON.parse(contents); } catch { throw new Error(`${filename}: member config is not valid JSON.`); }
  return validateMemberConfig(value, filename);
}

export async function writeMemberConfig(root: string | URL, config: MemberConfig): Promise<MemberConfig> {
  const filename = path.join(toPath(root), MEMBER_CONFIG_NAME);
  const validated = validateMemberConfig(config, filename);
  await mkdir(path.dirname(filename), { recursive: true });
  const temporary = `${filename}.${process.pid}.tmp`;
  await writeFile(temporary, `${JSON.stringify(validated, null, 2)}\n`, { encoding: 'utf8', mode: 0o600 });
  await rename(temporary, filename);
  return validated;
}

export async function resolveMemberRepository(source: string): Promise<string> {
  const resolved = await realpath(absolutePath(source, 'member source'));
  return resolved;
}

export function packageInput(config: PackageConfig): PackageInput { return Object.fromEntries(Object.entries(config).filter(([key]) => key !== 'module' && key !== 'enabled')); }
