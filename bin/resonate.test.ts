import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, realpath, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { execFile } from 'node:child_process';
import { EventEmitter } from 'node:events';
import { Writable } from 'node:stream';
import { askToInstall, run, selectOptionalPackages } from './resonate';
import { promisify } from 'node:util';

const exec = promisify(execFile);
const projectPath = new URL('../', import.meta.url).pathname.replace(/\/$/, '');
const retiredTerm = new RegExp(['cock', 'pit'].join(''), 'i');

test('the CLI help does not expose retired product terminology', async () => {
  const cli = await readFile(new URL('resonate', import.meta.url), 'utf8');
  assert.doesNotMatch(cli, retiredTerm);
});

test('opens the browser at the actual server port after startup', async () => {
  const logs = []; const openedUrls = []; const fakeServer = { address: () => ({ port: 4318 }) };
  await run(['--port', '4317'], { root: '/tmp/example-repository', isInstalledFn: async () => true, startServerFn: async (options) => { assert.equal(options.root, '/tmp/example-repository'); assert.equal(options.port, 4317); assert.equal(options.config.version, 1); assert.equal(options.registry, undefined); return fakeServer; }, openBrowserFn: (url) => openedUrls.push(url), log: (message) => logs.push(message) });
  assert.deepEqual(openedUrls, ['http://127.0.0.1:4318']); assert.ok(logs.includes('http://127.0.0.1:4318'));
});

test('declining first-run installation leaves the repository untouched', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-cli-')); const logs = []; let started = false;
  const result = await run([], { root, confirmInstallFn: async () => false, startServerFn: async () => { started = true; return null; }, log: (message) => logs.push(message) });
  assert.equal(result, null); assert.equal(started, false); assert.match(logs.join('\\n'), /not installed/i); await assert.rejects(() => stat(path.join(root, '.resonance')));
});

test('first-run approval installs selected packages before starting', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-cli-')); const openedUrls = []; const fakeServer = { address: () => ({ port: 4318 }) };
  const server = await run([], { root, confirmInstallFn: async () => true, selectPackagesFn: async () => ({ home: true, docs: false }), startServerFn: async ({ config }) => { assert.deepEqual(Object.keys(config.packages), ['shell', 'home']); return fakeServer; }, openBrowserFn: (url) => openedUrls.push(url), log: () => {} });
  assert.equal(server, fakeServer); assert.deepEqual(openedUrls, ['http://127.0.0.1:4318']);
  const config = JSON.parse(await readFile(path.join(root, '.resonance/config.json'), 'utf8')); assert.deepEqual(Object.keys(config.packages), ['shell', 'home']); assert.equal(config.packages.home.source, 'README.md');
});

test('install subcommand creates Shell and selected optional packages without Pi Agent', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-cli-')); let started = false;
  const result = await run(['install'], { root, selectPackagesFn: async () => ({ home: false, docs: true }), startServerFn: async () => { started = true; return null; }, log: () => {} });
  assert.equal(result, null); assert.equal(started, false);
  const config = JSON.parse(await readFile(path.join(root, '.resonance/config.json'), 'utf8')); assert.deepEqual(Object.keys(config.packages), ['shell', 'docs']); assert.deepEqual(config.packages.docs.ignoredDirectories, ['.git', 'node_modules']); assert.equal(config.packages['pi-agent'], undefined);
});

test('install subcommand preserves an existing repository configuration', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-cli-'));
  await run(['install'], { root, selectPackagesFn: async () => ({ home: true, docs: false }), log: () => {} });
  await run(['install'], { root, selectPackagesFn: async () => { throw new Error('must not prompt'); }, log: () => {} });
  const config = JSON.parse(await readFile(path.join(root, '.resonance/config.json'), 'utf8')); assert.deepEqual(Object.keys(config.packages), ['shell', 'home']);
});

test('non-interactive package selection accepts Home and Docs answers', async () => {
  const input = new EventEmitter(); input.isTTY = false; const output = new Writable({ write(_chunk, _encoding, callback) { callback(); } });
  const selection = selectOptionalPackages({ input, output });
  input.emit('data', 'y\nn\n'); input.emit('end');
  assert.deepEqual(await selection, { home: true, docs: false });
});

test('interactive package selection exits on Escape and restores terminal input', async () => {
  const input = new EventEmitter(); input.isTTY = true; input.rawMode = false; input.setRawMode = (value) => { input.rawMode = value; }; const output = new Writable({ write(_chunk, _encoding, callback) { callback(); } }); output.isTTY = true;
  const selection = selectOptionalPackages({ input, output });
  input.emit('data', '\x1b');
  assert.equal(await selection, null); assert.equal(input.rawMode, false);
});

test('interactive package selection handles arrows, space, and Enter', async () => {
  const input = new EventEmitter(); input.isTTY = true; input.rawMode = false; input.setRawMode = (value) => { input.rawMode = value; }; const output = new Writable({ write(_chunk, _encoding, callback) { callback(); } }); output.isTTY = true;
  const selection = selectOptionalPackages({ input, output });
  input.emit('data', '\x1b[B \r');
  assert.deepEqual(await selection, { home: false, docs: true }); assert.equal(input.rawMode, false);
});

test('interactive package selection treats Ctrl+C and Ctrl+D as cancellation', async () => {
  for (const key of ['\x03', '\x04']) {
    const input = new EventEmitter(); input.isTTY = true; input.rawMode = false; input.setRawMode = (value) => { input.rawMode = value; }; const output = new Writable({ write(_chunk, _encoding, callback) { callback(); } }); output.isTTY = true;
    const selection = selectOptionalPackages({ input, output }); input.emit('data', key);
    assert.equal(await selection, null); assert.equal(input.rawMode, false);
  }
});

test('first-run confirmation resumes terminal input before package selection', async () => {
  const input = new (await import('node:stream')).PassThrough(); input.isTTY = true; let resumeCount = 0; const resume = input.resume.bind(input); input.resume = () => { resumeCount += 1; return resume(); };
  const output = new Writable({ write(_chunk, _encoding, callback) { callback(); } });
  const confirmation = askToInstall({ input, output }); input.end('y\n'); assert.equal(await confirmation, true); assert.ok(resumeCount > 1);
});

test('publishes the source package tree and global CLI entry', async () => {
  const packageJson = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8'));
  assert.equal(packageJson.bin.resonate, 'bin/resonate'); assert.deepEqual(packageJson.files, ['bin', 'src', 'scripts', 'README.md']); assert.equal(packageJson.scripts.test, 'bun test');
  const cli = await readFile(new URL('resonate', import.meta.url), 'utf8'); assert.match(cli, /^#!\/usr\/bin\/env bun/);
});

test('the local installer symlinks the checkout and adds its bin directory to PATH', async () => {
  const home = await mkdtemp(path.join(tmpdir(), 'resonance-home-')); const binDirectory = path.join(home, '.local', 'bin'); const shellConfig = path.join(home, '.zshrc');
  await exec('bash', [new URL('../scripts/install-local.sh', import.meta.url).pathname], { cwd: projectPath, env: { ...process.env, HOME: home, RESONANCE_BIN_DIR: binDirectory, RESONANCE_SHELL_RC: shellConfig } });
  assert.equal(await realpath(path.join(binDirectory, 'resonate')), await realpath(new URL('resonate', import.meta.url)));
  assert.match(await readFile(shellConfig, 'utf8'), new RegExp(`PATH=.*${binDirectory.replaceAll('/', '\\/')}`));
});
