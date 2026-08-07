import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, realpath, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { execFile } from 'node:child_process';
import { EventEmitter } from 'node:events';
import { Writable } from 'node:stream';
import { askToInstall, parseArgs, run, runFocusedPackageTest, selectMemberPackages, selectOptionalPackages } from './resonate';
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

test('install subcommand creates Shell and selected optional packages only', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-cli-')); let started = false;
  const result = await run(['install'], { root, selectPackagesFn: async () => ({ home: false, docs: true }), startServerFn: async () => { started = true; return null; }, log: () => {} });
  assert.equal(result, null); assert.equal(started, false);
  const config = JSON.parse(await readFile(path.join(root, '.resonance/config.json'), 'utf8')); assert.deepEqual(Object.keys(config.packages), ['shell', 'docs']); assert.deepEqual(config.packages.docs.ignoredDirectories, ['.git', 'node_modules']);
});

test('install subcommand preserves an existing repository configuration', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-cli-'));
  await run(['install'], { root, selectPackagesFn: async () => ({ home: true, docs: false }), log: () => {} });
  await run(['install'], { root, selectPackagesFn: async () => { throw new Error('must not prompt'); }, log: () => {} });
  const config = JSON.parse(await readFile(path.join(root, '.resonance/config.json'), 'utf8')); assert.deepEqual(Object.keys(config.packages), ['shell', 'home']);
});

test('member install records the external source and narrow ignored state paths', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-member-cli-')); const source = await mkdtemp(path.join(tmpdir(), 'resonance-member-source-'));
  try {
    await writeFile(path.join(source, 'member-packages.json'), JSON.stringify({ version: 1, packages: { personal: { module: 'src/packages/personal/index.ts' } } }));
    await run(['member', 'install', source], { root, selectMemberPackagesFn: async ({ manifest }) => { assert.ok(manifest.packages.personal); return { personal: {} }; }, log: () => {} });
    const config = JSON.parse(await readFile(path.join(root, '.resonance/member-config.json'), 'utf8'));
    assert.equal(config.source, await realpath(source)); assert.deepEqual(config.packages, { personal: {} });
    assert.match(await readFile(path.join(root, '.gitignore'), 'utf8'), /\.resonance\/member-config\.json/); assert.match(await readFile(path.join(root, '.gitignore'), 'utf8'), /\.resonance\/member-state\//);
    assert.deepEqual(parseArgs(['member', 'install', source]), { command: 'member-install', packageId: null, memberSource: source, port: 4317, host: '127.0.0.1' });
  } finally { await rm(root, { recursive: true, force: true }); await rm(source, { recursive: true, force: true }); }
});

test('member init and member package create dispatch without viewed-repository startup', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-member-cli-')); const target = path.join(root, 'member-packages'); const logs = [];
  try {
    let initialized = false;
    await run(['member', 'init', target], {
      root, log: (message) => logs.push(message),
      planMemberRepositoryFn: ({ directory }) => ({ directory, files: [path.join(directory, 'member-packages.json')] }),
      createMemberRepositoryFn: async ({ directory }) => { initialized = directory === target; return { directory, files: [] }; },
      isInstalledFn: async () => { throw new Error('must not inspect viewed repository installation'); },
    });
    assert.equal(initialized, true);
    let created = false;
    await run(['member', 'package', 'create', 'personal-tools'], {
      root, log: () => {},
      planMemberPackageScaffoldFn: ({ memberRoot, id }) => ({ id, files: [path.join(memberRoot, 'src/packages', id, 'index.ts')], testFile: path.join(memberRoot, 'src/packages', id, `${id}.test.ts`) }),
      createMemberPackageScaffoldFn: async ({ memberRoot, id, verify }) => { created = memberRoot === root && id === 'personal-tools'; await verify({ testFile: path.join(root, 'member.test.ts') }); return { id, manifestSnippet: '"personal-tools": {}' }; },
      runFocusedTestFn: async (plan, { appRoot }) => { assert.equal(plan.testFile, path.join(root, 'member.test.ts')); assert.equal(appRoot, root); },
      isInstalledFn: async () => { throw new Error('must not inspect viewed repository installation'); },
    });
    assert.equal(created, true);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('interactive member package selection completes on Enter and cancels on control keys', async () => {
  const manifest = { version: 1, packages: { personal: { module: 'src/packages/personal/index.ts' } } };
  const input = new EventEmitter(); input.isTTY = true; input.rawMode = false; input.setRawMode = (value) => { input.rawMode = value; }; const output = new Writable({ write(_chunk, _encoding, callback) { callback(); } }); output.isTTY = true;
  const selection = selectMemberPackages({ manifest, input, output }); input.emit('data', ' \r');
  assert.deepEqual(await selection, { personal: {} }); assert.equal(input.rawMode, false);
  for (const key of ['\x03', '\x04']) {
    const cancelInput = new EventEmitter(); cancelInput.isTTY = true; cancelInput.rawMode = false; cancelInput.setRawMode = (value) => { cancelInput.rawMode = value; }; const cancelOutput = new Writable({ write(_chunk, _encoding, callback) { callback(); } }); cancelOutput.isTTY = true;
    const cancelled = selectMemberPackages({ manifest, input: cancelInput, output: cancelOutput }); cancelInput.emit('data', key);
    assert.equal(await cancelled, null); assert.equal(cancelInput.rawMode, false);
  }
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

test('parses the nested package create command and rejects malformed package commands', () => {
  assert.deepEqual(parseArgs(['package', 'create', 'reading-queue']), { command: 'package-create', packageId: 'reading-queue', port: 4317, host: '127.0.0.1' });
  for (const argumentsList of [['package'], ['package', 'remove', 'reading-queue'], ['package', 'create'], ['package', 'create', '--help'], ['package', 'create', 'reading-queue', 'extra']]) {
    assert.throws(() => parseArgs(argumentsList), /package command|Package create requires|Unexpected argument/i);
  }
});

test('runs only the generated package test and reports a non-zero exit', async () => {
  const plan = { testFile: '/tmp/reading-queue.test.ts' };
  const calls = [];
  await runFocusedPackageTest(plan, { appRoot: '/tmp/resonance', spawnFn: (command, options) => { calls.push({ command, options }); return { exited: Promise.resolve(0) }; } });
  assert.deepEqual(calls, [{ command: ['bun', 'test', plan.testFile], options: { cwd: '/tmp/resonance', stdout: 'inherit', stderr: 'inherit' } }]);
  await assert.rejects(() => runFocusedPackageTest(plan, { spawnFn: () => ({ exited: Promise.resolve(1) }) }), /Focused package test failed with exit code 1/);
});

test('package create dispatches before repository installation, config, server, and browser side effects', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-cli-repository-'));
  const appRoot = await mkdtemp(path.join(tmpdir(), 'resonance-cli-app-'));
  const logs = [];
  const plan = {
    id: 'reading-queue',
    directory: path.join(appRoot, 'src', 'packages', 'reading-queue'),
    files: [path.join(appRoot, 'src', 'packages', 'reading-queue', 'README.md'), path.join(appRoot, 'src', 'packages', 'reading-queue', 'index.ts')],
    manifestSnippet: '"reading-queue": { "module": "src/packages/reading-queue/index.ts" }',
    testFile: path.join(appRoot, 'src', 'packages', 'reading-queue', 'reading-queue.test.ts'),
  };
  let created = false;
  let verified = false;
  try {
    const result = await run(['package', 'create', plan.id], {
      root, appRoot, log: (message) => logs.push(message),
      isInstalledFn: async () => { throw new Error('must not check repository installation'); },
      confirmInstallFn: async () => { throw new Error('must not prompt for installation'); },
      selectPackagesFn: async () => { throw new Error('must not select packages'); },
      loadConfigFn: async () => { throw new Error('must not load repository configuration'); },
      startServerFn: async () => { throw new Error('must not start a server'); },
      openBrowserFn: () => { throw new Error('must not open a browser'); },
      planPackageScaffoldFn: ({ appRoot: receivedAppRoot, id }) => { assert.equal(receivedAppRoot, appRoot); assert.equal(id, plan.id); return plan; },
      createPackageScaffoldFn: async ({ appRoot: receivedAppRoot, id, verify }) => { assert.equal(receivedAppRoot, appRoot); assert.equal(id, plan.id); created = true; await verify(plan); return plan; },
      runFocusedTestFn: async (receivedPlan, { appRoot: receivedAppRoot }) => { assert.equal(receivedPlan, plan); assert.equal(receivedAppRoot, appRoot); verified = true; },
    });
    assert.equal(result, null);
    assert.equal(created, true);
    assert.equal(verified, true);
    await assert.rejects(() => stat(path.join(root, '.resonance')), { code: 'ENOENT' });
    assert.deepEqual(logs, ['Package files to create for reading-queue:', '  src/packages/reading-queue/README.md', '  src/packages/reading-queue/index.ts', 'Created package reading-queue.', 'Focused package test passed.', 'Add this explicit entry to the viewed repository manifest:', plan.manifestSnippet, 'Full regression: bun test']);
  } finally {
    await rm(root, { recursive: true, force: true });
    await rm(appRoot, { recursive: true, force: true });
  }
});

test('publishes the source package tree, canonical authoring skill, and global CLI entry', async () => {
  const packageJson = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8'));
  assert.equal(packageJson.bin.resonate, 'bin/resonate'); assert.deepEqual(packageJson.files, ['bin', 'src', 'scripts', '.agents', 'README.md']); assert.equal(packageJson.scripts.test, 'bun test');
  const cli = await readFile(new URL('resonate', import.meta.url), 'utf8'); assert.match(cli, /^#!\/usr\/bin\/env bun/);
  const skill = await readFile(new URL('../.agents/skills/package-authoring/SKILL.md', import.meta.url), 'utf8');
  assert.match(skill, /^---\nname: package-authoring\n/); assert.match(skill, /## Capture the Brief First/); assert.match(skill, /Purpose/); assert.match(skill, /Data ownership/); assert.match(skill, /Route, configuration, and UI/); assert.match(skill, /Risks/); assert.match(skill, /resonate package create <lowercase-kebab-id>/); assert.match(skill, /--skill <path>/);
});

test('publishes canonical Backlog maintenance guidance', async () => {
  const packageJson = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8'));
  assert.ok(packageJson.files.includes('.agents'));
  const skill = await readFile(new URL('../.agents/skills/backlog/SKILL.md', import.meta.url), 'utf8');
  assert.match(skill, /^---\nname: backlog\n/);
  assert.match(skill, /backlog\/todo\.yaml/);
  assert.match(skill, /recently-done/);
  assert.match(skill, /P0–P3/);
  assert.match(skill, /bun test src\/packages\/backlog\/backlog\.test\.ts/);
});

test('the local installer symlinks the checkout and adds its bin directory to PATH', async () => {
  const home = await mkdtemp(path.join(tmpdir(), 'resonance-home-')); const binDirectory = path.join(home, '.local', 'bin'); const shellConfig = path.join(home, '.zshrc');
  await exec('bash', [new URL('../scripts/install-local.sh', import.meta.url).pathname], { cwd: projectPath, env: { ...process.env, HOME: home, RESONANCE_BIN_DIR: binDirectory, RESONANCE_SHELL_RC: shellConfig } });
  assert.equal(await realpath(path.join(binDirectory, 'resonate')), await realpath(new URL('resonate', import.meta.url)));
  assert.match(await readFile(shellConfig, 'utf8'), new RegExp(`PATH=.*${binDirectory.replaceAll('/', '\\/')}`));
});
