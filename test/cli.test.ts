import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, realpath } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { execFile } from 'node:child_process';
import { run } from '../bin/theview';
import { promisify } from 'node:util';

const exec = promisify(execFile);
const projectPath = new URL('../', import.meta.url).pathname.replace(/\/$/, '');

test('opens the browser at the actual server port after startup', async () => {
  const logs = []; const openedUrls = []; const fakeServer = { address: () => ({ port: 4318 }) };
  await run(['--port', '4317'], { root: '/tmp/example-repository', startServerFn: async (options) => { assert.equal(options.root, '/tmp/example-repository'); assert.equal(options.port, 4317); assert.equal(options.config.version, 1); assert.equal(options.registry, undefined); return fakeServer; }, openBrowserFn: (url) => openedUrls.push(url), log: (message) => logs.push(message) });
  assert.deepEqual(openedUrls, ['http://127.0.0.1:4318']); assert.ok(logs.includes('http://127.0.0.1:4318'));
});

test('publishes the source package tree and global CLI entry', async () => {
  const packageJson = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8'));
  assert.equal(packageJson.bin.theview, 'bin/theview'); assert.deepEqual(packageJson.files, ['bin', 'src', 'scripts', 'README.md']); assert.equal(packageJson.scripts.test, 'bun test');
  const cli = await readFile(new URL('../bin/theview', import.meta.url), 'utf8'); assert.match(cli, /^#!\/usr\/bin\/env bun/);
});

test('the local installer symlinks the checkout and adds its bin directory to PATH', async () => {
  const home = await mkdtemp(path.join(tmpdir(), 'theview-home-')); const binDirectory = path.join(home, '.local', 'bin'); const shellConfig = path.join(home, '.zshrc');
  await exec('bash', [new URL('../scripts/install-local.sh', import.meta.url).pathname], { cwd: projectPath, env: { ...process.env, HOME: home, THEVIEW_BIN_DIR: binDirectory, THEVIEW_SHELL_RC: shellConfig } });
  assert.equal(await realpath(path.join(binDirectory, 'theview')), await realpath(new URL('../bin/theview', import.meta.url)));
  assert.match(await readFile(shellConfig, 'utf8'), new RegExp(`PATH=.*${binDirectory.replaceAll('/', '\\/')}`));
});
