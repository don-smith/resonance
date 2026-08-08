import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readFile, realpath, rename, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { createBacklogStore, parseBacklogItems, planForTitle } from './backlog-store.ts';

const source = (decisions: string) => `version: 1\ndecisions:\n${decisions}`;
const decision = (title: string, plan: string, status = 'in-planning', priority = 'P2') => `  - title: ${title}\n    plan: ${plan}\n    status: ${status}\n    priority: ${priority}\n`;
async function fixture() {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-backlog-store-'));
  await mkdir(path.join(root, 'backlog', 'plans'), { recursive: true });
  await writeFile(path.join(root, 'backlog', 'todo.yaml'), source(decision('Queue', 'plans/queue.md', 'in-planning', 'P2') + decision('Ready', 'plans/ready.md', 'is-ready', 'P1')));
  await writeFile(path.join(root, 'backlog', 'plans/queue.md'), '# Queue');
  await writeFile(path.join(root, 'backlog', 'plans/ready.md'), '# Ready');
  return root;
}
const cleanup = (root: string) => rm(root, { recursive: true, force: true });

test('parses strict source and derives stable plan names from titles', () => {
  assert.deepEqual(parseBacklogItems(source(decision('Queue', 'plans/queue.md'))), [{ title: 'Queue', plan: 'plans/queue.md', status: 'in-planning', priority: 'P2' }]);
  assert.equal(planForTitle('Backlog Agent & Skill'), 'plans/backlog-agent-skill.md');
  assert.equal(planForTitle('  Café: v2  '), 'plans/cafe-v2.md');
  assert.throws(() => planForTitle('!!!'), /letter or number/);
  for (const invalid of [source(decision('A', 'plans/same.md') + decision('B', 'plans/same.md')), 'version: 1\ndecisions: [', `${source(decision('Queue', 'plans/queue.md'))}unknown: true\n`]) {
    assert.throws(() => parseBacklogItems(invalid), /Backlog source is invalid/);
  }
});

test('lists authorized decisions and reads only contained linked plans', async () => {
  const root = await fixture(); const outside = await mkdtemp(path.join(tmpdir(), 'resonance-backlog-store-outside-'));
  try {
    await writeFile(path.join(outside, 'external.md'), '# External');
    await writeFile(path.join(root, 'backlog', 'todo.yaml'), source(decision('Queue', 'plans/queue.md') + decision('External', 'plans/external.md')));
    await symlink(path.join(outside, 'external.md'), path.join(root, 'backlog', 'plans', 'external.md'));
    const store = createBacklogStore({ repositoryRoot: root });
    assert.deepEqual((await store.listDecisions()).map((item) => item.title), ['Queue']);
    await assert.rejects(() => store.readDecision('backlog/plans/external.md'), /not found/);
  } finally { await cleanup(root); await cleanup(outside); }
});

test('creates, edits, updates, and deletes a paired decision', async () => {
  const root = await fixture(); const store = createBacklogStore({ repositoryRoot: root });
  try {
    assert.deepEqual(await store.createDecision({ title: 'New', status: 'in-planning', priority: 'P3', markdown: '# New' }), { affectedPaths: ['backlog/todo.yaml', 'backlog/plans/new.md'] });
    await store.editPlan('backlog/plans/new.md', '# Updated');
    await store.setStatus('backlog/plans/new.md', 'is-ready');
    await store.setPriority('backlog/plans/new.md', 'P1');
    assert.deepEqual(await store.readDecision('backlog/plans/new.md'), { path: 'backlog/plans/new.md', title: 'New', status: 'is-ready', priority: 'P1', markdown: '# Updated' });
    assert.deepEqual(await store.deleteDecision('backlog/plans/new.md'), { affectedPaths: ['backlog/todo.yaml', 'backlog/plans/new.md'] });
    await assert.rejects(() => store.readDecision('backlog/plans/new.md'), /not found/);
  } finally { await cleanup(root); }
});

test('updates decision metadata together and preserves the paired plan', async () => {
  const root = await fixture(); const store = createBacklogStore({ repositoryRoot: root });
  try {
    assert.deepEqual(await store.updateMetadata('backlog/plans/queue.md', { status: 'is-ready', priority: 'P0' }), { affectedPaths: ['backlog/todo.yaml'] });
    const updated = await store.readDecision('backlog/plans/queue.md');
    assert.equal(updated.status, 'is-ready'); assert.equal(updated.priority, 'P0'); assert.equal(updated.markdown, '# Queue');
    await assert.rejects(() => store.updateMetadata('backlog/plans/queue.md', { status: 'blocked' as never }), /invalid/);
  } finally { await cleanup(root); }
});

test('rejects prospective symlink parents and serializes concurrent mutations', async () => {
  const root = await fixture(); const outside = await mkdtemp(path.join(tmpdir(), 'resonance-backlog-store-outside-'));
  try {
    await rm(path.join(root, 'backlog', 'plans'), { recursive: true });
    await symlink(outside, path.join(root, 'backlog', 'plans'));
    const store = createBacklogStore({ repositoryRoot: root });
    await assert.rejects(() => store.createDecision({ title: 'Escape', status: 'in-planning', priority: 'P3', markdown: '# Escape' }), /escapes/);
    await rm(path.join(root, 'backlog', 'plans'), { recursive: true });
    await mkdir(path.join(root, 'backlog', 'plans'), { recursive: true });
    await writeFile(path.join(root, 'backlog', 'plans', 'queue.md'), '# Queue');
    await writeFile(path.join(root, 'backlog', 'plans', 'ready.md'), '# Ready');
    await Promise.all([store.setStatus('backlog/plans/queue.md', 'is-ready'), store.setPriority('backlog/plans/queue.md', 'P0')]);
    const updated = await store.readDecision('backlog/plans/queue.md');
    assert.equal(updated.status, 'is-ready'); assert.equal(updated.priority, 'P0');
  } finally { await cleanup(root); await cleanup(outside); }
});

async function failingStore(root: string, failTarget: string) {
  let failed = false;
  return createBacklogStore({ repositoryRoot: root, fileSystem: { rename: async (from, to) => { if (!failed && String(to).endsWith(failTarget)) { failed = true; throw new Error('injected rename failure'); } return rename(from, to); } } });
}

test('compensates failed create and delete paired replacements', async () => {
  const root = await fixture();
  try {
    const createStore = await failingStore(root, path.join('backlog', 'todo.yaml'));
    await assert.rejects(() => createStore.createDecision({ title: 'Rollback', status: 'in-planning', priority: 'P3', markdown: '# Rollback' }), /compensated/);
    await assert.rejects(() => realpath(path.join(root, 'backlog', 'plans', 'rollback.md')));
    const deleteStore = await failingStore(root, path.join('backlog', 'todo.yaml'));
    await assert.rejects(() => deleteStore.deleteDecision('backlog/plans/queue.md'), /compensated/);
    assert.equal(await readFile(path.join(root, 'backlog', 'plans', 'queue.md'), 'utf8'), '# Queue');
  } finally { await cleanup(root); }
});
