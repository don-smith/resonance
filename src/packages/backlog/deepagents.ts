import { lstat, readdir, readFile, realpath } from 'node:fs/promises';
import path from 'node:path';
import type { Telemetry } from '../../package-contract.ts';
import { createDeepAgent, type BackendProtocolV2 } from 'deepagents';
import { ChatOpenAI } from '@langchain/openai';
import { tool } from 'langchain';
import { z } from 'zod/v4';
import type { BacklogDecision, BacklogMutation, BacklogStore } from './backlog-store.ts';
import type { BacklogAgentRuntime, BacklogAgentRuntimeFactory, BacklogAgentRuntimeFactoryOptions, BacklogAgentTurn, BacklogAgentUpdate } from './agent-session.ts';

const skillRoot = '/skills';
const skillPath = '/skills/manage-backlog/SKILL.md';
const backlogSystemPrompt = 'You are Resonance Backlog Agent. Read /skills/manage-backlog/SKILL.md before acting. The whole viewed repository is mounted read-only at /. Use ls, read_file, glob, and grep to inspect repository context. Use only package-owned Backlog domain tools to access canonical Backlog data or make changes; generic filesystem writes are unavailable. Never inspect credentials or use shell or network access.';
const denied = (requestedPath: string) => `Permission denied: ${requestedPath} is not available to the Backlog agent.`;
const maxRepositoryFileBytes = 10 * 1024 * 1024;
const mimeTypes: Record<string, string> = {
  '.c4': 'text/plain', '.css': 'text/css', '.csv': 'text/csv', '.html': 'text/html', '.js': 'text/javascript',
  '.json': 'application/json', '.md': 'text/markdown', '.mjs': 'text/javascript', '.ts': 'text/typescript',
  '.tsx': 'text/typescript', '.txt': 'text/plain', '.yaml': 'text/yaml', '.yml': 'text/yaml',
};
const mimeTypeFor = (filename: string, content?: Uint8Array) => mimeTypes[path.extname(filename).toLowerCase()] || (content && !content.includes(0) ? 'text/plain' : 'application/octet-stream');
const isBinary = (content: Uint8Array) => content.includes(0);
const isContained = (root: string, filename: string) => filename === root || filename.startsWith(`${root}${path.sep}`);
const isSensitiveRepositoryPath = (virtualPath: string) => {
  const basename = path.posix.basename(virtualPath);
  return virtualPath === '/.git' || virtualPath.startsWith('/.git/') || basename === '.env' || basename.startsWith('.env.') || (virtualPath.startsWith('/.resonance/') && basename.endsWith('-agent.env'));
};
const normalizeVirtualPath = (requestedPath: string) => {
  if (typeof requestedPath !== 'string' || requestedPath.includes('\0') || requestedPath.includes('\\') || requestedPath.split('/').includes('..')) throw new Error('Invalid repository path.');
  const normalized = path.posix.normalize(requestedPath ? (requestedPath.startsWith('/') ? requestedPath : `/${requestedPath}`) : '/');
  return normalized === '.' ? '/' : normalized;
};
const globMatcher = (pattern: string) => {
  const expression = pattern.replace(/^\//, '').replaceAll('\\', '/');
  let source = '^';
  for (let index = 0; index < expression.length; index += 1) {
    const character = expression[index];
    if (character === '*') {
      if (expression[index + 1] === '*') {
        if (expression[index + 2] === '/') { source += '(?:.*/)?'; index += 2; }
        else { source += '.*'; index += 1; }
      } else source += '[^/]*';
    } else if (character === '?') source += '[^/]';
    else if ('.+^${}()|[]\\'.includes(character)) source += `\\${character}`;
    else source += character;
  }
  return new RegExp(`${source}$`);
};

class ReadonlyBacklogRepositoryBackend implements BackendProtocolV2 {
  private readonly root: string;
  private readonly rootRealpath: Promise<string>;

  constructor(repositoryRoot: string) {
    this.root = path.resolve(repositoryRoot);
    this.rootRealpath = realpath(this.root);
  }

  private filename(requestedPath: string) {
    const virtualPath = normalizeVirtualPath(requestedPath);
    const filename = path.resolve(this.root, virtualPath.slice(1));
    if (!isContained(this.root, filename)) throw new Error('Path is outside the repository.');
    return { filename, virtualPath };
  }

  private async existing(requestedPath: string) {
    const resolved = this.filename(requestedPath);
    const stat = await lstat(resolved.filename);
    if (stat.isSymbolicLink()) throw new Error('Symlinks are not available to the Backlog agent.');
    const physicalPath = await realpath(resolved.filename);
    if (!isContained(await this.rootRealpath, physicalPath)) throw new Error('Path is outside the repository.');
    return { ...resolved, stat };
  }

  private async fileInfo(filename: string, virtualPath: string) {
    if (isSensitiveRepositoryPath(virtualPath)) return null;
    const stat = await lstat(filename);
    if (stat.isSymbolicLink()) return null;
    const physicalPath = await realpath(filename);
    if (!isContained(await this.rootRealpath, physicalPath) || (!stat.isFile() && !stat.isDirectory())) return null;
    return { path: stat.isDirectory() ? `${virtualPath.replace(/\/$/, '')}/` : virtualPath, is_dir: stat.isDirectory(), size: stat.size, modified_at: stat.mtime.toISOString() };
  }

  private async filesUnder(requestedPath: string) {
    const base = await this.existing(requestedPath);
    if (base.stat.isFile()) return [{ filename: base.filename, virtualPath: base.virtualPath }];
    const results: Array<{ filename: string; virtualPath: string }> = [];
    const visit = async (filename: string, virtualPath: string): Promise<void> => {
      for (const entry of await readdir(filename, { withFileTypes: true })) {
        const childFilename = path.join(filename, entry.name);
        const childVirtualPath = `${virtualPath === '/' ? '' : virtualPath}/${entry.name}`;
        const info = await this.fileInfo(childFilename, childVirtualPath);
        if (!info) continue;
        if (info.is_dir) await visit(childFilename, childVirtualPath);
        else results.push({ filename: childFilename, virtualPath: childVirtualPath });
      }
    };
    await visit(base.filename, base.virtualPath);
    return results;
  }

  async ls(requestedPath: string) {
    try {
      const directory = await this.existing(requestedPath);
      if (!directory.stat.isDirectory()) return { files: [] };
      const files = [];
      for (const entry of await readdir(directory.filename, { withFileTypes: true })) {
        const childFilename = path.join(directory.filename, entry.name);
        const childVirtualPath = `${directory.virtualPath === '/' ? '' : directory.virtualPath}/${entry.name}`;
        const info = await this.fileInfo(childFilename, childVirtualPath);
        if (info) files.push(info);
      }
      return { files: files.sort((left, right) => left.path.localeCompare(right.path)) };
    } catch (error) { return { error: `Error listing '${requestedPath}': ${error instanceof Error ? error.message : String(error)}` }; }
  }

  private async contents(requestedPath: string) {
    const file = await this.existing(requestedPath);
    if (!file.stat.isFile()) throw new Error(`File '${requestedPath}' not found.`);
    if (isSensitiveRepositoryPath(file.virtualPath)) throw new Error(`File '${requestedPath}' is not available to the Backlog agent.`);
    if (file.stat.size > maxRepositoryFileBytes) throw new Error(`File '${requestedPath}' exceeds the 10 MB read limit.`);
    return { ...file, buffer: new Uint8Array(await readFile(file.filename)) };
  }

  async read(requestedPath: string, offset = 0, limit = 500) {
    try {
      const file = await this.contents(requestedPath);
      const mimeType = mimeTypeFor(file.filename, file.buffer);
      if (isBinary(file.buffer)) return { content: file.buffer, mimeType };
      const lines = new TextDecoder().decode(file.buffer).split('\n');
      if (offset >= lines.length) return { error: `Line offset ${offset} exceeds file length (${lines.length} lines)` };
      return { content: lines.slice(offset, offset + limit).join('\n'), mimeType };
    } catch (error) { return { error: `Error reading file '${requestedPath}': ${error instanceof Error ? error.message : String(error)}` }; }
  }

  async readRaw(requestedPath: string) {
    try {
      const file = await this.contents(requestedPath);
      return { data: { content: isBinary(file.buffer) ? file.buffer : new TextDecoder().decode(file.buffer), mimeType: mimeTypeFor(file.filename, file.buffer), created_at: file.stat.ctime.toISOString(), modified_at: file.stat.mtime.toISOString() } };
    } catch (error) { return { error: `Error reading file '${requestedPath}': ${error instanceof Error ? error.message : String(error)}` }; }
  }

  async glob(pattern: string, requestedPath = '/') {
    try {
      const base = normalizeVirtualPath(requestedPath);
      const matcher = globMatcher(pattern);
      const files = [];
      for (const file of await this.filesUnder(base)) {
        const relative = file.virtualPath.slice(base === '/' ? 1 : base.length + 1);
        if (!matcher.test(relative)) continue;
        const info = await this.fileInfo(file.filename, file.virtualPath);
        if (info) files.push(info);
      }
      return { files: files.sort((left, right) => left.path.localeCompare(right.path)) };
    } catch (error) { return { error: `Error searching repository: ${error instanceof Error ? error.message : String(error)}` }; }
  }

  async grep(pattern: string, requestedPath = '/', glob?: string | null) {
    try {
      const base = normalizeVirtualPath(requestedPath || '/');
      const matcher = glob ? globMatcher(glob) : null;
      const matches: Array<{ path: string; line: number; text: string }> = [];
      for (const file of await this.filesUnder(base)) {
        const relative = file.virtualPath.slice(base === '/' ? 1 : base.length + 1);
        if (matcher && !matcher.test(relative)) continue;
        const contents = await this.contents(file.virtualPath).catch(() => null);
        if (!contents || isBinary(contents.buffer)) continue;
        new TextDecoder().decode(contents.buffer).split('\n').forEach((line, index) => {
          if (line.includes(pattern) && matches.length < 10_000) matches.push({ path: file.virtualPath, line: index + 1, text: line });
        });
      }
      return { matches };
    } catch (error) { return { error: `Error searching repository: ${error instanceof Error ? error.message : String(error)}` }; }
  }

  write(requestedPath: string) { return { error: `Permission denied: ${requestedPath} is read-only for the Backlog agent.` }; }
  edit(requestedPath: string) { return { error: `Permission denied: ${requestedPath} is read-only for the Backlog agent.` }; }
}

export function createPackagedSkillBackend(skill: string, repositoryRoot?: string): BackendProtocolV2 {
  const now = new Date(0).toISOString();
  const directory = (requestedPath: string) => requestedPath.endsWith('/') ? requestedPath : `${requestedPath}/`;
  const skillBackend: BackendProtocolV2 = {
    ls(requestedPath) {
      switch (directory(requestedPath)) {
        case '/skills/': return { files: [{ path: '/skills/manage-backlog/', is_dir: true }] };
        case '/skills/manage-backlog/': return { files: [{ path: skillPath, is_dir: false }] };
        default: return { error: denied(requestedPath) };
      }
    },
    read(requestedPath) { return requestedPath === skillPath ? { content: skill, mimeType: 'text/markdown' } : { error: denied(requestedPath) }; },
    readRaw(requestedPath) { return requestedPath === skillPath ? { data: { content: skill, mimeType: 'text/markdown', created_at: now, modified_at: now } } : { error: denied(requestedPath) }; },
    grep(_pattern, requestedPath = '/') { return { error: denied(requestedPath || '/') }; },
    glob(_pattern, requestedPath = '/') { return { error: denied(requestedPath) }; },
    write(requestedPath) { return { error: denied(requestedPath) }; },
    edit(requestedPath) { return { error: denied(requestedPath) }; },
  };
  if (!repositoryRoot) return skillBackend;
  const repositoryBackend = new ReadonlyBacklogRepositoryBackend(repositoryRoot);
  const isSkillPath = (requestedPath: string) => requestedPath === skillRoot || requestedPath.startsWith(`${skillRoot}/`);
  return {
    async ls(requestedPath) {
      if (requestedPath === '/') {
        const result = await repositoryBackend.ls('/');
        return result.files ? { files: [...result.files.filter((file) => file.path !== `${skillRoot}/`), { path: `${skillRoot}/`, is_dir: true }] } : result;
      }
      return isSkillPath(requestedPath) ? skillBackend.ls(requestedPath) : repositoryBackend.ls(requestedPath);
    },
    read(requestedPath, offset, limit) { return isSkillPath(requestedPath) ? skillBackend.read(requestedPath, offset, limit) : repositoryBackend.read(requestedPath, offset, limit); },
    readRaw(requestedPath) { return isSkillPath(requestedPath) ? skillBackend.readRaw(requestedPath) : repositoryBackend.readRaw(requestedPath); },
    grep(pattern, requestedPath = '/', glob) { const base = requestedPath || '/'; return isSkillPath(base) ? skillBackend.grep(pattern, base, glob || undefined) : repositoryBackend.grep(pattern, base, glob || undefined); },
    glob(pattern, requestedPath = '/') { return isSkillPath(requestedPath) ? skillBackend.glob(pattern, requestedPath) : repositoryBackend.glob(pattern, requestedPath); },
    write(requestedPath, content) { return isSkillPath(requestedPath) ? skillBackend.write(requestedPath, content) : repositoryBackend.write(requestedPath, content); },
    edit(requestedPath, oldString, newString, replaceAll) { return isSkillPath(requestedPath) ? skillBackend.edit(requestedPath, oldString, newString, replaceAll) : repositoryBackend.edit(requestedPath, oldString, newString, replaceAll); },
  };
}

function mutationResult(options: BacklogAgentRuntimeFactoryOptions, result: BacklogMutation) {
  options.onMutation(result);
  return JSON.stringify({ changed: true, affectedPaths: result.affectedPaths });
}
function decisionResult(decision: BacklogDecision) {
  return JSON.stringify({ path: decision.path, title: decision.title, status: decision.status, priority: decision.priority, markdown: decision.markdown });
}
function createBacklogTools(options: BacklogAgentRuntimeFactoryOptions) {
  const canonicalPath = z.string().min(1).max(512).describe('A canonical decision path returned by list_decisions, such as backlog/plans/example.md.');
  return [
    tool(async () => JSON.stringify({ decisions: await options.store.listDecisions() }), {
      name: 'list_decisions', description: 'List authorized Backlog decisions and canonical paths. Never infer a path from a title.', schema: z.object({}),
    }),
    tool(async ({ path: requestedPath }) => decisionResult(await options.store.readDecision(requestedPath)), {
      name: 'read_plan', description: 'Read one authorized decision and linked Markdown by canonical decision path.', schema: z.object({ path: canonicalPath }),
    }),
    tool(async (input) => mutationResult(options, await options.store.createDecision(input)), {
      name: 'create_decision', description: 'Create one validated decision and linked Markdown plan. The store derives the plan path as plans/<kebab-case-title>.md; do not provide or invent a path.',
      schema: z.object({ title: z.string().trim().min(1).max(200), status: z.enum(['recently-done', 'in-progress', 'is-ready', 'in-planning']), priority: z.enum(['P0', 'P1', 'P2', 'P3']), markdown: z.string().max(256 * 1024) }),
    }),
    tool(async ({ path: requestedPath, markdown }) => mutationResult(options, await options.store.editPlan(requestedPath, markdown)), {
      name: 'edit_plan', description: 'Replace Markdown for one existing authorized decision path.', schema: z.object({ path: canonicalPath, markdown: z.string().max(256 * 1024) }),
    }),
    tool(async ({ path: requestedPath, status }) => mutationResult(options, await options.store.setStatus(requestedPath, status)), {
      name: 'set_status', description: 'Set status for one existing authorized decision path.', schema: z.object({ path: canonicalPath, status: z.enum(['recently-done', 'in-progress', 'is-ready', 'in-planning']) }),
    }),
    tool(async ({ path: requestedPath, priority }) => mutationResult(options, await options.store.setPriority(requestedPath, priority)), {
      name: 'set_priority', description: 'Set priority for one existing authorized decision path.', schema: z.object({ path: canonicalPath, priority: z.enum(['P0', 'P1', 'P2', 'P3']) }),
    }),
    tool(async ({ path: requestedPath }) => {
      const decision = await options.store.readDecision(requestedPath);
      return JSON.stringify(await options.requestDeletion(decision));
    }, {
      name: 'request_delete', description: 'Request browser-confirmed deletion. This never deletes files.', schema: z.object({ path: canonicalPath }),
    }),
  ];
}

function selectedContext(selected: BacklogDecision): string {
  return `<active-decision path="${selected.path}">\n<title>${selected.title}</title>\n<status>${selected.status}</status>\n<priority>${selected.priority}</priority>\n<plan-markdown>\n${selected.markdown}\n</plan-markdown>\n</active-decision>`;
}
function textOf(chunk: unknown): string {
  if (!chunk || typeof chunk !== 'object') return '';
  const content = (chunk as { content?: unknown }).content;
  if (typeof content === 'string') return content;
  if (!Array.isArray(content)) return '';
  return content.flatMap((part) => typeof part === 'string' ? [part] : part && typeof part === 'object' && typeof (part as { text?: unknown }).text === 'string' ? [(part as { text: string }).text] : []).join('');
}

export class DeepAgentsRuntime implements BacklogAgentRuntime {
  constructor(private readonly agent: any, private readonly telemetry: Telemetry) {}
  async *stream(turn: BacklogAgentTurn): AsyncIterable<BacklogAgentUpdate> {
    const messages = turn.messages.map((message, index) => ({
      role: message.role,
      content: message.role === 'user' && index === turn.messages.length - 1 ? `${selectedContext(turn.selected)}\n\n<user-request>\n${message.content}\n</user-request>` : message.content,
    }));
    const streamSpan = this.telemetry.span('backlog.model.stream', { observationType: 'generation', input: [{ role: 'system', content: backlogSystemPrompt }, ...messages] });
    this.telemetry.info('Backlog model stream started', { threadId: turn.threadId });
    let chunks = 0;
    let assistantResponseOpen = false;
    const responses: string[] = [];
    try {
      const stream = await this.agent.stream({ messages }, { configurable: { thread_id: turn.threadId }, streamMode: 'messages' });
      for await (const value of stream) {
        const [chunk, metadata] = value as [unknown, { langgraph_node?: unknown }];
        const isModelResponse = metadata?.langgraph_node === 'model' || metadata?.langgraph_node === 'model_request';
        if (!isModelResponse) { assistantResponseOpen = false; continue; }
        const text = textOf(chunk);
        if (text) {
          if (assistantResponseOpen) responses[responses.length - 1] += text;
          else responses.push(text);
          chunks += 1;
          assistantResponseOpen = true;
          yield { kind: 'assistant', text };
        }
      }
      streamSpan.end({ status: 'ok', chunks, output: responses.map((content) => ({ role: 'assistant', content })) });
      this.telemetry.info('Backlog model stream completed', { chunks });
    } catch (error) {
      streamSpan.fail(error, { chunks, output: responses.map((content) => ({ role: 'assistant', content })) });
      this.telemetry.error('Backlog model stream failed', { error, chunks });
      throw error;
    }
  }
  async dispose() {}
}

export function createDeepAgentsRuntimeFactory({ provider, model, repositoryRoot }: { provider: 'openai' | 'openrouter'; model: string; repositoryRoot: string }): BacklogAgentRuntimeFactory {
  return async (options) => {
    const skill = await readFile(new URL('./skills/manage-backlog/SKILL.md', import.meta.url), 'utf8');
    const runtimeTelemetry = options.telemetry.child({ provider, model });
    const agent = createDeepAgent({
      model: new ChatOpenAI({
        model,
        apiKey: options.apiKey,
        temperature: 0,
        configuration: provider === 'openrouter' ? { baseURL: 'https://openrouter.ai/api/v1' } : {},
      }),
      tools: createBacklogTools(options),
      backend: createPackagedSkillBackend(skill, repositoryRoot),
      skills: ['/skills/'],
      checkpointer: false,
      systemPrompt: backlogSystemPrompt,
    });
    return new DeepAgentsRuntime(agent, runtimeTelemetry);
  };
}
