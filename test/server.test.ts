import test from 'node:test';
import assert from 'node:assert/strict';
import { createApp, startServer } from '../src/server.ts';

const fixtureRoot = new URL('./fixtures/repository/', import.meta.url);

async function withServer(run) {
  const server = createApp({ root: fixtureRoot });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const { port } = server.address();

  try {
    await run(`http://127.0.0.1:${port}`);
  } finally {
    await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  }
}

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

test('serves the Markdown tree for the current repository', async () => {
  await withServer(async (baseUrl) => {
    const response = await fetch(`${baseUrl}/api/tree`);
    assert.equal(response.status, 200);
    assert.deepEqual(await response.json(), {
      rootName: 'repository',
      documents: [
        'README.md',
        'docs/architecture.md',
        'docs/guides/getting-started.md',
      ],
      tree: [
        { type: 'file', name: 'README.md', path: 'README.md' },
        {
          type: 'folder',
          name: 'docs',
          children: [
            { type: 'file', name: 'architecture.md', path: 'docs/architecture.md' },
            {
              type: 'folder',
              name: 'guides',
              children: [
                { type: 'file', name: 'getting-started.md', path: 'docs/guides/getting-started.md' },
              ],
            },
          ],
        },
      ],
    });
  });
});

test('serves a selected Markdown document and rejects traversal', async () => {
  await withServer(async (baseUrl) => {
    const document = await fetch(`${baseUrl}/api/document?path=docs%2Farchitecture.md`);
    assert.equal(document.status, 200);
    assert.deepEqual(await document.json(), {
      path: 'docs/architecture.md',
      content: '# Architecture\n',
      html: '<h1>Architecture</h1>\n',
    });

    const traversal = await fetch(`${baseUrl}/api/document?path=..%2Fpackage.json`);
    assert.equal(traversal.status, 404);
  });
});
