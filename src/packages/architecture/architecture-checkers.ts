import { readFile } from 'node:fs/promises';
import path from 'node:path';
import type { HostContext } from '../../package-contract.ts';
import type { ArchitectureArtifacts, ArchitectureModel, CheckStatus } from './architecture-store.ts';

export type ArchitectureFinding = { ruleId: string; name: string; severity: 'error' | 'warning' | 'info'; status: CheckStatus; message: string; checker: string; version: string; evidence: string[] };
export type ArchitectureValidation = { revision: string; results: ArchitectureFinding[] };
type Rule = ArchitectureArtifacts['rules']['rules'][number];

const boundedRead = async (filename: string, max = 256 * 1024) => { const source = await readFile(filename, 'utf8'); return source.length > max ? source.slice(0, max) : source; };
const configPath = (context: HostContext) => context.resolveRepositoryPath('.resonance/config.json');
async function repositoryConfig(context: HostContext): Promise<{ value: Record<string, unknown>; evidence: string } | null> {
  const relative = configPath(context);
  if (!relative) return null;
  try { return { value: JSON.parse(await boundedRead(path.resolve(context.repositoryRoot, relative), 128 * 1024)) as Record<string, unknown>, evidence: relative }; }
  catch { return null; }
}
function finding(rule: Rule, status: CheckStatus, message: string, evidence: string[] = []): ArchitectureFinding { return { ruleId: rule.id, name: rule.name, severity: rule.severity, status, message, checker: rule.checker, version: '1', evidence }; }
function packageEntities(model: ArchitectureModel) { return model.entities.filter((entity) => entity.type === 'package'); }
function evidencePaths(model: ArchitectureModel): string[] {
  const paths = new Set<string>();
  for (const entity of model.entities) for (const evidence of entity.evidence || []) paths.add(evidence.path);
  for (const relationship of model.relationships) for (const evidence of relationship.evidence || []) paths.add(evidence.path);
  return [...paths];
}

async function checkAuthoritativeConfig(context: HostContext, rule: Rule): Promise<ArchitectureFinding> {
  const loaded = await repositoryConfig(context);
  if (!loaded) return finding(rule, 'unknown', 'The repository manifest could not be read.', ['.resonance/config.json']);
  const packages = loaded.value.packages;
  if (loaded.value.version !== 1 || !packages || typeof packages !== 'object' || Array.isArray(packages)) return finding(rule, 'fail', 'The repository manifest is not an authoritative version 1 package configuration.', [loaded.evidence]);
  return finding(rule, 'pass', 'The repository uses an explicit version 1 package allowlist.', [loaded.evidence]);
}
async function checkShell(context: HostContext, rule: Rule): Promise<ArchitectureFinding> {
  const loaded = await repositoryConfig(context);
  if (!loaded) return finding(rule, 'unknown', 'The repository manifest could not be read.', ['.resonance/config.json']);
  const packages = loaded.value.packages;
  const shell = packages && typeof packages === 'object' && !Array.isArray(packages) ? (packages as Record<string, unknown>).shell : undefined;
  if (!shell || typeof shell !== 'object' || Array.isArray(shell)) return finding(rule, 'fail', 'Shell is not configured.', [loaded.evidence]);
  if ((shell as Record<string, unknown>).enabled === false) return finding(rule, 'fail', 'Shell is configured but disabled.', [loaded.evidence]);
  return finding(rule, 'pass', 'Shell is explicitly configured and enabled.', [loaded.evidence]);
}
async function checkOwnership(context: HostContext, artifacts: ArchitectureArtifacts, rule: Rule): Promise<ArchitectureFinding> {
  const loaded = await repositoryConfig(context);
  if (!loaded) return finding(rule, 'unknown', 'The repository manifest could not be read.', ['.resonance/config.json']);
  const packages = loaded.value.packages;
  if (!packages || typeof packages !== 'object' || Array.isArray(packages)) return finding(rule, 'unknown', 'The repository package allowlist is unavailable.', ['.resonance/config.json']);
  const problems: string[] = [];
  const packageList = packages as Record<string, unknown>;
  for (const entity of packageEntities(artifacts.model)) {
    const packageId = typeof entity.attributes?.packageId === 'string' ? entity.attributes.packageId : entity.id.replace(/-package$/, '');
    const selection = packageList[packageId];
    const module = typeof selection === 'object' && selection && !Array.isArray(selection) ? (selection as Record<string, unknown>).module : undefined;
    const expected = entity.evidence?.find((item) => item.path.endsWith('/index.ts') || item.path.endsWith('/index.js'))?.path;
    if (!selection || typeof module !== 'string') problems.push(`${entity.name} is not represented by ${packageId} in the manifest`);
    else if (expected && module !== expected) problems.push(`${entity.name} points at ${expected}, but the manifest points at ${module}`);
  }
  if (problems.length) return finding(rule, 'fail', problems.join('; '), ['.resonance/config.json']);
  return finding(rule, 'pass', `All ${packageEntities(artifacts.model).length} modeled packages have explicit manifest ownership.`, ['.resonance/config.json']);
}
async function checkRoutes(context: HostContext, rule: Rule): Promise<ArchitectureFinding> {
  const loaded = await repositoryConfig(context);
  if (!loaded || !loaded.value.packages || typeof loaded.value.packages !== 'object' || Array.isArray(loaded.value.packages)) return finding(rule, 'unknown', 'The repository package modules could not be inspected.', ['.resonance/config.json']);
  const problems: string[] = [];
  const evidence: string[] = [];
  for (const [id, selection] of Object.entries(loaded.value.packages as Record<string, unknown>)) {
    if (!selection || typeof selection !== 'object' || Array.isArray(selection)) continue;
    const module = (selection as Record<string, unknown>).module;
    if (typeof module !== 'string' || module.includes('..') || module.includes('\\') || module.startsWith('/')) { problems.push(`${id} has no safe module path`); continue; }
    const filename = path.resolve(context.appRoot, module);
    try {
      const source = await boundedRead(filename);
      evidence.push(module);
      const routePaths = [...source.matchAll(/path:\s*['"](\/api\/[^'"]*)['"]/g)].map((match) => match[1]);
      const assetPaths = [...source.matchAll(/path:\s*['"](\/assets\/[^'"]*)['"]/g)].map((match) => match[1]);
      for (const route of routePaths) if (!(route === `/api/${id}` || route.startsWith(`/api/${id}/`))) problems.push(`${id} route is not namespaced: ${route}`);
      for (const asset of assetPaths) if (!(id === 'shell' && (asset === '/assets/app.js' || asset === '/assets/styles.css')) && !(asset === `/assets/${id}` || asset.startsWith(`/assets/${id}/`))) problems.push(`${id} asset is not namespaced: ${asset}`);
    } catch { problems.push(`${id} module could not be read`); }
  }
  return problems.length ? finding(rule, 'fail', problems.join('; '), evidence) : finding(rule, 'pass', 'Configured package routes and assets use package namespaces.', evidence);
}
async function checkContainment(context: HostContext, artifacts: ArchitectureArtifacts, rule: Rule): Promise<ArchitectureFinding> {
  const paths = evidencePaths(artifacts.model);
  const missing: string[] = [];
  for (const relative of paths) if (!context.resolveRepositoryPath(relative)) missing.push(relative);
  if (missing.length) return finding(rule, 'unknown', `Evidence files are unavailable: ${missing.join(', ')}`, missing);
  return finding(rule, 'pass', 'All modeled evidence paths remain within the viewed repository.', paths);
}
async function checkGit(context: HostContext, rule: Rule): Promise<ArchitectureFinding> {
  const head = context.resolveRepositoryPath('.git/HEAD');
  if (!head) return finding(rule, 'unknown', 'The viewed repository does not expose a Git HEAD.', ['.git/HEAD']);
  try { const value = (await boundedRead(path.resolve(context.repositoryRoot, head), 4096)).trim(); return value ? finding(rule, 'pass', 'A Git revision source is available for architecture reports.', ['.git/HEAD']) : finding(rule, 'unknown', 'Git HEAD is empty.', ['.git/HEAD']); }
  catch { return finding(rule, 'unknown', 'Git HEAD could not be read.', ['.git/HEAD']); }
}

export async function validateArchitecture(context: HostContext, artifacts: ArchitectureArtifacts): Promise<ArchitectureValidation> {
  const results: ArchitectureFinding[] = [];
  for (const rule of artifacts.rules.rules) {
    let result: ArchitectureFinding;
    if (rule.checker === 'authoritative-config') result = await checkAuthoritativeConfig(context, rule);
    else if (rule.checker === 'package-ownership') result = await checkOwnership(context, artifacts, rule);
    else if (rule.checker === 'route-asset-namespacing') result = await checkRoutes(context, rule);
    else if (rule.checker === 'repository-containment') result = await checkContainment(context, artifacts, rule);
    else result = await checkGit(context, rule);
    results.push(result);
  }
  return { revision: artifacts.revision, results };
}