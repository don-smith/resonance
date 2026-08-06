import test from 'node:test';
import assert from 'node:assert/strict';
import { access, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseHTML } from 'linkedom';
import { createApp } from './server.ts';
import { createPackageScaffold, packageLabel, packageTemplates, planPackageScaffold, validatePackageId } from './package-scaffold.ts';

const appRoot = fileURLToPath(new URL('../', import.meta.url));

async function withServer(run, options) {
  const server = await createApp(options);
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const { port } = server.address();
  try { await run(`http://127.0.0.1:${port}`); }
  finally { await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve())); }
}

test('validates IDs and plans a complete deterministic starter', () => {
  assert.equal(validatePackageId('backlog-workspace'), 'backlog-workspace');
  assert.equal(packageLabel('backlog-workspace'), 'Backlog Workspace');
  for (const id of ['', 'Backlog', 'two--words', '../escape', 'one_word']) {
    assert.throws(() => validatePackageId(id), /lowercase kebab-case/);
  }
  const plan = planPackageScaffold({ appRoot, id: 'reading-queue' });
  assert.deepEqual(plan.files.map((file) => path.basename(file)).sort(), ['README.md', 'index.ts', 'reading-queue.css', 'reading-queue.js', 'reading-queue.test.ts']);
  assert.match(packageTemplates('reading-queue')['index.ts'], /PackageDefinition/);
  const generatedTest = packageTemplates('reading-queue')['reading-queue.test.ts'];
  assert.match(generatedTest, /import \{ mkdtemp, rm \} from 'node:fs\/promises';/);
  assert.match(generatedTest, /finally \{ await rm\(root, \{ recursive: true, force: true \}\); \}/);
});

test('refuses an existing package without invoking verification or changing its content', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-scaffold-'));
  const directory = path.join(root, 'src', 'packages', 'existing-package');
  await mkdir(directory, { recursive: true });
  const sentinel = path.join(directory, 'sentinel.txt');
  await writeFile(sentinel, 'keep');
  let verified = false;
  try {
    await assert.rejects(() => createPackageScaffold({ appRoot: root, id: 'existing-package', verify: async () => { verified = true; } }), { code: 'EEXIST' });
    assert.equal(await readFile(sentinel, 'utf8'), 'keep');
    assert.equal(verified, false);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('rolls back a reserved package when its focused verifier fails', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-scaffold-'));
  await mkdir(path.join(root, 'src', 'packages'), { recursive: true });
  const plan = planPackageScaffold({ appRoot: root, id: 'rollback-package' });
  try {
    await assert.rejects(() => createPackageScaffold({ appRoot: root, id: plan.id, verify: async () => { throw new Error('focused test failed'); } }), /focused test failed/);
    await assert.rejects(() => access(plan.directory), { code: 'ENOENT' });
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('composes a generated package through real server and browser contracts', async () => {
  const id = `scaffold-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  const repositoryRoot = await mkdtemp(path.join(tmpdir(), 'resonance-scaffold-repository-'));
  try {
    await createPackageScaffold({ appRoot, id, verify: async (plan) => {
      const config = { version: 1, packages: { shell: { module: 'src/packages/shell/index.ts' }, [id]: { module: `src/packages/${id}/index.ts` } } };
      await withServer(async (baseUrl) => {
        const manifest = await fetch(`${baseUrl}/api/manifest`).then((response) => response.json());
        assert.ok(manifest.packages.some((item) => item.id === id));
        assert.deepEqual(await fetch(`${baseUrl}/api/${id}`).then((response) => response.json()), { id, label: packageLabel(id) });
        assert.equal((await fetch(`${baseUrl}/assets/${id}/${id}.js`)).status, 200);
        assert.equal((await fetch(`${baseUrl}/assets/${id}/${id}.css`)).status, 200);
      }, { root: repositoryRoot, appRoot, config });
      await assert.rejects(() => access(path.join(repositoryRoot, '.resonance', 'config.json')), { code: 'ENOENT' });
      const { document } = parseHTML('<!doctype html><body></body>');
      const browser = (await import(`${pathToFileURL(path.join(plan.directory, `${id}.js`)).href}?${Date.now()}`)).default;
      const instance = browser({ fetchFn: async () => ({ ok: true, async json() { return { label: packageLabel(id) }; } }) });
      const mount = document.createElement('section');
      instance.mount(mount);
      await instance.activate();
      assert.equal(mount.querySelector('h1').textContent, packageLabel(id));
      instance.deactivate();
      assert.equal(mount.hidden, true);
    } });
  } finally {
    await rm(path.join(appRoot, 'src', 'packages', id), { recursive: true, force: true });
    await rm(repositoryRoot, { recursive: true, force: true });
  }
});
