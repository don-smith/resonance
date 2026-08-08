import test from 'node:test';
import assert from 'node:assert/strict';
import { createPackagedSkillBackend, DeepAgentsRuntime } from './deepagents.ts';
import { createTelemetry } from '../../telemetry.ts';
import { BacklogAgentBusyError, createBacklogAgentSession, type BacklogAgentRuntimeFactory } from './agent-session.ts';

const decision = { path: 'backlog/plans/queue.md', title: 'Queue', status: 'in-planning' as const, priority: 'P2' as const, markdown: '# Queue' };
function fakeStore() {
  const calls: string[] = [];
  return { calls, store: {
    async listDecisions() { return [{ path: decision.path, title: decision.title, status: decision.status, priority: decision.priority }]; },
    async readDecision(requestedPath: string) { calls.push(requestedPath); if (requestedPath !== decision.path) throw new Error('Backlog item not found'); return { ...decision }; },
    async createDecision() { return { affectedPaths: ['backlog/todo.yaml'] }; },
    async editPlan() { return { affectedPaths: [decision.path] }; },
    async updateMetadata() { return { affectedPaths: ['backlog/todo.yaml'] }; },
    async setStatus() { return { affectedPaths: ['backlog/todo.yaml'] }; },
    async setPriority() { return { affectedPaths: ['backlog/todo.yaml'] }; },
    async deleteDecision(requestedPath: string) { calls.push(`delete:${requestedPath}`); return { affectedPaths: ['backlog/todo.yaml', requestedPath] }; },
  } as any };
}
function fakeFactory(log: { created: number; apiKeys: string[]; turns: any[]; dispose: number; release?: () => void; onMutation?: (result: { affectedPaths: string[] }) => void; requestDeletion?: (value: typeof decision) => Promise<any> }): BacklogAgentRuntimeFactory {
  return async (options) => {
    log.created += 1; log.apiKeys.push(options.apiKey); log.onMutation = options.onMutation; log.requestDeletion = options.requestDeletion;
    return { async *stream(turn) { log.turns.push(turn); await new Promise<void>((resolve) => { log.release = resolve; }); yield { kind: 'assistant' as const, text: 'Applied.' }; }, async dispose() { log.dispose += 1; } };
  };
}

test('is lazy, requests a missing credential without serializing it, and only creates after Send', async () => {
  const { store } = fakeStore(); const log = { created: 0, apiKeys: [] as string[], turns: [] as any[], dispose: 0 };
  const session = createBacklogAgentSession({ store, credentialProvider: async () => null, runtimeFactory: fakeFactory(log) }); const events: any[] = []; session.subscribe((event) => events.push(event));
  assert.equal(session.snapshot().hasSession, false); assert.equal(log.created, 0);
  assert.deepEqual(await session.submitPrompt({ prompt: 'Review this.', selectedPath: decision.path }), { accepted: false, credentialRequired: true });
  assert.equal(log.created, 0); assert.equal(events.some((event) => event.type === 'credential-required'), true); assert.doesNotMatch(JSON.stringify(session.snapshot()), /sk-|OPENAI|key/i);
});

test('rehydrates selected decision for sequential prompts in one shared runtime and rejects overlap', async () => {
  const { store, calls } = fakeStore(); const log = { created: 0, apiKeys: [] as string[], turns: [] as any[], dispose: 0 };
  const records: any[] = [];
  const telemetry = createTelemetry({ config: { mode: 'console' }, console: null, exporter: { record(record) { records.push(record); }, async flush() {} } });
  const session = createBacklogAgentSession({ store, telemetry, credentialProvider: async () => 'sk-local-secret', runtimeFactory: fakeFactory(log) });
  await session.submitPrompt({ prompt: 'Review this.', selectedPath: decision.path }); await assert.rejects(() => session.submitPrompt({ prompt: 'Again.', selectedPath: decision.path }), BacklogAgentBusyError); log.release!(); await new Promise((resolve) => setTimeout(resolve, 0));
  await session.submitPrompt({ prompt: 'Now edit it.', selectedPath: decision.path }); log.release!(); await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(log.created, 1); assert.deepEqual(calls, [decision.path, decision.path]); assert.equal(log.turns[1].selected.markdown, '# Queue'); assert.equal(log.turns[1].messages.filter((message: any) => message.role === 'user').length, 2); assert.equal(session.snapshot().messages.filter((message) => message.role === 'assistant').length, 2); assert.doesNotMatch(JSON.stringify(session.snapshot()), /sk-local-secret/);
  const sessionIds = new Set(records.map((record) => record.fields.sessionId).filter(Boolean));
  assert.equal(sessionIds.size, 1); assert.equal(sessionIds.values().next().value, log.turns[0].threadId);
});

test('emits committed revisions, invalidates old confirmations, and ignores stale output after reset', async () => {
  const { store, calls } = fakeStore(); const log = { created: 0, apiKeys: [] as string[], turns: [] as any[], dispose: 0 };
  const session = createBacklogAgentSession({ store, credentialProvider: async () => 'sk-local-secret', runtimeFactory: fakeFactory(log) }); const events: any[] = []; session.subscribe((event) => events.push(event));
  await session.submitPrompt({ prompt: 'Remove this.', selectedPath: decision.path }); const confirmation = await log.requestDeletion!(decision); log.release!(); await new Promise((resolve) => setTimeout(resolve, 0));
  await session.confirmDeletion(confirmation.id); assert.deepEqual(calls.at(-1), `delete:${decision.path}`); assert.deepEqual(events.find((event) => event.type === 'mutation-committed'), { type: 'mutation-committed', revision: 1, affectedPaths: ['backlog/todo.yaml', decision.path] });
  await session.submitPrompt({ prompt: 'Remove it again.', selectedPath: decision.path }); const stale = await log.requestDeletion!(decision); log.release!(); await new Promise((resolve) => setTimeout(resolve, 0)); await session.reset(); await assert.rejects(() => session.confirmDeletion(stale.id), /no longer valid/); const committedBeforeStaleCallback = events.filter((event) => event.type === 'mutation-committed').length; log.onMutation!({ affectedPaths: ['backlog/todo.yaml'] }); assert.equal(events.filter((event) => event.type === 'mutation-committed').length, committedBeforeStaleCallback); log.release!(); await new Promise((resolve) => setTimeout(resolve, 0)); assert.equal(session.snapshot().messages.length, 0); assert.equal(log.dispose, 1);
});

test('records the original model failure while preserving the generic user-facing state', async () => {
  const { store } = fakeStore(); const records: any[] = [];
  const telemetry = createTelemetry({ config: { mode: 'console' }, console: null, exporter: { record(record) { records.push(record); }, async flush() {} } });
  const runtimeFactory: BacklogAgentRuntimeFactory = async () => ({ async *stream() { throw new Error('provider unavailable'); }, async dispose() {} });
  const session = createBacklogAgentSession({ store, telemetry, credentialProvider: async () => 'local-secret', runtimeFactory });
  await session.submitPrompt({ prompt: 'Review this.', selectedPath: decision.path });
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(session.snapshot().error, 'Backlog agent request failed.');
  assert.ok(records.some((record) => record.kind === 'log' && record.message === 'Backlog agent stream failed' && record.fields.error.message === 'provider unavailable'));
  assert.ok(records.some((record) => record.kind === 'span' && record.name === 'backlog.agent.turn' && record.error.message === 'provider unavailable'));
});

test('forwards assistant chunks from the current DeepAgents model node', async () => {
  const telemetry = createTelemetry({ config: { mode: 'off' }, console: null });
  const runtime = new DeepAgentsRuntime({
    async stream() {
      return (async function* () { yield [{ content: 'Hello from the model.' }, { langgraph_node: 'model_request' }]; })();
    },
  }, telemetry);
  const updates = [];
  for await (const update of runtime.stream({ messages: [], selected: decision, threadId: 'test-thread' })) updates.push(update);
  assert.deepEqual(updates, [{ kind: 'assistant', text: 'Hello from the model.' }]);
});

test('exposes only the packaged runtime skill through its virtual backend', async () => {
  const backend = createPackagedSkillBackend('---\nname: manage-backlog\ndescription: test\n---\n# Skill');
  assert.deepEqual(await backend.ls('/skills/'), { files: [{ path: '/skills/manage-backlog/', is_dir: true }] }); assert.match((await backend.read('/skills/manage-backlog/SKILL.md')).content!, /manage-backlog/); assert.match((await backend.read('/backlog/todo.yaml')).error!, /Permission denied/); assert.match((await backend.write('/skills/manage-backlog/SKILL.md', 'no')).error!, /Permission denied/);
});
