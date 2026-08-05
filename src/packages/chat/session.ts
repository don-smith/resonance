import { createRequire } from 'node:module';
import { spawn, type ChildProcess } from 'node:child_process';
import { Readable, Writable } from 'node:stream';
import { ClientSideConnection, ndJsonStream, type Client, type RequestPermissionRequest, type RequestPermissionResponse, type SessionNotification } from '@agentclientprotocol/sdk';

const require = createRequire(import.meta.url);

export type ChatStatus = 'idle' | 'working' | 'error';
export type ChatMessage = { id: string; role: 'user' | 'assistant' | 'activity'; content: string };
export type ChatSnapshot = { messages: ChatMessage[]; status: ChatStatus; hasSession: boolean; error: string | null };
export type ChatAgentUpdate = { kind: 'assistant' | 'activity' | 'error'; text: string };
export type ChatEvent =
  | { type: 'snapshot'; snapshot: ChatSnapshot }
  | { type: 'message'; message: ChatMessage }
  | { type: 'activity'; message: ChatMessage }
  | { type: 'status'; status: ChatStatus }
  | { type: 'error'; message: string }
  | { type: 'done' };
export type AcpAdapter = { sessionId: string; prompt(text: string): Promise<void>; cancel(): Promise<void>; close(): Promise<void> };
export type AcpFactory = (options: { cwd: string; onUpdate(update: ChatAgentUpdate): void; onFailure(error: Error): void }) => Promise<AcpAdapter>;

export class ChatBusyError extends Error {
  status = 409;
  constructor() { super('A prompt is already running.'); this.name = 'ChatBusyError'; }
}
export class ChatUnavailableError extends Error {
  status = 503;
  constructor(message: string, options?: { cause?: unknown }) { super(message, options); this.name = 'ChatUnavailableError'; }
}

function messageId(): string { return crypto.randomUUID(); }
function textOf(content: unknown): string | null {
  const value = content as { type?: unknown; text?: unknown } | null;
  return value?.type === 'text' && typeof value.text === 'string' ? value.text : null;
}
function normalize(update: unknown): ChatAgentUpdate | null {
  const value = update as { sessionUpdate?: unknown; content?: unknown; title?: unknown; status?: unknown } | null;
  if (value?.sessionUpdate === 'agent_message_chunk') {
    const text = textOf(value.content);
    return text ? { kind: 'assistant', text } : null;
  }
  if (value?.sessionUpdate === 'tool_call' || value?.sessionUpdate === 'tool_call_update') {
    return { kind: 'activity', text: `${String(value.title || 'Pi tool')} ${String(value.status || 'working')}`.trim() };
  }
  return null;
}
async function waitForSpawn(child: ChildProcess): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    child.once('spawn', resolve);
    child.once('error', reject);
  });
}

export const createPiAcpFactory = (): AcpFactory => async ({ cwd, onUpdate, onFailure }) => {
  let child: ChildProcess | null = null;
  let connection: ClientSideConnection | null = null;
  let closing = false;
  const client: Client = {
    async sessionUpdate(params: SessionNotification) {
      const update = normalize(params.update);
      if (update) onUpdate(update);
    },
    async requestPermission(_params: RequestPermissionRequest): Promise<RequestPermissionResponse> {
      return { outcome: { outcome: 'cancelled' } };
    },
  };

  try {
    const entry = require.resolve('pi-acp');
    child = spawn(process.execPath, [entry], { cwd, stdio: ['pipe', 'pipe', 'pipe'] });
    child.stderr?.resume();
    await waitForSpawn(child);
    const processHandle = child;
    if (!processHandle.stdin || !processHandle.stdout) throw new Error('Pi ACP did not expose piped stdio.');
    const stream = ndJsonStream(
      Writable.toWeb(processHandle.stdin) as WritableStream<Uint8Array>,
      Readable.toWeb(processHandle.stdout) as ReadableStream<Uint8Array>,
    );
    connection = new ClientSideConnection(() => client, stream);
    processHandle.once('exit', (code, signal) => {
      if (!closing) onFailure(new Error(`Pi ACP exited (${signal || code || 'unknown'}).`));
    });
    await connection.initialize({
      protocolVersion: 1,
      clientInfo: { name: 'resonance', title: 'Resonance', version: '0.1.0' },
      clientCapabilities: {},
    });
    const { sessionId } = await connection.newSession({ cwd, mcpServers: [] });
    return {
      sessionId,
      prompt: async (text: string) => { await connection!.prompt({ sessionId, prompt: [{ type: 'text', text }] }); },
      cancel: async () => { await connection!.cancel({ sessionId }); },
      close: async () => {
        if (closing) return;
        closing = true;
        const cancellation = connection?.cancel({ sessionId }).catch(() => undefined);
        processHandle.kill('SIGTERM');
        await cancellation;
      },
    };
  } catch (error) {
    closing = true;
    child?.kill('SIGTERM');
    throw error;
  }
};

export function createChatSession({ repositoryRoot, adapterFactory = createPiAcpFactory() }: { repositoryRoot: string; adapterFactory?: AcpFactory }) {
  let adapter: AcpAdapter | null = null;
  let status: ChatStatus = 'idle';
  let error: string | null = null;
  let messages: ChatMessage[] = [];
  let generation = 0;
  let starting = false;
  let closing: Promise<void> | null = null;
  let assistantId: string | null = null;
  const listeners = new Set<(event: ChatEvent) => void>();
  const snapshot = (): ChatSnapshot => ({ messages: messages.map((message) => ({ ...message })), status, hasSession: Boolean(adapter), error });
  const emit = (event: ChatEvent) => listeners.forEach((listener) => listener(event));
  const setStatus = (next: ChatStatus) => { status = next; emit({ type: 'status', status }); };
  const closeAdapter = (current: AcpAdapter) => {
    const pending = current.close().catch(() => undefined);
    closing = pending;
    void pending.finally(() => { if (closing === pending) closing = null; });
  };
  const onUpdate = (turn: number, update: ChatAgentUpdate) => {
    if (turn !== generation) return;
    if (update.kind === 'error') {
      error = update.text;
      setStatus('error');
      emit({ type: 'error', message: update.text });
      return;
    }
    if (update.kind === 'assistant') {
      const previous = assistantId ? messages.find((message) => message.id === assistantId) : undefined;
      if (previous) previous.content += update.text;
      else {
        const message = { id: messageId(), role: 'assistant' as const, content: update.text };
        assistantId = message.id;
        messages.push(message);
      }
      const message = messages.find((item) => item.id === assistantId)!;
      emit({ type: 'message', message: { ...message } });
      return;
    }
    // Tool-call lifecycle updates are intentionally not transcript messages.
    // The working status is the single UI indicator for an active request.
    return;
  };
  const failAdapter = (turn: number, failure: Error) => {
    if (turn !== generation) return;
    const failed = adapter;
    adapter = null;
    if (failed) closeAdapter(failed);
    onUpdate(turn, { kind: 'error', text: failure.message });
  };
  const ensure = async (turn: number): Promise<AcpAdapter> => {
    if (adapter) return adapter;
    try {
      const next = await adapterFactory({
        cwd: repositoryRoot,
        onUpdate: (update) => onUpdate(turn, update),
        onFailure: (failure) => failAdapter(turn, failure),
      });
      if (turn !== generation) {
        await next.close();
        throw new Error('Chat session was reset.');
      }
      adapter = next;
      return next;
    } catch (failure) {
      throw new ChatUnavailableError(failure instanceof Error ? failure.message : String(failure), { cause: failure });
    }
  };
  const run = async (turn: number, current: AcpAdapter, text: string) => {
    try {
      await current.prompt(text);
      if (turn === generation && status === 'working') {
        assistantId = null;
        setStatus('idle');
        emit({ type: 'done' });
      }
    } catch (failure) {
      if (turn === generation) {
        if (adapter === current) {
          adapter = null;
          closeAdapter(current);
        }
        error = failure instanceof Error ? failure.message : String(failure);
        setStatus('error');
        emit({ type: 'error', message: error });
      }
    }
  };
  return {
    snapshot,
    subscribe(listener: (event: ChatEvent) => void) {
      listeners.add(listener);
      listener({ type: 'snapshot', snapshot: snapshot() });
      return () => { listeners.delete(listener); };
    },
    async submitPrompt(text: string) {
      if (closing) await closing;
      const prompt = text.trim();
      if (!prompt) throw new Error('Prompt must not be empty.');
      if (status === 'working' || starting) throw new ChatBusyError();
      const turn = generation;
      starting = true;
      error = null;
      setStatus('working');
      try {
        const current = await ensure(turn);
        if (turn !== generation) throw new Error('Chat session was reset.');
        const user = { id: messageId(), role: 'user' as const, content: prompt };
        messages.push(user);
        assistantId = null;
        emit({ type: 'message', message: user });
        void run(turn, current, prompt);
      } catch (failure) {
        if (turn === generation) {
          error = failure instanceof Error ? failure.message : String(failure);
          setStatus('error');
          emit({ type: 'error', message: error });
        }
        throw failure;
      } finally {
        if (turn === generation) starting = false;
      }
    },
    async reset() {
      if (closing) await closing;
      generation += 1;
      starting = false;
      const current = adapter;
      adapter = null;
      assistantId = null;
      messages = [];
      error = null;
      setStatus('idle');
      emit({ type: 'snapshot', snapshot: snapshot() });
      if (current) {
        const pending = current.close();
        const tracked = pending.catch(() => undefined);
        closing = tracked;
        try { await pending; }
        catch (failure) {
          error = failure instanceof Error ? failure.message : String(failure);
          setStatus('error');
          emit({ type: 'error', message: error });
        }
        finally { if (closing === tracked) closing = null; }
      }
      return snapshot();
    },
    async dispose() {
      await this.reset();
      listeners.clear();
    },
  };
}
