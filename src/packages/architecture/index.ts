import { execFile } from 'node:child_process';
import { lstat, mkdir, readFile, realpath, rename, unlink, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { promisify } from 'node:util';
import { z } from 'zod';
import type { HostContext, HostResponse, PackageDefinition, PackageInput, PackageRegistration, Telemetry } from '../../package-contract.ts';
import { createArchitectureAgentSession, type ArchitectureAgentRuntimeFactory } from './architecture-agent.ts';
import { createDeepAgentsRuntimeFactory } from './architecture-deepagents.ts';
import { ArchitectureSourceError, ArchitectureStaleWriteError, ArchitectureStoreError, createArchitectureStore } from './architecture-store.ts';
import { validateArchitecture } from './architecture-checkers.ts';

const metadata = { id: 'architecture', version: '1.0.0', hostVersion: '1', label: 'Architecture', order: 25 } as const;
const inputSchema = z.object({ artifactRoot: z.string().trim().min(1).max(80).refine((value) => !value.startsWith('/') && !value.includes('\\') && !value.split('/').includes('..') && value.split('/').length <= 2, 'artifactRoot must be a narrow repository-relative directory.').optional(), provider: z.enum(['openai', 'openrouter']), model: z.string().trim().min(1).max(256) }).strict();
const isRecord = (value: unknown): value is Record<string, unknown> => Boolean(value) && typeof value === 'object' && !Array.isArray(value);
const runFile = promisify(execFile);
const CREDENTIAL_PATH = '.resonance/architecture-agent.env';
const within = (root: string, candidate: string) => candidate === root || candidate.startsWith(`${root}${path.sep}`);
const isMissing = (error: unknown) => Boolean(error && typeof error === 'object' && (error as { code?: unknown }).code === 'ENOENT');
class CredentialInputError extends Error { status = 400; }

export function architectureInput(input: PackageInput): { artifactRoot: string; provider: 'openai' | 'openrouter'; model: string } {
  const parsed = inputSchema.safeParse(input);
  if (!parsed.success) throw new Error(`Architecture input is invalid: ${parsed.error.issues[0].message}`);
  return { artifactRoot: parsed.data.artifactRoot || 'architecture', provider: parsed.data.provider, model: parsed.data.model };
}
function sendError(response: HostResponse, error: unknown, telemetry: Telemetry): void {
  const status = isRecord(error) && typeof error.status === 'number' ? error.status : error instanceof ArchitectureStaleWriteError ? 409 : error instanceof ArchitectureSourceError ? 422 : error instanceof ArchitectureStoreError ? 404 : 500;
  const message = error instanceof Error ? error.message : String(error);
  telemetry.error('Architecture route failed', { error, status }); response.json(status, { error: message });
}
function query(request: { url: string }, name: string): string { return new URL(request.url, 'http://127.0.0.1').searchParams.get(name) || ''; }
function credentialName(provider: 'openai' | 'openrouter'): string { return provider === 'openrouter' ? 'OPENROUTER_API_KEY' : 'OPENAI_API_KEY'; }
async function credentialLocations(root: string) {
  const physicalRoot = await realpath(path.resolve(root));
  const directory = path.join(physicalRoot, '.resonance'); const filename = path.join(directory, 'architecture-agent.env');
  if (!within(physicalRoot, directory) || !within(physicalRoot, filename)) throw new Error('Credential path escapes the repository.');
  return { root: physicalRoot, directory, filename };
}
async function regularCredential(filename: string): Promise<boolean> {
  let stats; try { stats = await lstat(filename); } catch (error) { if (isMissing(error)) return false; throw error; }
  if (stats.isSymbolicLink() || !stats.isFile() || (stats.mode & 0o777) !== 0o600) throw new Error('Architecture credential file must be a regular 0600 file.');
  return true;
}
async function secureCredentialDirectory(directory: string): Promise<void> {
  let stats; try { stats = await lstat(directory); } catch (error) { if (!isMissing(error)) throw error; await mkdir(directory, { recursive: true, mode: 0o700 }); stats = await lstat(directory); }
  if (stats.isSymbolicLink() || !stats.isDirectory() || await realpath(directory) !== directory) throw new Error('Architecture credential directory must be a regular directory.');
}
async function isTracked(root: string): Promise<boolean> { try { await runFile('git', ['-C', root, 'ls-files', '--error-unmatch', '--', CREDENTIAL_PATH], { encoding: 'utf8' }); return true; } catch { return false; } }
async function readCredential(root: string, provider: 'openai' | 'openrouter'): Promise<string | null> {
  const locations = await credentialLocations(root);
  let directoryStats; try { directoryStats = await lstat(locations.directory); } catch (error) { if (isMissing(error)) return null; throw error; }
  if (directoryStats.isSymbolicLink() || !directoryStats.isDirectory()) throw new Error('Architecture credential directory must be a regular directory.');
  await secureCredentialDirectory(locations.directory);
  if (!await regularCredential(locations.filename)) return null;
  const source = await readFile(locations.filename, 'utf8'); const match = /^(OPENAI_API_KEY|OPENROUTER_API_KEY)=([^\r\n]+)\n?$/.exec(source);
  if (!match || match[1] !== credentialName(provider) || !match[2].trim() || match[2] !== match[2].trim()) throw new Error('Architecture credential file is invalid.');
  return match[2];
}
async function writeCredential(root: string, provider: 'openai' | 'openrouter', apiKey: string): Promise<void> {
  if (!apiKey || apiKey.length > 4096 || /[\r\n]/.test(apiKey) || apiKey !== apiKey.trim()) throw new CredentialInputError('Credential must be a single-line API key.');
  const locations = await credentialLocations(root); await secureCredentialDirectory(locations.directory);
  if (await isTracked(locations.root)) throw new Error('Credential file must not be tracked.');
  await regularCredential(locations.filename);
  const temporary = path.join(locations.directory, `.architecture-agent.${crypto.randomUUID()}.tmp`);
  try { await writeFile(temporary, `${credentialName(provider)}=${apiKey}\n`, { encoding: 'utf8', mode: 0o600 }); await rename(temporary, locations.filename); } catch (error) { await unlink(temporary).catch(() => undefined); throw error; }
}

export function createArchitecturePackage({ runtimeFactory }: { runtimeFactory?: ArchitectureAgentRuntimeFactory } = {}): PackageDefinition {
  return {
    metadata,
    register(context, input): PackageRegistration {
      const config = architectureInput(input); const telemetry = context.telemetry.child({ package: metadata.id });
      const store = createArchitectureStore({ context, artifactRoot: config.artifactRoot });
      const agent = createArchitectureAgentSession({ store, context, telemetry, credentialProvider: () => readCredential(context.repositoryRoot, config.provider), runtimeFactory: runtimeFactory || createDeepAgentsRuntimeFactory({ provider: config.provider, model: config.model, artifactRoot: config.artifactRoot }) });
      const activeStreams = new Set<() => void>(); const read = async () => store.read();
      return {
        metadata,
        routes: [
          { method: 'GET', path: '/api/architecture/artifacts', handler: async (_request, response) => { try { response.json(200, await read()); } catch (error) { sendError(response, error, telemetry); } } },
          { method: 'GET', path: '/api/architecture/model', handler: async (_request, response) => { try { const value = await read(); try { const likec4 = await store.likec4(); response.json(200, { model: value.model, likec4: likec4.dump, likec4Views: likec4.views, revision: value.revision, likec4Revision: likec4.revision }); } catch (error) { const message = error instanceof Error ? error.message : String(error); telemetry.warn('Architecture LikeC4 source is invalid; serving recoverable model metadata.', { error }); response.json(200, { model: value.model, revision: value.revision, likec4Error: message }); } } catch (error) { sendError(response, error, telemetry); } } },
          { method: 'GET', path: '/api/architecture/views', handler: async (_request, response) => { try { const value = await read(); response.json(200, { ...value.views, revision: value.revision }); } catch (error) { sendError(response, error, telemetry); } } },
          { method: 'GET', path: '/api/architecture/graph', handler: async (request, response) => { try { response.json(200, await store.graph(query(request, 'view') || 'systemContext', query(request, 'filter'))); } catch (error) { sendError(response, error, telemetry); } } },
          { method: 'GET', path: '/api/architecture/evidence', handler: async (request, response) => { try { response.json(200, await store.readEvidence(query(request, 'path'))); } catch (error) { sendError(response, error, telemetry); } } },
          { method: 'GET', path: '/api/architecture/validation', handler: async (_request, response) => { try { const value = await read(); response.json(200, await validateArchitecture(context, value)); } catch (error) { sendError(response, error, telemetry); } } },
          { method: 'GET', path: '/api/architecture/agent/state', handler: async (_request, response) => response.json(200, agent.snapshot()) },
          { method: 'GET', path: '/api/architecture/agent/events', handler: async (request, response) => {
            const stream = response.sse(); let closed = false; let unsubscribe: (() => void) | null = null; let cleanupBeforeSubscribe = false; let resolveClosed = () => { }; const closedPromise = new Promise<void>((resolve) => { resolveClosed = resolve; });
            const close = () => { if (closed) return; closed = true; if (unsubscribe) unsubscribe(); else cleanupBeforeSubscribe = true; activeStreams.delete(close); stream.close(); resolveClosed(); };
            activeStreams.add(close); request.onAbort(close); response.onClose(close); unsubscribe = agent.subscribe((event) => { stream.write(event); if (response.closed) close(); }); if (cleanupBeforeSubscribe) unsubscribe(); await closedPromise;
          } },
          { method: 'POST', path: '/api/architecture/agent/prompt', handler: async (request, response) => {
            try { const body = await request.readJson<{ prompt?: unknown; selectedId?: unknown; selectedView?: unknown }>(32 * 1024); if (!isRecord(body) || typeof body.prompt !== 'string' || !body.prompt.trim() || (body.selectedId !== undefined && typeof body.selectedId !== 'string') || (body.selectedView !== undefined && typeof body.selectedView !== 'string')) { response.json(400, { error: 'prompt must be a non-empty string.' }); return; } response.json(202, await agent.submitPrompt({ prompt: body.prompt, selectedId: body.selectedId as string | undefined, selectedView: body.selectedView as string | undefined })); }
            catch (error) { sendError(response, error, telemetry); }
          } },
          { method: 'POST', path: '/api/architecture/agent/credential', handler: async (request, response) => { try { const body = await request.readJson<{ apiKey?: unknown }>(8 * 1024); if (!isRecord(body) || typeof body.apiKey !== 'string') { response.json(400, { error: 'apiKey must be a string.' }); return; } await writeCredential(context.repositoryRoot, config.provider, body.apiKey); response.json(200, { ok: true }); } catch (error) { sendError(response, error, telemetry); } } },
          { method: 'POST', path: '/api/architecture/agent/reset', handler: async (_request, response) => { try { response.json(200, { state: await agent.reset() }); } catch (error) { sendError(response, error, telemetry); } } },
          { method: 'POST', path: '/api/architecture/edit', handler: async (request, response) => {
            try { const body = await request.readJson<{ kind?: unknown; value?: unknown; revision?: unknown; confirmed?: unknown }>(256 * 1024); if (!isRecord(body) || !['model', 'views', 'rules', 'patterns', 'decisions'].includes(String(body.kind)) || typeof body.revision !== 'string') { response.json(400, { error: 'kind, value, and revision are required.' }); return; } const kind = body.kind as 'model' | 'views' | 'rules' | 'patterns' | 'decisions'; if (['model', 'rules', 'decisions'].includes(kind) && body.confirmed !== true) { response.json(409, { error: 'This architecture change requires explicit confirmation.' }); return; } response.json(200, await agent.applyEdit({ kind, value: body.value, revision: body.revision, confirmed: body.confirmed === true })); }
            catch (error) { sendError(response, error, telemetry); }
          } },
        ],
        assets: [{ path: '/assets/architecture/architecture.js', file: 'src/packages/architecture/architecture.js', contentType: 'text/javascript; charset=utf-8' }, { path: '/assets/architecture/architecture.css', file: 'src/packages/architecture/architecture.css', contentType: 'text/css; charset=utf-8' }],
        navigation: [{ id: metadata.id, label: metadata.label, order: metadata.order }],
        browser: { id: metadata.id, entry: '/assets/architecture/architecture.js', stylesheet: '/assets/architecture/architecture.css' },
        dispose: async () => { [...activeStreams].forEach((close) => close()); await agent.dispose(); },
      };
    },
  };
}

const architecturePackage = createArchitecturePackage();
export default architecturePackage;
