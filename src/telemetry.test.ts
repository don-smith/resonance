import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { createTelemetry } from './telemetry.ts';

function fakeConsole() {
  const calls: Array<{ level: string; values: unknown[] }> = [];
  const record = (level: string) => (...values: unknown[]) => calls.push({ level, values });
  return { calls, console: { debug: record('debug'), info: record('info'), warn: record('warn'), error: record('error') } };
}

test('logs structured child fields, filters levels, and redacts secrets and content', async () => {
  const output = fakeConsole();
  const telemetry = createTelemetry({ root: '/workspace/resonance', config: { mode: 'console', level: 'info' }, console: output.console });
  const child = telemetry.child({ package: 'backlog', apiKey: 'secret' });
  child.debug('hidden');
  child.info('started', { prompt: 'private', requestId: 'request-1' });
  assert.deepEqual(output.calls, [{ level: 'info', values: ['started', { repository: 'resonance', package: 'backlog', apiKey: '[REDACTED]', prompt: '[CONTENT REDACTED]', requestId: 'request-1' }] }]);
  await telemetry.dispose();
});

test('uses the Git origin name for repository telemetry metadata', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-telemetry-'));
  await mkdir(path.join(root, '.git'));
  await writeFile(path.join(root, '.git', 'config'), '[remote "origin"]\n\turl = git@github.com:example/repository.git\n');
  const records: any[] = [];
  const telemetry = createTelemetry({ root, config: { mode: 'console' }, console: null, exporter: { record(record) { records.push(record); }, async flush() {} } });
  telemetry.info('repository identified');
  assert.equal(records[0].fields.repository, 'repository');
  await telemetry.dispose();
});

test('records completed and failed spans through the exporter seam', async () => {
  const records: any[] = [];
  let flushes = 0;
  const telemetry = createTelemetry({ config: { mode: 'console', level: 'error' }, console: null, exporter: { record(record) { records.push(record); }, async flush() { flushes += 1; } } });
  const completed = telemetry.span('complete', { package: 'docs' });
  completed.event('checkpoint', { count: 2 });
  completed.end({ status: 200 });
  const failed = telemetry.span('failed');
  failed.fail(new Error('provider returned sk-or-v1-secret-value'), { status: 500 });
  failed.end();
  assert.equal(records.filter((record) => record.kind === 'event').length, 1);
  assert.equal(records.filter((record) => record.kind === 'span').length, 2);
  assert.equal(records.find((record) => record.name === 'failed').error.message, 'provider returned [REDACTED]');
  await telemetry.dispose();
  await telemetry.dispose();
  assert.equal(flushes, 1);
});

test('flushes Langfuse OpenTelemetry traces with process credentials, sessions, and no content by default', async () => {
  const requests: Array<{ input: string; init: RequestInit }> = [];
  const telemetry = createTelemetry({
    root: '/workspace/project-a',
    config: { mode: 'langfuse', level: 'info', baseUrl: 'http://127.0.0.1:13000', publicKey: 'public', secretKey: 'private' },
    console: null,
    fetchFn: async (input, init) => { requests.push({ input, init: init || {} }); return new Response('{}', { status: 200 }); },
  });
  const session = telemetry.session('agent-session-1', { agent: 'backlog' });
  session.info('request complete', { requestId: 'r1', output: 'not sent' });
  const span = session.span('agent turn'); span.end({ status: 'ok' });
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(requests.length, 1);
  assert.equal(requests[0].input, 'http://127.0.0.1:13000/api/public/otel/v1/traces');
  assert.equal(requests[0].init.headers && (requests[0].init.headers as Record<string, string>).authorization, `Basic ${Buffer.from('public:private').toString('base64')}`);
  assert.equal((requests[0].init.headers as Record<string, string>)['x-langfuse-ingestion-version'], '4');
  const body = JSON.parse(String(requests[0].init.body));
  const resourceAttributes = body.resourceSpans[0].resource.attributes;
  assert.equal(resourceAttributes.find((item: any) => item.key === 'repository').value.stringValue, 'project-a');
  const spans = body.resourceSpans[0].scopeSpans[0].spans;
  assert.equal(spans.find((item: any) => item.name === 'agent turn').attributes.find((item: any) => item.key === 'repository').value.stringValue, 'project-a');
  assert.equal(spans.find((item: any) => item.name === 'agent turn').attributes.find((item: any) => item.key === 'langfuse.session.id').value.stringValue, 'agent-session-1');
  assert.ok(spans.some((item: any) => item.name === 'agent turn'));
  assert.ok(spans.some((item: any) => item.name === 'log:request complete'));
  assert.equal(spans.find((item: any) => item.name === 'log:request complete').attributes.find((item: any) => item.key === 'langfuse.observation.output').value.stringValue, '[CONTENT REDACTED]');
  await telemetry.dispose();
});

test('maps captured model input and output to Langfuse observation fields', async () => {
  const requests: RequestInit[] = [];
  const telemetry = createTelemetry({
    config: { mode: 'langfuse', publicKey: 'public', secretKey: 'private', captureContent: true },
    console: null,
    fetchFn: async (_input, init) => { requests.push(init || {}); return new Response('{}', { status: 200 }); },
  });
  const span = telemetry.span('agent.model.stream', {
    observationType: 'generation',
    model: 'test-model',
    input: [{ role: 'user', content: 'Captured request' }],
  });
  span.end({ output: [{ role: 'assistant', content: 'Captured response' }] });
  await new Promise((resolve) => setTimeout(resolve, 0));
  const body = JSON.parse(String(requests[0].body));
  const attributes = body.resourceSpans[0].scopeSpans[0].spans[0].attributes;
  const attribute = (key: string) => attributes.find((item: any) => item.key === key)?.value.stringValue;
  assert.equal(attribute('langfuse.observation.type'), 'generation');
  assert.equal(attribute('langfuse.observation.model.name'), 'test-model');
  assert.equal(attribute('langfuse.observation.input'), JSON.stringify([{ role: 'user', content: 'Captured request' }]));
  assert.equal(attribute('langfuse.observation.output'), JSON.stringify([{ role: 'assistant', content: 'Captured response' }]));
  await telemetry.dispose();
});

test('loads repository-local telemetry settings from .resonance/.env', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'resonance-telemetry-'));
  await mkdir(path.join(root, '.resonance'));
  await writeFile(path.join(root, '.resonance', '.env'), 'RESONANCE_TELEMETRY=langfuse\nRESONANCE_TELEMETRY_CAPTURE_CONTENT=true\nLANGFUSE_PUBLIC_KEY=file-public\nLANGFUSE_SECRET_KEY=file-secret\n');
  const requests: RequestInit[] = [];
  const telemetry = createTelemetry({ root, console: null, fetchFn: async (_input, init) => { requests.push(init || {}); return new Response('{}', { status: 200 }); } });
  telemetry.info('loaded from repository', { output: 'captured response' });
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(requests.length, 1);
  assert.equal((requests[0].headers as Record<string, string>).authorization, `Basic ${Buffer.from('file-public:file-secret').toString('base64')}`);
  const body = JSON.parse(String(requests[0].body));
  const output = body.resourceSpans[0].scopeSpans[0].spans[0].attributes.find((item: any) => item.key === 'langfuse.observation.output');
  assert.equal(output.value.stringValue, 'captured response');
  await telemetry.dispose();
});

test('reports exporter failures locally without throwing from telemetry calls or dispose', async () => {
  const output = fakeConsole();
  const telemetry = createTelemetry({ config: { mode: 'console' }, console: output.console, exporter: { record() { throw new Error('record offline'); }, async flush() { throw new Error('flush offline'); } } });
  assert.doesNotThrow(() => telemetry.info('still serving'));
  await assert.doesNotReject(() => telemetry.dispose());
  assert.ok(output.calls.some((call) => String(call.values[0]).includes('Telemetry exporter record failed')));
  assert.ok(output.calls.some((call) => String(call.values[0]).includes('Telemetry flush failed')));
});

test('bounds exporter flush during shutdown', async () => {
  const output = fakeConsole();
  const telemetry = createTelemetry({ config: { mode: 'console' }, console: output.console, flushTimeoutMs: 5, exporter: { record() {}, async flush() { await new Promise(() => {}); } } });
  await assert.doesNotReject(() => telemetry.dispose());
  assert.match(JSON.stringify(output.calls.at(-1)?.values[1]), /timed out/);
});
