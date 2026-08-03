import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const publicRoot = new URL('../public/', import.meta.url);

test('the cockpit shell exposes a navigation tree and document pane', async () => {
  const html = await readFile(new URL('index.html', publicRoot), 'utf8');

  assert.match(html, /<nav[^>]+id="primary-navigation"/);
  assert.match(html, /data-view="home"/);
  assert.match(html, /data-view="docs"/);
  assert.match(html, /<aside[^>]+id="document-sidebar"[^>]+hidden/);
  assert.match(html, /<nav[^>]+id="document-tree"/);
  assert.match(html, /<main[^>]+id="document-pane"/);
  assert.match(html, /id="project-name"/);
  assert.match(html, /id="document-path"/);
});

test('the browser client loads the tree and selected documents through the local API', async () => {
  const script = await readFile(new URL('app.js', publicRoot), 'utf8');

  assert.match(script, /fetch\('\/api\/tree'\)/);
  assert.match(script, /fetch\(`\/api\/document\?path=/);
  assert.match(script, /document-tree/);
  assert.match(script, /primary-navigation/);
  assert.match(script, /data-view/);
  assert.match(script, /documentSidebar\.hidden/);
  assert.match(script, /appShell\.classList\.toggle\('docs-active'/);
});
