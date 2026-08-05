import test from 'node:test';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { defaultRepositoryConfig } from '../src/config.ts';
import { createHost } from '../src/host.ts';
import { loadConfiguredPackages } from '../src/packages/index.ts';
import { homeInput } from '../src/packages/home/index.ts';
import { docsPackage } from '../src/packages/docs/index.ts';

function packageDefinition(id, { order = 1, assetFile = 'src/packages/shell/app.js', extraRoute = false } = {}) {
  const metadata = { id, version: '1.0.0', hostVersion: '1', label: id.toUpperCase(), order };
  return {
    metadata,
    register() {
      return {
        metadata,
        routes: [{ method: 'GET', path: `/api/${id}`, handler: async () => {} }, ...(extraRoute ? [{ method: 'GET', path: `/api/${id}`, handler: async () => {} }] : [])],
        assets: [{ path: `/assets/${id}/entry.js`, file: assetFile, contentType: 'text/javascript' }, { path: `/assets/${id}/styles.css`, file: 'src/packages/shell/styles.css', contentType: 'text/css' }],
        navigation: [{ id, label: id.toUpperCase(), order }],
        browser: { id, entry: `/assets/${id}/entry.js`, stylesheet: `/assets/${id}/styles.css` },
      };
    },
  };
}

test('loads configured built-in modules and assembles a deterministic registry', async () => {
  const appRoot = fileURLToPath(new URL('../', import.meta.url));
  const config = defaultRepositoryConfig();
  const packages = await loadConfiguredPackages({ config, appRoot });
  const registry = createHost({ appRoot, config, packages });
  assert.deepEqual(registry.manifest.navigation.map((item) => item.id), ['home', 'docs', 'pi-agent']);
  assert.deepEqual(registry.manifest.packages.map((item) => item.id), ['shell', 'home', 'docs', 'pi-agent']);
  assert.equal(registry.assets['/assets/home/home.js'].file, 'src/packages/home/home.js');
  assert.ok(Object.isFrozen(registry.manifest));
});

test('skips disabled and invalid optional packages while keeping Shell required', async () => {
  const config = { version: 1, packages: { shell: { module: 'src/packages/shell/index.ts' }, home: { module: 'missing-home.ts' }, docs: { module: 'src/packages/docs/index.ts', enabled: false } } };
  const warnings = [];
  const packages = await loadConfiguredPackages({ config, appRoot: fileURLToPath(new URL('../', import.meta.url)), warn: (message) => warnings.push(message) });
  assert.deepEqual(packages.map((item) => item.metadata.id), ['shell']);
  assert.match(warnings[0], /Skipping package home/);
  await assert.rejects(() => loadConfiguredPackages({ config: { ...config, packages: { ...config.packages, shell: { module: 'missing-shell.ts' } } }, appRoot: fileURLToPath(new URL('../', import.meta.url)) }), /Shell/);
  await assert.rejects(() => loadConfiguredPackages({ config: { ...config, packages: { ...config.packages, shell: { module: 'src/packages/shell/index.ts', enabled: false } } }, appRoot: fileURLToPath(new URL('../', import.meta.url)) }), /Shell package cannot be disabled/);
});

test('validates contributions and isolates optional registration failures', () => {
  const warnings = [];
  const registry = createHost({ packages: [packageDefinition('alpha', { extraRoute: true }), packageDefinition('beta', { assetFile: '../entry.js' })], warn: (message) => warnings.push(message) });
  assert.equal(registry.manifest.packages.length, 0);
  assert.match(warnings.join(' '), /Duplicate route path/);
  assert.match(warnings.join(' '), /Asset file/);
  const definition = packageDefinition('alpha');
  const originalRegister = definition.register;
  definition.register = (...args) => ({ ...originalRegister(...args), browser: { id: 'alpha', entry: '/assets/alpha/missing.js', stylesheet: '/assets/alpha/styles.css' } });
  assert.doesNotThrow(() => createHost({ packages: [definition], warn: () => {} }));
  const malformed = { metadata: { id: 'optional', version: '1', hostVersion: '1', label: 'Optional', order: 1 }, register: () => null };
  const malformedWarnings = [];
  const malformedRegistry = createHost({ packages: [malformed], warn: (message) => malformedWarnings.push(message) });
  assert.equal(malformedRegistry.manifest.packages.length, 0);
  assert.match(malformedWarnings[0], /invalid registration/);
});

test('preserves Home validation and removes Docs aliases', () => {
  assert.equal(homeInput({ source: '.resonance/home.html' }).source, '.resonance/home.html');
  assert.throws(() => homeInput({ source: 'home.txt' }), /Markdown file/);
  const registry = createHost({ packages: [docsPackage] });
  assert.ok(registry.routes['GET /api/docs/tree']);
  assert.ok(registry.routes['GET /api/docs/document']);
  assert.equal(registry.routes['GET /api/tree'], undefined);
});

function methodPackage() {
  const metadata = { id: 'methods', version: '1.0.0', hostVersion: '1', label: 'Methods', order: 1 };
  return { metadata, register() { return {
    metadata,
    routes: [
      { method: 'GET', path: '/api/methods/value', handler: async () => {} },
      { method: 'POST', path: '/api/methods/value', handler: async () => {} },
    ],
    assets: [
      { path: '/assets/methods/app.js', file: 'src/packages/shell/app.js', contentType: 'text/javascript' },
      { path: '/assets/methods/styles.css', file: 'src/packages/shell/styles.css', contentType: 'text/css' },
    ],
    navigation: [{ id: 'methods', label: 'Methods', order: 1 }],
    browser: { id: 'methods', entry: '/assets/methods/app.js', stylesheet: '/assets/methods/styles.css' },
    dispose() {},
  }; } };
}

test('registers distinct methods on one pathname and disposes packages idempotently', async () => {
  const registry = createHost({ packages: [methodPackage()] });
  assert.ok(registry.routes['GET /api/methods/value']);
  assert.ok(registry.routes['POST /api/methods/value']);
  await registry.dispose();
  await registry.dispose();
});
