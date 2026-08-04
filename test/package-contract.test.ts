import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { defaultRepositoryConfig, loadRepositoryConfig, validateRepositoryConfig } from '../src/config.ts';

test('uses version-one defaults when a repository has no manifest', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'theview-config-'));
  const config = await loadRepositoryConfig(root);
  assert.deepEqual(config, defaultRepositoryConfig());
  assert.equal(config.version, 1);
  assert.equal(config.packages.home.source, 'README.md');
  assert.deepEqual(config.packages.docs, {
    extensions: ['.md', '.markdown'],
    ignoredDirectories: ['.git', 'node_modules'],
  });
});

test('loads package inputs from a repository manifest', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'theview-config-'));
  await writeFile(path.join(root, '.theview.json'), JSON.stringify({ version: 1, packages: { home: { source: 'docs/index.md' }, docs: { extensions: ['.markdown'] } } }));
  const config = await loadRepositoryConfig(root);
  assert.equal(config.version, 1);
  assert.equal(config.packages.home.source, 'docs/index.md');
  assert.deepEqual(config.packages.docs.extensions, ['.markdown']);
});

test('supplies omitted home and docs package inputs', () => {
  assert.deepEqual(validateRepositoryConfig({ version: 1, packages: { custom: { enabled: true } } }).packages, {
    custom: { enabled: true },
    home: { source: 'README.md' },
    docs: { extensions: ['.md', '.markdown'], ignoredDirectories: ['.git', 'node_modules'] },
  });
});

test('rejects unsupported versions and non-object package inputs', () => {
  assert.throws(() => validateRepositoryConfig({ version: 2 }), /version must be 1/);
  assert.throws(() => validateRepositoryConfig({ version: 1, packages: [] }), /packages must be an object/);
  assert.throws(() => validateRepositoryConfig({ version: 1, packages: { docs: [] } }), /inputs must be an object/);
});

test('loads the fixture manifest and selects home.md', async () => {
  const fixture = new URL('./fixtures/repository/', import.meta.url);
  const config = await loadRepositoryConfig(fixture);
  assert.equal(config.version, 1);
  assert.equal(config.packages.home.source, 'home.md');
});
