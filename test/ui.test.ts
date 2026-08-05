import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const shellRoot = new URL('../src/packages/shell/', import.meta.url);

test('the Shell document contains only fixed Shell browser wiring', async () => {
  const html = await readFile(new URL('index.html', shellRoot), 'utf8');
  assert.match(html, /id="primary-navigation"/);
  assert.match(html, /id="package-mount"/);
  assert.match(html, /assets\/shell\/shell\.css/);
  assert.match(html, /assets\/app\.js/);
  assert.doesNotMatch(html, /assets\/home\/home\.css/);
  assert.doesNotMatch(html, /assets\/docs\/docs\.css/);
});

test('the repository Home presents the Resonance manifesto', async () => {
  const html = await readFile(new URL('../.resonance/home.html', import.meta.url), 'utf8');
  assert.match(html, /Integrated Application Environment/);
  assert.match(html, /The repository defines the team(?:'|’)s shared understanding/);
  assert.match(html, /Resonate is the action/);
  assert.equal((html.match(/class="home-section"/g) || []).length, 7);
  assert.match(html, /<pre><code>resonate init/);
  assert.match(html, /aria-labelledby="home-commands-title"/);
});
