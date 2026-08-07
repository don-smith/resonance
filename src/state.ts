import { mkdir, readFile, rename, writeFile } from 'node:fs/promises';
import { readFileSync } from 'node:fs';
import path from 'node:path';

export const PACKAGE_STATE_MAX_BYTES = 64 * 1024;
export type PackageState = { read<T = unknown>(): Promise<T | null>; write(value: unknown): Promise<void> };

function stateFilename(repositoryRoot: string, scope: 'team' | 'member', packageId: string): string {
  if (!/^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/.test(packageId)) throw new Error(`Invalid package state id: ${packageId}`);
  return path.join(repositoryRoot, '.resonance', scope === 'member' ? 'member-state' : 'state', packageId, 'state.json');
}
function encode(value: unknown): string {
  let text: string;
  try { text = JSON.stringify(value); } catch (error) { throw new Error('Package state must be JSON serializable.', { cause: error }); }
  if (text === undefined || Buffer.byteLength(text, 'utf8') > PACKAGE_STATE_MAX_BYTES) throw new Error(`Package state must be at most ${PACKAGE_STATE_MAX_BYTES} bytes.`);
  return `${text}\n`;
}
function parse(contents: string, filename: string): unknown {
  if (Buffer.byteLength(contents, 'utf8') > PACKAGE_STATE_MAX_BYTES) throw new Error(`${filename}: package state is too large.`);
  try { return JSON.parse(contents); } catch (error) { throw new Error(`${filename}: package state is not valid JSON.`, { cause: error }); }
}

export function createPackageState(repositoryRoot: string, scope: 'team' | 'member', packageId: string): PackageState {
  const filename = stateFilename(repositoryRoot, scope, packageId);
  return {
    async read<T = unknown>() {
      try { return parse(await readFile(filename, 'utf8'), filename) as T; }
      catch (error) { if (error?.code === 'ENOENT') return null; throw error; }
    },
    async write(value: unknown) {
      const contents = encode(value);
      await mkdir(path.dirname(filename), { recursive: true });
      const temporary = `${filename}.${process.pid}.tmp`;
      try { await writeFile(temporary, contents, { encoding: 'utf8', mode: 0o600 }); await rename(temporary, filename); }
      catch (error) { throw new Error(`${filename}: package state could not be written.`, { cause: error }); }
    },
  };
}

// Startup validation lets the host isolate a package with corrupt state before it registers routes.
export function validatePackageState(repositoryRoot: string, scope: 'team' | 'member', packageId: string): void {
  const filename = stateFilename(repositoryRoot, scope, packageId);
  try { parse(readFileSync(filename, 'utf8'), filename); }
  catch (error) { if (error?.code !== 'ENOENT') throw error; }
}
