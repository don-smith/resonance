import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { defaultRepositoryConfig, loadRepositoryConfig, validateRepositoryConfig } from '../src/config.ts';

test('uses version-one defaults with explicit built-in modules when config is absent', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'theview-config-'));
  const config = await loadRepositoryConfig(root);
  assert.deepEqual(config, defaultRepositoryConfig());
  assert.equal(config.packages.shell.module, 'src/packages/shell/index.ts');
  assert.equal(config.packages.home.source, 'README.md');
  assert.deepEqual(config.packages.docs.extensions, ['.md', '.markdown']);
});

test('loads package selections from .theview/config.json', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'theview-config-'));
  await mkdir(path.join(root, '.theview'));
  await writeFile(path.join(root, '.theview', 'config.json'), JSON.stringify({ version: 1, packages: { shell: { module: 'src/packages/shell/index.ts' }, home: { module: 'src/packages/home/index.ts', source: 'docs/index.md' }, docs: { module: 'src/packages/docs/index.ts', extensions: ['.markdown'] } } }));
  const config = await loadRepositoryConfig(root);
  assert.equal(config.packages.home.module, 'src/packages/home/index.ts');
  assert.equal(config.packages.home.source, 'docs/index.md');
  assert.deepEqual(config.packages.docs.extensions, ['.markdown']);
});

test('does not read the legacy manifest', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'theview-config-'));
  const legacyManifest = path.join(root, '.theview' + '.json');
  await writeFile(legacyManifest, JSON.stringify({ version: 1, packages: { home: { module: 'wrong.ts', source: 'wrong.md' } } }));
  const config = await loadRepositoryConfig(root);
  assert.equal(config.packages.home.module, 'src/packages/home/index.ts');
  assert.equal(config.packages.home.source, 'README.md');
});

test('validates manifest containers and enabled flags', () => {
  assert.throws(() => validateRepositoryConfig({ version: 2 }), /version must be 1/);
  assert.throws(() => validateRepositoryConfig({ version: 1, packages: [] }), /packages must be an object/);
  assert.throws(() => validateRepositoryConfig({ version: 1, packages: { docs: [] } }), /inputs must be an object/);
  assert.throws(() => validateRepositoryConfig({ version: 1, packages: { docs: { module: 'src/packages/docs/index.ts', enabled: 'yes' } } }), /enabled must be a boolean/);
  assert.equal(validateRepositoryConfig({ version: 1, packages: { custom: { module: 'src/packages/custom/index.ts', enabled: false } } }).packages.custom.enabled, false);
});

test('reports invalid JSON at the canonical filename', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'theview-config-'));
  await mkdir(path.join(root, '.theview'));
  await writeFile(path.join(root, '.theview', 'config.json'), '{');
  await assert.rejects(() => loadRepositoryConfig(root), /config\.json: manifest is not valid JSON/);
});
