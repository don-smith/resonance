import { createHash } from 'node:crypto';
import { lstat, mkdir, readFile, readdir, realpath, rename, unlink, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { z } from 'zod';
import type { HostContext } from '../../package-contract.ts';
import { LikeC4 } from 'likec4';

export const architectureVersion = 1 as const;
export const entityTypes = ['system', 'package', 'module', 'concept', 'seam', 'dependency', 'data-store', 'data-flow', 'pattern', 'rule', 'decision', 'evidence'] as const;
export const c4Levels = ['system-context', 'container', 'component', 'code'] as const;
export const relationshipTypes = ['contains', 'depends-on', 'implements', 'uses', 'reads', 'writes', 'flows-through', 'conforms-to', 'documents', 'evidences'] as const;
export const checkStatuses = ['pass', 'fail', 'unknown'] as const;
export type CheckStatus = typeof checkStatuses[number];

const relativePath = z.string().min(1).refine((value) => !value.startsWith('/') && !value.includes('\\') && !value.split('/').includes('..'), 'Path must be repository-relative.');
const evidenceSchema = z.object({ path: relativePath, label: z.string().trim().min(1).max(200).optional(), line: z.number().int().positive().optional() }).strict();
const entitySchema = z.object({
  id: z.string().regex(/^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/),
  type: z.enum(entityTypes),
  c4: z.object({ level: z.enum(c4Levels), technology: z.string().trim().max(200).optional() }).strict().optional(),
  name: z.string().trim().min(1).max(200),
  description: z.string().max(16 * 1024).optional(),
  tags: z.array(z.string().trim().min(1).max(80)).max(32).optional(),
  evidence: z.array(evidenceSchema).max(32).optional(),
  attributes: z.record(z.string(), z.unknown()).optional(),
}).strict();
const relationshipSchema = z.object({
  id: z.string().regex(/^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/).optional(),
  type: z.enum(relationshipTypes),
  source: z.string().min(1),
  target: z.string().min(1),
  label: z.string().trim().max(200).optional(),
  evidence: z.array(evidenceSchema).max(32).optional(),
}).strict();
export const modelSchema = z.object({ version: z.literal(architectureVersion), entities: z.array(entitySchema).max(2000), relationships: z.array(relationshipSchema).max(5000) }).strict().superRefine((model, context) => {
  const ids = new Set<string>();
  model.entities.forEach((entity, index) => { if (ids.has(entity.id)) context.addIssue({ code: z.ZodIssueCode.custom, path: ['entities', index, 'id'], message: 'Entity IDs must be unique.' }); ids.add(entity.id); });
  const relationshipIds = new Set<string>();
  model.relationships.forEach((relationship, index) => {
    if (!ids.has(relationship.source) || !ids.has(relationship.target)) context.addIssue({ code: z.ZodIssueCode.custom, path: ['relationships', index], message: 'Relationships must reference modeled entity IDs.' });
    if (relationship.id && relationshipIds.has(relationship.id)) context.addIssue({ code: z.ZodIssueCode.custom, path: ['relationships', index, 'id'], message: 'Relationship IDs must be unique.' });
    if (relationship.id) relationshipIds.add(relationship.id);
  });
});
const querySchema = z.object({ types: z.array(z.enum(entityTypes)).max(32).optional(), tags: z.array(z.string()).max(32).optional(), ids: z.array(z.string()).max(2000).optional() }).strict();
const viewSchema = z.object({
  id: z.string().regex(/^[a-z][a-zA-Z0-9]*(?:-[a-zA-Z0-9]+)*$/),
  type: z.enum(c4Levels).optional(),
  name: z.string().trim().min(1).max(200),
  description: z.string().max(4000).optional(),
  query: querySchema.optional(),
  presentation: z.object({ groups: z.array(z.object({ id: z.string(), label: z.string() }).strict()).max(100).optional(), labels: z.record(z.string(), z.string()).optional(), styles: z.record(z.string(), z.record(z.string(), z.string())).optional(), layout: z.record(z.string(), z.object({ x: z.number().finite(), y: z.number().finite() }).strict()).optional() }).strict().optional(),
}).strict();
export const viewsSchema = z.object({ version: z.literal(architectureVersion), views: z.array(viewSchema).max(100) }).strict();
const ruleSchema = z.object({
  id: z.string().regex(/^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/),
  name: z.string().trim().min(1).max(200),
  description: z.string().max(4000),
  appliesTo: z.array(z.string().min(1)).max(200),
  checker: z.enum(['authoritative-config', 'package-ownership', 'route-asset-namespacing', 'repository-containment', 'git-revision']),
  severity: z.enum(['error', 'warning', 'info']),
  constraints: z.record(z.string(), z.unknown()).optional(),
}).strict();
export const rulesSchema = z.object({ version: z.literal(architectureVersion), rules: z.array(ruleSchema).max(200) }).strict();
const patternSchema = z.object({ id: z.string().regex(/^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/), name: z.string().trim().min(1).max(200), description: z.string().max(16 * 1024), evidence: z.array(evidenceSchema).max(32).optional() }).strict();
export const patternsSchema = z.object({ version: z.literal(architectureVersion), patterns: z.array(patternSchema).max(200) }).strict();
const decisionSchema = z.object({ id: z.string().regex(/^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/), title: z.string().trim().min(1).max(200), status: z.enum(['proposed', 'accepted', 'superseded', 'rejected']), summary: z.string().max(4000), path: relativePath.optional(), evidence: z.array(evidenceSchema).max(32).optional() }).strict();
export const decisionsSchema = z.object({ version: z.literal(architectureVersion), decisions: z.array(decisionSchema).max(200) }).strict();
export type ArchitectureModel = z.infer<typeof modelSchema>;
export type ArchitectureViews = z.infer<typeof viewsSchema>;
export type ArchitectureRules = z.infer<typeof rulesSchema>;
export type ArchitecturePatterns = z.infer<typeof patternsSchema>;
export type ArchitectureDecisions = z.infer<typeof decisionsSchema>;
export type ArchitectureArtifacts = { model: ArchitectureModel; views: ArchitectureViews; rules: ArchitectureRules; patterns: ArchitecturePatterns; decisions: ArchitectureDecisions; revision: string };
export type ArchitectureMutation = { revision: string; affectedPaths: string[] };
export type LikeC4Snapshot = { dump: unknown; views: Array<{ id: string; name: string; type: 'element' | 'dynamic' | 'deployment'; description?: string; parentId?: string }>; revision: string };
export type ArchitectureStore = {
  read(): Promise<ArchitectureArtifacts>;
  likec4(): Promise<LikeC4Snapshot>;
  graph(viewId: string, filter?: string): Promise<{ view: unknown; nodes: unknown[]; edges: unknown[]; revision: string }>;
  readEvidence(relative: string): Promise<{ path: string; content: string; truncated: boolean; entities: string[] }>;
  replace(kind: 'model' | 'views' | 'rules' | 'patterns' | 'decisions', value: unknown, expectedRevision: string): Promise<ArchitectureMutation>;
};

const filenames = { model: 'model.json', views: 'views.json', rules: 'rules.json', patterns: 'patterns.json', decisions: 'decisions.json' } as const;
const schemas = { model: modelSchema, views: viewsSchema, rules: rulesSchema, patterns: patternsSchema, decisions: decisionsSchema } as const;
const isMissing = (error: unknown) => Boolean(error && typeof error === 'object' && (error as { code?: unknown }).code === 'ENOENT');
const inside = (root: string, candidate: string) => candidate === root || candidate.startsWith(`${root}${path.sep}`);

export class ArchitectureSourceError extends Error { status = 422; }
export class ArchitectureStoreError extends Error { status = 404; }
export class ArchitectureValidationError extends ArchitectureStoreError { status = 400; }
export class ArchitectureStaleWriteError extends ArchitectureStoreError { status = 409; }

function parse<T>(kind: keyof typeof schemas, source: string): T {
  let value: unknown;
  try { value = JSON.parse(source); } catch (error) { throw new ArchitectureSourceError(`${filenames[kind]} is not valid JSON: ${error instanceof Error ? error.message : String(error)}`); }
  const result = schemas[kind].safeParse(value);
  if (!result.success) throw new ArchitectureSourceError(`${filenames[kind]} is invalid: ${result.error.issues[0].message}`);
  return result.data as T;
}
function hash(sources: Record<string, string>): string {
  const digest = createHash('sha256');
  for (const key of Object.keys(sources).sort()) digest.update(key).update('\0').update(sources[key]).update('\0');
  return digest.digest('hex');
}
function safeRoot(repositoryRoot: string, artifactRoot: string): string {
  if (!artifactRoot || artifactRoot.startsWith('/') || artifactRoot.includes('\\') || artifactRoot.split('/').includes('..') || artifactRoot.split('/').length > 2) throw new ArchitectureStoreError('Architecture artifactRoot must be a narrow repository-relative directory.');
  const root = path.resolve(repositoryRoot, artifactRoot);
  if (!inside(path.resolve(repositoryRoot), root)) throw new ArchitectureStoreError('Architecture artifactRoot escapes the repository.');
  return root;
}

export function createArchitectureStore({ context, artifactRoot = 'architecture' }: { context: HostContext; artifactRoot?: string }): ArchitectureStore {
  const repositoryRoot = path.resolve(context.repositoryRoot);
  const artifactDirectory = safeRoot(repositoryRoot, artifactRoot);
  let queue = Promise.resolve();
  const serialize = <T>(operation: () => Promise<T>) => { const previous = queue; const current = previous.then(operation, operation); queue = current.then(() => undefined, () => undefined); return current; };
  const canonical = (relative: string) => path.posix.join(artifactRoot, relative);
  const existingFile = async (relative: string) => {
    const repositoryPath = canonical(relative);
    const resolved = context.resolveRepositoryPath(repositoryPath);
    if (!resolved) throw new ArchitectureStoreError(`Architecture artifact not found: ${repositoryPath}`);
    const filename = path.resolve(repositoryRoot, resolved);
    const physicalRoot = await realpath(repositoryRoot);
    const physicalFile = await realpath(filename);
    if (!inside(physicalRoot, physicalFile) || !(await lstat(physicalFile)).isFile()) throw new ArchitectureStoreError(`Architecture artifact is not a regular file: ${repositoryPath}`);
    return filename;
  };
  const readSources = async () => {
    const sources = {} as Record<keyof typeof filenames, string>;
    for (const [kind, filename] of Object.entries(filenames) as [keyof typeof filenames, string][]) sources[kind] = await readFile(await existingFile(filename), 'utf8');
    return sources;
  };
  const readLikeC4Sources = async (directory: string, prefix = ''): Promise<Record<string, string>> => {
    const sources: Record<string, string> = {};
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const relative = prefix ? `${prefix}/${entry.name}` : entry.name;
      if (entry.isDirectory()) Object.assign(sources, await readLikeC4Sources(path.join(directory, entry.name), relative));
      else if (entry.name.endsWith('.c4') || entry.name.endsWith('.likec4')) {
        const repositoryPath = canonical(relative); const resolved = context.resolveRepositoryPath(repositoryPath);
        if (!resolved) throw new ArchitectureStoreError(`LikeC4 source is not contained in the repository: ${repositoryPath}`);
        const filename = path.resolve(repositoryRoot, resolved); const physicalRoot = await realpath(repositoryRoot); const physicalFile = await realpath(filename);
        if (!inside(physicalRoot, physicalFile) || !(await lstat(physicalFile)).isFile()) throw new ArchitectureStoreError(`LikeC4 source is not a regular file: ${repositoryPath}`);
        sources[relative] = await readFile(filename, 'utf8');
      }
    }
    return sources;
  };
  const readLikeC4 = async (): Promise<LikeC4Snapshot> => {
    const sources = await readLikeC4Sources(artifactDirectory);
    if (!Object.keys(sources).length) throw new ArchitectureSourceError('No LikeC4 source files were found in the architecture artifact root.');
    const likec4 = await LikeC4.fromWorkspace(artifactDirectory, { logger: false, graphviz: 'wasm' });
    try {
      if (likec4.hasErrors()) {
        const first = likec4.getErrors()[0];
        throw new ArchitectureSourceError(`LikeC4 source is invalid: ${first?.message || 'unknown parse error'}${first ? ` (${first.sourceFsPath}:${first.line + 1})` : ''}`);
      }
      const model = await likec4.layoutedModel();
      const modelViews = [...model.views()];
      const parentByView = new Map<string, string>();
      for (const child of modelViews) {
        if (child.id === 'index' || child.id === 'systemContext' || child.id === 'system-context' || !('viewOf' in child.$view)) continue;
        const childScope = child.$view.viewOf;
        const candidates = modelViews.filter((parent) => parent.id !== 'index' && parent.id !== child.id && parent.$view.nodes.some((node) => node.modelRef === childScope));
        const parent = candidates.find((candidate) => candidate.$view.nodes.some((node) => node.modelRef === childScope && node.navigateTo === child.id)) || candidates[0];
        if (parent) parentByView.set(child.id, parent.id);
      }
      const views = modelViews.map((view) => ({ id: view.id, name: view.title || view.id, type: view.$view._type, ...(view.description ? { description: view.description } : {}), ...(parentByView.has(view.id) ? { parentId: parentByView.get(view.id) } : {}) }));
      return { dump: model.$data, views, revision: hash(sources) };
    } finally { await likec4.dispose(); }
  };
  const read = async (): Promise<ArchitectureArtifacts> => {
    const sources = await readSources();
    return { model: parse('model', sources.model), views: parse('views', sources.views), rules: parse('rules', sources.rules), patterns: parse('patterns', sources.patterns), decisions: parse('decisions', sources.decisions), revision: hash(sources) };
  };
  const atomicWrite = async (filename: string, source: string) => {
    const physicalRoot = await realpath(repositoryRoot);
    const parent = path.dirname(filename);
    const parentPhysical = await realpath(parent);
    if (!inside(physicalRoot, parentPhysical)) throw new ArchitectureStoreError('Architecture write escapes the repository.');
    await mkdir(parent, { recursive: true });
    const temporary = path.join(parent, `.${path.basename(filename)}.${crypto.randomUUID()}.tmp`);
    try { await writeFile(temporary, source, { encoding: 'utf8', mode: 0o600 }); await rename(temporary, filename); }
    catch (error) { await unlink(temporary).catch(() => undefined); throw error; }
  };
  return {
    read,
    likec4: readLikeC4,
    async graph(viewId, filter = '') {
      const snapshot = await readLikeC4();
      const normalizedId = viewId.replace(/-([a-z])/g, (_match, letter: string) => letter.toUpperCase());
      const data = snapshot.dump as { views?: Record<string, { id: string; title?: string | null; description?: string | null; nodes?: unknown[]; edges?: unknown[] }> };
      const view = data.views?.[viewId] || data.views?.[normalizedId];
      if (!view) throw new ArchitectureStoreError(`LikeC4 view not found: ${viewId}`);
      let nodes = view.nodes || [];
      if (filter.trim()) { const needle = filter.trim().toLowerCase(); nodes = nodes.filter((node) => JSON.stringify(node).toLowerCase().includes(needle)); }
      const ids = new Set(nodes.map((node) => (node as { id?: string }).id));
      const edges = (view.edges || []).filter((edge) => ids.has((edge as { source?: string }).source) && ids.has((edge as { target?: string }).target));
      return { view, nodes, edges, revision: snapshot.revision };
    },
    async readEvidence(relative) {
      const artifacts = await read();
      const normalized = relative.replaceAll('\\', '/');
      const references = new Set<string>();
      for (const entity of artifacts.model.entities) for (const evidence of entity.evidence || []) if (evidence.path === normalized) references.add(entity.id);
      for (const edge of artifacts.model.relationships) for (const evidence of edge.evidence || []) if (evidence.path === normalized) references.add(edge.source);
      for (const pattern of artifacts.patterns.patterns) for (const evidence of pattern.evidence || []) if (evidence.path === normalized) references.add(pattern.id);
      for (const decision of artifacts.decisions.decisions) for (const evidence of decision.evidence || []) if (evidence.path === normalized) references.add(decision.id);
      if (references.size === 0) throw new ArchitectureStoreError('Evidence path is not linked from the architecture model.');
      const resolved = context.resolveRepositoryPath(normalized);
      if (!resolved) throw new ArchitectureStoreError('Evidence file is not contained in the repository.');
      const contents = await readFile(path.resolve(repositoryRoot, resolved), 'utf8');
      const limit = 64 * 1024;
      return { path: normalized, content: contents.slice(0, limit), truncated: contents.length > limit, entities: [...references] };
    },
    async replace(kind, value, expectedRevision) { return serialize(async () => {
      const current = await read();
      if (current.revision !== expectedRevision) throw new ArchitectureStaleWriteError('Architecture artifacts changed; reload before writing.');
      const result = schemas[kind].safeParse(value);
      if (!result.success) throw new ArchitectureValidationError(`Architecture ${kind} is invalid: ${result.error.issues[0].message}`);
      const filename = path.join(artifactDirectory, filenames[kind]);
      const source = `${JSON.stringify(result.data, null, 2)}\n`;
      await atomicWrite(filename, source);
      const next = await read();
      return { revision: next.revision, affectedPaths: [canonical(filenames[kind])] };
    }); },
  };
}

export const architectureQuerySchema = querySchema;