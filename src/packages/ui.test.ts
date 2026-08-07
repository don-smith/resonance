import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { parseHTML } from 'linkedom';

const shellRoot = new URL('./shell/', import.meta.url);
const retiredTerm = new RegExp(['cock', 'pit'].join(''), 'i');

test('the Shell document contains only fixed Shell browser wiring', async () => {
  const html = await readFile(new URL('index.html', shellRoot), 'utf8');
  assert.match(html, /id="primary-navigation"/);
  assert.match(html, /id="package-mount"/);
  assert.match(html, /assets\/shell\/shell\.css/);
  assert.match(html, /assets\/app\.js/);
  assert.doesNotMatch(html, /assets\/home\/home\.css/);
  assert.doesNotMatch(html, /assets\/docs\/docs\.css/);
  assert.doesNotMatch(html, retiredTerm);
});

test('the Shell keeps primary navigation fixed while the package area scrolls', async () => {
  const css = await readFile(new URL('./shell/styles.css', import.meta.url), 'utf8');
  assert.match(css, /\.app-shell \{[^}]*height: 100vh;[^}]*min-height: 0;[^}]*overflow: hidden;/s);
  assert.match(css, /\.primary-sidebar \{[^}]*height: 100vh;[^}]*min-height: 0;[^}]*overflow: hidden;/s);
  assert.match(css, /\.package-mount-region \{[^}]*min-height: 0;[^}]*overflow-y: auto;/s);
  assert.match(css, /@media \(max-width: 720px\)[\s\S]*\.app-shell \{[^}]*height: auto;/s);
  assert.match(css, /@media \(max-width: 720px\)[\s\S]*\.primary-sidebar \{[^}]*height: auto;[^}]*overflow: visible;/s);
  assert.match(css, /@media \(max-width: 720px\)[\s\S]*\.package-mount-region \{[^}]*height: auto;[^}]*overflow: visible;/s);
  assert.match(css, /@media \(max-width: 720px\)[\s\S]*\.package-mount \{[^}]*height: auto;/s);
});

test('the Docs keeps its tree fixed while both panes scroll when needed', async () => {
  const css = await readFile(new URL('./docs/docs.css', import.meta.url), 'utf8');
  assert.match(css, /\.docs-layout \{[^}]*height: 100%;[^}]*min-height: 0;[^}]*overflow: hidden;/s);
  assert.match(css, /\.document-sidebar \{[^}]*display: flex;[^}]*height: 100%;[^}]*min-height: 0;[^}]*overflow: hidden;[^}]*padding: 32px 10px 20px 20px;/s);
  assert.match(css, /\.document-tree \{[^}]*min-height: 0;[^}]*flex: 1;[^}]*overflow-y: auto;[^}]*scrollbar-width: thin;[^}]*scrollbar-color: var\(--line\) transparent;/s);
  assert.match(css, /\.document-tree::-webkit-scrollbar \{[^}]*width: 4px;/s);
  assert.match(css, /\.document-tree::-webkit-scrollbar-thumb \{[^}]*background: var\(--line\);/s);
  assert.match(css, /\.document-pane \{[^}]*height: 100%;[^}]*min-height: 0;[^}]*overflow-y: auto;/s);
  assert.match(css, /@media \(max-width: 720px\)[\s\S]*\.docs-layout \{[^}]*height: auto;[^}]*overflow: visible;/s);
  assert.match(css, /@media \(max-width: 720px\)[\s\S]*\.document-sidebar \{[^}]*height: auto;[^}]*overflow: visible;/s);
  assert.match(css, /@media \(max-width: 720px\)[\s\S]*\.document-pane \{[^}]*height: auto;[^}]*overflow: visible;/s);
});

test('Pi Agent exposes a live history, composer, retry, and New Session controls', async () => {
  const { document } = parseHTML('<!doctype html><body></body>');
  let source;
  const module = await import(`./pi-agent/pi-agent.js?ui=${Date.now()}`);
  const instance = module.default({
    fetchFn: async () => ({ ok: true, status: 202, async json() { return { ok: true, state: { messages: [], status: 'idle', hasSession: false, error: null } }; } }),
    eventSourceFactory: () => { source = { close() {} }; return source; },
  });
  const root = document.createElement('section');
  instance.mount(root);
  await instance.activate();
  assert.ok(root.querySelector('.pi-agent-history'));
  assert.equal(root.querySelector('.pi-agent-history').getAttribute('aria-live'), 'polite');
  assert.ok(root.querySelector('.pi-agent-wait'));
  assert.equal(root.querySelector('.pi-agent-wait').hidden, true);
  assert.ok(root.querySelector('.pi-agent-composer textarea'));
  assert.ok(root.querySelector('.pi-agent-new'));
  assert.ok(root.querySelector('.pi-agent-retry'));
  instance.deactivate();
  assert.equal(root.hidden, true);
});

test('Pi Agent submits on Shift+Enter while preserving plain Enter', async () => {
  const { document, window } = parseHTML('<!doctype html><body></body>');
  const calls = [];
  const module = await import(`./pi-agent/pi-agent.js?keyboard=${Date.now()}`);
  const instance = module.default({
    fetchFn: async (url, options) => {
      calls.push([url, options]);
      return { ok: true, status: 202, async json() { return { accepted: true }; } };
    },
    eventSourceFactory: () => ({ close() {} }),
  });
  const root = document.createElement('section');
  instance.mount(root);
  await instance.activate();
  const input = root.querySelector('.pi-agent-composer textarea');
  input.value = 'shift prompt';

  const shiftEnter = new window.Event('keydown', { bubbles: true, cancelable: true });
  Object.defineProperties(shiftEnter, { key: { value: 'Enter' }, shiftKey: { value: true } });
  input.dispatchEvent(shiftEnter);
  assert.equal(shiftEnter.defaultPrevented, true);
  assert.equal(calls.length, 1);
  assert.equal(JSON.parse(calls[0][1].body).prompt, 'shift prompt');
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(input.value, '');

  input.value = 'prompt with a newline';
  const enter = new window.Event('keydown', { bubbles: true, cancelable: true });
  Object.defineProperties(enter, { key: { value: 'Enter' }, shiftKey: { value: false } });
  input.dispatchEvent(enter);
  assert.equal(enter.defaultPrevented, false);
  assert.equal(calls.length, 1);
});

test('Pi Agent uses fixed-pane scrolling and responsive mobile boundaries', async () => {
  const css = await readFile(new URL('./pi-agent/pi-agent.css', import.meta.url), 'utf8');
  assert.match(css, /\.pi-agent-workspace \{[^}]*height: 100%;[^}]*min-height: 0;[^}]*overflow: hidden;/s);
  assert.match(css, /\.pi-agent-history \{[^}]*min-height: 0;[^}]*overflow-y: auto;/s);
  assert.match(css, /@media \(max-width: 720px\)[\s\S]*\.pi-agent-workspace \{[^}]*height: auto;[^}]*overflow: visible;/s);
  assert.match(css, /@media \(max-width: 720px\)[\s\S]*\.pi-agent-history \{[^}]*overflow: visible;/s);
  assert.match(css, /\.pi-agent-wait \{[^}]*animation:/s);
  assert.match(css, /prefers-reduced-motion: reduce[\s\S]*\.pi-agent-wait/);
  assert.match(css, /\.pi-agent-composer textarea \{[^}]*border: 1px solid var\(--line\);/s);
});

test('the Home package does not expose retired product terminology', async () => {
  const homeModule = await readFile(new URL('./home/home.js', import.meta.url), 'utf8');
  assert.doesNotMatch(homeModule, retiredTerm);
});

test('the repository Home presents the Resonance manifesto', async () => {
  const html = await readFile(new URL('../../.resonance/home.html', import.meta.url), 'utf8');
  assert.match(html, /Integrated Application Environment/);
  assert.match(html, /The repository defines the team(?:'|’)s shared understanding/);
  assert.match(html, /Resonate is the action/);
  assert.equal((html.match(/class="home-section"/g) || []).length, 7);
  assert.match(html, /<pre><code>resonate init/);
  assert.match(html, /aria-labelledby="home-commands-title"/);
});
