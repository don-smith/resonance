import { lstat, mkdir, readFile, realpath, rename, unlink, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { parseDocument, stringify } from 'yaml';
import { z } from 'zod';

export const backlogStatuses = ['recently-done', 'in-progress', 'is-ready', 'in-planning'] as const;
export const backlogPriorities = ['P0', 'P1', 'P2', 'P3'] as const;
export type BacklogStatus = typeof backlogStatuses[number];
export type BacklogPriority = typeof backlogPriorities[number];
export type BacklogDecisionSummary = { path: string; title: string; status: BacklogStatus; priority: BacklogPriority };
export type BacklogDecision = BacklogDecisionSummary & { markdown: string };
export type BacklogCreateInput = { title: string; status: BacklogStatus; priority: BacklogPriority; markdown: string };
export type BacklogMetadata = { status?: BacklogStatus; priority?: BacklogPriority };
export type BacklogMutation = { affectedPaths: string[] };
export type BacklogStore = {
  listDecisions(): Promise<BacklogDecisionSummary[]>;
  readDecision(path: string): Promise<BacklogDecision>;
  createDecision(input: BacklogCreateInput): Promise<BacklogMutation>;
  editPlan(path: string, markdown: string): Promise<BacklogMutation>;
  updateMetadata(path: string, metadata: BacklogMetadata): Promise<BacklogMutation>;
  setStatus(path: string, status: BacklogStatus): Promise<BacklogMutation>;
  setPriority(path: string, priority: BacklogPriority): Promise<BacklogMutation>;
  deleteDecision(path: string): Promise<BacklogMutation>;
};

const decisionSchema = z.object({
  title: z.string().trim().min(1),
  plan: z.string().min(1).refine((value) => !value.startsWith('/') && !value.includes('\\'), 'Plan must be a forward-slash relative path.'),
  status: z.enum(backlogStatuses),
  priority: z.enum(backlogPriorities),
}).strict();
const createSchema = z.object({
  title: z.string().trim().min(1).max(200),
  status: z.enum(backlogStatuses),
  priority: z.enum(backlogPriorities),
  markdown: z.string().max(256 * 1024),
}).strict();
const backlogSchema = z.object({ version: z.literal(1), decisions: z.array(decisionSchema).superRefine((decisions, context) => {
  const seen = new Set<string>();
  decisions.forEach((decision, index) => {
    const identity = canonicalPlan(decision.plan);
    if (seen.has(identity)) context.addIssue({ code: z.ZodIssueCode.custom, path: [index, 'plan'], message: 'Plan paths must be unique.' });
    seen.add(identity);
  });
}) }).strict();
export type ParsedDecision = z.infer<typeof decisionSchema>;

type FileSystem = {
  lstat: typeof lstat;
  mkdir: typeof mkdir;
  readFile: typeof readFile;
  realpath: typeof realpath;
  rename: typeof rename;
  unlink: typeof unlink;
  writeFile: typeof writeFile;
};
const defaultFileSystem: FileSystem = { lstat, mkdir, readFile, realpath, rename, unlink, writeFile };
const TODO_PATH = 'backlog/todo.yaml';

export class BacklogStoreError extends Error {
  constructor(message: string, options?: { cause?: unknown }) { super(message, options); this.name = 'BacklogStoreError'; }
}
export class BacklogSourceError extends BacklogStoreError {
  constructor(message: string) { super(message); this.name = 'BacklogSourceError'; }
}

const isMissing = (error: unknown) => Boolean(error && typeof error === 'object' && (error as { code?: unknown }).code === 'ENOENT');
const within = (root: string, candidate: string) => candidate === root || candidate.startsWith(`${root}${path.sep}`);
const canonicalPlan = (plan: string) => path.posix.join('backlog', plan);
const isBacklogPath = (relativePath: string) => relativePath === 'backlog' || relativePath.startsWith('backlog/');

export function planForTitle(title: string): string {
  const slug = title.normalize('NFKD').replace(/[\u0300-\u036f]/g, '').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
  if (!slug) throw new BacklogStoreError('Decision title must contain at least one letter or number.');
  return `plans/${slug}.md`;
}

export function parseBacklogItems(source: string): ParsedDecision[] {
  let document;
  try { document = parseDocument(source, { uniqueKeys: true }); }
  catch (error) { throw new BacklogSourceError(`Backlog source is invalid: ${error instanceof Error ? error.message : String(error)}`); }
  if (document.errors.length > 0) throw new BacklogSourceError(`Backlog source is invalid: ${document.errors[0].message}`);
  const parsed = backlogSchema.safeParse(document.toJS());
  if (!parsed.success) throw new BacklogSourceError(`Backlog source is invalid: ${parsed.error.issues[0].message}`);
  return parsed.data.decisions;
}

export function createBacklogStore({ repositoryRoot, fileSystem: overrides = {} }: { repositoryRoot: string; fileSystem?: Partial<FileSystem> }): BacklogStore {
  const fs: FileSystem = { ...defaultFileSystem, ...overrides };
  let queue = Promise.resolve();
  const serialize = <T>(operation: () => Promise<T>) => {
    const previous = queue;
    const current = previous.then(operation, operation);
    queue = current.then(() => undefined, () => undefined);
    return current;
  };
  const rootPath = () => fs.realpath(path.resolve(repositoryRoot));
  const lexicalPath = (root: string, relativePath: string) => {
    const candidate = path.resolve(root, relativePath);
    if (!within(root, candidate)) throw new BacklogStoreError(`Path escapes the repository: ${relativePath}`);
    return candidate;
  };
  const existingPath = async (root: string, relativePath: string) => {
    const candidate = lexicalPath(root, relativePath);
    const physical = await fs.realpath(candidate);
    if (!within(root, physical)) throw new BacklogStoreError(`Path escapes the repository: ${relativePath}`);
    if (!(await fs.lstat(physical)).isFile()) throw new BacklogStoreError(`Backlog plan is not a regular file: ${relativePath}`);
    return candidate;
  };
  const existingOrMissing = async (root: string, relativePath: string): Promise<string | null> => {
    try { return await existingPath(root, relativePath); }
    catch (error) { if (isMissing(error) || error instanceof BacklogStoreError) return null; throw error; }
  };
  const prospectivePath = async (root: string, relativePath: string) => {
    const candidate = lexicalPath(root, relativePath);
    let parent = path.dirname(candidate);
    while (true) {
      try {
        const physical = await fs.realpath(parent);
        if (!within(root, physical)) throw new BacklogStoreError(`Path escapes the repository: ${relativePath}`);
        break;
      } catch (error) {
        if (!isMissing(error) || parent === root) throw error;
        parent = path.dirname(parent);
      }
    }
    try {
      const physical = await fs.realpath(candidate);
      if (!within(root, physical)) throw new BacklogStoreError(`Path escapes the repository: ${relativePath}`);
    } catch (error) {
      if (!isMissing(error)) throw error;
    }
    return candidate;
  };
  const readText = async (filename: string) => await fs.readFile(filename, 'utf8') as string;
  const load = async () => {
    const root = await rootPath();
    const todo = await existingPath(root, TODO_PATH);
    const source = await readText(todo);
    const decisions = parseBacklogItems(source);
    const entries = await Promise.all(decisions.map(async (decision, sourceIndex) => ({
      decision,
      sourceIndex,
      path: canonicalPlan(decision.plan),
      filename: isBacklogPath(canonicalPlan(decision.plan)) ? await existingOrMissing(root, canonicalPlan(decision.plan)) : null,
    })));
    return { root, todo, source, decisions, entries };
  };
  const findEntry = async (state: Awaited<ReturnType<typeof load>>, requestedPath: string) => {
    const entry = state.entries.find((candidate) => candidate.path === requestedPath && candidate.filename);
    if (!entry || !entry.filename) throw new BacklogStoreError('Backlog item not found.');
    return entry as typeof entry & { filename: string };
  };
  const summaries = (state: Awaited<ReturnType<typeof load>>) => state.entries
    .filter((entry): entry is typeof entry & { filename: string } => Boolean(entry.filename))
    .sort((left, right) => backlogStatuses.indexOf(left.decision.status) - backlogStatuses.indexOf(right.decision.status)
      || backlogPriorities.indexOf(left.decision.priority) - backlogPriorities.indexOf(right.decision.priority)
      || left.sourceIndex - right.sourceIndex)
    .map(({ decision, path: decisionPath }) => ({ path: decisionPath, title: decision.title, status: decision.status, priority: decision.priority }));
  const atomicReplace = async (filename: string, content: string) => {
    const temporary = path.join(path.dirname(filename), `.${path.basename(filename)}.${crypto.randomUUID()}.tmp`);
    try {
      await fs.writeFile(temporary, content, { encoding: 'utf8', mode: 0o600 });
      await fs.rename(temporary, filename);
    } catch (error) {
      await fs.unlink(temporary).catch(() => undefined);
      throw error;
    }
  };
  const replaceYaml = (state: Awaited<ReturnType<typeof load>>, decisions: ParsedDecision[]) => atomicReplace(state.todo, stringify({ version: 1, decisions }));
  const requireCreate = (input: BacklogCreateInput) => {
    const parsed = createSchema.safeParse(input);
    if (!parsed.success) throw new BacklogStoreError(`Backlog decision is invalid: ${parsed.error.issues[0].message}`);
    const { markdown, ...fields } = parsed.data;
    const decision = { ...fields, plan: planForTitle(fields.title) };
    return { decision, markdown };
  };
  const updateMetadata = (requestedPath: string, metadata: BacklogMetadata) => serialize(async () => {
    if (metadata.status !== undefined && !backlogStatuses.includes(metadata.status)) throw new BacklogStoreError('Backlog status is invalid.');
    if (metadata.priority !== undefined && !backlogPriorities.includes(metadata.priority)) throw new BacklogStoreError('Backlog priority is invalid.');
    if (metadata.status === undefined && metadata.priority === undefined) throw new BacklogStoreError('Backlog metadata is empty.');
    const state = await load(); const entry = await findEntry(state, requestedPath);
    await replaceYaml(state, state.decisions.map((decision) => decision.plan === entry.decision.plan ? { ...decision, ...(metadata.status === undefined ? {} : { status: metadata.status }), ...(metadata.priority === undefined ? {} : { priority: metadata.priority }) } : decision));
    return { affectedPaths: [TODO_PATH] };
  });
  return {
    async listDecisions() { return summaries(await load()); },
    async readDecision(requestedPath) {
      const state = await load();
      const entry = await findEntry(state, requestedPath);
      return { path: entry.path, title: entry.decision.title, status: entry.decision.status, priority: entry.decision.priority, markdown: await readText(entry.filename) };
    },
    async createDecision(input) { return serialize(async () => {
      const { decision, markdown } = requireCreate(input);
      const state = await load();
      if (state.decisions.some((candidate) => canonicalPlan(candidate.plan) === canonicalPlan(decision.plan))) throw new BacklogStoreError('Plan paths must be unique.');
      const planRelativePath = canonicalPlan(decision.plan);
      const filename = await prospectivePath(state.root, planRelativePath);
      try { await fs.lstat(filename); throw new BacklogStoreError('Backlog plan already exists.'); }
      catch (error) { if (!isMissing(error)) throw error; }
      await fs.mkdir(path.dirname(filename), { recursive: true });
      try {
        await atomicReplace(filename, markdown);
        await replaceYaml(state, [...state.decisions, decision]);
      } catch (error) {
        try { await fs.unlink(filename); }
        catch (rollbackError) { throw new BacklogStoreError('Backlog creation failed and rollback also failed.', { cause: rollbackError }); }
        throw new BacklogStoreError('Backlog creation failed and was compensated.', { cause: error });
      }
      return { affectedPaths: [TODO_PATH, planRelativePath] };
    }); },
    async editPlan(requestedPath, markdown) { return serialize(async () => {
      if (typeof markdown !== 'string' || markdown.length > 256 * 1024) throw new BacklogStoreError('Plan Markdown is invalid.');
      const state = await load(); const entry = await findEntry(state, requestedPath);
      await atomicReplace(entry.filename, markdown);
      return { affectedPaths: [requestedPath] };
    }); },
    updateMetadata,
    async setStatus(requestedPath, status) { return updateMetadata(requestedPath, { status }); },
    async setPriority(requestedPath, priority) { return updateMetadata(requestedPath, { priority }); },
    async deleteDecision(requestedPath) { return serialize(async () => {
      const state = await load(); const entry = await findEntry(state, requestedPath);
      const originalPlan = await readText(entry.filename);
      const decisions = state.decisions.filter((decision) => decision.plan !== entry.decision.plan);
      try {
        await fs.unlink(entry.filename);
        await replaceYaml(state, decisions);
      } catch (error) {
        try { await atomicReplace(entry.filename, originalPlan); }
        catch (rollbackError) { throw new BacklogStoreError('Backlog deletion failed and rollback also failed.', { cause: rollbackError }); }
        throw new BacklogStoreError('Backlog deletion failed and was compensated.', { cause: error });
      }
      return { affectedPaths: [TODO_PATH, requestedPath] };
    }); },
  };
}

export type { FileSystem };
