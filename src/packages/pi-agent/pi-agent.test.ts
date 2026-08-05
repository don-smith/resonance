import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { createPiAgentPackage } from './index.ts';
import { createPiAgentSession, type AcpFactory } from './session.ts';
import { createHost } from '../../host.ts';
import { createApp } from '../../server.ts';
import { docsPackage } from '../docs/index.ts';

function fakeFactory(log: { cwd?: string; prompts: string[]; closes: number; update?: (value: any) => void; release?: () => void }): AcpFactory {
  return async ({ cwd, onUpdate }) => {
    log.cwd = cwd;
    log.update = onUpdate;
    return {
      sessionId: 'fake-session',
      async prompt(text) {
        log.prompts.push(text);
        await new Promise<void>((resolve) => { log.release = resolve; });
        onUpdate({ kind: 'assistant', text: 'answer' });
      },
      async cancel() {},
      async close() { log.closes += 1; },
    };
  };
}

test('anchors one session, suppresses tool activity, and rejects concurrent prompts', async () => {
  const log = { prompts: [] as string[], closes: 0 };
  const session = createPiAgentSession({ repositoryRoot: '/repo', adapterFactory: fakeFactory(log) });
  const events: any[] = [];
  session.subscribe((event) => events.push(event));
  await session.submitPrompt('hello');
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(log.cwd, '/repo');
  assert.equal(session.snapshot().status, 'working');
  await assert.rejects(() => session.submitPrompt('second'), /already running/);
  log.update!({ kind: 'activity', text: 'Edit file · pending' });
  log.update!({ kind: 'activity', text: 'Edit file · in_progress' });
  log.update!({ kind: 'activity', text: 'Edit file · completed' });
  assert.equal(events.some((event) => event.type === 'activity'), false);
  assert.equal(session.snapshot().messages.some((message) => message.role === 'activity'), false);
  assert.equal(events.some((event) => event.type === 'message' && event.message.content === 'answer'), false);
  log.release!();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.match(JSON.stringify(events), /answer/);
  assert.equal(session.snapshot().status, 'idle');
});

test('New Pi Agent session drops stale updates and allows a replacement after close', async () => {
  const updates: Array<(value: any) => void> = [];
  let closes = 0;
  let prompts = 0;
  const adapterFactory: AcpFactory = async ({ onUpdate }) => {
    updates.push(onUpdate);
    const id = String(++prompts);
    return { sessionId: id, prompt: async () => {}, cancel: async () => {}, close: async () => { closes += 1; } };
  };
  const session = createPiAgentSession({ repositoryRoot: '/repo', adapterFactory });
  await session.submitPrompt('first');
  await session.reset();
  updates[0]({ kind: 'assistant', text: 'stale' });
  assert.equal(session.snapshot().messages.length, 0);
  await session.submitPrompt('second');
  assert.equal(closes, 1);
  assert.equal(prompts, 2);
});

test('makes a fake Pi Markdown edit visible through Docs from the same repository root', async () => {
  const repositoryRoot = await mkdtemp(path.join(tmpdir(), 'resonance-pi-agent-'));
  await mkdir(path.join(repositoryRoot, 'docs'));
  await writeFile(path.join(repositoryRoot, 'README.md'), '# Repository');
  let closeCount = 0;
  const piAgent = createPiAgentPackage(async ({ cwd, onUpdate }) => ({
    sessionId: 'mutation',
    prompt: async () => {
      await writeFile(path.join(cwd, 'docs', 'from-pi.md'), '# Edited by Pi');
      onUpdate({ kind: 'assistant', text: 'updated docs' });
    },
    cancel: async () => {},
    close: async () => { closeCount += 1; },
  }));
  const registry = createHost({ root: repositoryRoot, config: { version: 1, packages: { docs: { module: 'src/packages/docs/index.ts' }, 'pi-agent': { module: 'src/packages/pi-agent/index.ts' } } }, packages: [docsPackage, piAgent] });
  const server = await createApp({ root: repositoryRoot, registry });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const baseUrl = `http://127.0.0.1:${server.address().port}`;
  try {
    const prompt = await fetch(`${baseUrl}/api/pi-agent/prompt`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ prompt: 'Update the docs' }) });
    assert.equal(prompt.status, 202);
    const tree = await fetch(`${baseUrl}/api/docs/tree`).then((response) => response.json());
    assert.ok(tree.documents.includes('docs/from-pi.md'));
    const document = await fetch(`${baseUrl}/api/docs/document?path=docs%2Ffrom-pi.md`).then((response) => response.json());
    assert.match(document.html, /Edited by Pi/);
  } finally {
    await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  }
  assert.equal(closeCount, 1);
});

test('registers default Pi Agent routes, assets, navigation, and disposal', async () => {
  const definition = createPiAgentPackage(async () => ({ sessionId: 'test', prompt: async () => {}, cancel: async () => {}, close: async () => {} }));
  const registry = createHost({ config: { version: 1, packages: { 'pi-agent': { module: 'src/packages/pi-agent/index.ts' } } }, packages: [definition] });
  assert.deepEqual(registry.manifest.navigation, [{ id: 'pi-agent', label: 'Pi Agent', order: 30 }]);
  assert.ok(registry.routes['GET /api/pi-agent/state']);
  assert.ok(registry.routes['GET /api/pi-agent/events']);
  assert.ok(registry.routes['POST /api/pi-agent/prompt']);
  assert.ok(registry.routes['POST /api/pi-agent/reset']);
  assert.equal(registry.manifest.packages[0].id, 'pi-agent');
  assert.equal(registry.manifest.packages[0].entry, '/assets/pi-agent/pi-agent.js');
  assert.equal(registry.manifest.packages[0].stylesheet, '/assets/pi-agent/pi-agent.css');
  await registry.dispose();
});
