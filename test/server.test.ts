import test from 'node:test';
import assert from 'node:assert/strict';
import { createApp, startServer } from '../src/server.ts';
import { createHost } from '../src/host.ts';
import { defaultRepositoryConfig, loadRepositoryConfig } from '../src/config.ts';

const fixtureRoot = new URL('./fixtures/repository/', import.meta.url);
const appRoot = new URL('../', import.meta.url);
const moduleConfig = defaultRepositoryConfig();
function configWith(overrides) { return { version: 1, packages: { ...moduleConfig.packages, ...overrides } }; }
function packageDefinition() {
  const metadata = { id: 'test', version: '1.0.0', hostVersion: '1', label: 'Test', order: 1 };
  return {
    metadata,
    register() {
      return {
        metadata,
        routes: [{ method: 'GET', path: '/api/test/value', handler: async (_request, response, context) => context.sendJson(response, 200, { ok: true }) }, { method: 'GET', path: '/api/test/failure', handler: async () => { throw new Error('boom'); } }],
        assets: [{ path: '/assets/test/app.js', file: 'src/packages/shell/app.js', contentType: 'text/javascript; charset=utf-8' }, { path: '/assets/test/styles.css', file: 'src/packages/shell/styles.css', contentType: 'text/css; charset=utf-8' }],
        navigation: [{ id: 'test', label: 'Test', order: 1 }],
        browser: { id: 'test', entry: '/assets/test/app.js', stylesheet: '/assets/test/styles.css' },
      };
    },
  };
}
async function withServer(run, options = {}) {
  const server = await createApp({ root: fixtureRoot, ...options });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const { port } = server.address();
  try { await run(`http://127.0.0.1:${port}`); }
  finally { await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve())); }
}

test('serves canonical Docs routes and rejects removed aliases', async () => {
  const config = await loadRepositoryConfig(fixtureRoot);
  await withServer(async (baseUrl) => {
    const tree = await fetch(`${baseUrl}/api/docs/tree`);
    assert.equal(tree.status, 200);
    assert.deepEqual((await tree.json()).documents, ['README.md', 'docs/architecture.md', 'docs/guides/getting-started.md', 'home.md']);
    const documentResponse = await fetch(`${baseUrl}/api/docs/document?path=docs%2Farchitecture.md`);
    assert.equal(documentResponse.status, 200);
    assert.equal((await documentResponse.json()).path, 'docs/architecture.md');
    assert.equal((await fetch(`${baseUrl}/api/tree`)).status, 404);
    assert.equal((await fetch(`${baseUrl}/api/document?path=README.md`)).status, 404);
    assert.equal((await fetch(`${baseUrl}/api/docs/document?path=.git%2Fignored.md`)).status, 404);
  }, { config });
});

test('serves configured repository Home content and preserves HTML', async () => {
  await withServer(async (baseUrl) => { const body = await fetch(`${baseUrl}/api/home`).then((response) => { assert.equal(response.status, 200); return response.json(); }); assert.equal(body.path, 'home.md'); assert.match(body.html, /Fixture Home/); }, { config: await loadRepositoryConfig(fixtureRoot) });
  await withServer(async (baseUrl) => { const body = await fetch(`${baseUrl}/api/home`).then((response) => { assert.equal(response.status, 200); return response.json(); }); assert.equal(body.path, 'home.html'); assert.match(body.html, /Repository-owned markup/); }, { config: configWith({ home: { module: 'src/packages/home/index.ts', source: 'home.html' } }) });
});

test('serves package-local assets through preserved public URLs', async () => {
  await withServer(async (baseUrl) => {
    const home = await fetch(`${baseUrl}/assets/home/home.js`); const docs = await fetch(`${baseUrl}/assets/docs/docs.css`); const shell = await fetch(`${baseUrl}/assets/app.js`);
    assert.equal(home.status, 200); assert.match(await home.text(), /export default/);
    assert.equal(docs.status, 200); assert.match(await docs.text(), /docs-layout/);
    assert.equal(shell.status, 200); assert.match(await shell.text(), /import\(packageInfo\.entry\)/);
    assert.equal((await fetch(`${baseUrl}/not-registered.js`)).status, 404);
  }, { config: moduleConfig });
});

test('serves injected routes/assets and generic failures', async () => {
  const registry = createHost({ root: fixtureRoot, packages: [packageDefinition()] });
  await withServer(async (baseUrl) => {
    assert.deepEqual(await fetch(`${baseUrl}/api/manifest`).then((response) => response.json()), registry.manifest);
    assert.deepEqual(await fetch(`${baseUrl}/api/test/value`).then((response) => response.json()), { ok: true });
    assert.equal((await fetch(`${baseUrl}/assets/test/app.js`)).status, 200);
    assert.equal((await fetch(`${baseUrl}/api/test/failure`)).status, 500);
    const post = await fetch(`${baseUrl}/`, { method: 'POST' });
    assert.equal(post.status, 405); assert.equal(post.headers.get('allow'), 'GET');
  }, { registry });
});

test('starts on the next port when the requested port is occupied', async () => {
  const occupiedServer = await createApp({ root: fixtureRoot });
  await new Promise((resolve) => occupiedServer.listen(0, '127.0.0.1', resolve));
  const occupiedPort = occupiedServer.address().port;
  const fallbackServer = await startServer({ root: fixtureRoot, port: occupiedPort, maxPortAttempts: 3 });
  try { assert.equal(fallbackServer.address().port, occupiedPort + 1); }
  finally { await new Promise((resolve, reject) => fallbackServer.close((error) => error ? reject(error) : resolve())); await new Promise((resolve, reject) => occupiedServer.close((error) => error ? reject(error) : resolve())); }
});
