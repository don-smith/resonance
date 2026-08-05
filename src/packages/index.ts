import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import type { PackageConfig, PackageDefinition, RepositoryConfig } from '../package-contract.ts';

function toPath(value: string | URL): string { return value instanceof URL ? fileURLToPath(value) : value; }
function resolveModulePath(appRoot: string | URL, modulePath: string): string {
  if (!modulePath || path.posix.isAbsolute(modulePath) || path.win32.isAbsolute(modulePath) || /\\/.test(modulePath) || modulePath.split('/').includes('..')) throw new Error(`Package module must be a non-empty app-relative path: ${modulePath}`);
  const root = path.resolve(toPath(appRoot));
  const absolute = path.resolve(root, modulePath);
  const relative = path.relative(root, absolute);
  if (relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) throw new Error(`Package module must stay inside the application root: ${modulePath}`);
  return absolute;
}
function isPackageDefinition(value: unknown): value is PackageDefinition {
  if (!value || typeof value !== 'object') return false;
  const candidate = value as Partial<PackageDefinition>;
  const metadata = candidate.metadata;
  return Boolean(metadata && typeof metadata.id === 'string' && typeof metadata.version === 'string' && typeof metadata.hostVersion === 'string' && typeof metadata.label === 'string' && typeof metadata.order === 'number' && typeof candidate.register === 'function');
}
function packageInput(config: PackageConfig) { return Object.fromEntries(Object.entries(config).filter(([key]) => key !== 'module' && key !== 'enabled')); }
function bindPackageInput(definition: PackageDefinition, config: PackageConfig): PackageDefinition {
  const input = packageInput(config);
  return { metadata: definition.metadata, register: (context) => definition.register(context, input) };
}

export async function loadConfiguredPackages({ config, appRoot, warn = console.warn }: { config: RepositoryConfig; appRoot: string | URL; warn?: (message: string) => void }): Promise<PackageDefinition[]> {
  const definitions: PackageDefinition[] = [];
  let shellLoaded = false;
  for (const [id, selection] of Object.entries(config.packages)) {
    if (selection.enabled === false) { if (id === 'shell') throw new Error('Shell package cannot be disabled.'); continue; }
    try {
      if (typeof selection.module !== 'string') throw new Error(`Package ${id} module must be a string.`);
      const filename = resolveModulePath(appRoot, selection.module);
      const loaded = await import(pathToFileURL(filename).href);
      const definition = loaded.default;
      if (!isPackageDefinition(definition) || definition.metadata.id !== id) throw new Error(`Module must default-export a PackageDefinition with id ${id}.`);
      definitions.push(bindPackageInput(definition, selection));
      if (id === 'shell') shellLoaded = true;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (id === 'shell') throw new Error(`Unable to load Shell package ${selection.module}: ${message}`, { cause: error });
      warn(`Skipping package ${id} from ${String(selection.module)}: ${message}`);
    }
  }
  if (!shellLoaded) throw new Error('Shell package must be configured and load successfully.');
  return definitions;
}
