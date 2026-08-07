import test from 'node:test';
import assert from 'node:assert/strict';
import { access, mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createMemberPackageScaffold, createMemberRepository, planMemberPackageScaffold, planMemberRepository } from './member-scaffold.ts';

const appRoot = fileURLToPath(new URL('../', import.meta.url));

test('plans a member repository and member package without touching a viewed repository', () => {
  const root = path.join('/tmp', 'resonance-member-repository');
  const repository = planMemberRepository({ directory: root });
  assert.deepEqual(repository.files.map((file) => path.relative(root, file)).sort(), ['.agents/skills/package-authoring/SKILL.md', '.gitignore', 'README.md', 'member-packages.json', 'package.json', 'src/package-contract.ts'].sort());
  const packagePlan = planMemberPackageScaffold({ memberRoot: root, id: 'personal-tools' });
  assert.match(packagePlan.manifestSnippet, /personal-tools/);
  assert.equal(path.relative(root, packagePlan.testFile), 'src/packages/personal-tools/personal-tools.test.ts');
});

test('initializes a self-contained member repository and copies authoring guidance', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-member-init-'));
  try {
    const plan = await createMemberRepository({ directory: root, appRoot, gitInitFn: async () => {} });
    assert.ok(plan.files.length > 0);
    assert.deepEqual(JSON.parse(await readFile(path.join(root, 'member-packages.json'), 'utf8')), { version: 1, packages: {} });
    assert.match(await readFile(path.join(root, '.agents/skills/package-authoring/SKILL.md'), 'utf8'), /member package/);
    assert.equal(await readFile(path.join(root, 'src/package-contract.ts'), 'utf8'), await readFile(path.join(appRoot, 'src/package-contract.ts'), 'utf8'));
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('creates and verifies a member package starter, rolling back failed starters', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-member-package-'));
  try {
    await createMemberRepository({ directory: root, appRoot, gitInitFn: async () => {} });
    const plan = await createMemberPackageScaffold({ memberRoot: root, id: 'personal-tools', verify: async (candidate) => {
      const child = Bun.spawn(['bun', 'test', candidate.testFile], { cwd: root, stdout: 'ignore', stderr: 'ignore' });
      assert.equal(await child.exited, 0);
    } });
    assert.ok(await access(plan.directory).then(() => true));
    await assert.rejects(() => createMemberPackageScaffold({ memberRoot: root, id: 'rollback-tools', verify: async () => { throw new Error('focused member test failed'); } }), /focused member test failed/);
    await assert.rejects(() => access(path.join(root, 'src/packages/rollback-tools')));
  } finally { await rm(root, { recursive: true, force: true }); }
});
