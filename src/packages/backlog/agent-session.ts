import type { Telemetry } from '../../package-contract.ts';
import { createTelemetry } from '../../telemetry.ts';
import type { BacklogDecision, BacklogMutation, BacklogStore } from './backlog-store.ts';

export type BacklogAgentStatus = 'idle' | 'working' | 'error';
export type BacklogAgentMessage = { id: string; role: 'user' | 'assistant'; content: string };
export type BacklogDeletionConfirmation = { id: string; path: string; title: string };
export type BacklogAgentSnapshot = { messages: BacklogAgentMessage[]; status: BacklogAgentStatus; hasSession: boolean; error: string | null; pendingDeletion: BacklogDeletionConfirmation | null };
export type BacklogAgentUpdate = { kind: 'assistant'; text: string };
export type BacklogAgentTurn = { messages: readonly BacklogAgentMessage[]; selected: BacklogDecision; threadId: string };
export type BacklogAgentRuntime = { stream(turn: BacklogAgentTurn): AsyncIterable<BacklogAgentUpdate>; dispose(): Promise<void> };
export type BacklogAgentRuntimeFactoryOptions = { apiKey: string; store: BacklogStore; telemetry: Telemetry; onMutation(result: BacklogMutation): void; requestDeletion(decision: BacklogDecision): Promise<BacklogDeletionConfirmation> };
export type BacklogAgentRuntimeFactory = (options: BacklogAgentRuntimeFactoryOptions) => Promise<BacklogAgentRuntime>;
export type BacklogAgentEvent =
  | { type: 'snapshot'; snapshot: BacklogAgentSnapshot }
  | { type: 'message'; message: BacklogAgentMessage }
  | { type: 'status'; status: BacklogAgentStatus }
  | { type: 'error'; message: string }
  | { type: 'credential-required' }
  | { type: 'deletion-confirmation'; confirmation: BacklogDeletionConfirmation }
  | { type: 'mutation-committed'; revision: number; affectedPaths: string[] }
  | { type: 'done' };

export class BacklogAgentBusyError extends Error {
  status = 409;
  constructor() { super('A prompt is already running.'); this.name = 'BacklogAgentBusyError'; }
}
export class BacklogAgentConfirmationError extends Error {
  status = 409;
  constructor(message: string) { super(message); this.name = 'BacklogAgentConfirmationError'; }
}
export class BacklogAgentUnavailableError extends Error {
  status = 503;
  constructor(message = 'Backlog agent is unavailable.', options?: { cause?: unknown }) { super(message, options); this.name = 'BacklogAgentUnavailableError'; }
}
class CredentialRequiredError extends Error {}

const messageId = () => crypto.randomUUID();
const newThreadId = () => crypto.randomUUID();

export function createBacklogAgentSession({ store, credentialProvider, runtimeFactory, telemetry: providedTelemetry }: { store: BacklogStore; credentialProvider(): Promise<string | null>; runtimeFactory: BacklogAgentRuntimeFactory; telemetry?: Telemetry }) {
  const telemetry = providedTelemetry || createTelemetry({ config: { mode: 'off' } });
  const threadId = newThreadId();
  const agentTelemetry = telemetry.child({ package: 'backlog', component: 'agent' }).session(threadId);
  let runtime: BacklogAgentRuntime | null = null;
  let status: BacklogAgentStatus = 'idle';
  let error: string | null = null;
  let messages: BacklogAgentMessage[] = [];
  let pendingDeletion: BacklogDeletionConfirmation | null = null;
  let revision = 0;
  let generation = 0;
  let starting = false;
  let closing: Promise<void> | null = null;
  let assistantId: string | null = null;
  const listeners = new Set<(event: BacklogAgentEvent) => void>();

  const snapshot = (): BacklogAgentSnapshot => ({
    messages: messages.map((message) => ({ ...message })),
    status,
    hasSession: Boolean(runtime),
    error,
    pendingDeletion: pendingDeletion ? { ...pendingDeletion } : null,
  });
  const emit = (event: BacklogAgentEvent) => listeners.forEach((listener) => listener(event));
  const setStatus = (next: BacklogAgentStatus) => { status = next; emit({ type: 'status', status }); };
  const close = async (current: BacklogAgentRuntime, reportFailure = false) => {
    agentTelemetry.debug('Disposing Backlog agent runtime', { reportFailure });
    const pending = current.dispose();
    closing = pending;
    try { await pending; }
    catch (cause) {
      agentTelemetry.error('Backlog agent runtime disposal failed', { error: cause });
      if (reportFailure) {
        error = 'Backlog agent cleanup failed.';
        setStatus('error');
        emit({ type: 'error', message: error });
      }
    } finally { if (closing === pending) closing = null; }
  };
  const commit = (result: BacklogMutation) => {
    agentTelemetry.info('Backlog mutation committed', { affectedPaths: result.affectedPaths });
    if (pendingDeletion) {
      pendingDeletion = null;
      emit({ type: 'snapshot', snapshot: snapshot() });
    }
    revision += 1;
    emit({ type: 'mutation-committed', revision, affectedPaths: [...result.affectedPaths] });
  };
  const requestDeletion = async (decision: BacklogDecision): Promise<BacklogDeletionConfirmation> => {
    if (pendingDeletion) throw new BacklogAgentConfirmationError('A deletion confirmation is already pending.');
    pendingDeletion = { id: messageId(), path: decision.path, title: decision.title };
    emit({ type: 'deletion-confirmation', confirmation: { ...pendingDeletion } });
    return { ...pendingDeletion };
  };
  const onUpdate = (turn: number, update: BacklogAgentUpdate) => {
    if (turn !== generation || update.kind !== 'assistant' || !update.text) return;
    const prior = assistantId ? messages.find((message) => message.id === assistantId) : undefined;
    if (prior) prior.content += update.text;
    else {
      const message = { id: messageId(), role: 'assistant' as const, content: update.text };
      assistantId = message.id;
      messages.push(message);
    }
    const message = messages.find((item) => item.id === assistantId);
    if (message) emit({ type: 'message', message: { ...message } });
  };
  const ensure = async (turn: number): Promise<BacklogAgentRuntime> => {
    if (runtime) return runtime;
    agentTelemetry.debug('Creating Backlog agent runtime');
    try {
      const apiKey = await credentialProvider();
      if (!apiKey?.trim()) {
        agentTelemetry.warn('Backlog agent credential is required');
        emit({ type: 'credential-required' });
        throw new CredentialRequiredError();
      }
      const next = await runtimeFactory({
        apiKey,
        store,
        telemetry: agentTelemetry,
        onMutation: (result) => { if (turn === generation) commit(result); },
        requestDeletion: async (decision) => {
          if (turn !== generation) throw new BacklogAgentConfirmationError('Deletion confirmation is no longer valid.');
          return requestDeletion(decision);
        },
      });
      if (turn !== generation) {
        await next.dispose();
        throw new Error('Backlog agent session was reset.');
      }
      runtime = next;
      return next;
    } catch (cause) {
      if (cause instanceof CredentialRequiredError) throw cause;
      if (cause instanceof BacklogAgentUnavailableError) throw cause;
      agentTelemetry.error('Backlog agent runtime creation failed', { error: cause });
      throw new BacklogAgentUnavailableError('Backlog agent is unavailable.', { cause });
    }
  };
  const currentTurnOutput = () => {
    const lastUser = messages.findLastIndex((message) => message.role === 'user');
    return messages.slice(lastUser + 1).filter((message) => message.role === 'assistant').map((message) => ({ role: message.role, content: message.content }));
  };
  const run = async (turn: number, current: BacklogAgentRuntime, selected: BacklogDecision, turnSpan: ReturnType<Telemetry['span']>) => {
    agentTelemetry.info('Backlog agent stream started', { selectedPath: selected.path });
    try {
      for await (const update of current.stream({ messages: messages.map((message) => ({ ...message })), selected, threadId })) onUpdate(turn, update);
      if (turn === generation && status === 'working') {
        assistantId = null;
        setStatus('idle');
        turnSpan.end({ status: 'ok', output: currentTurnOutput() });
        agentTelemetry.info('Backlog agent stream completed', { selectedPath: selected.path });
        emit({ type: 'done' });
      }
    } catch (cause) {
      if (turn !== generation) return;
      turnSpan.fail(cause, { status: 500, output: currentTurnOutput() });
      agentTelemetry.error('Backlog agent stream failed', { error: cause, selectedPath: selected.path });
      if (runtime === current) { runtime = null; void close(current); }
      error = 'Backlog agent request failed.';
      setStatus('error');
      emit({ type: 'error', message: error });
    }
  };

  return {
    snapshot,
    subscribe(listener: (event: BacklogAgentEvent) => void) {
      listeners.add(listener);
      listener({ type: 'snapshot', snapshot: snapshot() });
      return () => { listeners.delete(listener); };
    },
    async submitPrompt({ prompt, selectedPath }: { prompt: string; selectedPath: string }) {
      if (closing) await closing;
      const content = prompt.trim();
      if (!content) throw new Error('Prompt must not be empty.');
      if (status === 'working' || starting) throw new BacklogAgentBusyError();
      const turn = generation;
      const turnSpan = agentTelemetry.span('backlog.agent.turn', { selectedPath, input: [{ role: 'user', content }] });
      starting = true;
      error = null;
      pendingDeletion = null;
      agentTelemetry.info('Backlog prompt accepted', { selectedPath });
      emit({ type: 'snapshot', snapshot: snapshot() });
      try {
        const selected = await store.readDecision(selectedPath);
        turnSpan.event('selected decision loaded');
        const current = await ensure(turn);
        if (turn !== generation) throw new Error('Backlog agent session was reset.');
        const user = { id: messageId(), role: 'user' as const, content };
        messages.push(user);
        assistantId = null;
        emit({ type: 'message', message: { ...user } });
        setStatus('working');
        void run(turn, current, selected, turnSpan);
        return { accepted: true as const };
      } catch (cause) {
        if (turn === generation && cause instanceof CredentialRequiredError) {
          turnSpan.end({ status: 'credential-required' });
          setStatus('idle');
          return { accepted: false as const, credentialRequired: true as const };
        }
        if (turn === generation) {
          turnSpan.fail(cause, { status: 500 });
          agentTelemetry.error('Backlog prompt setup failed', { error: cause, selectedPath });
          error = cause instanceof BacklogAgentUnavailableError ? cause.message : 'Selected Backlog decision is unavailable.';
          setStatus('error');
          emit({ type: 'error', message: error });
        }
        throw cause;
      } finally { if (turn === generation) starting = false; }
    },
    async confirmDeletion(id: string) {
      if (!pendingDeletion || pendingDeletion.id !== id) throw new BacklogAgentConfirmationError('Deletion confirmation is no longer valid.');
      if (status === 'working' || starting) throw new BacklogAgentBusyError();
      const confirmation = pendingDeletion;
      pendingDeletion = null;
      emit({ type: 'snapshot', snapshot: snapshot() });
      const result = await store.deleteDecision(confirmation.path);
      commit(result);
      return { ok: true as const, revision };
    },
    async reset() {
      agentTelemetry.info('Resetting Backlog agent session');
      if (closing) await closing;
      generation += 1;
      starting = false;
      const current = runtime;
      runtime = null;
      assistantId = null;
      messages = [];
      error = null;
      pendingDeletion = null;
      setStatus('idle');
      emit({ type: 'snapshot', snapshot: snapshot() });
      if (current) await close(current, true);
      return snapshot();
    },
    async dispose() { await this.reset(); listeners.clear(); },
  };
}
