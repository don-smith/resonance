import test from 'node:test';
import assert from 'node:assert/strict';
import { defaultRepositoryConfig } from '../src/config.ts';
import { createHost } from '../src/host.ts';
import { homeInput } from '../src/packages/home.ts';
import { createDefaultPackages } from '../src/packages/index.ts';

function packageDefinition(id, {
  routePath = `/api/${id}`,
  assetPath = `/assets/${id}/entry.js`,
  assetFile = 'app.js',
  navigationId = id,
  browserId = id,
  order = 1,
  extraRoute = false,
  extraAsset = false,
  extraNavigation = false,
  register = undefined,
} = {}) {
  const metadata = { id, version: '1.0.0', hostVersion: '1', label: id.toUpperCase(), order };
  return {
    metadata,
    register(context, input) {
      register?.(context, input);
      return {
        metadata,
        routes: [
          { method: 'GET', path: routePath, handler: async (_request, response, hostContext) => hostContext.sendJson(response, 200, { id }) },
          ...(extraRoute ? [{ method: 'GET', path: routePath, handler: async () => {} }] : []),
        ],
        assets: [
          { path: assetPath, file: assetFile, contentType: 'text/javascript; charset=utf-8' },
          ...(extraAsset ? [{ path: assetPath, file: 'app.js', contentType: 'text/javascript; charset=utf-8' }] : []),
        ],
        navigation: [{ id: navigationId, label: id.toUpperCase(), order }, ...(extraNavigation ? [{ id: navigationId, label: id.toUpperCase(), order }] : [])],
        browser: { id: browserId, entry: assetPath, stylesheet: `/assets/${id}/styles.css` },
      };
    },
  };
}

test('creates a deterministic immutable registry and assembles each package once', () => {
  const calls = [];
  const registry = createHost({ packages: [
    packageDefinition('beta', { order: 20, register: () => calls.push('beta') }),
    packageDefinition('alpha', { order: 10, register: () => calls.push('alpha') }),
  ] });

  assert.deepEqual(calls, ['beta', 'alpha']);
  assert.deepEqual(registry.manifest.navigation.map((item) => item.id), ['alpha', 'beta']);
  assert.deepEqual(registry.manifest.packages.map((item) => item.id), ['beta', 'alpha']);
  assert.equal(registry.routes['/api/alpha'].method, 'GET');
  assert.ok(Object.isFrozen(registry.routes));
  assert.ok(Object.isFrozen(registry.assets));
  assert.ok(Object.isFrozen(registry.manifest));
  assert.ok(Object.isFrozen(registry.manifest.navigation));
  assert.ok(Object.isFrozen(registry.manifest.packages));
});

test('assembles the fixed package order and final namespaced assets', () => {
  const packages = createDefaultPackages(defaultRepositoryConfig());
  const registry = createHost({ packages });
  assert.deepEqual(packages.map((definition) => definition.metadata.id), ['shell', 'home', 'docs']);
  assert.deepEqual(registry.manifest.navigation.map((item) => item.id), ['home', 'docs']);
  assert.deepEqual(registry.manifest.packages.map((item) => item.id), ['shell', 'home', 'docs']);
  assert.equal(registry.routes['/api/home'].method, 'GET');
  assert.deepEqual(Object.keys(registry.assets), [
    '/', '/assets/app.js', '/assets/styles.css',
    '/assets/shell/shell.js', '/assets/shell/shell.css',
    '/assets/home/home.js', '/assets/home/home.css',
    '/assets/docs/docs.js', '/assets/docs/docs.css',
  ]);
  assert.equal(registry.manifest.packages.find((item) => item.id === 'shell').entry, '/assets/app.js');
  assert.equal(registry.manifest.packages.find((item) => item.id === 'home').entry, '/assets/home/home.js');
  assert.equal(registry.manifest.packages.find((item) => item.id === 'docs').entry, '/assets/docs/docs.js');
});

test('validates Home inputs while host containment rejects traversal', () => {
  assert.equal(homeInput({ source: 'README.md' }).source, 'README.md');
  assert.throws(() => homeInput({ source: '' }), /non-empty relative path/);
  assert.throws(() => homeInput({ source: '/tmp/home.md' }), /non-empty relative path/);
  assert.throws(() => homeInput({ source: 'docs\\\\home.md' }), /non-empty relative path/);
  assert.throws(() => homeInput({ source: 'home.txt' }), /Markdown file/);
  const registry = createHost({ root: '/tmp/repository', packages: [] });
  assert.equal(registry.context.resolveRepositoryPath('../home.md'), null);
});

test('keeps repository path resolution inside the configured root without requiring Markdown', () => {
  const registry = createHost({ root: '/tmp/repository', packages: [] });
  assert.equal(registry.context.resolveRepositoryPath('docs/readme.txt'), 'docs/readme.txt');
  assert.equal(registry.context.resolveRepositoryPath(''), null);
  assert.equal(registry.context.resolveRepositoryPath('/tmp/readme.txt'), null);
  assert.equal(registry.context.resolveRepositoryPath('C:/tmp/readme.txt'), null);
  assert.equal(registry.context.resolveRepositoryPath('docs\\readme.txt'), null);
  assert.equal(registry.context.resolveRepositoryPath('../package.json'), null);
});

test('rejects duplicate package and contribution identifiers before runtime', () => {
  assert.throws(() => createHost({ packages: [packageDefinition('alpha'), packageDefinition('alpha')] }), /Duplicate package id/);
  assert.throws(() => createHost({ packages: [packageDefinition('alpha', { extraRoute: true })] }), /Duplicate route path/);
  assert.throws(() => createHost({ packages: [packageDefinition('alpha', { extraAsset: true })] }), /Duplicate asset path/);
  assert.throws(() => createHost({ packages: [packageDefinition('alpha', { extraNavigation: true })] }), /Duplicate navigation id/);
  assert.throws(() => createHost({ packages: [packageDefinition('alpha'), packageDefinition('beta', { navigationId: 'alpha' })] }), /Duplicate navigation id/);
  assert.throws(() => createHost({ packages: [packageDefinition('alpha'), packageDefinition('beta', { browserId: 'alpha' })] }), /Duplicate browser package id/);
});

test('requires namespaced routes and assets except for Shell compatibility assets', () => {
  assert.throws(() => createHost({ packages: [packageDefinition('alpha', { routePath: '/api/tree' })] }), /namespaced/);
  assert.throws(() => createHost({ packages: [packageDefinition('alpha', { assetPath: '/assets/app.js' })] }), /namespaced/);
  assert.throws(() => createHost({ packages: [packageDefinition('alpha', { routePath: '/api/manifest' })] }), /reserved/);
  assert.throws(() => createHost({ packages: [packageDefinition('alpha', { assetFile: '../entry.js' })] }), /Asset file/);
});
