import test from 'node:test';
import assert from 'node:assert/strict';
import { access, mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { createApp, startServer } from './server.ts';
import { createHost } from './host.ts';
import { createRepositoryConfig, loadRepositoryConfig, writeRepositoryConfig } from './config.ts';
import { createTelemetry } from './telemetry.ts';


const fixtureRoot = new URL('../test/fixtures/repository/', import.meta.url);
const appRoot = new URL('../', import.meta.url);
const moduleConfig = createRepositoryConfig({ home: true, docs: true });
function configWith(overrides) { return { version: 1, packages: { ...moduleConfig.packages, ...overrides } }; }
function packageDefinition() {
  const metadata = { id: 'test', version: '1.0.0', hostVersion: '1', label: 'Test', order: 1 };
  return {
    metadata,
    register() {
      return {
        metadata,
        routes: [{ method: 'GET', path: '/api/test/value', handler: async (_request, response) => response.json(200, { ok: true }) }, { method: 'GET', path: '/api/test/failure', handler: async () => { throw new Error('boom'); } }],
        assets: [{ path: '/assets/test/app.js', file: 'src/packages/shell/app.js', contentType: 'text/javascript; charset=utf-8' }, { path: '/assets/test/styles.css', file: 'src/packages/shell/styles.css', contentType: 'text/css; charset=utf-8' }],
        navigation: [{ id: 'test', label: 'Test', order: 1 }],
        browser: { id: 'test', entry: '/assets/test/app.js', stylesheet: '/assets/test/styles.css' },
      };
    },
  };
}
function transportPackage(log) {
  const metadata = { id: 'transport', version: '1.0.0', hostVersion: '1', label: 'Transport', order: 1 };
  return { metadata, register() { return { metadata, routes: [
    { method: 'GET', path: '/api/transport/value', handler: async (_request, response) => response.json(200, { method: 'GET' }) },
    { method: 'POST', path: '/api/transport/value', handler: async (_request, response) => response.json(202, { method: 'POST' }) },
    { method: 'POST', path: '/api/transport/post-only', handler: async (_request, response) => response.json(202, { method: 'POST' }) },
    { method: 'POST', path: '/api/transport/body', handler: async (request, response) => { try { response.json(200, await request.readJson(8)); } catch (error) { const status = typeof error === 'object' && error && 'status' in error && typeof error.status === 'number' ? error.status : 500; response.json(status, { error: error instanceof Error ? error.message : String(error) }); } } },
    { method: 'GET', path: '/api/transport/stream', handler: async (_request, response) => { const stream = response.sse(); stream.write({ type: 'hello' }); throw new Error('after stream'); } },
  ], assets: [{ path: '/assets/transport/app.js', file: 'src/packages/shell/app.js', contentType: 'text/javascript' }, { path: '/assets/transport/styles.css', file: 'src/packages/shell/styles.css', contentType: 'text/css' }], navigation: [{ id: 'transport', label: 'Transport', order: 1 }], browser: { id: 'transport', entry: '/assets/transport/app.js', stylesheet: '/assets/transport/styles.css' }, dispose() { log.disposed += 1; } }; } };
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
  await withServer(async (baseUrl) => { const body = await fetch(`${baseUrl}/api/home`).then((response) => { assert.equal(response.status, 200); return response.json(); }); assert.equal(body.path, 'home.md'); assert.match(body.html, /Fixture Home/); }, { config: configWith({ home: { module: 'src/packages/home/index.ts', source: 'home.md' } }) });
  await withServer(async (baseUrl) => { const body = await fetch(`${baseUrl}/api/home`).then((response) => { assert.equal(response.status, 200); return response.json(); }); assert.equal(body.path, 'home.html'); assert.match(body.html, /Repository-owned markup/); }, { config: configWith({ home: { module: 'src/packages/home/index.ts', source: 'home.html' } }) });
});

test('serves package-local assets through preserved public URLs', async () => {
  await withServer(async (baseUrl) => {
    const home = await fetch(`${baseUrl}/assets/home/home.js`); const docs = await fetch(`${baseUrl}/assets/docs/docs.css`); const shell = await fetch(`${baseUrl}/assets/app.js`); const theme = await fetch(`${baseUrl}/assets/shell/theme-bootstrap.js`);
    assert.equal(home.status, 200); assert.match(await home.text(), /export default/);
    assert.equal(docs.status, 200); assert.match(await docs.text(), /docs-layout/);
    assert.equal(shell.status, 200); assert.match(await shell.text(), /import\(packageInfo\.entry\)/);
    assert.equal(theme.status, 200); assert.match(await theme.text(), /prefers-color-scheme: dark/);
    assert.equal((await fetch(`${baseUrl}/not-registered.js`)).status, 404);
  }, { config: moduleConfig });
});

test('serves injected routes/assets and generic failures', async () => {
  const registry = createHost({ root: fixtureRoot, config: { version: 1, packages: { test: { module: 'test.ts' } } }, packages: [packageDefinition()] });
  await withServer(async (baseUrl) => {
    assert.deepEqual(await fetch(`${baseUrl}/api/manifest`).then((response) => response.json()), registry.manifest);
    assert.deepEqual(await fetch(`${baseUrl}/api/test/value`).then((response) => response.json()), { ok: true });
    assert.equal((await fetch(`${baseUrl}/assets/test/app.js`)).status, 200);
    assert.equal((await fetch(`${baseUrl}/api/test/failure`)).status, 500);
    const post = await fetch(`${baseUrl}/`, { method: 'POST' });
    assert.equal(post.status, 405); assert.equal(post.headers.get('allow'), 'GET');
  }, { registry });
});

test('emits request status and route fields through host telemetry', async () => {
  const records: any[] = [];
  const telemetry = createTelemetry({ config: { mode: 'console' }, console: null, exporter: { record(record) { records.push(record); }, async flush() {} } });
  const registry = createHost({ root: fixtureRoot, config: { version: 1, packages: { test: { module: 'test.ts' } } }, packages: [packageDefinition()], telemetry });
  await withServer(async (baseUrl) => { assert.equal((await fetch(`${baseUrl}/api/test/value`)).status, 200); }, { registry });
  const request = records.find((record) => record.kind === 'span' && record.name === 'http.request');
  assert.equal(request.fields.path, '/api/test/value'); assert.equal(request.fields.status, 200);
});

test('dispatches methods, bounds JSON bodies, and protects streamed responses', async () => {
  const log = { disposed: 0 };
  const registry = createHost({ root: fixtureRoot, config: { version: 1, packages: { transport: { module: 'test.ts' } } }, packages: [transportPackage(log)] });
  await withServer(async (baseUrl) => {
    assert.deepEqual(await fetch(`${baseUrl}/api/transport/value`, { method: 'GET' }).then((response) => response.json()), { method: 'GET' });
    assert.deepEqual(await fetch(`${baseUrl}/api/transport/value`, { method: 'POST' }).then((response) => response.json()), { method: 'POST' });
    const postOnlyGet = await fetch(`${baseUrl}/api/transport/post-only`); assert.equal(postOnlyGet.status, 405); assert.equal(postOnlyGet.headers.get('allow'), 'POST');
    const unsupported = await fetch(`${baseUrl}/api/transport/value`, { method: 'PUT' }); assert.equal(unsupported.status, 405); assert.equal(unsupported.headers.get('allow'), 'GET, POST');
    const valid = await fetch(`${baseUrl}/api/transport/body`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ ok: 1 }) }); assert.equal(valid.status, 200); assert.deepEqual(await valid.json(), { ok: 1 });
    const nonJson = await fetch(`${baseUrl}/api/transport/body`, { method: 'POST', headers: { 'content-type': 'text/plain' }, body: '{}' }); assert.equal(nonJson.status, 415);
    const malformed = await fetch(`${baseUrl}/api/transport/body`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: '{' }); assert.equal(malformed.status, 400);
    const oversized = await fetch(`${baseUrl}/api/transport/body`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ value: 'too long' }) }); assert.equal(oversized.status, 413);
    const streamed = await fetch(`${baseUrl}/api/transport/stream`); assert.equal(streamed.status, 200); assert.match(streamed.headers.get('content-type'), /text\/event-stream/); assert.equal(streamed.headers.get('cache-control'), 'no-cache, no-store'); assert.match(await streamed.text(), /"type":"hello"/);
  }, { registry });
  assert.equal(log.disposed, 1);
});

test('starts from an explicitly installed Shell+Docs config', async () => {
  const root = await mkdtemp(`${tmpdir()}/resonance-generated-`);
  await writeRepositoryConfig(root, createRepositoryConfig({ docs: true }));
  await withServer(async (baseUrl) => {
    const manifest = await fetch(`${baseUrl}/api/manifest`).then((response) => response.json());
    assert.deepEqual(manifest.packages.map((item) => item.id), ['shell', 'docs']);
  }, { root });
});

test('does not load repository config when a registry is supplied', async () => {
  const root = await mkdtemp(`${tmpdir()}/resonance-server-`);
  const registry = createHost({ root, config: { version: 1, packages: {} } });
  await createApp({ root, registry });
  await assert.rejects(() => access(`${root}/.resonance/config.json`), { code: 'ENOENT' });
});

test('starts on the next port when the requested port is occupied', async () => {
  const occupiedServer = await createApp({ root: fixtureRoot });
  await new Promise((resolve) => occupiedServer.listen(0, '127.0.0.1', resolve));
  const occupiedPort = occupiedServer.address().port;
  const fallbackServer = await startServer({ root: fixtureRoot, port: occupiedPort, maxPortAttempts: 3 });
  try { assert.equal(fallbackServer.address().port, occupiedPort + 1); }
  finally { await new Promise((resolve, reject) => fallbackServer.close((error) => error ? reject(error) : resolve())); await new Promise((resolve, reject) => occupiedServer.close((error) => error ? reject(error) : resolve())); }
});
