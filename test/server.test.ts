import test from 'node:test';
import assert from 'node:assert/strict';
import { createApp, startServer } from '../src/server.ts';
import { createHost } from '../src/host.ts';
import { loadRepositoryConfig } from '../src/config.ts';

const fixtureRoot = new URL('./fixtures/repository/', import.meta.url);

function packageDefinition() {
  const metadata = { id: 'test', version: '1.0.0', hostVersion: '1', label: 'Test', order: 1 };
  return {
    metadata,
    register() {
      return {
        metadata,
        routes: [
          { method: 'GET', path: '/api/test/value', handler: async (_request, response, context) => context.sendJson(response, 200, { ok: true }) },
          { method: 'GET', path: '/api/test/failure', handler: async () => { throw new Error('boom'); } },
        ],
        assets: [{ path: '/assets/test/styles.css', file: 'styles.css', contentType: 'text/css; charset=utf-8' }],
        navigation: [{ id: 'test', label: 'Test', order: 1 }],
        browser: { id: 'test', entry: '/assets/test/app.js', stylesheet: '/assets/test/styles.css' },
      };
    },
  };
}

async function withServer(run, options = {}) {
  const server = createApp({ root: fixtureRoot, ...options });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const { port } = server.address();
  try {
    await run(`http://127.0.0.1:${port}`);
  } finally {
    await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  }
}

test('serves canonical Docs routes and equivalent compatibility aliases', async () => {
  const config = await loadRepositoryConfig(fixtureRoot);
  await withServer(async (baseUrl) => {
    const canonicalTree = await fetch(`${baseUrl}/api/docs/tree`);
    const aliasTree = await fetch(`${baseUrl}/api/tree`);
    assert.equal(canonicalTree.status, 200);
    assert.deepEqual(await canonicalTree.json(), await aliasTree.json());

    const canonicalDocument = await fetch(`${baseUrl}/api/docs/document?path=docs%2Farchitecture.md`);
    const aliasDocument = await fetch(`${baseUrl}/api/document?path=docs%2Farchitecture.md`);
    assert.equal(canonicalDocument.status, 200);
    const canonicalBody = await canonicalDocument.json();
    assert.deepEqual(canonicalBody, await aliasDocument.json());
    assert.deepEqual(canonicalBody, {
      path: 'docs/architecture.md',
      content: '# Architecture\n',
      html: '<h1>Architecture</h1>\n',
    });

    const tree = await fetch(`${baseUrl}/api/docs/tree`).then((response) => response.json());
    assert.deepEqual(tree, {
      rootName: 'repository',
      documents: ['README.md', 'docs/architecture.md', 'docs/guides/getting-started.md', 'home.md'],
      tree: [
        { type: 'file', name: 'README.md', path: 'README.md' },
        { type: 'file', name: 'home.md', path: 'home.md' },
        { type: 'folder', name: 'docs', children: [
          { type: 'file', name: 'architecture.md', path: 'docs/architecture.md' },
          { type: 'folder', name: 'guides', children: [
            { type: 'file', name: 'getting-started.md', path: 'docs/guides/getting-started.md' },
          ] },
        ] },
      ],
    });
  }, { config });
});

test('serves the configured Home source with safe rendered HTML', async () => {
  await withServer(async (baseUrl) => {
    const response = await fetch(`${baseUrl}/api/home`);
    assert.equal(response.status, 200);
    assert.deepEqual(await response.json(), {
      path: 'home.md',
      content: '# Fixture Home\n\nThis repository has a configured landing page.\n',
      html: '<h1>Fixture Home</h1>\n<p>This repository has a configured landing page.</p>\n',
    });
  }, { config: await loadRepositoryConfig(fixtureRoot) });
});

test('returns the Home 404 shape for missing and traversal sources', async () => {
  for (const source of ['missing.md', '../README.md']) {
    await withServer(async (baseUrl) => {
      const response = await fetch(`${baseUrl}/api/home`);
      assert.equal(response.status, 404, source);
      assert.deepEqual(await response.json(), { error: 'Home source not found' });
    }, { config: { version: 1, packages: { home: { source } } } });
  }
});

test('applies configured Docs extension filtering', async () => {
  await withServer(async (baseUrl) => {
    const tree = await fetch(`${baseUrl}/api/docs/tree`).then((response) => response.json());
    assert.deepEqual(tree.documents, []);
  }, { config: { version: 1, packages: { docs: { extensions: ['.markdown'], ignoredDirectories: ['.git', 'node_modules'] } } } });
});

test('returns the Docs 404 shape for invalid, traversal, non-Markdown, and missing paths', async () => {
  await withServer(async (baseUrl) => {
    for (const value of ['', '../README.md', 'docs\\\\architecture.md', 'README.txt', 'missing.md']) {
      const response = await fetch(`${baseUrl}/api/docs/document?path=${encodeURIComponent(value)}`);
      assert.equal(response.status, 404, value);
      assert.deepEqual(await response.json(), { error: 'Markdown document not found' });
    }
  });
});

test('serves registered routes and assets, the generic manifest, and generic failures', async () => {
  const registry = createHost({ root: fixtureRoot, packages: [packageDefinition()] });
  await withServer(async (baseUrl) => {
    const manifest = await fetch(`${baseUrl}/api/manifest`);
    assert.equal(manifest.status, 200);
    assert.deepEqual(await manifest.json(), registry.manifest);

    const route = await fetch(`${baseUrl}/api/test/value`);
    assert.equal(route.status, 200);
    assert.deepEqual(await route.json(), { ok: true });

    const asset = await fetch(`${baseUrl}/assets/test/styles.css`);
    assert.equal(asset.status, 200);
    assert.match(await asset.text(), /--ink/);

    const missing = await fetch(`${baseUrl}/missing`);
    assert.equal(missing.status, 404);
    assert.equal(await missing.text(), 'Not found');

    const failure = await fetch(`${baseUrl}/api/test/failure`);
    assert.equal(failure.status, 500);
    assert.deepEqual(await failure.json(), { error: 'Internal server error' });
  }, { registry });
});

test('serves the transitional Shell assets and keeps the transport GET-only', async () => {
  await withServer(async (baseUrl) => {
    const shell = await fetch(`${baseUrl}/`);
    const app = await fetch(`${baseUrl}/assets/app.js`);
    const styles = await fetch(`${baseUrl}/assets/styles.css`);
    const docsScript = await fetch(`${baseUrl}/assets/docs/docs.js`);
    const docsStyles = await fetch(`${baseUrl}/assets/docs/docs.css`);
    const manifest = await fetch(`${baseUrl}/api/manifest`);
    const post = await fetch(`${baseUrl}/`, { method: 'POST' });
    assert.equal(shell.status, 200);
    assert.equal(app.status, 200);
    assert.equal(styles.status, 200);
    assert.equal(docsScript.status, 200);
    assert.equal(docsStyles.status, 200);
    assert.equal(manifest.status, 200);
    assert.deepEqual((await manifest.json()).packages.map((item) => item.id), ['shell', 'home', 'docs']);
    assert.equal(post.status, 405);
    assert.equal(post.headers.get('allow'), 'GET');
    assert.match(await shell.text(), /id="package-mount"/);
    assert.match(await docsStyles.text(), /docs-layout/);
  });
});

test('starts on the next port when the requested port is already in use', async () => {
  const occupiedServer = createApp({ root: fixtureRoot });
  await new Promise((resolve) => occupiedServer.listen(0, '127.0.0.1', resolve));
  const occupiedPort = occupiedServer.address().port;
  const fallbackServer = await startServer({ root: fixtureRoot, port: occupiedPort, maxPortAttempts: 3 });

  try {
    assert.equal(fallbackServer.address().port, occupiedPort + 1);
  } finally {
    await new Promise((resolve, reject) => fallbackServer.close((error) => error ? reject(error) : resolve()));
    await new Promise((resolve, reject) => occupiedServer.close((error) => error ? reject(error) : resolve()));
  }
});
