import { realpathSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import type { AssetContribution, BrowserContribution, HostContext, HttpMethod, NavigationContribution, PackageDefinition, PackageInput, PackageRegistration, RepositoryConfig, RouteContribution } from './package-contract.ts';
import { MANIFEST_VERSION } from './package-contract.ts';
import { defaultRepositoryConfig } from './config.ts';
import { createPackageState, validatePackageState } from './state.ts';

function toPath(value: string | URL): string { return value instanceof URL ? fileURLToPath(value) : value; }
function createContext(repositoryRoot: string, appRoot: string): HostContext {
  const context: HostContext = {
    repositoryRoot, appRoot,
    resolveRepositoryPath(relativePath) {
      if (!relativePath || path.posix.isAbsolute(relativePath) || path.win32.isAbsolute(relativePath) || /\\/.test(relativePath)) return null;
      const root = path.resolve(repositoryRoot); const absolute = path.resolve(root, relativePath); const relative = path.relative(root, absolute);
      if (relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) return null;
      try {
        const physicalRoot = realpathSync(root);
        const physicalCandidate = realpathSync(absolute);
        const physicalRelative = path.relative(physicalRoot, physicalCandidate);
        if (physicalRelative === '..' || physicalRelative.startsWith(`..${path.sep}`) || path.isAbsolute(physicalRelative)) return null;
      } catch { return null; }
      return relative;
    },
  };
  return Object.freeze(context);
}
function assertPath(value: string, kind: string): void { if (!value || !value.startsWith('/') || value.includes('?') || value.includes('#') || /\\/.test(value)) throw new Error(`${kind} path must be an absolute pathname.`); }
function assertAssetFile(file: string, root: string): void {
  if (!file || path.posix.isAbsolute(file) || path.win32.isAbsolute(file) || /\\/.test(file) || file.split('/').includes('..')) throw new Error(`Asset file must stay inside its package root: ${file}`);
  const absolute = path.resolve(root, file); const relative = path.relative(path.resolve(root), absolute);
  if (relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) throw new Error(`Asset file must stay inside its package root: ${file}`);
  try {
    const physicalRoot = realpathSync(root); const physicalCandidate = realpathSync(absolute); const physicalRelative = path.relative(physicalRoot, physicalCandidate);
    if (physicalRelative === '..' || physicalRelative.startsWith(`..${path.sep}`) || path.isAbsolute(physicalRelative)) throw new Error(`Asset file must stay inside its package root: ${file}`);
  } catch (error) {
    if (error instanceof Error && /must stay inside/.test(error.message)) throw error;
  }
}
function assertRoutePath(pathname: string, packageId: string): void {
  assertPath(pathname, 'Route');
  if (pathname === '/api/manifest') throw new Error('The host manifest path is reserved.');
  const namespaced = pathname === `/api/${packageId}` || pathname.startsWith(`/api/${packageId}/`);
  if (!namespaced) throw new Error(`Route path must be namespaced for package ${packageId}: ${pathname}`);
}
function assertAssetPath(pathname: string, packageId: string): void {
  assertPath(pathname, 'Asset');
  const namespaced = pathname === `/assets/${packageId}` || pathname.startsWith(`/assets/${packageId}/`);
  const shellAsset = packageId === 'shell' && (pathname === '/' || pathname === '/assets/app.js' || pathname === '/assets/styles.css');
  if (!namespaced && !shellAsset) throw new Error(`Asset path must be namespaced for package ${packageId}: ${pathname}`);
}
function assertBrowserContribution(browser: BrowserContribution, packageId: string, assets: Record<string, AssetContribution>): void {
  if (browser.id !== packageId) throw new Error(`Browser package id does not match package: ${packageId}`);
  assertAssetPath(browser.entry, packageId); assertAssetPath(browser.stylesheet, packageId);
  if (!assets[browser.entry]) throw new Error(`Browser entry is not a registered asset: ${browser.entry}`);
  if (!assets[browser.stylesheet]) throw new Error(`Browser stylesheet is not a registered asset: ${browser.stylesheet}`);
}
function isPackageRegistration(value: unknown): value is PackageRegistration {
  if (!value || typeof value !== 'object') return false;
  const candidate = value as Partial<PackageRegistration>;
  const metadata = candidate.metadata;
  return Boolean(metadata && typeof metadata.id === 'string' && typeof metadata.version === 'string' && typeof metadata.hostVersion === 'string' && typeof metadata.label === 'string' && typeof metadata.order === 'number' && Array.isArray(candidate.routes) && Array.isArray(candidate.assets) && Array.isArray(candidate.navigation) && candidate.browser && typeof candidate.browser === 'object');
}
export function routeKey(method: HttpMethod, pathname: string): string { return `${method} ${pathname}`; }
export type PackageDiagnostic = { scope: 'member'; id: string; status: 'disabled' | 'failed'; message: string };
export type HostManifest = { version: typeof MANIFEST_VERSION; navigation: readonly NavigationContribution[]; packages: readonly BrowserContribution[]; diagnostics?: readonly PackageDiagnostic[] };
export type HostRegistry = { readonly context: HostContext; readonly routes: Readonly<Record<string, RouteContribution>>; readonly assets: Readonly<Record<string, AssetContribution>>; readonly manifest: HostManifest; dispose(): Promise<void> };
type MutableRegistry = { context: HostContext; routes: Record<string, RouteContribution>; assets: Record<string, AssetContribution>; navigation: NavigationContribution[]; packages: BrowserContribution[]; disposers: Array<() => void | Promise<void>>; diagnostics: PackageDiagnostic[] };
function addRegistration(registry: MutableRegistry, registration: PackageRegistration, seenPackages: Set<string>, packageRoot: string, scope: 'team' | 'member', packageContext: HostContext): void {
  const next: MutableRegistry = {
    context: registry.context,
    routes: { ...registry.routes },
    assets: { ...registry.assets },
    navigation: [...registry.navigation],
    packages: [...registry.packages],
    disposers: [...registry.disposers],
    diagnostics: [...registry.diagnostics],
  };
  const nextSeen = new Set(seenPackages);
  const { metadata } = registration;
  if (nextSeen.has(metadata.id)) throw new Error(`Duplicate package id: ${metadata.id}`);
  nextSeen.add(metadata.id);
  for (const route of registration.routes) {
    assertRoutePath(route.path, metadata.id);
    if (route.method !== 'GET' && route.method !== 'POST') throw new Error(`Unsupported route method: ${route.method}`);
    const key = routeKey(route.method, route.path);
    if (next.routes[key]) throw new Error(`Duplicate route path: ${route.path} (${route.method})`);
    next.routes[key] = { ...route, handler: (request, response) => route.handler(request, response, packageContext) };
  }
  for (const asset of registration.assets) {
    assertAssetPath(asset.path, metadata.id);
    assertAssetFile(asset.file, packageRoot);
    if (next.assets[asset.path]) throw new Error(`Duplicate asset path: ${asset.path}`);
    next.assets[asset.path] = packageRoot === path.resolve(registry.context.appRoot) ? asset : { ...asset, root: packageRoot };
  }
  assertBrowserContribution(registration.browser, metadata.id, next.assets);
  for (const contribution of registration.navigation) {
    const navigation = scope === 'member' ? { ...contribution, scope: 'member' as const } : contribution;
    if (next.navigation.some((item) => item.id === navigation.id)) throw new Error(`Duplicate navigation id: ${navigation.id}`);
    next.navigation.push(navigation);
  }
  if (next.packages.some((item) => item.id === registration.browser.id)) throw new Error(`Duplicate browser package id: ${registration.browser.id}`);
  next.packages.push(registration.browser);
  if (registration.dispose) next.disposers.push(registration.dispose);
  Object.assign(registry, next);
  seenPackages.clear();
  nextSeen.forEach((id) => seenPackages.add(id));
}

export function createHost({ root = process.cwd(), appRoot = process.cwd(), config = defaultRepositoryConfig(), memberConfig, packages = [], diagnostics = [], warn = console.warn }: { root?: string | URL; appRoot?: string | URL; config?: RepositoryConfig; memberConfig?: { packages: Record<string, PackageInput> }; packages?: PackageDefinition[]; diagnostics?: PackageDiagnostic[]; warn?: (message: string) => void } = {}): HostRegistry {
  const repositoryRoot = toPath(root); const applicationRoot = toPath(appRoot);
  const context = createContext(repositoryRoot, applicationRoot);
  const mutable: MutableRegistry = { context, routes: Object.create(null), assets: Object.create(null), navigation: [], packages: [], disposers: [], diagnostics: [...diagnostics] };
  const seenPackages = new Set<string>();
  for (const definition of packages) {
    const scope = definition.scope || 'team';
    const configured = scope === 'member' ? memberConfig?.packages[definition.metadata.id] : config.packages[definition.metadata.id];
    if (!configured || configured.enabled === false) {
      if (scope === 'member' && configured?.enabled === false) mutable.diagnostics.push({ scope, id: definition.metadata.id, status: 'disabled', message: 'Member package is disabled in local configuration.' });
      continue;
    }
    try {
      const packageRoot = path.resolve(definition.packageRoot || applicationRoot);
      validatePackageState(repositoryRoot, scope, definition.metadata.id);
      const packageContext = Object.freeze({ ...context, appRoot: packageRoot, state: createPackageState(repositoryRoot, scope, definition.metadata.id) });
      const input: PackageInput = configured || {};
      const registration = definition.register(packageContext, input);
      if (!isPackageRegistration(registration)) throw new Error(`Package ${definition.metadata.id} returned an invalid registration.`);
      if (registration.metadata.id !== definition.metadata.id) throw new Error(`Package registration id does not match definition: ${definition.metadata.id}`);
      addRegistration(mutable, registration, seenPackages, path.resolve(definition.packageRoot || applicationRoot), scope, packageContext);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (definition.metadata.id === 'shell') throw new Error(`Unable to register Shell package: ${message}`, { cause: error });
      if (scope === 'member') mutable.diagnostics.push({ scope, id: definition.metadata.id, status: 'failed', message });
      warn(`Skipping ${scope} package ${definition.metadata.id}: ${message}`);
    }
  }
  const navigation = Object.freeze([...mutable.navigation].sort((left, right) => left.order - right.order || (left.id < right.id ? -1 : left.id > right.id ? 1 : 0)).map((item) => Object.freeze({ ...item })));
  const packageManifest = Object.freeze(mutable.packages.map((item) => Object.freeze({ ...item })));
  const packageDiagnostics = Object.freeze(mutable.diagnostics.map((item) => Object.freeze({ ...item })));
  const manifest = Object.freeze({ version: MANIFEST_VERSION, navigation, packages: packageManifest, ...(packageDiagnostics.length ? { diagnostics: packageDiagnostics } : {}) });
  let disposed = false;
  async function dispose(): Promise<void> {
    if (disposed) return;
    disposed = true;
    for (const cleanup of [...mutable.disposers].reverse()) {
      try { await cleanup(); }
      catch (error) { warn(`Package cleanup failed: ${error instanceof Error ? error.message : String(error)}`); }
    }
  }
  return Object.freeze({ context, routes: Object.freeze({ ...mutable.routes }), assets: Object.freeze({ ...mutable.assets }), manifest, dispose });
}
