import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { createApp } from './server.ts';
import { writeMemberConfig, validateMemberConfig, validateMemberManifest } from './member.ts';

async function withServer(root: string, run: (base: string) => Promise<void>) {
  const server = await createApp({ root, config: { version: 1, packages: { shell: { module: 'src/packages/shell/index.ts' } } } });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  try { await run(`http://127.0.0.1:${server.address().port}`); }
  finally { await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve())); }
}

test('validates member manifests and keeps module paths out of local selection', () => {
  assert.deepEqual(validateMemberManifest({ version: 1, packages: { personal: { module: 'src/packages/personal/index.ts' } } }).packages.personal.module, 'src/packages/personal/index.ts');
  assert.throws(() => validateMemberConfig({ version: 1, source: '/tmp/member', packages: { personal: { module: 'wrong.ts' } } }), /must not duplicate/);
});

test('loads a live external member package with member navigation and state', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-member-view-'));
  const memberRoot = await mkdtemp(path.join(tmpdir(), 'resonance-member-source-'));
  try {
    await mkdir(path.join(memberRoot, 'src'));
    await writeFile(path.join(memberRoot, 'member-packages.json'), JSON.stringify({ version: 1, packages: { personal: { module: 'src/personal.ts' } } }));
    await writeFile(path.join(memberRoot, 'src', 'personal.ts'), `const metadata = { id: 'personal', version: '1.0.0', hostVersion: '1', label: 'Personal', order: 1 }; export default { metadata, register(context) { return { metadata, routes: [{ method: 'GET', path: '/api/personal/state', handler: async (_request, response, routeContext) => { await routeContext.state.write({ ready: true }); response.json(200, { appRoot: routeContext.appRoot, state: await routeContext.state.read() }); } }], assets: [{ path: '/assets/personal/personal.js', file: 'src/personal.js', contentType: 'text/javascript' }], navigation: [{ id: 'personal', label: 'Personal', order: 1 }], browser: { id: 'personal', entry: '/assets/personal/personal.js', stylesheet: '/assets/personal/personal.js' } }; } };`);
    await writeFile(path.join(memberRoot, 'src', 'personal.js'), 'export default function personal() {}');
    await writeMemberConfig(root, { version: 1, source: memberRoot, packages: { personal: {} } });
    await withServer(root, async (base) => {
      const manifest = await fetch(`${base}/api/manifest`).then((response) => response.json());
      assert.deepEqual(manifest.navigation, [{ id: 'personal', label: 'Personal', order: 1, scope: 'member' }]);
      const state = await fetch(`${base}/api/personal/state`).then((response) => response.json());
      assert.equal(state.appRoot, memberRoot);
      assert.deepEqual(state.state, { ready: true });
      assert.equal((await fetch(`${base}/assets/personal/personal.js`)).status, 200);
    });
    assert.deepEqual(JSON.parse(await readFile(path.join(root, '.resonance', 'member-state', 'personal', 'state.json'), 'utf8')), { ready: true });
  } finally { await rm(root, { recursive: true, force: true }); await rm(memberRoot, { recursive: true, force: true }); }
});
