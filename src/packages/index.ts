import path from 'node:path';
import { realpathSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import type { PackageConfig, PackageDefinition, PackageInput, RepositoryConfig } from '../package-contract.ts';
import type { MemberConfig } from '../member.ts';
import { loadMemberManifest } from '../member.ts';
import type { PackageDiagnostic } from '../host.ts';

function toPath(value: string | URL): string { return value instanceof URL ? fileURLToPath(value) : value; }
function resolveModulePath(rootValue: string | URL, modulePath: string): string {
  if (!modulePath || path.posix.isAbsolute(modulePath) || path.win32.isAbsolute(modulePath) || /\\/.test(modulePath) || modulePath.split('/').includes('..')) throw new Error(`Package module must be a non-empty repository-relative path: ${modulePath}`);
  const root = path.resolve(toPath(rootValue));
  const absolute = path.resolve(root, modulePath);
  const relative = path.relative(root, absolute);
  if (relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) throw new Error(`Package module must stay inside its package repository: ${modulePath}`);
  try {
    const physicalRoot = realpathSync(root); const physicalCandidate = realpathSync(absolute); const physicalRelative = path.relative(physicalRoot, physicalCandidate);
    if (physicalRelative === '..' || physicalRelative.startsWith(`..${path.sep}`) || path.isAbsolute(physicalRelative)) throw new Error(`Package module must stay inside its package repository: ${modulePath}`);
  } catch (error) {
    if (error instanceof Error && /must stay inside/.test(error.message)) throw error;
  }
  return absolute;
}
function isPackageDefinition(value: unknown): value is PackageDefinition {
  if (!value || typeof value !== 'object') return false;
  const candidate = value as Partial<PackageDefinition>;
  const metadata = candidate.metadata;
  return Boolean(metadata && typeof metadata.id === 'string' && typeof metadata.version === 'string' && typeof metadata.hostVersion === 'string' && typeof metadata.label === 'string' && typeof metadata.order === 'number' && typeof candidate.register === 'function');
}
function packageInput(config: PackageConfig): PackageInput { return Object.fromEntries(Object.entries(config).filter(([key]) => key !== 'module' && key !== 'enabled')); }
function bindPackageInput(definition: PackageDefinition, config: PackageConfig, scope: 'team' | 'member', packageRoot?: string): PackageDefinition {
  return {
    metadata: definition.metadata,
    scope,
    packageRoot,
    register: (context) => {
      const registration = definition.register(context, packageInput(config));
      return scope === 'member' ? { ...registration, navigation: registration.navigation.map((item) => ({ ...item, scope: 'member' as const })) } : registration;
    },
  };
}
function diagnostic(diagnostics: PackageDiagnostic[] | undefined, id: string, status: PackageDiagnostic['status'], message: string): void {
  diagnostics?.push({ scope: 'member', id, status, message });
}

export async function loadConfiguredPackages({ config, memberConfig, appRoot, repositoryRoot, warn = console.warn, diagnostics }: { config: RepositoryConfig; memberConfig?: MemberConfig; appRoot: string | URL; repositoryRoot?: string | URL; warn?: (message: string) => void; diagnostics?: PackageDiagnostic[] }): Promise<PackageDefinition[]> {
  const definitions: PackageDefinition[] = [];
  let shellLoaded = false;
  const applicationRoot = toPath(appRoot);
  for (const [id, selection] of Object.entries(config.packages)) {
    if (selection.enabled === false) { if (id === 'shell') throw new Error('Shell package cannot be disabled.'); continue; }
    try {
      if (typeof selection.module !== 'string') throw new Error(`Package ${id} module must be a string.`);
      const filename = resolveModulePath(applicationRoot, selection.module);
      const loaded = await import(pathToFileURL(filename).href);
      const definition = loaded.default;
      if (!isPackageDefinition(definition) || definition.metadata.id !== id) throw new Error(`Module must default-export a PackageDefinition with id ${id}.`);
      definitions.push(bindPackageInput(definition, selection, 'team', applicationRoot));
      if (id === 'shell') shellLoaded = true;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (id === 'shell') throw new Error(`Unable to load Shell package ${selection.module}: ${message}`, { cause: error });
      warn(`Skipping package ${id} from ${String(selection.module)}: ${message}`);
    }
  }
  if (!shellLoaded) throw new Error('Shell package must be configured and load successfully.');

  if (!memberConfig) return definitions;
  const memberRoot = path.resolve(memberConfig.source);
  const selected = memberConfig.packages;
  let manifest;
  try { manifest = await loadMemberManifest(memberRoot); }
  catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    Object.keys(selected).forEach((id) => diagnostic(diagnostics, id, 'failed', message));
    warn(`Skipping member packages: ${message}`);
    return definitions;
  }
  for (const [id, selection] of Object.entries(selected)) {
    if (config.packages[id]) {
      const message = `Member package ${id} conflicts with team package ${id}.`;
      diagnostic(diagnostics, id, 'failed', message); warn(message); continue;
    }
    if (selection.enabled === false) { diagnostic(diagnostics, id, 'disabled', 'Member package is disabled in local configuration.'); continue; }
    const manifestSelection = manifest.packages[id];
    if (!manifestSelection) {
      const message = `Member package ${id} is not present in ${memberRoot}.`;
      diagnostic(diagnostics, id, 'failed', message); warn(message); continue;
    }
    try {
      if (typeof manifestSelection.module !== 'string') throw new Error(`Member package ${id} module must be a string.`);
      const filename = resolveModulePath(memberRoot, manifestSelection.module);
      const loaded = await import(pathToFileURL(filename).href);
      const definition = loaded.default;
      if (!isPackageDefinition(definition) || definition.metadata.id !== id) throw new Error(`Module must default-export a PackageDefinition with id ${id}.`);
      const combined = { ...manifestSelection, ...selection };
      definitions.push(bindPackageInput(definition, combined, 'member', memberRoot));
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      diagnostic(diagnostics, id, 'failed', message);
      warn(`Skipping member package ${id}: ${message}`);
    }
  }
  return definitions;
}
