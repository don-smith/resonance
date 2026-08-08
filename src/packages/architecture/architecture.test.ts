import test from 'node:test';
import assert from 'node:assert/strict';
import { cp, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseHTML } from 'linkedom';
import { createApp } from '../../server.ts';
import { createHost } from '../../host.ts';
import type { HostContext } from '../../package-contract.ts';
import { createTelemetry } from '../../telemetry.ts';
import { createArchitectureStore, modelSchema } from './architecture-store.ts';
import { createArchitectureAgentSession } from './architecture-agent.ts';
import { ArchitectureChatOpenAI, createArchitectureTools, createPackagedSkillBackend, DeepAgentsRuntime, providerFetch } from './architecture-deepagents.ts';
import { validateArchitecture } from './architecture-checkers.ts';
import createPackage, { architectureInput } from './index.ts';
import createBrowser from './architecture.js';

const appRoot = fileURLToPath(new URL('../../../', import.meta.url));
async function withServer(run: (baseUrl: string) => Promise<void>, options: Parameters<typeof createApp>[0]) { const server = await createApp(options); await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve)); const address = server.address(); if (!address || typeof address === 'string') throw new Error('Server did not bind.'); try { await run(`http://127.0.0.1:${address.port}`); } finally { await new Promise<void>((resolve, reject) => server.close((error) => error ? reject(error) : resolve())); } }
async function fixture() { const root = await mkdtemp(path.join(tmpdir(), 'resonance-architecture-')); await mkdir(path.join(root, 'architecture'), { recursive: true }); await mkdir(path.join(root, '.resonance'), { recursive: true }); await cp(path.join(appRoot, 'src'), path.join(root, 'src'), { recursive: true }); for (const filename of ['model.json', 'model.c4', 'views.json', 'rules.json', 'patterns.json', 'decisions.json']) await cp(path.join(appRoot, 'architecture', filename), path.join(root, 'architecture', filename)); await writeFile(path.join(root, '.resonance/config.json'), JSON.stringify({ version: 1, packages: { shell: { module: 'src/packages/shell/index.ts' }, architecture: { module: 'src/packages/architecture/index.ts' } } })); return root; }
function context(root: string): HostContext { const telemetry = createTelemetry({ root, console: null }); return { repositoryRoot: root, appRoot, telemetry, resolveRepositoryPath(relative) { if (!relative || relative.startsWith('/') || relative.includes('\\') || relative.split('/').includes('..')) return null; return relative; } }; }

test('validates the typed model and rejects unstable or malformed entities', () => {
  assert.equal(modelSchema.parse({ version: 1, entities: [{ id: 'shell', type: 'package', name: 'Shell' }], relationships: [] }).entities[0].id, 'shell');
  assert.throws(() => modelSchema.parse({ version: 1, entities: [{ id: 'Shell', type: 'package', name: 'Shell' }], relationships: [] }));
  assert.deepEqual(architectureInput({ provider: 'openrouter', model: 'test-model' }), { artifactRoot: 'architecture', provider: 'openrouter', model: 'test-model' });
  assert.throws(() => architectureInput({ artifactRoot: '../outside', provider: 'openrouter', model: 'test-model' }));
  assert.throws(() => architectureInput({ artifactRoot: 'architecture' }));
});

test('mounts the renamed C4 codebase architecture skill at the agent skill path', async () => {
  const skill = await readFile(new URL('./skills/c4-codebase-architecture-skill/SKILL.md', import.meta.url), 'utf8');
  assert.match(skill, /^---\nname: c4-codebase-architecture-skill\n/);
  const backend = createPackagedSkillBackend(skill);
  assert.deepEqual(backend.ls('/skills'), { files: [{ path: '/skills/c4-codebase-architecture-skill/', is_dir: true }] });
  assert.deepEqual(backend.ls('/skills/c4-codebase-architecture-skill'), { files: [{ path: '/skills/c4-codebase-architecture-skill/SKILL.md', is_dir: false }] });
  assert.deepEqual(backend.read('/skills/c4-codebase-architecture-skill/SKILL.md'), { content: skill, mimeType: 'text/markdown' });
  assert.match(String(backend.read('/skills/c4-architecture/SKILL.md').error), /Permission denied/);
});

test('gives the Architecture agent architecture and Markdown write access without implementation write access', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-architecture-readonly-'));
  try {
    await mkdir(path.join(root, 'src', 'nested'), { recursive: true });
    await mkdir(path.join(root, 'architecture'), { recursive: true });
    await mkdir(path.join(root, '.resonance'), { recursive: true });
    await writeFile(path.join(root, '.resonance', 'architecture-agent.env'), 'OPENROUTER_API_KEY=do-not-read\n');
    await writeFile(path.join(root, 'README.md'), 'repository needle\n');
    await writeFile(path.join(root, '.hidden.ts'), 'hidden needle\n');
    await writeFile(path.join(root, 'src', 'nested', 'main.ts'), 'nested needle\n');
    const skill = await readFile(new URL('./skills/c4-codebase-architecture-skill/SKILL.md', import.meta.url), 'utf8');
    const backend = createPackagedSkillBackend(skill, root);
    const listing = await backend.ls('/');
    assert.ok(listing.files?.some((file) => file.path === '/.hidden.ts'));
    assert.ok(listing.files?.some((file) => file.path === '/src/' && file.is_dir));
    assert.match(String((await backend.read('/src/nested/main.ts')).content), /nested needle/);
    assert.equal((await backend.read('/skills/c4-codebase-architecture-skill/SKILL.md')).content, skill);
    assert.ok((await backend.glob('**/*.ts')).files?.some((file) => file.path === '/src/nested/main.ts'));
    assert.ok((await backend.grep('needle')).matches?.some((match) => match.path === '/README.md'));
    assert.equal((await backend.write('/architecture/model.c4', 'model content\n')).path, '/architecture/model.c4');
    assert.equal((await backend.edit('/architecture/model.c4', 'model', 'updated')).occurrences, 1);
    assert.equal(await readFile(path.join(root, 'architecture', 'model.c4'), 'utf8'), 'updated content\n');
    assert.equal((await backend.edit('/README.md', 'repository needle', 'updated needle')).occurrences, 1);
    assert.equal(await readFile(path.join(root, 'README.md'), 'utf8'), 'updated needle\n');
    assert.equal((await backend.write('/docs/architecture-notes.md', '# Notes\n')).path, '/docs/architecture-notes.md');
    assert.match(String((await backend.write('/src/nested/main.ts', 'changed')).error), /read-only/);
    assert.match(String((await backend.edit('/src/nested/main.ts', 'nested', 'changed')).error), /read-only/);
    assert.match(String((await backend.write('/.git/config', 'changed')).error), /read-only/);
    assert.match(String((await backend.write('/.resonance/architecture-agent.env', 'changed')).error), /read-only/);
    assert.match(String((await backend.read('/../README.md')).error), /Invalid repository path/);
    assert.match(String((await backend.read('/.resonance/architecture-agent.env')).error), /not available/);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('expands the architecture surface when the agent is hidden and aligns panel headers', async () => {
  const css = await readFile(new URL('./architecture.css', import.meta.url), 'utf8');
  assert.match(css, /\.architecture-workspace\.architecture-agent-hidden \{[^}]*grid-template-columns: minmax\(180px, 220px\) minmax\(0, 1fr\);/s);
  assert.match(css, /\.architecture-header \{[^}]*min-height: 77px;/s);
  assert.match(css, /\.architecture-agent-header \{[^}]*min-height: 77px;/s);
  assert.match(css, /\.architecture-validation-view \.architecture-validation-button \{[^}]*background: var\(--ink/s);
  assert.match(css, /\.architecture-message-user \{[^}]*background: var\(--accent-soft/s);
  assert.match(css, /\.architecture-nav-group-items\[hidden\] \{[^}]*display: none/s);
  assert.match(css, /\.architecture-message-content p \{[^}]*margin: 0;/s);
  assert.doesNotMatch(css, /\.architecture-context \{/);
  assert.match(css, /\.architecture-context-usage \{/);
  const diagram = await readFile(new URL('./architecture-likec4.tsx', import.meta.url), 'utf8');
  assert.match(diagram, /mantineTheme=\{\{ white: 'var\(--paper, #f7f8f6\)' \}\}/);
  assert.match(diagram, /reactFlowProps=\{\{ zoomOnScroll: true, panOnScroll: false \}\}/);
  const browserSource = await readFile(new URL('./architecture-source.js', import.meta.url), 'utf8');
  assert.match(browserSource, /onNavigate: async \(viewId\) => \{ activeView = viewId; updateViewHeader\(viewId\); await refreshWorkspace\(\); \}/);
  assert.match(browserSource, /architecture-message-content/);
  assert.match(browserSource, /element\('strong', 'Agent'\)/);
  assert.match(browserSource, /\(modelResponse\?\.likec4Views \|\| viewsResponse\.views\)\.filter\(\(view\) => view\.id !== 'index'\)/);
});

test('returns LikeC4 parse failures as recoverable model-tool context', async () => {
  const root = await fixture();
  try {
    const filename = path.join(root, 'architecture', 'model.c4');
    const source = await readFile(filename, 'utf8');
    await writeFile(filename, source.replace('resonanceRuntime.host.telemetry', 'resonanceRuntime.host.missing'));
    const store = createArchitectureStore({ context: context(root) });
    const tools = createArchitectureTools({ store, context: context(root), apiKey: 'test-key', telemetry: createTelemetry({ root, console: null }) });
    const result = JSON.parse(await tools.find((candidate) => candidate.name === 'read_model')!.invoke({}));
    assert.match(result.likec4Error, /Could not resolve reference to Referenceable named 'missing'/);
    assert.ok(result.rules);
    const viewResult = JSON.parse(await tools.find((candidate) => candidate.name === 'read_view')!.invoke({ view: 'systemContext' }));
    assert.match(viewResult.error, /Could not resolve reference to Referenceable named 'missing'/);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('returns invalid architecture metadata as recoverable agent context', async () => {
  const root = await fixture();
  try {
    await writeFile(path.join(root, 'architecture', 'views.json'), JSON.stringify({ version: 1, views: [{ id: 'broken' }] }));
    const store = createArchitectureStore({ context: context(root) });
    const tools = createArchitectureTools({ store, context: context(root), apiKey: 'test-key', telemetry: createTelemetry({ root, console: null }) });
    const result = JSON.parse(await tools.find((candidate) => candidate.name === 'read_model')!.invoke({}));
    assert.match(result.error, /views\.json is invalid: Required/);
    assert.match(result.recovery, /views\.json/);
    assert.match(result.recovery, /read_model/);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('reads projections, creates deterministic graphs, and validates local evidence', async () => {
  const root = await fixture();
  try {
    const store = createArchitectureStore({ context: context(root) });
    const artifacts = await store.read();
    assert.equal(artifacts.model.entities.length > 0, true);
    const graph = await store.graph('system-context');
    assert.equal(graph.nodes.some((node) => node.id === 'resonanceRuntime'), true);
    const evidence = await store.readEvidence('src/host.ts');
    assert.equal(evidence.entities.includes('host'), true);
    const validation = await validateArchitecture(context(root), artifacts);
    assert.equal(validation.results.every((result) => ['pass', 'fail', 'unknown'].includes(result.status)), true);
    assert.equal(validation.results.find((result) => result.ruleId === 'authoritative-package-configuration')?.status, 'pass');
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('uses the configured architecture runtime lazily and streams chat state', async () => {
  const root = await fixture();
  try {
    const store = createArchitectureStore({ context: context(root) });
    let created = 0;
    const session = createArchitectureAgentSession({
      store, context: context(root), credentialProvider: async () => 'test-key', telemetry: createTelemetry({ root, console: null }),
      runtimeFactory: async () => { created += 1; return { async *stream() { yield { kind: 'assistant' as const, text: 'C4 answer' }; }, async dispose() {} }; },
    });
    assert.equal(created, 0);
    assert.deepEqual(await session.submitPrompt({ prompt: 'Explain this view', selectedView: 'system-context' }), { accepted: true });
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.equal(created, 1); assert.equal(session.snapshot().messages.at(-1)?.content, 'C4 answer'); assert.equal(session.snapshot().status, 'idle');
    await session.dispose();
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('keeps separate model responses in separate assistant messages', async () => {
  const root = await fixture();
  try {
    const session = createArchitectureAgentSession({
      store: createArchitectureStore({ context: context(root) }),
      context: context(root),
      credentialProvider: async () => 'test-key',
      runtimeFactory: async () => ({
        async *stream() {
          yield { kind: 'context' as const, context: { inputTokens: 42876, maxInputTokens: 1048576 } };
          yield { kind: 'assistant' as const, text: 'First response.' };
          yield { kind: 'assistant' as const, text: 'Second response.', newParagraph: true };
        },
        async dispose() {},
      }),
    });
    await session.submitPrompt({ prompt: 'Explain this view', selectedView: 'system-context' });
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.deepEqual(session.snapshot().messages.map((message) => message.content), ['Explain this view', 'First response.', 'Second response.']);
    assert.deepEqual(session.snapshot().context, { inputTokens: 42876, maxInputTokens: 1048576 });
    await session.reset();
    assert.equal(session.snapshot().context, null);
    await session.dispose();
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('supplies the OpenRouter DeepSeek Pro profile to Deep Agents', () => {
  const model = new ArchitectureChatOpenAI({ model: 'deepseek/deepseek-v4-pro', apiKey: 'test-key' });
  assert.deepEqual(model.profile, {
    maxInputTokens: 1_048_576,
    maxOutputTokens: 393_216,
    imageInputs: false,
    imageUrlInputs: false,
    pdfInputs: false,
    audioInputs: false,
    videoInputs: false,
    imageToolMessage: false,
    pdfToolMessage: false,
    reasoningOutput: true,
    imageOutputs: false,
    audioOutputs: false,
    videoOutputs: false,
    toolCalling: true,
    toolChoice: true,
    structuredOutput: true,
  });
});

test('forwards provider context usage from the current DeepAgents model node', async () => {
  const telemetry = createTelemetry({ config: { mode: 'off' }, console: null });
  const runtime = new DeepAgentsRuntime({
    async stream() {
      return (async function* () { yield [{ content: '' , usage_metadata: { input_tokens: 42876 } }, { langgraph_node: 'model_request' }]; })();
    },
  }, telemetry, 1048576);
  const updates = [];
  for await (const update of runtime.stream({ messages: [], threadId: 'test-thread' })) updates.push(update);
  assert.deepEqual(updates, [{ kind: 'context', context: { inputTokens: 42876, maxInputTokens: 1048576 } }]);
});

test('forwards assistant chunks from the current DeepAgents model node', async () => {
  const telemetry = createTelemetry({ config: { mode: 'off' }, console: null });
  const runtime = new DeepAgentsRuntime({
    async stream() {
      return (async function* () { yield [{ content: 'Hello from the model.' }, { langgraph_node: 'model_request' }]; })();
    },
  }, telemetry);
  const updates = [];
  for await (const update of runtime.stream({ messages: [], threadId: 'test-thread' })) updates.push(update);
  assert.deepEqual(updates, [{ kind: 'assistant', text: 'Hello from the model.' }]);
});

test('starts a new chat paragraph when the agent produces another model response', async () => {
  const telemetry = createTelemetry({ config: { mode: 'off' }, console: null });
  const runtime = new DeepAgentsRuntime({
    async stream() {
      return (async function* () {
        yield [{ content: 'First response.' }, { langgraph_node: 'model_request' }];
        yield [{ content: '' }, { langgraph_node: 'tools' }];
        yield [{ content: 'Second response.' }, { langgraph_node: 'model_request' }];
      })();
    },
  }, telemetry);
  const updates = [];
  for await (const update of runtime.stream({ messages: [], threadId: 'test-thread' })) updates.push(update);
  assert.deepEqual(updates, [
    { kind: 'assistant', text: 'First response.' },
    { kind: 'assistant', text: 'Second response.', newParagraph: true },
  ]);
});

test('records redacted provider failure metadata without exposing request content', async () => {
  const records: any[] = [];
  const telemetry = createTelemetry({ config: { mode: 'console' }, console: null, exporter: { record(record) { records.push(record); }, async flush() {} } });
  const response = await providerFetch('https://provider.invalid', { body: JSON.stringify({ messages: ['private'] }) }, telemetry, async () => new Response(JSON.stringify({ error: { message: 'Internal Server Error', code: 500, type: 'server_error' } }), { status: 500, headers: { 'x-generation-id': 'generation-1', 'x-provider-name': 'provider-1' } }));
  assert.equal(response.status, 500);
  const record = records.find((item) => item.kind === 'log' && item.message === 'Architecture provider request failed');
  assert.deepEqual(record.fields.providerError, { message: 'Internal Server Error', code: 500, type: 'server_error' });
  assert.equal(record.fields.requestBytes, JSON.stringify({ messages: ['private'] }).length);
  assert.equal(record.fields.generationId, 'generation-1');
  assert.equal(record.fields.providerName, 'provider-1');
  await telemetry.dispose();
});

test('emits session, turn tracing, and original stream errors', async () => {
  const root = await fixture();
  try {
    const records: any[] = [];
    const telemetry = createTelemetry({ root, console: null, exporter: { record(record) { records.push(record); }, async flush() {} } });
    const store = createArchitectureStore({ context: context(root) });
    const session = createArchitectureAgentSession({
      store,
      context: { ...context(root), telemetry },
      telemetry,
      credentialProvider: async () => 'local-secret',
      runtimeFactory: async () => ({ async *stream() { throw new Error('provider unavailable'); }, async dispose() {} }),
    });
    await session.submitPrompt({ prompt: 'Explain this view.', selectedView: 'system-context' });
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.equal(session.snapshot().error, 'Architecture agent request failed.');
    assert.ok(records.some((record) => record.kind === 'log' && record.message === 'Architecture agent stream failed' && record.fields.error.message === 'provider unavailable'));
    assert.ok(records.some((record) => record.kind === 'span' && record.name === 'architecture.agent.turn' && record.error.message === 'provider unavailable'));
    const sessionIds = new Set(records.map((record) => record.fields.sessionId).filter(Boolean));
    assert.equal(sessionIds.size, 1);
    await session.dispose();
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('uses revision checks and atomic schema-validated edits', async () => {
  const root = await fixture();
  try {
    const store = createArchitectureStore({ context: context(root) }); const current = await store.read();
    const nextViews = { ...current.views, views: current.views.views.slice(0, 1) };
    const mutation = await store.replace('views', nextViews, current.revision);
    assert.notEqual(mutation.revision, current.revision);
    await assert.rejects(() => store.replace('views', nextViews, current.revision), /changed; reload/);
    await assert.rejects(() => store.replace('rules', { version: 1, rules: [{ bad: true }] }, mutation.revision), /invalid/);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('serves recoverable model metadata when LikeC4 source is invalid', async () => {
  const root = await fixture();
  try {
    const filename = path.join(root, 'architecture', 'model.c4');
    await writeFile(filename, (await readFile(filename, 'utf8')).replace('resonanceRuntime.host.telemetry', 'resonanceRuntime.host.missing'));
    await withServer(async (baseUrl) => {
      const response = await fetch(`${baseUrl}/api/architecture/model`);
      assert.equal(response.status, 200);
      const body = await response.json();
      assert.match(body.likec4Error, /Could not resolve reference to Referenceable named 'missing'/);
      assert.ok(body.model);
    }, { root, appRoot, config: { version: 1, packages: { shell: { module: 'src/packages/shell/index.ts' }, architecture: { module: 'src/packages/architecture/index.ts', provider: 'openrouter', model: 'test-model' } } } });
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('composes through the host contract and exposes namespaced read-only routes', async () => {
  const root = await fixture();
  try {
    const config = { version: 1 as const, packages: { shell: { module: 'src/packages/shell/index.ts' }, architecture: { module: 'src/packages/architecture/index.ts', provider: 'openrouter', model: 'test-model' } } };
    await withServer(async (baseUrl) => {
      const manifest = await fetch(`${baseUrl}/api/manifest`).then((response) => response.json());
      assert.ok(manifest.packages.some((item) => item.id === 'architecture'));
      const model = await fetch(`${baseUrl}/api/architecture/model`).then((response) => response.json());
      assert.equal(model.model.entities.some((entity) => entity.id === 'resonance'), true);
      assert.ok(model.likec4.elements.resonanceRuntime);
      assert.equal(model.likec4.elements.teamMember.title, 'Team member');
      assert.equal(model.likec4.elements.targetRepository.title, 'Target repository');
      assert.equal(model.likec4.elements.llmProvider.title, 'LLM provider');
      assert.equal(model.likec4Views.some((view: { id: string }) => view.id === 'packageRelations'), false);
      assert.ok(model.likec4Views.some((view: { id: string }) => view.id === 'systemContext'));
      const contextGraph = await fetch(`${baseUrl}/api/architecture/graph?view=systemContext`).then((response) => response.json());
      assert.ok(contextGraph.nodes.some((node: { modelRef?: string }) => node.modelRef === 'llmProvider'));
      assert.equal((await fetch(`${baseUrl}/api/architecture/graph?view=systemContext`)).status, 200);
      assert.equal((await fetch(`${baseUrl}/assets/architecture/architecture.js`)).status, 200);
      assert.equal((await fetch(`${baseUrl}/assets/architecture/architecture.css`)).status, 200);
      const credential = await fetch(`${baseUrl}/api/architecture/agent/credential`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ apiKey: 'architecture-test-key' }) });
      assert.equal(credential.status, 200);
      assert.match(await readFile(path.join(root, '.resonance/architecture-agent.env'), 'utf8'), /OPENROUTER_API_KEY=architecture-test-key/);
      assert.equal((await fetch(`${baseUrl}/api/architecture/evidence?path=src%2Fhost.ts`)).status, 200);
      assert.equal((await fetch(`${baseUrl}/api/architecture/edit`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ kind: 'rules', value: {}, revision: model.revision }) })).status, 409);
    }, { root, appRoot, config });
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('renders LikeC4 through the React diagram package', async () => {
  const root = await fixture();
  try {
    const architectureStore = createArchitectureStore({ context: context(root) });
    const likec4 = await architectureStore.likec4();
    const graph = await architectureStore.graph('systemContext');
    const { window, document } = parseHTML('<!doctype html><head></head><body></body>'); globalThis.window = window; globalThis.document = document; globalThis.MutationObserver = window.MutationObserver;
    const mount = document.createElement('section'); document.body.append(mount);
    const browser = createBrowser({ fetchFn: async (url) => { if (url.endsWith('/model')) return { ok: true, async json() { return { model: { entities: [] }, likec4: likec4.dump, likec4Views: likec4.views }; } }; if (url.endsWith('/views')) return { ok: true, async json() { return { views: likec4.views }; } }; if (url.endsWith('/agent/state')) return { ok: true, async json() { return { messages: [], status: 'idle', error: null }; } }; return { ok: true, async json() { return graph; } }; }, eventSourceFactory: () => null });
    browser.mount(mount); await browser.activate(); await new Promise((resolve) => setTimeout(resolve, 20)); assert.equal(mount.querySelector('.architecture-graph').children.length > 0, true); assert.doesNotMatch(mount.textContent, /LikeC4Model not found/); assert.equal(mount.querySelector('[data-view-id="index"]'), null); assert.equal(mount.querySelector('nav > .architecture-nav-view')?.dataset.viewId, 'validation'); assert.equal(mount.querySelectorAll('nav > .architecture-nav-view')[1]?.dataset.viewId, 'systemContext'); assert.deepEqual([...mount.querySelectorAll('.architecture-nav-group-toggle span:first-child')].map((label) => label.textContent), ['Containers', 'Components', 'Code']); const containers = mount.querySelector('[data-nav-group="containers"]'); assert.ok(containers?.querySelector('[data-view-id="runtimeContainers"]')); assert.ok(containers?.querySelector('[data-view-id="packageContainers"]')); const containerToggle = containers?.querySelector('.architecture-nav-group-toggle') as HTMLButtonElement; assert.equal((containers?.querySelector('.architecture-nav-group-items') as HTMLElement).hidden, false); containerToggle.click(); let collapsedContainers = mount.querySelector('[data-nav-group="containers"]')!; assert.equal((collapsedContainers.querySelector('.architecture-nav-group-items') as HTMLElement).hidden, true); assert.equal(collapsedContainers.querySelector('.architecture-nav-group-toggle')?.getAttribute('aria-expanded'), 'false'); (collapsedContainers.querySelector('.architecture-nav-group-toggle') as HTMLButtonElement).click(); collapsedContainers = mount.querySelector('[data-nav-group="containers"]')!; assert.equal((collapsedContainers.querySelector('.architecture-nav-group-items') as HTMLElement).hidden, false); assert.equal(collapsedContainers.querySelector('.architecture-nav-group-toggle')?.getAttribute('aria-expanded'), 'true'); browser.deactivate(); await new Promise((resolve) => setTimeout(resolve, 100)); delete globalThis.window; delete globalThis.document; delete globalThis.MutationObserver;
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('refreshes the architecture diagram when the agent finishes a turn', async () => {
  const { document } = parseHTML('<!doctype html><head></head><body></body>');
  globalThis.document = document;
  const mount = document.createElement('section'); document.body.append(mount);
  const views = [{ id: 'system-context', name: 'System context', query: {} }];
  let modelRequest = 0;
  let eventSource;
  const firstModel = { entities: [{ id: 'shell', type: 'package', name: 'Shell' }] };
  const updatedModel = { entities: [{ id: 'database', type: 'data-store', name: 'Database' }] };
  const browser = createBrowser({ fetchFn: async (url) => {
    if (url.endsWith('/model')) return { ok: true, async json() { return { model: modelRequest++ === 0 ? firstModel : updatedModel, revision: 'r' }; } };
    if (url.endsWith('/views')) return { ok: true, async json() { return { views, revision: 'r' }; } };
    if (url.endsWith('/agent/state')) return { ok: true, async json() { return { messages: [], status: 'idle', error: null }; } };
    return { ok: true, async json() { return { view: views[0], nodes: modelRequest > 1 ? updatedModel.entities : firstModel.entities, edges: [], revision: 'r' }; } };
  }, eventSourceFactory: () => (eventSource = { onmessage: null, close() {} }) });
  browser.mount(mount); await browser.activate(); assert.match(mount.textContent, /Shell/); assert.equal(mount.querySelector('.architecture-context-usage').textContent, '');
  eventSource.onmessage({ data: JSON.stringify({ type: 'context', context: { inputTokens: 42876, maxInputTokens: 1048576 } }) });
  assert.equal(mount.querySelector('.architecture-context-usage').textContent, '42k / 1M');
  eventSource.onmessage({ data: JSON.stringify({ type: 'done' }) });
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.match(mount.textContent, /Database/);
  browser.deactivate(); delete globalThis.document;
});

test('keeps the Architecture agent available when LikeC4 model loading fails', async () => {
  const { document } = parseHTML('<!doctype html><head></head><body></body>');
  globalThis.document = document;
  const mount = document.createElement('section'); document.body.append(mount);
  const views = [{ id: 'system-context', name: 'System context', query: {} }];
  const browser = createBrowser({ fetchFn: async (url) => {
    if (url.endsWith('/model')) return { ok: false, async json() { return { error: "LikeC4 source is invalid: Could not resolve reference to Referenceable named 'telemetry'." }; } };
    if (url.endsWith('/views')) return { ok: true, async json() { return { views }; } };
    if (url.endsWith('/agent/state')) return { ok: true, async json() { return { messages: [], status: 'idle', error: null }; } };
    return { ok: false, async json() { return { error: 'LikeC4 source is invalid.' }; } };
  }, eventSourceFactory: () => null });
  browser.mount(mount); await browser.activate(); assert.equal(mount.hidden, false); assert.ok(mount.querySelector('.architecture-agent')); assert.match(mount.querySelector('.architecture-error').textContent, /telemetry/); browser.deactivate(); assert.equal(mount.hidden, true); delete globalThis.document;
});

test('renders a selectable graph workspace without raw artifact HTML', async () => {
  const { window, document } = parseHTML('<!doctype html><head></head><body></body>');
  globalThis.document = document;
  const mount = document.createElement('section'); document.body.append(mount);
  const model = { entities: [{ id: 'shell', type: 'package', name: 'Shell', description: 'Browser frame', evidence: [{ path: 'src/host.ts' }] }] };
  const views = [{ id: 'system-context', name: 'System context', query: {} }];
  const browser = createBrowser({ fetchFn: async (url) => { if (url.endsWith('/model')) return { ok: true, async json() { return { model, revision: 'r' }; } }; if (url.endsWith('/views')) return { ok: true, async json() { return { views, revision: 'r' }; } }; if (url.endsWith('/agent/state')) return { ok: true, async json() { return { messages: [], status: 'idle', error: null }; } }; if (url.endsWith('/validation')) return { ok: true, async json() { return { results: [{ status: 'pass', name: 'Configuration', message: 'Configuration is valid.' }] }; } }; return { ok: true, async json() { return { view: views[0], nodes: model.entities, edges: [], revision: 'r' }; } }; }, eventSourceFactory: () => null });
  browser.mount(mount); await browser.activate(); assert.equal(mount.hidden, false); assert.equal(mount.querySelectorAll('.architecture-node').length, 1); assert.match(mount.textContent, /Shell/); assert.equal(mount.querySelector('select'), null); assert.equal(mount.querySelector('input[type="search"]'), null); assert.equal(mount.querySelector('.architecture-agent') !== null, true); assert.ok(mount.querySelector('.architecture-agent-toggle svg')); assert.equal(mount.querySelector('.architecture-context'), null); assert.ok(mount.querySelector('[data-view-id="validation"]')); mount.querySelector('.architecture-node').dispatchEvent(new window.Event('click', { bubbles: true })); mount.querySelector('[data-view-id="validation"]').click(); await new Promise((resolve) => setTimeout(resolve, 0)); assert.match(mount.querySelector('.architecture-validation-view').textContent, /Run validation/); mount.querySelector('.architecture-validation-button').click(); await new Promise((resolve) => setTimeout(resolve, 0)); assert.match(mount.querySelector('.architecture-validation').textContent, /Configuration is valid/); mount.querySelector('.architecture-agent-toggle').click(); assert.equal(mount.querySelector('.architecture-agent').hidden, true); assert.equal(mount.querySelector('.architecture-workspace').classList.contains('architecture-agent-hidden'), true); const prompt = mount.querySelector('.architecture-composer textarea') as HTMLTextAreaElement; prompt.value = 'Keep this draft while I browse'; prompt.dispatchEvent(new window.Event('input')); browser.deactivate(); await browser.activate(); assert.equal((mount.querySelector('.architecture-composer textarea') as HTMLTextAreaElement).value, 'Keep this draft while I browse'); browser.deactivate(); assert.equal(mount.hidden, true); delete globalThis.document;
});

test('groups consecutive agent responses under one label and highlights user prompts', async () => {
  const { document } = parseHTML('<!doctype html><head></head><body></body>');
  globalThis.document = document;
  const mount = document.createElement('section'); document.body.append(mount);
  const views = [{ id: 'system-context', name: 'System context', query: {} }];
  const messages = [
    { id: 'user-1', role: 'user', content: 'What is this?', createdAt: '2026-01-01T00:00:00.000Z' },
    { id: 'agent-1', role: 'assistant', content: 'The first response.', createdAt: '2026-01-01T00:00:01.000Z' },
    { id: 'agent-2', role: 'assistant', content: 'The follow-up response.', createdAt: '2026-01-01T00:00:02.000Z' },
  ];
  const browser = createBrowser({ fetchFn: async (url) => { if (url.endsWith('/model')) return { ok: true, async json() { return { model: { entities: [] }, revision: 'r' }; } }; if (url.endsWith('/views')) return { ok: true, async json() { return { views }; } }; if (url.endsWith('/agent/state')) return { ok: true, async json() { return { messages, status: 'idle', error: null }; } }; return { ok: true, async json() { return { view: views[0], nodes: [], edges: [], revision: 'r' }; } }; }, eventSourceFactory: () => null });
  browser.mount(mount); await browser.activate();
  assert.equal(mount.querySelectorAll('.architecture-message-user').length, 1);
  assert.equal(mount.querySelectorAll('.architecture-message-assistant').length, 1);
  assert.equal(mount.querySelectorAll('.architecture-message-assistant strong').length, 1);
  assert.equal(mount.querySelector('.architecture-message-assistant strong').textContent, 'Agent');
  assert.deepEqual([...mount.querySelectorAll('.architecture-message-content p')].map((paragraph) => paragraph.textContent), ['The first response.', 'The follow-up response.']);
  assert.equal(mount.querySelector('.architecture-message-user strong').textContent, 'You');
  browser.deactivate(); delete globalThis.document;
});