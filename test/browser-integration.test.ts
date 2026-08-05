import test from 'node:test';
import assert from 'node:assert/strict';
import { parseHTML } from 'linkedom';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { fileURLToPath, pathToFileURL } from 'node:url';

const shellPath = new URL('../src/packages/shell/app.js', import.meta.url);
const shellModulePath = new URL('../src/packages/shell/shell.js', import.meta.url);
const homeModulePath = new URL('../src/packages/home/home.js', import.meta.url);
const docsModulePath = new URL('../src/packages/docs/docs.js', import.meta.url);
const piAgentModulePath = new URL('../src/packages/pi-agent/pi-agent.js', import.meta.url);
function response(body, status = 200) { return { ok: status >= 200 && status < 300, status, async json() { return body; } }; }
async function loadCoordinator(window, document) {
  globalThis.window = window; globalThis.document = document; globalThis.__RESONANCE_TEST__ = true;
  let source = await readFile(shellPath, 'utf8');
  source = source.replace("'/assets/shell/shell.js'", JSON.stringify(pathToFileURL(fileURLToPath(shellModulePath)).href));
  const modules = { '/assets/home/home.js': pathToFileURL(fileURLToPath(homeModulePath)).href, '/assets/docs/docs.js': pathToFileURL(fileURLToPath(docsModulePath)).href, '/assets/pi-agent/pi-agent.js': pathToFileURL(fileURLToPath(piAgentModulePath)).href };
  source = source.replace('import(packageInfo.entry)', `import(${JSON.stringify(modules)}[packageInfo.entry])`);
  const directory = await mkdtemp(`${tmpdir()}/resonance-browser-`); const filename = `${directory}/app.js`; await writeFile(filename, source);
  try { return await import(`${pathToFileURL(filename).href}?${Date.now()}`); } finally { await rm(directory, { recursive: true, force: true }); }
}
function cleanup() { delete globalThis.window; delete globalThis.document; delete globalThis.__RESONANCE_TEST__; }

test('loads browser modules and stylesheets from the manifest', async () => {
  const { window, document } = parseHTML('<!doctype html><head></head><body><nav id="primary-navigation"></nav><main id="package-mount"></main></body>');
  const calls = [];
  const fetchFn = async (url) => { calls.push(url); if (url === '/api/manifest') return response({ version: 1, navigation: [{ id: 'home', label: 'Home', order: 10 }, { id: 'docs', label: 'Docs', order: 20 }], packages: [{ id: 'shell', entry: '/assets/app.js', stylesheet: '/assets/shell/shell.css' }, { id: 'home', entry: '/assets/home/home.js', stylesheet: '/assets/home/home.css' }, { id: 'docs', entry: '/assets/docs/docs.js', stylesheet: '/assets/docs/docs.css' }] }); if (url === '/api/home') return response({ html: '<h1>Fixture Home</h1>' }); if (url === '/api/docs/tree') return response({ rootName: 'fixture', documents: ['README.md'], tree: [{ type: 'file', name: 'README.md', path: 'README.md' }] }); if (url === '/api/docs/document?path=README.md') return response({ path: 'README.md', html: '<h1>README</h1>' }); throw new Error(`Unexpected request: ${url}`); };
  try { const coordinator = await loadCoordinator(window, document); const application = await coordinator.startApplication({ documentRoot: document, fetchFn }); assert.equal(document.querySelectorAll('#primary-navigation [data-package]').length, 2); assert.equal(document.querySelectorAll('#package-mount > [data-package]').length, 2); assert.equal(document.querySelectorAll('link[data-package-style]').length, 2); assert.match(document.querySelector('.package-home').innerHTML, /Fixture Home/); await application.activate('docs'); assert.match(document.querySelector('.package-docs .document-content').innerHTML, /README/); assert.deepEqual(calls, ['/api/manifest', '/api/home', '/api/docs/tree', '/api/docs/document?path=README.md']); } finally { cleanup(); }
});

test('loads Pi Agent, preserves transcript across navigation, resets, and recovers its stream', async () => {
  const { window, document } = parseHTML('<!doctype html><head></head><body><nav id="primary-navigation"></nav><main id="package-mount"></main></body>');
  class FakeEventSource {
    static instances = [];
    constructor(url) { this.url = url; this.closed = false; FakeEventSource.instances.push(this); }
    close() { this.closed = true; }
    emit(event) { this.onmessage?.({ data: JSON.stringify(event) }); }
    fail() { this.onerror?.(new Error('stream failed')); }
  }
  const calls = [];
  const fetchFn = async (url, options) => {
    calls.push([url, options]);
    if (url === '/api/manifest') return response({ version: 1, navigation: [{ id: 'home', label: 'Home', order: 10 }, { id: 'docs', label: 'Docs', order: 20 }, { id: 'pi-agent', label: 'Pi Agent', order: 30 }], packages: [
      { id: 'shell', entry: '/assets/app.js', stylesheet: '/assets/shell/shell.css' },
      { id: 'home', entry: '/assets/home/home.js', stylesheet: '/assets/home/home.css' },
      { id: 'docs', entry: '/assets/docs/docs.js', stylesheet: '/assets/docs/docs.css' },
      { id: 'pi-agent', entry: '/assets/pi-agent/pi-agent.js', stylesheet: '/assets/pi-agent/pi-agent.css' },
    ] });
    if (url === '/api/home') return response({ html: '<h1>Home</h1>' });
    if (url === '/api/docs/tree') return response({ rootName: 'fixture', documents: ['README.md'], tree: [{ type: 'file', name: 'README.md', path: 'README.md' }] });
    if (url === '/api/docs/document?path=README.md') return response({ path: 'README.md', html: '<h1>README</h1>' });
    if (url === '/api/pi-agent/prompt') return response({ accepted: true }, 202);
    if (url === '/api/pi-agent/reset') return response({ ok: true, state: { messages: [{ id: 'reset-tool', role: 'activity', content: 'Reset tool · completed' }], status: 'idle', hasSession: false, error: null } });
    throw new Error(`Unexpected request: ${url}`);
  };
  try {
    const coordinator = await loadCoordinator(window, document);
    const application = await coordinator.startApplication({ documentRoot: document, fetchFn, eventSourceFactory: (url) => new FakeEventSource(url) });
    await application.activate('pi-agent');
    assert.equal(FakeEventSource.instances.length, 1);
    assert.equal(FakeEventSource.instances[0].url, '/api/pi-agent/events');
    FakeEventSource.instances[0].emit({ type: 'snapshot', snapshot: { messages: [{ id: 'u1', role: 'user', content: 'hello' }], status: 'idle', hasSession: true, error: null } });
    assert.match(document.querySelector('.package-pi-agent .pi-agent-history').textContent, /hello/);
    await application.activate('docs');
    assert.equal(FakeEventSource.instances[0].closed, true);
    await application.activate('pi-agent');
    assert.equal(FakeEventSource.instances.length, 2);
    assert.match(document.querySelector('.package-pi-agent .pi-agent-history').textContent, /hello/);
    const composer = document.querySelector('.package-pi-agent .pi-agent-composer');
    const input = composer.querySelector('textarea');
    input.value = 'next prompt';
    composer.dispatchEvent(new window.Event('submit', { bubbles: true, cancelable: true }));
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.ok(calls.some(([url, options]) => url === '/api/pi-agent/prompt' && JSON.parse(options.body).prompt === 'next prompt'));
    FakeEventSource.instances[1].emit({ type: 'status', status: 'working' });
    assert.equal(document.querySelector('.package-pi-agent .pi-agent-wait').hidden, false);
    FakeEventSource.instances[1].emit({ type: 'activity', message: { id: 'tool-1', role: 'activity', content: 'Edit file · in_progress' } });
    FakeEventSource.instances[1].emit({ type: 'message', message: { id: 'tool-2', role: 'activity', content: 'Delete file · completed' } });
    assert.equal(document.querySelector('.package-pi-agent .pi-agent-history').textContent.includes('Edit file'), false);
    assert.equal(document.querySelector('.package-pi-agent .pi-agent-history').textContent.includes('Delete file'), false);
    FakeEventSource.instances[1].emit({ type: 'message', message: { id: 'a1', role: 'assistant', content: 'answer' } });
    assert.match(document.querySelector('.package-pi-agent .pi-agent-history').textContent, /answer/);
    FakeEventSource.instances[1].emit({ type: 'done' });
    assert.equal(document.querySelector('.package-pi-agent .pi-agent-wait').hidden, true);
    document.querySelector('.package-pi-agent .pi-agent-new').click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.equal(document.querySelector('.package-pi-agent .pi-agent-history').textContent.includes('answer'), false);
    assert.equal(document.querySelector('.package-pi-agent .pi-agent-history').textContent.includes('Reset tool'), false);
    FakeEventSource.instances[1].fail();
    assert.equal(document.querySelector('.package-pi-agent .pi-agent-retry').hidden, false);
    document.querySelector('.package-pi-agent .pi-agent-retry').click();
    assert.equal(document.querySelector('.package-pi-agent').dataset.status, 'error');
    assert.equal(FakeEventSource.instances.length, 3);
  } finally { cleanup(); }
});

test('keeps a failed package activation local to that package', async () => {
  const { window, document } = parseHTML('<!doctype html><head></head><body><nav id="primary-navigation"></nav><main id="package-mount"></main></body>');
  let homeCalls = 0;
  const fetchFn = async (url) => { if (url === '/api/manifest') return response({ version: 1, navigation: [{ id: 'home', label: 'Home', order: 10 }, { id: 'docs', label: 'Docs', order: 20 }], packages: [{ id: 'shell', entry: '/assets/app.js', stylesheet: '/assets/shell/shell.css' }, { id: 'home', entry: '/assets/home/home.js', stylesheet: '/assets/home/home.css' }, { id: 'docs', entry: '/assets/docs/docs.js', stylesheet: '/assets/docs/docs.css' }] }); if (url === '/api/home') return homeCalls++ === 0 ? response({ html: '<h1>Home</h1>' }) : response({ error: 'missing' }, 404); if (url === '/api/docs/tree') return response({ rootName: 'fixture', documents: [], tree: [] }); throw new Error(`Unexpected request: ${url}`); };
  try { const coordinator = await loadCoordinator(window, document); const application = await coordinator.startApplication({ documentRoot: document, fetchFn }); await assert.rejects(() => application.activate('home'), /Home source could not be loaded/); await application.activate('docs'); assert.match(document.querySelector('.package-docs .document-content').textContent, /no Markdown documents/); } finally { cleanup(); }
});
