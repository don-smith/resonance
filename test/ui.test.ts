import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const publicRoot = new URL('../public/', import.meta.url);

test('the shell keeps static package invariants', async () => {
  const html = await readFile(new URL('index.html', publicRoot), 'utf8');
  assert.match(html, /id="primary-navigation"/);
  assert.match(html, /id="package-mount"/);
  assert.doesNotMatch(html, /id="document-sidebar"/);
  assert.match(html, /assets\/shell\/shell\.css/);
  assert.match(html, /assets\/home\/home\.css/);
  assert.match(html, /assets\/docs\/docs\.css/);
});
