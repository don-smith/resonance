import { execFile } from 'node:child_process';
import { lstat, mkdir, readFile, realpath, rename, unlink, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { promisify } from 'node:util';
import { z } from 'zod';
import { createMarkdownRenderer } from '../../markdown.ts';
import type { HostContext, HostResponse, PackageDefinition, PackageInput, PackageRegistration } from '../../package-contract.ts';
import { createBacklogAgentSession, type BacklogAgentRuntimeFactory } from './agent-session.ts';
import { createDeepAgentsRuntimeFactory } from './deepagents.ts';
import { BacklogSourceError, BacklogStoreError, createBacklogStore, parseBacklogItems } from './backlog-store.ts';

export { parseBacklogItems } from './backlog-store.ts';

const runFile = promisify(execFile);
const metadata = { id: 'backlog', version: '1.0.0', hostVersion: '1', label: 'Backlog', order: 30 } as const;
const CREDENTIAL_PATH = '.resonance/backlog-agent.env';
const inputSchema = z.object({ provider: z.literal('openai'), model: z.literal('gpt-4.1') }).strict();
const isRecord = (value: unknown): value is Record<string, unknown> => Boolean(value) && typeof value === 'object' && !Array.isArray(value);
const isMissing = (error: unknown) => Boolean(error && typeof error === 'object' && (error as { code?: unknown }).code === 'ENOENT');
const within = (root: string, candidate: string) => candidate === root || candidate.startsWith(`${root}${path.sep}`);
class CredentialInputError extends Error { status = 400; }

export function backlogInput(input: PackageInput) {
  const parsed = inputSchema.safeParse(input);
  if (!parsed.success) throw new Error(`Backlog input is invalid: ${parsed.error.issues[0].message}`);
  return parsed.data;
}

function sendError(response: HostResponse, error: unknown): void {
  const status = isRecord(error) && typeof error.status === 'number' ? error.status : error instanceof BacklogSourceError ? 422 : error instanceof BacklogStoreError ? 404 : 500;
  const message = error instanceof Error ? error.message : String(error);
  response.json(status, { error: message });
}
function sendReadFailure(response: HostResponse, error: unknown, fallback: string) {
  if (error instanceof BacklogSourceError) response.json(422, { error: error.message });
  else response.json(404, { error: fallback });
}
function secureRoot(root: string): Promise<string> { return realpath(path.resolve(root)); }
async function credentialFilename(root: string): Promise<{ root: string; directory: string; filename: string }> {
  const physicalRoot = await secureRoot(root);
  const directory = path.join(physicalRoot, '.resonance');
  const filename = path.join(directory, 'backlog-agent.env');
  if (!within(physicalRoot, directory) || !within(physicalRoot, filename)) throw new Error('Credential path escapes the repository.');
  return { root: physicalRoot, directory, filename };
}
async function regularCredentialFile(filename: string): Promise<boolean> {
  let stats;
  try { stats = await lstat(filename); }
  catch (error) { if (isMissing(error)) return false; throw error; }
  if (stats.isSymbolicLink() || !stats.isFile()) throw new Error('Credential file must be a regular file.');
  if ((stats.mode & 0o777) !== 0o600) throw new Error('Credential file must have mode 0600.');
  return true;
}
async function secureCredentialDirectory(directory: string): Promise<void> {
  let stats;
  try { stats = await lstat(directory); }
  catch (error) {
    if (!isMissing(error)) throw error;
    await mkdir(directory, { recursive: true, mode: 0o700 });
    stats = await lstat(directory);
  }
  if (stats.isSymbolicLink() || !stats.isDirectory()) throw new Error('Credential directory must be a regular directory.');
  if (await realpath(directory) !== directory) throw new Error('Credential directory must not be a symlink.');
}
async function isTracked(root: string): Promise<boolean> {
  try {
    await runFile('git', ['-C', root, 'ls-files', '--error-unmatch', '--', CREDENTIAL_PATH], { encoding: 'utf8' });
    return true;
  } catch {
    return false;
  }
}
function parseCredential(contents: string): string {
  const match = /^(?:OPENAI_API_KEY=)([^\r\n]+)\n?$/.exec(contents);
  if (!match || !match[1].trim() || match[1] !== match[1].trim()) throw new Error('Credential file is invalid.');
  return match[1];
}
async function readCredential(root: string): Promise<string | null> {
  const locations = await credentialFilename(root);
  const directoryStats = await lstat(locations.directory).catch((error: unknown) => isMissing(error) ? null : Promise.reject(error));
  if (!directoryStats) return null;
  await secureCredentialDirectory(locations.directory);
  if (!await regularCredentialFile(locations.filename)) return null;
  return parseCredential(await readFile(locations.filename, 'utf8'));
}
async function writeCredential(root: string, apiKey: string): Promise<void> {
  if (!apiKey || apiKey.length > 4096 || /[\r\n]/.test(apiKey) || apiKey !== apiKey.trim()) throw new CredentialInputError('Credential must be a single-line API key.');
  const locations = await credentialFilename(root);
  await secureCredentialDirectory(locations.directory);
  if (await isTracked(locations.root)) throw new Error('Credential file must not be tracked.');
  await regularCredentialFile(locations.filename);
  const temporary = path.join(locations.directory, `.backlog-agent.${crypto.randomUUID()}.tmp`);
  try {
    await writeFile(temporary, `OPENAI_API_KEY=${apiKey}\n`, { encoding: 'utf8', mode: 0o600 });
    await rename(temporary, locations.filename);
  } catch (error) {
    await unlink(temporary).catch(() => undefined);
    throw error;
  }
}

export function createBacklogPackage({ runtimeFactory }: { runtimeFactory?: BacklogAgentRuntimeFactory } = {}): PackageDefinition {
  return {
    metadata,
    register(context, input): PackageRegistration {
      const config = backlogInput(input);
      const store = createBacklogStore({ repositoryRoot: context.repositoryRoot });
      const renderer = createMarkdownRenderer();
      const session = createBacklogAgentSession({
        store,
        credentialProvider: () => readCredential(context.repositoryRoot),
        runtimeFactory: runtimeFactory || createDeepAgentsRuntimeFactory({ model: config.model }),
      });
      const activeStreams = new Set<() => void>();
      const items = async () => store.listDecisions();
      return {
        metadata,
        routes: [
          {
            method: 'GET', path: '/api/backlog/items', handler: async (_request, response) => {
              try { response.json(200, { items: await items() }); }
              catch (error) { sendReadFailure(response, error, 'Backlog source not found'); }
            },
          },
          {
            method: 'GET', path: '/api/backlog/plan', handler: async (request, response) => {
              const requested = new URL(request.url, 'http://127.0.0.1').searchParams.get('path') || '';
              try {
                const item = await store.readDecision(requested);
                response.json(200, { path: item.path, title: item.title, html: renderer.render(item.markdown) });
              } catch (error) { sendReadFailure(response, error, 'Backlog item not found'); }
            },
          },
          { method: 'GET', path: '/api/backlog/agent/state', handler: async (_request, response) => response.json(200, session.snapshot()) },
          {
            method: 'GET', path: '/api/backlog/agent/events', handler: async (request, response) => {
              const stream = response.sse();
              let closed = false;
              let unsubscribe: (() => void) | null = null;
              let cleanupBeforeSubscribe = false;
              let resolveClosed = () => { };
              const closedPromise = new Promise<void>((resolve) => { resolveClosed = resolve; });
              const close = () => {
                if (closed) return;
                closed = true;
                if (unsubscribe) unsubscribe(); else cleanupBeforeSubscribe = true;
                activeStreams.delete(close);
                stream.close();
                resolveClosed();
              };
              activeStreams.add(close);
              request.onAbort(close);
              response.onClose(close);
              unsubscribe = session.subscribe((event) => { stream.write(event); if (response.closed) close(); });
              if (cleanupBeforeSubscribe) unsubscribe();
              await closedPromise;
            },
          },
          {
            method: 'POST', path: '/api/backlog/agent/prompt', handler: async (request, response) => {
              try {
                const body = await request.readJson<{ prompt?: unknown; selectedPath?: unknown }>(32 * 1024);
                if (!isRecord(body) || typeof body.prompt !== 'string' || !body.prompt.trim() || typeof body.selectedPath !== 'string' || !body.selectedPath.trim()) {
                  response.json(400, { error: 'Prompt and selectedPath must be non-empty strings.' }); return;
                }
                const result = await session.submitPrompt({ prompt: body.prompt, selectedPath: body.selectedPath });
                response.json(202, result);
              } catch (error) { sendError(response, error); }
            },
          },
          {
            method: 'POST', path: '/api/backlog/agent/credential', handler: async (request, response) => {
              try {
                const body = await request.readJson<{ apiKey?: unknown }>(8 * 1024);
                if (!isRecord(body) || typeof body.apiKey !== 'string') { response.json(400, { error: 'apiKey must be a string.' }); return; }
                await writeCredential(context.repositoryRoot, body.apiKey);
                response.json(200, { ok: true });
              } catch (error) { sendError(response, error); }
            },
          },
          {
            method: 'POST', path: '/api/backlog/agent/confirm-deletion', handler: async (request, response) => {
              try {
                const body = await request.readJson<{ id?: unknown }>(8 * 1024);
                if (!isRecord(body) || typeof body.id !== 'string' || !body.id) { response.json(400, { error: 'Confirmation id must be a non-empty string.' }); return; }
                response.json(200, await session.confirmDeletion(body.id));
              } catch (error) { sendError(response, error); }
            },
          },
          {
            method: 'POST', path: '/api/backlog/agent/reset', handler: async (_request, response) => {
              try { response.json(200, { ok: true, state: await session.reset() }); }
              catch (error) { sendError(response, error); }
            },
          },
        ],
        assets: [
          { path: '/assets/backlog/backlog.js', file: 'src/packages/backlog/backlog.js', contentType: 'text/javascript; charset=utf-8' },
          { path: '/assets/backlog/backlog.css', file: 'src/packages/backlog/backlog.css', contentType: 'text/css; charset=utf-8' },
        ],
        navigation: [{ id: metadata.id, label: metadata.label, order: metadata.order }],
        browser: { id: metadata.id, entry: '/assets/backlog/backlog.js', stylesheet: '/assets/backlog/backlog.css' },
        dispose: async () => { [...activeStreams].forEach((close) => close()); await session.dispose(); },
      };
    },
  };
}

const backlogPackage = createBacklogPackage();
export default backlogPackage;
