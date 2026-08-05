import test from 'node:test';
import assert from 'node:assert/strict';
import { access, mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { createRepositoryConfig, defaultRepositoryConfig, loadRepositoryConfig, validateRepositoryConfig, writeRepositoryConfig } from './config.ts';

test('does not create a config while loading an uninstalled repository', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-config-'));
  await assert.rejects(() => loadRepositoryConfig(root), (error) => error?.code === 'ENOENT');
  await assert.rejects(() => access(path.join(root, '.resonance/config.json')));
});

test('builds install config with Shell and selected optional packages only', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-config-'));
  const config = createRepositoryConfig({ home: true, docs: false });
  assert.deepEqual(Object.keys(config.packages), ['shell', 'home']);
  assert.equal(config.packages.home.source, 'README.md');
  assert.equal(config.packages.docs, undefined);
  assert.equal(config.packages['pi-agent'], undefined);
  await writeRepositoryConfig(root, config);
  assert.deepEqual(JSON.parse(await readFile(path.join(root, '.resonance/config.json'), 'utf8')), config);
  assert.deepEqual(await loadRepositoryConfig(root), config);
});

test('default install config contains the required Shell package', () => {
  assert.deepEqual(Object.keys(defaultRepositoryConfig().packages), ['shell']);
});

test('loads package selections from .resonance/config.json', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-config-'));
  await mkdir(path.join(root, '.resonance'));
  await writeFile(path.join(root, '.resonance', 'config.json'), JSON.stringify({ version: 1, packages: { shell: { module: 'src/packages/shell/index.ts' }, home: { module: 'src/packages/home/index.ts', source: 'docs/index.md' }, docs: { module: 'src/packages/docs/index.ts', extensions: ['.markdown'] } } }));
  const config = await loadRepositoryConfig(root);
  assert.equal(config.packages.home.module, 'src/packages/home/index.ts');
  assert.equal(config.packages.home.source, 'docs/index.md');
  assert.deepEqual(config.packages.docs.extensions, ['.markdown']);
  assert.deepEqual(Object.keys(config.packages), ['shell', 'home', 'docs']);
  assert.equal(config.packages['pi-agent'], undefined);
});

test('does not read or create a config from the legacy manifest', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-config-'));
  const legacyManifest = path.join(root, '.resonance' + '.json');
  await writeFile(legacyManifest, JSON.stringify({ version: 1, packages: { home: { module: 'wrong.ts', source: 'wrong.md' } } }));
  await assert.rejects(() => loadRepositoryConfig(root), (error) => error?.code === 'ENOENT');
  await assert.rejects(() => access(path.join(root, '.resonance/config.json')));
});

test('validates manifest containers and enabled flags', () => {
  assert.throws(() => validateRepositoryConfig({ version: 2 }), /version must be 1/);
  assert.throws(() => validateRepositoryConfig({ version: 1 }), /packages must be an object/);
  assert.throws(() => validateRepositoryConfig({ version: 1, packages: [] }), /packages must be an object/);
  assert.throws(() => validateRepositoryConfig({ version: 1, packages: { docs: [] } }), /inputs must be an object/);
  assert.throws(() => validateRepositoryConfig({ version: 1, packages: { docs: { module: 'src/packages/docs/index.ts', enabled: 'yes' } } }), /enabled must be a boolean/);
  assert.equal(validateRepositoryConfig({ version: 1, packages: { custom: { module: 'src/packages/custom/index.ts', enabled: false } } }).packages.custom.enabled, false);
});

test('reports invalid JSON at the canonical filename', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-config-'));
  await mkdir(path.join(root, '.resonance'));
  await writeFile(path.join(root, '.resonance', 'config.json'), '{');
  await assert.rejects(() => loadRepositoryConfig(root), /config\.json: manifest is not valid JSON/);
});
