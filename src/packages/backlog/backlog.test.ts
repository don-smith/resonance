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
const config = { version: 1 as const, packages: { shell: { module: 'src/packages/shell/index.ts' }, backlog: { module: 'src/packages/backlog/index.ts' } } };
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
  assert.throws(() => backlogInput({ source: 'elsewhere' }), /does not accept/);
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

test('uses the checked-in migration and explicit config', async () => {
  const repositoryConfig = JSON.parse(await readFile(new URL('../../../.resonance/config.json', import.meta.url), 'utf8'));
  assert.deepEqual(repositoryConfig.packages.backlog, { module: 'src/packages/backlog/index.ts' });
  assert.equal(repositoryConfig.packages['backlog-workspace'], undefined);
  assert.equal(parseBacklogItems(await readFile(new URL('../../../backlog/todo.yaml', import.meta.url), 'utf8')).length, 7);
  await withServer(async (base) => assert.equal((await fetch(`${base}/api/backlog/items`).then((response) => response.json())).items.length, 7), appRoot);
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
  assert.equal(root.querySelector('.backlog-list h2').textContent, 'Decisions');
  assert.deepEqual([...root.querySelectorAll('.backlog-group h3')].map((heading) => heading.textContent), ['recently-done', 'in-progress', 'is-ready', 'in-planning']);
  assert.match(root.querySelector('.backlog-content').textContent, /Progress/);
  instance.deactivate();
  assert.equal(root.hidden, true);
});
