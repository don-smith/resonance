import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export type RepositoryPresentation = {
  name: string;
  tagline: string;
  version?: string;
};

function toPath(value: string | URL): string { return value instanceof URL ? fileURLToPath(value) : value; }

function packageMetadata(root: string): { name?: string; description?: string; version?: string } {
  try {
    const value = JSON.parse(readFileSync(path.join(root, 'package.json'), 'utf8'));
    if (!value || typeof value !== 'object' || Array.isArray(value)) return {};
    const metadata = value as Record<string, unknown>;
    return {
      name: typeof metadata.name === 'string' ? metadata.name : undefined,
      description: typeof metadata.description === 'string' ? metadata.description : undefined,
      version: typeof metadata.version === 'string' ? metadata.version : undefined,
    };
  } catch { return {}; }
}

export function repositoryName(rootValue: string | URL): string {
  const root = path.resolve(toPath(rootValue));
  let gitPath = path.join(root, '.git');
  try {
    const gitPointer = readFileSync(gitPath, 'utf8').trim();
    if (gitPointer.startsWith('gitdir:')) gitPath = path.resolve(root, gitPointer.slice('gitdir:'.length).trim());
  } catch {}
  try {
    const gitConfig = readFileSync(path.join(gitPath, 'config'), 'utf8');
    const originSection = /\[remote "origin"\]([\s\S]*?)(?:\n\[|$)/.exec(gitConfig)?.[1] || '';
    const origin = /^\s*url\s*=\s*(\S+)/m.exec(originSection)?.[1]?.replace(/\/$/, '');
    const remoteName = origin && /([^/:]+?)(?:\.git)?$/.exec(origin)?.[1];
    if (remoteName) return remoteName;
  } catch {}
  return packageMetadata(root).name || path.basename(root) || root;
}

export function repositoryPresentation(rootValue: string | URL, configured: { name?: string; tagline?: string } = {}): RepositoryPresentation {
  const root = path.resolve(toPath(rootValue));
  const metadata = packageMetadata(root);
  return {
    name: configured.name ?? repositoryName(root),
    tagline: configured.tagline ?? metadata.description ?? '',
    ...(metadata.version ? { version: metadata.version } : {}),
  };
}

export function runtimeVersion(rootValue: string | URL): string | undefined {
  return packageMetadata(path.resolve(toPath(rootValue))).version;
}
