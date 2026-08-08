import { lstat, mkdir, readdir, readFile, realpath, rename, unlink, writeFile } from 'node:fs/promises';
import path from 'node:path';
import type { Telemetry } from '../../package-contract.ts';
import { createDeepAgent, type BackendProtocolV2 } from 'deepagents';
import { ChatOpenAI } from '@langchain/openai';
import type { ModelProfile } from '@langchain/core/language_models/profile';
import { tool } from 'langchain';
import { z } from 'zod/v4';
import type { ArchitectureAgentRuntime, ArchitectureAgentRuntimeFactory, ArchitectureAgentRuntimeFactoryOptions, ArchitectureAgentTurn, ArchitectureAgentUpdate } from './architecture-agent.ts';
import { validateArchitecture } from './architecture-checkers.ts';
import { getArchitectureModelProfile } from './architecture-model-profiles.ts';

const skillRoot = '/skills';
const defaultSkillName = 'likec4-dsl';
type PackagedSkills = string | Readonly<Record<string, string>>;
const denied = (requestedPath: string) => `Permission denied: ${requestedPath} is not available to the Architecture agent.`;
const readOnlyDenied = (requestedPath: string) => `Permission denied: ${requestedPath} is read-only for the Architecture agent.`;
const maxRepositoryFileBytes = 10 * 1024 * 1024;
const markdownExtensions = new Set(['.md', '.markdown']);

export class ArchitectureChatOpenAI extends ChatOpenAI {
  override get profile(): ModelProfile {
    const profile = getArchitectureModelProfile(this.model);
    return profile ? { ...super.profile, ...profile } : super.profile;
  }
}

const mimeTypes: Record<string, string> = {
  '.c4': 'text/plain', '.css': 'text/css', '.csv': 'text/csv', '.html': 'text/html', '.js': 'text/javascript',
  '.json': 'application/json', '.md': 'text/markdown', '.mjs': 'text/javascript', '.ts': 'text/typescript',
  '.tsx': 'text/typescript', '.txt': 'text/plain', '.yaml': 'text/yaml', '.yml': 'text/yaml',
};
const mimeTypeFor = (filename: string) => mimeTypes[path.extname(filename).toLowerCase()] || 'application/octet-stream';
const isBinary = (filename: string, content: Uint8Array) => mimeTypeFor(filename) === 'application/octet-stream' || content.includes(0);
const isContained = (root: string, filename: string) => filename === root || filename.startsWith(`${root}${path.sep}`);
const isSensitiveRepositoryPath = (virtualPath: string) => {
  const basename = path.posix.basename(virtualPath);
  return basename === '.env' || basename.startsWith('.env.') || (virtualPath.startsWith('/.resonance/') && basename.endsWith('-agent.env'));
};
const isProtectedWritePath = (virtualPath: string) => virtualPath.split('/').includes('.git') || isSensitiveRepositoryPath(virtualPath);
const isMarkdownPath = (virtualPath: string) => markdownExtensions.has(path.posix.extname(virtualPath).toLowerCase());
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
const normalizeVirtualPath = (requestedPath: string) => {
  if (typeof requestedPath !== 'string' || requestedPath.includes('\0') || requestedPath.includes('\\') || requestedPath.split('/').includes('..')) throw new Error('Invalid repository path.');
  const normalized = path.posix.normalize(requestedPath ? (requestedPath.startsWith('/') ? requestedPath : `/${requestedPath}`) : '/');
  if (normalized === '..' || normalized.startsWith('../')) throw new Error('Path traversal is not allowed.');
  return normalized === '.' ? '/' : normalized;
};
const isMissing = (error: unknown) => Boolean(error && typeof error === 'object' && (error as { code?: unknown }).code === 'ENOENT');

class ReadonlyRepositoryBackend implements BackendProtocolV2 {
  private readonly root: string;
  private readonly rootRealpath: Promise<string>;
  private readonly writableRoot: string | null;

  constructor(repositoryRoot: string, writableRoot: string | null = null) {
    this.root = path.resolve(repositoryRoot);
    this.rootRealpath = realpath(this.root);
    this.writableRoot = writableRoot === null ? null : normalizeVirtualPath(writableRoot);
  }

  private filename(requestedPath: string) {
    const virtualPath = normalizeVirtualPath(requestedPath);
    const filename = path.resolve(this.root, virtualPath.slice(1));
    if (!isContained(this.root, filename)) throw new Error('Path is outside the repository.');
    return { filename, virtualPath };
  }

  private async writable(requestedPath: string) {
    const resolved = this.filename(requestedPath);
    const inArtifactRoot = this.writableRoot !== null && (resolved.virtualPath === this.writableRoot || resolved.virtualPath.startsWith(`${this.writableRoot}/`));
    if (isProtectedWritePath(resolved.virtualPath) || (!inArtifactRoot && !isMarkdownPath(resolved.virtualPath))) throw new Error(readOnlyDenied(requestedPath));
    return resolved;
  }

  private async ensureParent(filename: string) {
    const relative = path.relative(this.root, path.dirname(filename));
    let current = this.root;
    for (const segment of relative.split(path.sep).filter(Boolean)) {
      current = path.join(current, segment);
      let stats;
      try { stats = await lstat(current); }
      catch (error) {
        if (!isMissing(error)) throw error;
        await mkdir(current, { mode: 0o755 });
        stats = await lstat(current);
      }
      if (stats.isSymbolicLink() || !stats.isDirectory()) throw new Error('Write parent is not a regular directory.');
      if (!isContained(await this.rootRealpath, await realpath(current))) throw new Error('Write parent escapes the repository.');
    }
  }

  private async writeText(requestedPath: string, content: string) {
    const resolved = await this.writable(requestedPath);
    if (Buffer.byteLength(content, 'utf8') > maxRepositoryFileBytes) throw new Error('File content exceeds the 10 MB write limit.');
    await this.ensureParent(resolved.filename);
    let mode = 0o644;
    try {
      const stats = await lstat(resolved.filename);
      if (stats.isSymbolicLink() || stats.isDirectory()) throw new Error('Target is not a regular file.');
      mode = stats.mode & 0o777;
    } catch (error) {
      if (!isMissing(error)) throw error;
    }
    const temporary = path.join(path.dirname(resolved.filename), `.${path.basename(resolved.filename)}.${crypto.randomUUID()}.tmp`);
    try {
      await writeFile(temporary, content, { encoding: 'utf8', mode });
      await rename(temporary, resolved.filename);
    } catch (error) {
      await unlink(temporary).catch(() => undefined);
      throw error;
    }
    return { path: resolved.virtualPath, filesUpdate: null };
  }

  private async existing(requestedPath: string) {
    const resolved = this.filename(requestedPath);
    const stat = await lstat(resolved.filename);
    if (stat.isSymbolicLink()) throw new Error('Symlinks are not available to the Architecture agent.');
    const real = await realpath(resolved.filename);
    if (!isContained(await this.rootRealpath, real)) throw new Error('Path is outside the repository.');
    return { ...resolved, stat };
  }

  private async fileInfo(filename: string, virtualPath: string) {
    const stat = await lstat(filename);
    if (stat.isSymbolicLink()) return null;
    const real = await realpath(filename);
    if (!isContained(await this.rootRealpath, real)) return null;
    if (!stat.isFile() && !stat.isDirectory()) return null;
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
    if (isSensitiveRepositoryPath(file.virtualPath)) throw new Error(`File '${requestedPath}' is not available to the Architecture agent.`);
    if (file.stat.size > maxRepositoryFileBytes) throw new Error(`File '${requestedPath}' exceeds the 10 MB read limit.`);
    const buffer = new Uint8Array(await readFile(file.filename));
    return { ...file, buffer };
  }

  async read(requestedPath: string, offset = 0, limit = 500) {
    try {
      const file = await this.contents(requestedPath);
      const mimeType = mimeTypeFor(file.filename);
      if (isBinary(file.filename, file.buffer)) return { content: file.buffer, mimeType };
      const content = new TextDecoder().decode(file.buffer);
      const lines = content.split('\n');
      if (offset >= lines.length) return { error: `Line offset ${offset} exceeds file length (${lines.length} lines)` };
      return { content: lines.slice(offset, offset + limit).join('\n'), mimeType };
    } catch (error) { return { error: `Error reading file '${requestedPath}': ${error instanceof Error ? error.message : String(error)}` }; }
  }

  async readRaw(requestedPath: string) {
    try {
      const file = await this.contents(requestedPath);
      const now = new Date(0).toISOString();
      return { data: { content: isBinary(file.filename, file.buffer) ? file.buffer : new TextDecoder().decode(file.buffer), mimeType: mimeTypeFor(file.filename), created_at: file.stat.ctime.toISOString() || now, modified_at: file.stat.mtime.toISOString() || now } };
    } catch (error) { return { error: `Error reading file '${requestedPath}': ${error instanceof Error ? error.message : String(error)}` }; }
  }

  async glob(pattern: string, requestedPath = '/') {
    try {
      const base = normalizeVirtualPath(requestedPath);
      const matcher = globMatcher(pattern);
      const matches = [];
      for (const file of await this.filesUnder(base)) {
        const relative = file.virtualPath.slice(base === '/' ? 1 : base.length + 1);
        if (!matcher.test(relative)) continue;
        const info = await this.fileInfo(file.filename, file.virtualPath);
        if (info) matches.push(info);
      }
      return { files: matches.sort((left, right) => left.path.localeCompare(right.path)) };
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
        if (isSensitiveRepositoryPath(file.virtualPath)) continue;
        const contents = await this.contents(file.virtualPath);
        if (isBinary(file.filename, contents.buffer)) continue;
        const lines = new TextDecoder().decode(contents.buffer).split('\n');
        lines.forEach((line, index) => { if (line.includes(pattern) && matches.length < 10_000) matches.push({ path: file.virtualPath, line: index + 1, text: line }); });
      }
      return { matches };
    } catch (error) { return { error: `Error searching repository: ${error instanceof Error ? error.message : String(error)}` }; }
  }

  async write(requestedPath: string, content: string) {
    try { return await this.writeText(requestedPath, content); }
    catch (error) { return { error: `Error writing file '${requestedPath}': ${error instanceof Error ? error.message : String(error)}` }; }
  }

  async edit(requestedPath: string, oldString: string, newString: string, replaceAll = false) {
    try {
      const resolved = await this.writable(requestedPath);
      const file = await this.contents(resolved.virtualPath);
      if (isBinary(file.filename, file.buffer)) throw new Error('Binary files cannot be edited.');
      const content = new TextDecoder().decode(file.buffer);
      const occurrences = content.split(oldString).length - 1;
      if (!oldString || occurrences === 0) throw new Error('Old text was not found.');
      const updated = replaceAll ? content.split(oldString).join(newString) : content.replace(oldString, newString);
      const result = await this.writeText(resolved.virtualPath, updated);
      return { ...result, occurrences: replaceAll ? occurrences : 1 };
    } catch (error) { return { error: `Error editing file '${requestedPath}': ${error instanceof Error ? error.message : String(error)}` }; }
  }
}

export function createReadonlyRepositoryBackend(repositoryRoot: string): BackendProtocolV2 {
  return new ReadonlyRepositoryBackend(repositoryRoot);
}

export function createPackagedSkillBackend(skills: PackagedSkills, repositoryRoot?: string, writableRoot = 'architecture'): BackendProtocolV2 {
  const now = new Date(0).toISOString();
  const packagedSkills = typeof skills === 'string' ? { [defaultSkillName]: skills } : skills;
  const names = Object.keys(packagedSkills).sort();
  if (names.some((name) => !/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(name))) throw new Error('Packaged skill names must be lowercase kebab-case.');
  const skillFile = (name: string) => `${skillRoot}/${name}/SKILL.md`;
  const skillNameForFile = (requestedPath: string) => names.find((name) => requestedPath === skillFile(name));
  const directory = (requestedPath: string) => requestedPath.endsWith('/') ? requestedPath : `${requestedPath}/`;
  const skillBackend: BackendProtocolV2 = {
    ls(requestedPath) {
      const requestedDirectory = directory(requestedPath);
      if (requestedDirectory === `${skillRoot}/`) return { files: names.map((name) => ({ path: `${skillRoot}/${name}/`, is_dir: true })) };
      const name = names.find((candidate) => requestedDirectory === `${skillRoot}/${candidate}/`);
      return name ? { files: [{ path: skillFile(name), is_dir: false }] } : { error: denied(requestedPath) };
    },
    read(requestedPath) {
      const name = skillNameForFile(requestedPath);
      return name ? { content: packagedSkills[name], mimeType: 'text/markdown' } : { error: denied(requestedPath) };
    },
    readRaw(requestedPath) {
      const name = skillNameForFile(requestedPath);
      return name ? { data: { content: packagedSkills[name], mimeType: 'text/markdown', created_at: now, modified_at: now } } : { error: denied(requestedPath) };
    },
    grep(_pattern, requestedPath = '/') { return { error: denied(requestedPath || '/') }; },
    glob(_pattern, requestedPath = '/') { return { error: denied(requestedPath) }; },
    write(requestedPath) { return { error: denied(requestedPath) }; },
    edit(requestedPath) { return { error: denied(requestedPath) }; },
  };
  if (!repositoryRoot) return skillBackend;
  const repositoryBackend = new ReadonlyRepositoryBackend(repositoryRoot, writableRoot);
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
    grep(pattern, requestedPath = '/', glob) { const basePath = requestedPath || '/'; return isSkillPath(basePath) ? skillBackend.grep(pattern, basePath, glob || undefined) : repositoryBackend.grep(pattern, basePath, glob || undefined); },
    glob(pattern, requestedPath = '/') { return isSkillPath(requestedPath) ? skillBackend.glob(pattern, requestedPath) : repositoryBackend.glob(pattern, requestedPath); },
    write(requestedPath, content) { return isSkillPath(requestedPath) ? skillBackend.write(requestedPath, content) : repositoryBackend.write(requestedPath, content); },
    edit(requestedPath, oldString, newString, replaceAll) { return isSkillPath(requestedPath) ? skillBackend.edit(requestedPath, oldString, newString, replaceAll) : repositoryBackend.edit(requestedPath, oldString, newString, replaceAll); },
  };
}

function textOf(chunk: unknown): string {
  if (!chunk || typeof chunk !== 'object') return '';
  const content = (chunk as { content?: unknown }).content;
  if (typeof content === 'string') return content;
  if (!Array.isArray(content)) return '';
  return content.flatMap((part) => typeof part === 'string' ? [part] : part && typeof part === 'object' && typeof (part as { text?: unknown }).text === 'string' ? [(part as { text: string }).text] : []).join('');
}
function inputTokensOf(chunk: unknown): number | null {
  if (!chunk || typeof chunk !== 'object') return null;
  const message = chunk as { usage_metadata?: { input_tokens?: unknown }; response_metadata?: { usage?: { prompt_tokens?: unknown } } };
  const inputTokens = message.usage_metadata?.input_tokens ?? message.response_metadata?.usage?.prompt_tokens;
  return typeof inputTokens === 'number' && Number.isFinite(inputTokens) ? inputTokens : null;
}

function recoverableArchitectureToolError(error: unknown, operation: string): string {
  const message = error instanceof Error ? error.message : String(error);
  const artifact = message.match(/(?:model|views|rules|patterns|decisions)\.json/)?.[0];
  const target = artifact ? ` Inspect ${artifact} and repair it with the repository filesystem tools.` : ' Inspect the architecture artifacts and repository evidence with the available tools.';
  return JSON.stringify({ recoverable: true, error: message, recovery: `${target} Then call ${operation} again and explain the problem to the user if it cannot be repaired.` });
}

export function createArchitectureTools(options: ArchitectureAgentRuntimeFactoryOptions) {
  const entityId = z.string().min(1).max(200);
  return [
    tool(async () => {
      try {
        const artifacts = await options.store.read();
        try {
          const likec4 = await options.store.likec4();
          return JSON.stringify({ likec4: likec4.dump, views: likec4.views, rules: artifacts.rules, patterns: artifacts.patterns, decisions: artifacts.decisions });
        } catch (error) {
          return JSON.stringify({ recoverable: true, model: artifacts.model, views: artifacts.views, rules: artifacts.rules, patterns: artifacts.patterns, decisions: artifacts.decisions, likec4Error: error instanceof Error ? error.message : String(error), recovery: 'Inspect the LikeC4 sources under the architecture artifact root, repair the invalid reference or syntax, then call read_model again.' });
        }
      } catch (error) { return recoverableArchitectureToolError(error, 'read_model'); }
    }, { name: 'read_model', description: 'Read the committed LikeC4 model, views, and package-owned architecture metadata. Artifact and LikeC4 errors are returned as recoverable context so you can repair them or explain them to the user.', schema: z.object({}) }),
    tool(async ({ view }) => {
      try { return JSON.stringify(await options.store.graph(view)); }
      catch (error) { return recoverableArchitectureToolError(error, 'read_view'); }
    }, {
      name: 'read_view', description: 'Read one named architecture view and its projected graph. Errors are returned as recoverable context.', schema: z.object({ view: z.string().min(1).max(200) }),
    }),
    tool(async ({ path: requestedPath }) => {
      try { return JSON.stringify(await options.store.readEvidence(requestedPath)); }
      catch (error) { return recoverableArchitectureToolError(error, 'read_evidence'); }
    }, {
      name: 'read_evidence', description: 'Read bounded implementation evidence linked from the architecture artifacts. Errors are returned as recoverable context.', schema: z.object({ path: z.string().min(1).max(512) }),
    }),
    tool(async () => {
      try {
        const artifacts = await options.store.read();
        return JSON.stringify(await validateArchitecture(options.context, artifacts));
      } catch (error) { return recoverableArchitectureToolError(error, 'validate_architecture'); }
    }, { name: 'validate_architecture', description: 'Run deterministic local architecture validation. Artifact errors are returned as recoverable context.', schema: z.object({}) }),
    tool(async ({ id }) => {
      try {
        const artifacts = await options.store.read();
        const entity = artifacts.model.entities.find((candidate) => candidate.id === id);
        return entity ? JSON.stringify(entity) : `No modeled entity has id ${id}.`;
      } catch (error) { return recoverableArchitectureToolError(error, 'read_entity'); }
    }, { name: 'read_entity', description: 'Read one modeled entity by its stable id. Artifact errors are returned as recoverable context.', schema: z.object({ id: entityId }) }),
  ];
}

function selectedContext(turn: ArchitectureAgentTurn): string {
  const selected = [turn.selectedId ? `<selected-entity>${turn.selectedId}</selected-entity>` : '', turn.selectedView ? `<selected-view>${turn.selectedView}</selected-view>` : ''].filter(Boolean).join('\n');
  return selected ? `<architecture-context>\n${selected}\n</architecture-context>` : '';
}

export class DeepAgentsRuntime implements ArchitectureAgentRuntime {
  constructor(private readonly agent: any, private readonly telemetry: Telemetry, private readonly maxInputTokens?: number, private readonly systemPrompt?: string) {}
  async *stream(turn: ArchitectureAgentTurn, signal: AbortSignal): AsyncIterable<ArchitectureAgentUpdate> {
    const messages = turn.messages.map((message, index) => ({ role: message.role, content: message.role === 'user' && index === turn.messages.length - 1 ? `${selectedContext(turn)}\n\n<user-request>\n${message.content}\n</user-request>` : message.content }));
    const input = [...(this.systemPrompt ? [{ role: 'system', content: this.systemPrompt }] : []), ...messages];
    const streamSpan = this.telemetry.span('architecture.model.stream', { observationType: 'generation', input });
    this.telemetry.info('Architecture model stream started', { threadId: turn.threadId });
    let chunks = 0;
    let assistantResponseOpen = false;
    const responses: string[] = [];
    try {
      const stream = await this.agent.stream({ messages }, { configurable: { thread_id: turn.threadId }, streamMode: 'messages', signal });
      for await (const value of stream) {
        const [chunk, metadata] = value as [unknown, { langgraph_node?: unknown }];
        const isModelResponse = metadata?.langgraph_node === 'model' || metadata?.langgraph_node === 'model_request';
        if (!isModelResponse) { assistantResponseOpen = false; continue; }
        const inputTokens = inputTokensOf(chunk);
        if (inputTokens !== null && this.maxInputTokens !== undefined) yield { kind: 'context', context: { inputTokens, maxInputTokens: this.maxInputTokens } };
        const text = textOf(chunk);
        if (text) {
          const newParagraph = chunks > 0 && !assistantResponseOpen;
          if (assistantResponseOpen) responses[responses.length - 1] += text;
          else responses.push(text);
          chunks += 1;
          assistantResponseOpen = true;
          yield { kind: 'assistant', text, ...(newParagraph ? { newParagraph: true } : {}) };
        }
      }
      const output = responses.map((content) => ({ role: 'assistant', content }));
      if (signal.aborted) {
        streamSpan.end({ status: 'stopped', chunks, output });
        this.telemetry.info('Architecture model stream stopped', { chunks });
        return;
      }
      streamSpan.end({ status: 'ok', chunks, output });
      this.telemetry.info('Architecture model stream completed', { chunks });
    } catch (error) {
      const output = responses.map((content) => ({ role: 'assistant', content }));
      if (signal.aborted) {
        streamSpan.end({ status: 'stopped', chunks, output });
        this.telemetry.info('Architecture model stream stopped', { chunks });
      } else {
        streamSpan.fail(error, { chunks, output });
        this.telemetry.error('Architecture model stream failed', { error, chunks });
      }
      throw error;
    }
  }
  async dispose() {}
}

export async function providerFetch(input: RequestInfo | URL, init: RequestInit | undefined, telemetry: Telemetry, fetchFn: typeof fetch = fetch): Promise<Response> {
  const response = await fetchFn(input, init);
  if (response.ok) return response;
  let providerError: Record<string, unknown> = {};
  try {
    const body = await response.clone().json() as { error?: unknown };
    if (body.error && typeof body.error === 'object' && !Array.isArray(body.error)) {
      const error = body.error as Record<string, unknown>;
      providerError = Object.fromEntries(['message', 'code', 'type'].filter((key) => typeof error[key] === 'string' || typeof error[key] === 'number').map((key) => [key, error[key]]));
    }
  } catch {}
  telemetry.error('Architecture provider request failed', {
    status: response.status,
    requestBytes: typeof init?.body === 'string' ? init.body.length : undefined,
    requestId: response.headers.get('x-request-id') || undefined,
    generationId: response.headers.get('x-generation-id') || undefined,
    providerName: response.headers.get('x-provider-name') || undefined,
    cloudflareRay: response.headers.get('cf-ray') || undefined,
    providerError,
  });
  return response;
}

export function createDeepAgentsRuntimeFactory({ provider, model, artifactRoot = 'architecture' }: { provider: 'openai' | 'openrouter'; model: string; artifactRoot?: string }): ArchitectureAgentRuntimeFactory {
  return async (options) => {
    const [likec4Skill, codeStructuralViewSkill] = await Promise.all([
      readFile(new URL('./skills/likec4-dsl/SKILL.md', import.meta.url), 'utf8'),
      readFile(new URL('./skills/code-structural-view/SKILL.md', import.meta.url), 'utf8'),
    ]);
    const runtimeTelemetry = options.telemetry.child({ provider, model });
    const chatModel = new ArchitectureChatOpenAI({ model, apiKey: options.apiKey, temperature: 0, configuration: provider === 'openrouter' ? { baseURL: 'https://openrouter.ai/api/v1', maxRetries: 5, fetch: (input, init) => providerFetch(input, init, runtimeTelemetry) } : { maxRetries: 5, fetch: (input, init) => providerFetch(input, init, runtimeTelemetry) } });
    const systemPrompt = `You are Resonance Architecture Agent. Read /skills/likec4-dsl/SKILL.md before answering. Read and follow /skills/code-structural-view/SKILL.md when the user asks to understand or improve a code-level structural architecture view. The whole viewed repository is mounted at /. Use ls, read_file, glob, and grep to inspect any repository file needed for an assessment. You may use write_file and edit_file for files under /${artifactRoot} and for Markdown documents anywhere in the repository. Use those write capabilities to keep the LikeC4 model and architecture metadata current, and to correct or improve repository documentation when asked. If an Architecture tool reports a LikeC4 parse or reference error, do not stop at reporting it: inspect the relevant source under /${artifactRoot}, repair it with edit_file or write_file, and retry the Architecture tool. All other repository files are read-only. Credential files such as .env files and repository-local agent credential files are intentionally unavailable; never modify .git or credentials, and never use shell or network access. Use Architecture tools for model facts and distinguish evidence, intent, and assessment.`;
    const agent = createDeepAgent({
      model: chatModel,
      tools: createArchitectureTools(options),
      backend: createPackagedSkillBackend({ 'likec4-dsl': likec4Skill, 'code-structural-view': codeStructuralViewSkill }, options.context.repositoryRoot, artifactRoot),
      skills: ['/skills/'],
      checkpointer: false,
      systemPrompt,
    });
    return new DeepAgentsRuntime(agent, runtimeTelemetry, chatModel.profile.maxInputTokens, systemPrompt);
  };
}
