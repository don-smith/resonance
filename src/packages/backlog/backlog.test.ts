import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readFile, rm, symlink, unlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseHTML } from 'linkedom';
import { createApp } from '../../server.ts';
import { backlogInput, parseBacklogItems } from './index.ts';
import createBacklog from './backlog.js';

const appRoot = fileURLToPath(new URL('../../../', import.meta.url));
const config = { version: 1 as const, packages: { shell: { module: 'src/packages/shell/index.ts' }, backlog: { module: 'src/packages/backlog/index.ts', provider: 'openrouter', model: 'deepseek/deepseek-v4-flash' } } };
const item = '  - title: Queue\n    plan: plans/queue.md\n    status: in-planning\n    priority: P2\n';
const valid = `version: 1\ndecisions:\n${item}`;

async function withServer(run: (base: string) => Promise<void>, root: string) {
  const server = await createApp({ root, appRoot, config });
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  try { await run(`http://127.0.0.1:${server.address().port}`); }
  finally { await new Promise<void>((resolve, reject) => server.close((error) => error ? reject(error) : resolve())); }
}

test('validates a closed versioned decisions document', () => {
  assert.deepEqual(parseBacklogItems(valid), [{ title: 'Queue', plan: 'plans/queue.md', status: 'in-planning', priority: 'P2' }]);
  for (const source of [
    'version: 1\ndecisions: [',
    `${valid}unknown: true\n`,
    valid.replace('in-planning', 'blocked'),
    valid.replace('P2', 'P4'),
    valid.replace('title: Queue', 'title: ""'),
    valid.replace('plans/queue.md', '\\plans\\queue.md'),
    `${valid}decisions: []\n`,
  ]) assert.throws(() => parseBacklogItems(source), /Backlog source is invalid/);
  assert.deepEqual(backlogInput({ provider: 'openrouter', model: 'deepseek/deepseek-v4-flash' }), { provider: 'openrouter', model: 'deepseek/deepseek-v4-flash' });
  assert.deepEqual(backlogInput({ provider: 'openai', model: 'gpt-4.1' }), { provider: 'openai', model: 'gpt-4.1' });
  assert.throws(() => backlogInput({ provider: 'unsupported', model: 'model' }), /input is invalid/);
  assert.throws(() => backlogInput({ provider: 'openrouter', model: '' }), /input is invalid/);
  assert.throws(() => backlogInput({ source: 'elsewhere' }), /input is invalid/);
});

test('serves sorted contained decisions through the renamed package', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-backlog-'));
  const outside = await mkdtemp(path.join(tmpdir(), 'resonance-backlog-outside-'));
  try {
    await mkdir(path.join(root, 'backlog', 'plans'), { recursive: true });
    await writeFile(path.join(root, 'backlog', 'todo.yaml'), `version: 1\ndecisions:\n  - title: Planning\n    plan: plans/planning.md\n    status: in-planning\n    priority: P3\n  - title: Ready\n    plan: plans/ready.md\n    status: is-ready\n    priority: P0\n  - title: Progress\n    plan: plans/progress.md\n    status: in-progress\n    priority: P1\n  - title: Done\n    plan: plans/done.md\n    status: recently-done\n    priority: P2\n  - title: Escape\n    plan: ../../outside.md\n    status: in-progress\n    priority: P0\n  - title: External\n    plan: plans/external.md\n    status: in-progress\n    priority: P0\n`);
    for (const name of ['planning', 'ready', 'progress', 'done']) await writeFile(path.join(root, 'backlog/plans', `${name}.md`), `# ${name}`);
    await writeFile(path.join(outside, 'external.md'), '# outside');
    await symlink(path.join(outside, 'external.md'), path.join(root, 'backlog/plans/external.md'));
    await withServer(async (base) => {
      const manifest = await fetch(`${base}/api/manifest`).then((response) => response.json());
      assert.deepEqual(manifest.packages.find((value: { id: string }) => value.id === 'backlog'), { id: 'backlog', entry: '/assets/backlog/backlog.js', stylesheet: '/assets/backlog/backlog.css' });
      assert.equal((await fetch(`${base}/api/backlog-workspace/items`)).status, 404);
      assert.equal((await fetch(`${base}/assets/backlog/backlog.js`)).status, 200);
      const result = await fetch(`${base}/api/backlog/items`).then((response) => response.json());
      assert.deepEqual(result.items.map((value: { title: string }) => value.title), ['Done', 'Progress', 'Ready', 'Planning']);
      assert.match((await fetch(`${base}/api/backlog/plan?path=backlog%2Fplans%2Fprogress.md`).then((response) => response.json())).html, /progress/);
      const update = await fetch(`${base}/api/backlog/metadata`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ path: 'backlog/plans/progress.md', status: 'is-ready', priority: 'P0' }) });
      assert.equal(update.status, 200); assert.deepEqual(await update.json(), { affectedPaths: ['backlog/todo.yaml'] });
      const invalidUpdate = await fetch(`${base}/api/backlog/metadata`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ path: 'backlog/plans/progress.md', status: 'blocked' }) });
      assert.equal(invalidUpdate.status, 400);
      const updatedItems = await fetch(`${base}/api/backlog/items`).then((response) => response.json());
      assert.deepEqual(updatedItems.items.find((value: { path: string }) => value.path === 'backlog/plans/progress.md'), { path: 'backlog/plans/progress.md', title: 'Progress', status: 'is-ready', priority: 'P0' });
      assert.equal((await fetch(`${base}/api/backlog/plan?path=README.md`)).status, 404);
      assert.equal((await fetch(`${base}/api/backlog/items`, { method: 'POST' })).status, 405);
    }, root);
  } finally {
    await rm(root, { recursive: true, force: true });
    await rm(outside, { recursive: true, force: true });
  }
});

test('distinguishes invalid and escaping sources', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-backlog-'));
  const outside = await mkdtemp(path.join(tmpdir(), 'resonance-backlog-outside-'));
  try {
    await mkdir(path.join(root, 'backlog'), { recursive: true });
    await writeFile(path.join(root, 'backlog/todo.yaml'), 'version: 1\ndecisions: [');
    await withServer(async (base) => {
      assert.equal((await fetch(`${base}/api/backlog/items`)).status, 422);
      assert.equal((await fetch(`${base}/api/backlog/plan?path=backlog%2Fplans%2Fqueue.md`)).status, 422);
    }, root);
    await unlink(path.join(root, 'backlog/todo.yaml'));
    await writeFile(path.join(outside, 'todo.yaml'), valid);
    await symlink(path.join(outside, 'todo.yaml'), path.join(root, 'backlog/todo.yaml'));
    await withServer(async (base) => assert.equal((await fetch(`${base}/api/backlog/items`)).status, 404), root);
  } finally {
    await rm(root, { recursive: true, force: true });
    await rm(outside, { recursive: true, force: true });
  }
});

test('uses the checked-in backlog package config', async () => {
  const repositoryConfig = JSON.parse(await readFile(new URL('../../../.resonance/config.json', import.meta.url), 'utf8'));
  assert.deepEqual(repositoryConfig.packages.backlog, { module: 'src/packages/backlog/index.ts', provider: 'openrouter', model: 'deepseek/deepseek-v4-flash' });
  assert.equal(repositoryConfig.packages['backlog-workspace'], undefined);
});

test('exposes lazy agent state, bounded agent routes, and non-secret credential responses', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-backlog-agent-'));
  try {
    await mkdir(path.join(root, 'backlog', 'plans'), { recursive: true });
    await mkdir(path.join(root, '.resonance'), { recursive: true });
    await writeFile(path.join(root, '.resonance', 'backlog-agent.env'), 'OPENAI_API_KEY=old-provider-key\n', { mode: 0o600 });
    await writeFile(path.join(root, 'backlog', 'todo.yaml'), valid);
    await writeFile(path.join(root, 'backlog', 'plans', 'queue.md'), '# Queue');
    await withServer(async (base) => {
      const state = await fetch(`${base}/api/backlog/agent/state`).then((response) => response.json());
      assert.deepEqual(state, { messages: [], status: 'idle', hasSession: false, error: null, pendingDeletion: null });
      const missing = await fetch(`${base}/api/backlog/agent/prompt`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ prompt: 'Review', selectedPath: 'backlog/plans/queue.md' }) });
      assert.equal(missing.status, 202); assert.deepEqual(await missing.json(), { accepted: false, credentialRequired: true });
      const key = await fetch(`${base}/api/backlog/agent/credential`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ apiKey: 'sk-local-secret' }) });
      assert.equal(key.status, 200); const keyBody = await key.json(); assert.deepEqual(keyBody, { ok: true }); assert.doesNotMatch(JSON.stringify(keyBody), /sk-local-secret/);
      assert.match(await readFile(path.join(root, '.resonance', 'backlog-agent.env'), 'utf8'), /OPENROUTER_API_KEY=sk-local-secret/);
      assert.equal((await fetch(`${base}/api/backlog/agent/prompt`, { method: 'POST', headers: { 'content-type': 'text/plain' }, body: '{}' })).status, 415);
      assert.equal((await fetch(`${base}/api/backlog/agent/credential`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ apiKey: 'x'.repeat(9000) }) })).status, 413);
    }, root);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('keeps plan and agent header separators aligned', async () => {
  const css = await readFile(new URL('./backlog.css', import.meta.url), 'utf8');
  const sharedCss = await readFile(new URL('../../ui/ui.css', import.meta.url), 'utf8');
  assert.match(css, /\.backlog-plan header \{[^}]*padding: 28px 52px 20px;/s);
  assert.match(sharedCss, /\.resonance-agent-header \{[^}]*padding: 28px 20px 20px;/s);
  assert.match(css, /\.backlog-agent-toggle \{[^}]*border: 1px solid transparent;/s);
  assert.match(css, /\.backlog-agent-toggle:hover \{[^}]*border-color: var\(--accent\);/s);
  assert.match(css, /\.backlog-agent-toggle svg \{[^}]*stroke: currentColor;/s);
  assert.match(css, /\.backlog-agent\[hidden\] \{[^}]*display: none !important;/s);
  assert.match(css, /\.backlog-content \{[^}]*padding: 28px 52px 72px;/s);
  assert.match(css, /\.backlog-metadata select \{[^}]*border: 0;[^}]*background: transparent;/s);
  assert.match(css, /\.backlog-list \{[^}]*display: flex;[^}]*flex-direction: column;[^}]*padding: 32px 6px 32px 20px;/s);
  assert.match(css, /\.backlog-items \{[^}]*min-height: 0;[^}]*flex: 1;[^}]*overflow-y: auto;[^}]*scrollbar-width: thin;/s);
  assert.match(css, /\.backlog-group-toggle \{[^}]*width: calc\(100% - 18px\);[^}]*border-top: 1px solid var\(--line\);/s);
  assert.match(css, /\.backlog-group-items\[hidden\] \{[^}]*display: none;/s);
  assert.match(css, /\.backlog-item\.active \{[^}]*margin-right: 6px;[^}]*border-left-color: var\(--accent\);/s);
});

test('renders ordered Decisions and selects the first non-done plan', async () => {
  const { document } = parseHTML('<!doctype html><body></body>');
  const root = document.createElement('section');
  const items = ['recently-done', 'in-progress', 'is-ready', 'in-planning'].map((status, index) => ({ title: status, path: `backlog/plans/${status}.md`, status, priority: `P${index}` }));
  const instance = createBacklog({ fetchFn: async (url) => url === '/api/backlog/items'
    ? { ok: true, async json() { return { items }; } }
    : { ok: true, async json() { return { path: items[1].path, title: items[1].title, html: '<h1>Progress</h1>' }; } },
  });
  instance.mount(root);
  await instance.activate();
  assert.equal(root.querySelector('.backlog-list .eyebrow').textContent, 'WORKSPACE');
  assert.equal(root.querySelector('.backlog-list h2').textContent, 'Backlog');
  assert.deepEqual([...root.querySelectorAll('.backlog-group-toggle span:first-child')].map((heading) => heading.textContent), ['recently-done', 'in-progress', 'is-ready', 'in-planning']);
  const group = root.querySelector('[data-backlog-group="recently-done"]');
  assert.equal(group.querySelector('.backlog-group-items').hidden, false);
  group.querySelector('.backlog-group-toggle').click();
  let collapsedGroup = root.querySelector('[data-backlog-group="recently-done"]');
  assert.equal(collapsedGroup.querySelector('.backlog-group-items').hidden, true);
  assert.equal(collapsedGroup.querySelector('.backlog-group-toggle').getAttribute('aria-expanded'), 'false');
  collapsedGroup.querySelector('.backlog-group-toggle').click();
  collapsedGroup = root.querySelector('[data-backlog-group="recently-done"]');
  assert.equal(collapsedGroup.querySelector('.backlog-group-items').hidden, false);
  assert.equal(collapsedGroup.querySelector('.backlog-group-toggle').getAttribute('aria-expanded'), 'true');
  assert.match(root.querySelector('.backlog-content').textContent, /Progress/);
  assert.equal(root.querySelector('.backlog-metadata-priority').value, 'P1');
  assert.equal(root.querySelector('.backlog-metadata-status').value, 'in-progress');
  instance.deactivate();
  assert.equal(root.hidden, true);
});

test('renders editable priority and status controls and refreshes the decision list', async () => {
  const { document } = parseHTML('<!doctype html><body></body>');
  const root = document.createElement('section');
  const items: any[] = [{ title: 'Queue', path: 'backlog/plans/queue.md', status: 'in-planning', priority: 'P2' }];
  const updates: any[] = [];
  const instance = createBacklog({ fetchFn: async (url, options) => {
    if (url === '/api/backlog/items') return { ok: true, async json() { return { items: items.map((item) => ({ ...item })) }; } };
    if (url.startsWith('/api/backlog/plan')) return { ok: true, async json() { return { ...items[0], html: '<h1>Queue</h1>' }; } };
    if (url === '/api/backlog/metadata') { const body = JSON.parse(options.body); updates.push(body); Object.assign(items[0], body); return { ok: true, async json() { return { affectedPaths: ['backlog/todo.yaml'] }; } }; }
    throw new Error(`Unexpected request: ${url}`);
  } });
  instance.mount(root); await instance.activate();
  assert.equal(root.querySelector('.backlog-metadata-priority').value, 'P2');
  assert.equal(root.querySelector('.backlog-metadata-status').value, 'in-planning');
  const priority = root.querySelector('.backlog-metadata-priority') as HTMLSelectElement;
  (priority.querySelector('option[value="P0"]') as HTMLOptionElement).selected = true; priority.dispatchEvent(new document.defaultView.Event('change', { bubbles: true }));
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(updates, [{ path: 'backlog/plans/queue.md', priority: 'P0' }]);
  assert.equal(root.querySelector('.backlog-metadata-priority').value, 'P0');
  assert.equal(root.querySelector('.backlog-item .backlog-priority').textContent, 'P0');
  instance.deactivate();
});

test('retains user and agent messages across sequential transcript events', async () => {
  const { document } = parseHTML('<!doctype html><body></body>');
  const root = document.createElement('section'); let stream: any;
  const items = [{ title: 'Queue', path: 'backlog/plans/queue.md', status: 'in-planning', priority: 'P2' }];
  const instance = createBacklog({
    eventSourceFactory: () => { stream = { close() {} }; return stream; },
    fetchFn: async (url) => url === '/api/backlog/items'
      ? { ok: true, async json() { return { items }; } }
      : url === '/api/backlog/agent/reset'
        ? { ok: true, async json() { return { state: { messages: [], status: 'idle', error: null, pendingDeletion: null } }; } }
        : { ok: true, async json() { return { ...items[0], html: '<h1>Queue</h1>' }; } },
  });
  instance.mount(root); await instance.activate();
  const emit = (value: unknown) => stream.onmessage({ data: JSON.stringify(value) });
  const messages = [
    { id: 'user-1', role: 'user', content: 'First' },
    { id: 'agent-1', role: 'assistant', content: 'First response' },
  ];
  emit({ type: 'snapshot', snapshot: { messages: [], status: 'idle', error: null, pendingDeletion: null } });
  emit({ type: 'message', message: messages[0] });
  emit({ type: 'message', message: messages[1] });
  emit({ type: 'snapshot', snapshot: { messages: [messages[0]], status: 'idle', error: null, pendingDeletion: null } });
  emit({ type: 'message', message: { id: 'user-2', role: 'user', content: 'Second' } });
  emit({ type: 'message', message: { id: 'agent-2', role: 'assistant', content: 'Second response' } });
  assert.deepEqual([...root.querySelectorAll('.backlog-message strong')].map((label) => label.textContent), ['You', 'Agent', 'You', 'Agent']);
  assert.match(root.querySelector('.backlog-transcript').textContent, /First response/);
  assert.match(root.querySelector('.backlog-transcript').textContent, /Second response/);
  const agentToggle = root.querySelector('.backlog-agent-toggle');
  const agentPanel = root.querySelector('.backlog-agent');
  agentToggle.click();
  assert.equal(agentPanel.hidden, true);
  assert.equal(agentToggle.getAttribute('aria-expanded'), 'false');
  assert.equal(root.querySelector('.backlog-workspace').classList.contains('backlog-agent-hidden'), true);
  assert.match(root.querySelector('.backlog-transcript').textContent, /First response/);
  agentToggle.click();
  assert.equal(agentPanel.hidden, false);
  assert.equal(agentToggle.getAttribute('aria-expanded'), 'true');
  assert.match(root.querySelector('.backlog-transcript').textContent, /Second response/);
  root.querySelector('.backlog-reset').click(); await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(root.querySelectorAll('.backlog-message').length, 0);
  instance.deactivate();
});

test('opens the agent stream only while active and submits the selected canonical path', async () => {
  const { document } = parseHTML('<!doctype html><body></body>');
  const root = document.createElement('section'); const streams: any[] = []; const requests: any[] = [];
  const items = [{ title: 'Queue', path: 'backlog/plans/queue.md', status: 'in-planning', priority: 'P2' }];
  const instance = createBacklog({
    eventSourceFactory: (url) => { const stream: any = { url, close() { stream.closed = true; } }; streams.push(stream); return stream; },
    fetchFn: async (url, options) => { requests.push({ url, options }); if (url === '/api/backlog/items') return { ok: true, async json() { return { items }; } }; if (url.startsWith('/api/backlog/plan')) return { ok: true, async json() { return { ...items[0], html: '<h1>Queue</h1>' }; } }; return { ok: true, async json() { return { accepted: true }; } }; },
  });
  instance.mount(root);
  assert.equal(root.querySelector('.backlog-agent-header .eyebrow').textContent, 'AGENT / CHAT');
  assert.equal(root.querySelector('.backlog-agent-header .backlog-status').textContent, 'Ready');
  assert.equal(root.querySelector('.backlog-agent-header .backlog-reset'), null);
  assert.equal(root.querySelector('.backlog-agent-status'), null);
  assert.ok(root.querySelector('.backlog-agent-toggle svg'));
  assert.deepEqual([...root.querySelectorAll('.backlog-composer-actions button')].map((button) => button.textContent), ['Send', 'New Chat']);
  await instance.activate(); assert.equal(streams.length, 1); assert.equal(streams[0].url, '/api/backlog/agent/events');
  streams[0].onmessage({ data: JSON.stringify({ type: 'snapshot', snapshot: { messages: [], status: 'idle', error: null, pendingDeletion: null } }) });
  const input: any = root.querySelector('.backlog-composer textarea'); input.value = 'Review this'; input.dispatchEvent(new document.defaultView.Event('input')); await new Promise((resolve) => setTimeout(resolve, 0)); root.querySelector('.backlog-composer').dispatchEvent(new document.defaultView.Event('submit', { bubbles: true, cancelable: true })); await new Promise((resolve) => setTimeout(resolve, 0));
  const prompt = requests.find((request) => request.url === '/api/backlog/agent/prompt'); assert.deepEqual(JSON.parse(prompt.options.body), { prompt: 'Review this', selectedPath: items[0].path });
  instance.deactivate(); assert.equal(streams[0].closed, true);
});
