import test from 'node:test';
import assert from 'node:assert/strict';
import { parseHTML } from 'linkedom';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { fileURLToPath, pathToFileURL } from 'node:url';

const publicPath = fileURLToPath(new URL('../public/', import.meta.url));
const coordinatorPath = new URL('../public/app.js', import.meta.url);
function response(body, status = 200) { return { ok: status >= 200 && status < 300, status, async json() { return body; } }; }
async function loadCoordinator(window, document) {
  globalThis.window = window; globalThis.document = document; globalThis.__THEVIEW_TEST__ = true;
  const source = await readFile(coordinatorPath, 'utf8');
  const replacements = {
    "'/assets/shell/shell.js'": JSON.stringify(pathToFileURL(`${publicPath}/shell.js`).href),
    "'/assets/home/home.js'": JSON.stringify(pathToFileURL(`${publicPath}/home.js`).href),
    "'/assets/docs/docs.js'": JSON.stringify(pathToFileURL(`${publicPath}/docs.js`).href),
  };
  const transformed = Object.entries(replacements).reduce((value, [from, to]) => value.replaceAll(from, to), source);
  const directory = await mkdtemp(`${tmpdir()}/theview-browser-`);
  const filename = `${directory}/app.js`;
  await writeFile(filename, transformed);
  try {
    return await import(`${pathToFileURL(filename).href}?${Date.now()}`);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}
function cleanup() { delete globalThis.window; delete globalThis.document; delete globalThis.__THEVIEW_TEST__; }

test('executes package registration and browser lifecycle in a DOM harness', async () => {
  const { window, document } = parseHTML('<!doctype html><body><nav id="primary-navigation"></nav><main id="package-mount"></main></body>');
  const calls = [];
  const fetchFn = async (url) => {
    calls.push(url);
    if (url === '/api/manifest') return response({ version: 1, navigation: [{ id: 'home', label: 'Home', order: 10 }, { id: 'docs', label: 'Docs', order: 20 }], packages: [{ id: 'shell' }, { id: 'home' }, { id: 'docs' }] });
    if (url === '/api/home') return response({ html: '<h1>Fixture Home</h1>' });
    if (url === '/api/docs/tree') return response({ rootName: 'fixture', documents: ['README.md'], tree: [{ type: 'file', name: 'README.md', path: 'README.md' }] });
    if (url === '/api/docs/document?path=README.md') return response({ path: 'README.md', html: '<h1>README</h1>' });
    throw new Error(`Unexpected request: ${url}`);
  };
  try {
    const coordinator = await loadCoordinator(window, document);
    const application = await coordinator.startApplication({ documentRoot: document, fetchFn });
    assert.equal(document.querySelectorAll('#primary-navigation [data-package]').length, 2);
    assert.equal(document.querySelectorAll('#package-mount > [data-package]').length, 2);
    assert.match(document.querySelector('.package-home').innerHTML, /Fixture Home/);
    document.querySelector('#primary-navigation [data-package="docs"]').dispatchEvent(new window.Event('click', { bubbles: true }));
    await new Promise((resolve) => setTimeout(resolve, 0));
    document.querySelector('.package-docs .tree-file').dispatchEvent(new window.Event('click', { bubbles: true }));
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.match(document.querySelector('.package-docs .document-content').innerHTML, /README/);
    assert.deepEqual(calls, ['/api/manifest', '/api/home', '/api/docs/tree', '/api/docs/document?path=README.md', '/api/docs/document?path=README.md']);
  } finally { cleanup(); }
});

test('keeps a failed package activation local to that package', async () => {
  const { window, document } = parseHTML('<!doctype html><body><nav id="primary-navigation"></nav><main id="package-mount"></main></body>');
  let homeCalls = 0;
  const fetchFn = async (url) => {
    if (url === '/api/manifest') return response({ version: 1, navigation: [{ id: 'home', label: 'Home', order: 10 }, { id: 'docs', label: 'Docs', order: 20 }], packages: [{ id: 'shell' }, { id: 'home' }, { id: 'docs' }] });
    if (url === '/api/home') return homeCalls++ === 0 ? response({ html: '<h1>Home</h1>' }) : response({ error: 'missing' }, 404);
    if (url === '/api/docs/tree') return response({ rootName: 'fixture', documents: [], tree: [] });
    throw new Error(`Unexpected request: ${url}`);
  };
  try {
    const coordinator = await loadCoordinator(window, document);
    const application = await coordinator.startApplication({ documentRoot: document, fetchFn });
    await assert.rejects(() => application.activate('home'), /Home source could not be loaded/);
    await application.activate('docs');
    assert.equal(document.querySelector('.package-docs .document-content').textContent.trim(), 'This repository has no Markdown documents yet.');
  } finally { cleanup(); }
});
