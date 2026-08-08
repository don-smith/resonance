import type { Telemetry, HostContext } from '../../package-contract.ts';
import { createTelemetry } from '../../telemetry.ts';
import type { ArchitectureStore } from './architecture-store.ts';
import { validateArchitecture, type ArchitectureValidation } from './architecture-checkers.ts';

export type ArchitectureAgentStatus = 'idle' | 'working' | 'error';
export type ArchitectureAgentMessage = { id: string; role: 'user' | 'assistant'; content: string; createdAt: string };
export type ArchitectureAgentContext = { inputTokens: number; maxInputTokens: number };
export type ArchitectureAgentSnapshot = { messages: ArchitectureAgentMessage[]; status: ArchitectureAgentStatus; hasSession: boolean; error: string | null; validation: ArchitectureValidation | null; context: ArchitectureAgentContext | null };
export type ArchitectureAgentTurn = { messages: readonly ArchitectureAgentMessage[]; selectedId?: string; selectedView?: string; threadId: string };
export type ArchitectureAgentUpdate =
  | { kind: 'assistant'; text: string; newParagraph?: boolean }
  | { kind: 'context'; context: ArchitectureAgentContext };
export type ArchitectureAgentRuntime = { stream(turn: ArchitectureAgentTurn): AsyncIterable<ArchitectureAgentUpdate>; dispose(): Promise<void> };
export type ArchitectureAgentRuntimeFactoryOptions = { apiKey: string; store: ArchitectureStore; context: HostContext; telemetry: Telemetry };
export type ArchitectureAgentRuntimeFactory = (options: ArchitectureAgentRuntimeFactoryOptions) => Promise<ArchitectureAgentRuntime>;
export type ArchitectureAgentEvent =
  | { type: 'snapshot'; snapshot: ArchitectureAgentSnapshot }
  | { type: 'message'; message: ArchitectureAgentMessage }
  | { type: 'status'; status: ArchitectureAgentStatus }
  | { type: 'context'; context: ArchitectureAgentContext }
  | { type: 'error'; message: string }
  | { type: 'credential-required' }
  | { type: 'done' };

export class ArchitectureAgentBusyError extends Error { status = 409; constructor() { super('A prompt is already running.'); this.name = 'ArchitectureAgentBusyError'; } }
class CredentialRequiredError extends Error {}
const messageId = () => crypto.randomUUID();
const newThreadId = () => crypto.randomUUID();

export function createArchitectureAgentSession({ store, context, telemetry: providedTelemetry, credentialProvider, runtimeFactory }: { store: ArchitectureStore; context: HostContext; telemetry?: Telemetry; credentialProvider?: () => Promise<string | null>; runtimeFactory?: ArchitectureAgentRuntimeFactory }) {
  const telemetry = providedTelemetry || createTelemetry({ config: { mode: 'off' } });
  const threadId = newThreadId();
  const agentTelemetry = telemetry.child({ package: 'architecture', component: 'agent' }).session(threadId);
  let runtime: ArchitectureAgentRuntime | null = null;
  let status: ArchitectureAgentStatus = 'idle';
  let error: string | null = null;
  let messages: ArchitectureAgentMessage[] = [];
  let validation: ArchitectureValidation | null = null;
  let contextUsage: ArchitectureAgentContext | null = null;
  let generation = 0;
  let starting = false;
  let closing: Promise<void> | null = null;
  let assistantId: string | null = null;
  const listeners = new Set<(event: ArchitectureAgentEvent) => void>();
  const snapshot = (): ArchitectureAgentSnapshot => ({ messages: messages.map((message) => ({ ...message })), status, hasSession: Boolean(runtime), error, validation: validation ? structuredClone(validation) : null, context: contextUsage ? { ...contextUsage } : null });
  const emit = (event: ArchitectureAgentEvent) => listeners.forEach((listener) => listener(event));
  const setStatus = (next: ArchitectureAgentStatus) => { status = next; emit({ type: 'status', status }); };
  const close = async (current: ArchitectureAgentRuntime, reportFailure = false) => {
    agentTelemetry.debug('Disposing Architecture agent runtime', { reportFailure });
    const pending = current.dispose();
    closing = pending;
    try { await pending; }
    catch (cause) {
      agentTelemetry.error('Architecture agent runtime disposal failed', { error: cause });
      if (reportFailure) {
        error = 'Architecture agent cleanup failed.';
        setStatus('error');
        emit({ type: 'error', message: error });
      }
    } finally { if (closing === pending) closing = null; }
  };
  const ensure = async (turn: number): Promise<ArchitectureAgentRuntime> => {
    if (runtime) return runtime;
    agentTelemetry.debug('Creating Architecture agent runtime');
    try {
      const apiKey = credentialProvider ? await credentialProvider() : null;
      if (!apiKey?.trim()) {
        agentTelemetry.warn('Architecture agent credential is required');
        emit({ type: 'credential-required' });
        throw new CredentialRequiredError();
      }
      if (!runtimeFactory) throw new Error('Architecture agent runtime is not configured.');
      const next = await runtimeFactory({ apiKey, store, context, telemetry: agentTelemetry });
      if (turn !== generation) { await next.dispose(); throw new Error('Architecture agent session was reset.'); }
      runtime = next;
      return next;
    } catch (cause) {
      if (cause instanceof CredentialRequiredError) throw cause;
      agentTelemetry.error('Architecture agent runtime creation failed', { error: cause });
      throw cause;
    }
  };
  const onUpdate = (turn: number, update: ArchitectureAgentUpdate) => {
    if (turn !== generation) return;
    if (update.kind === 'context') { contextUsage = { ...update.context }; emit({ type: 'context', context: { ...contextUsage } }); return; }
    if (update.kind !== 'assistant' || !update.text) return;
    const prior = assistantId ? messages.find((message) => message.id === assistantId) : undefined;
    if (prior && !update.newParagraph) prior.content += update.text;
    else { const message = { id: messageId(), role: 'assistant' as const, content: update.text, createdAt: new Date().toISOString() }; assistantId = message.id; messages.push(message); }
    const message = messages.find((item) => item.id === assistantId);
    if (message) emit({ type: 'message', message: { ...message } });
  };
  const run = async (turn: number, current: ArchitectureAgentRuntime, selectedId: string | undefined, selectedView: string | undefined, turnSpan: ReturnType<Telemetry['span']>) => {
    agentTelemetry.info('Architecture agent stream started', { selectedId, selectedView });
    try {
      for await (const update of current.stream({ messages: messages.map((message) => ({ ...message })), selectedId, selectedView, threadId })) onUpdate(turn, update);
      if (turn === generation && status === 'working') {
        assistantId = null;
        setStatus('idle');
        turnSpan.end({ status: 'ok' });
        agentTelemetry.info('Architecture agent stream completed', { selectedId, selectedView });
        emit({ type: 'done' });
      }
    } catch (cause) {
      if (turn !== generation) return;
      turnSpan.fail(cause, { status: 500 });
      if (runtime === current) { runtime = null; void close(current); }
      error = 'Architecture agent request failed.';
      setStatus('error');
      emit({ type: 'error', message: error });
      agentTelemetry.error('Architecture agent stream failed', { error: cause, selectedId, selectedView });
    }
  };
  return {
    snapshot,
    subscribe(listener: (event: ArchitectureAgentEvent) => void) { listeners.add(listener); listener({ type: 'snapshot', snapshot: snapshot() }); return () => listeners.delete(listener); },
    async submitPrompt({ prompt, selectedId, selectedView }: { prompt: string; selectedId?: string; selectedView?: string }) {
      if (closing) await closing;
      const content = prompt.trim();
      if (!content) throw new Error('Prompt must not be empty.');
      if (status === 'working' || starting) throw new ArchitectureAgentBusyError();
      const turn = generation;
      const turnSpan = agentTelemetry.span('architecture.agent.turn', { selectedId, selectedView });
      starting = true;
      error = null;
      agentTelemetry.info('Architecture prompt accepted', { selectedId, selectedView });
      emit({ type: 'snapshot', snapshot: snapshot() });
      try {
        const current = await ensure(turn);
        if (turn !== generation) throw new Error('Architecture agent session was reset.');
        turnSpan.event('architecture runtime ready');
        const user = { id: messageId(), role: 'user' as const, content, createdAt: new Date().toISOString() };
        messages.push(user);
        assistantId = null;
        emit({ type: 'message', message: { ...user } });
        setStatus('working');
        void run(turn, current, selectedId, selectedView, turnSpan);
        return { accepted: true as const };
      } catch (cause) {
        if (turn === generation && cause instanceof CredentialRequiredError) {
          turnSpan.end({ status: 'credential-required' });
          setStatus('idle');
          return { accepted: false as const, credentialRequired: true as const };
        }
        if (turn === generation) {
          turnSpan.fail(cause, { status: 500 });
          agentTelemetry.error('Architecture prompt setup failed', { error: cause, selectedId, selectedView });
          error = cause instanceof Error ? cause.message : String(cause);
          setStatus('error');
          emit({ type: 'error', message: error });
        }
        throw cause;
      } finally { if (turn === generation) starting = false; }
    },
    async applyEdit(input: { kind: 'model' | 'views' | 'rules' | 'patterns' | 'decisions'; value: unknown; revision: string; confirmed: boolean }) {
      if (['model', 'rules', 'decisions'].includes(input.kind) && !input.confirmed) throw new Error('This architecture change requires explicit confirmation.');
      const result = await store.replace(input.kind, input.value, input.revision); validation = null; emit({ type: 'snapshot', snapshot: snapshot() }); return result;
    },
    async reset() {
      agentTelemetry.info('Resetting Architecture agent session');
      if (closing) await closing;
      generation += 1;
      starting = false;
      const current = runtime;
      runtime = null;
      assistantId = null;
      messages = [];
      error = null;
      validation = null;
      contextUsage = null;
      setStatus('idle');
      emit({ type: 'snapshot', snapshot: snapshot() });
      if (current) await close(current, true);
      return snapshot();
    },
    async dispose() { await this.reset(); listeners.clear(); },
    async runValidation() { validation = await validateArchitecture(context, await store.read()); emit({ type: 'snapshot', snapshot: snapshot() }); return validation; },
  };
}
