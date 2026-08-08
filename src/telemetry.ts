import { randomUUID } from 'node:crypto';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Telemetry, TelemetryController, TelemetryFields, TelemetryLevel, TelemetrySpan } from './package-contract.ts';
export type { Telemetry, TelemetryController, TelemetryFields, TelemetryLevel, TelemetrySpan } from './package-contract.ts';

type LogRecord = { kind: 'log'; timestamp: number; traceId: string; level: TelemetryLevel; message: string; fields: Record<string, unknown> };
type EventRecord = { kind: 'event'; timestamp: number; traceId: string; spanId: string; name: string; fields: Record<string, unknown> };
type SpanRecord = { kind: 'span'; timestamp: number; traceId: string; spanId: string; name: string; startedAt: number; endedAt: number; fields: Record<string, unknown>; error?: Record<string, unknown> };
type TelemetryRecord = LogRecord | EventRecord | SpanRecord;
type Exporter = { record(record: TelemetryRecord): void; flush(): Promise<void> };
type TelemetryConsole = { debug(...values: unknown[]): void; info(...values: unknown[]): void; warn(...values: unknown[]): void; error(...values: unknown[]): void };
type Fetch = (input: string, init?: RequestInit) => Promise<Response>;

type TelemetryConfig = {
  mode?: 'off' | 'console' | 'langfuse';
  level?: TelemetryLevel;
  baseUrl?: string;
  publicKey?: string;
  secretKey?: string;
  captureContent?: boolean;
};

const levels: TelemetryLevel[] = ['debug', 'info', 'warn', 'error'];
const secretKey = /^(?:.*(?:api[_-]?key|secret|token|password|authorization|credential).*)$/i;
const contentKey = /^(?:.*(?:prompt|content|input|output|transcript).*)$/i;
const secretValue = /(?:sk-(?:or-v1-)?[A-Za-z0-9_-]{8,}|Bearer\s+[A-Za-z0-9._~+/=-]+)/g;
function toPath(value: string | URL): string { return value instanceof URL ? fileURLToPath(value) : value; }
function repositoryName(root: string | URL | undefined): string | undefined {
  if (!root) return undefined;
  const absoluteRoot = path.resolve(toPath(root));
  let gitPath = path.join(absoluteRoot, '.git');
  try {
    const gitPointer = readFileSync(gitPath, 'utf8').trim();
    if (gitPointer.startsWith('gitdir:')) gitPath = path.resolve(absoluteRoot, gitPointer.slice('gitdir:'.length).trim());
  } catch {}
  try {
    const gitConfig = readFileSync(path.join(gitPath, 'config'), 'utf8');
    const originSection = /\[remote "origin"\]([\s\S]*?)(?:\n\[|$)/.exec(gitConfig)?.[1] || '';
    const origin = /^\s*url\s*=\s*(\S+)/m.exec(originSection)?.[1]?.replace(/\/$/, '');
    const remoteName = origin && /([^/:]+?)(?:\.git)?$/.exec(origin)?.[1];
    if (remoteName) return remoteName;
  } catch {}
  return path.basename(absoluteRoot) || absoluteRoot;
}
function readDotEnv(root: string | URL | undefined): Record<string, string> {
  if (!root) return {};
  let contents: string;
  try { contents = readFileSync(path.join(toPath(root), '.resonance', '.env'), 'utf8'); }
  catch (error) { if (error?.code === 'ENOENT') return {}; throw error; }
  const values: Record<string, string> = {};
  for (const line of contents.split(/\r?\n/)) {
    const match = /^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*?)\s*$/.exec(line);
    if (!match || match[1].startsWith('#')) continue;
    let value = match[2];
    if ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'"))) value = value.slice(1, -1);
    values[match[1]] = value;
  }
  return values;
}
function environmentConfig(root?: string | URL): TelemetryConfig {
  const environment = { ...readDotEnv(root), ...process.env };
  return {
    mode: (environment.RESONANCE_TELEMETRY || 'console') as TelemetryConfig['mode'],
    level: (environment.RESONANCE_TELEMETRY_LEVEL || 'info') as TelemetryLevel,
    baseUrl: environment.LANGFUSE_BASE_URL || 'http://127.0.0.1:13000',
    publicKey: environment.LANGFUSE_PUBLIC_KEY,
    secretKey: environment.LANGFUSE_SECRET_KEY,
    captureContent: environment.RESONANCE_TELEMETRY_CAPTURE_CONTENT === 'true',
  };
}

function isObject(value: unknown): value is Record<string, unknown> { return Boolean(value) && typeof value === 'object' && !Array.isArray(value); }
function errorValue(value: unknown): Record<string, unknown> | null {
  if (!(value instanceof Error)) return null;
  return { name: value.name, message: value.message, ...(value.stack ? { stack: value.stack } : {}), ...(value.cause ? { cause: errorValue(value.cause) || String(value.cause) } : {}) };
}
function sanitize(value: unknown, captureContent: boolean, key = ''): unknown {
  if (secretKey.test(key)) return '[REDACTED]';
  if (!captureContent && contentKey.test(key)) return '[CONTENT REDACTED]';
  const error = errorValue(value);
  if (error) return sanitize(error, captureContent, key);
  if (Array.isArray(value)) return value.map((item) => sanitize(item, captureContent, key));
  if (isObject(value)) return Object.fromEntries(Object.entries(value).map(([entryKey, entryValue]) => [entryKey, sanitize(entryValue, captureContent, entryKey)]));
  if (typeof value === 'string') return value.replace(secretValue, '[REDACTED]');
  if (typeof value === 'bigint') return String(value);
  if (value === undefined) return undefined;
  if (typeof value === 'function' || typeof value === 'symbol') return String(value);
  return value;
}
function fields(value: TelemetryFields | undefined, captureContent: boolean): Record<string, unknown> {
  return (sanitize(value || {}, captureContent) || {}) as Record<string, unknown>;
}
function levelEnabled(level: TelemetryLevel, minimum: TelemetryLevel): boolean { return levels.indexOf(level) >= levels.indexOf(minimum); }
function id(): string { return randomUUID(); }

class NullExporter implements Exporter {
  record(_record: TelemetryRecord): void {}
  async flush(): Promise<void> {}
}

function otelTraceId(value: string): string { return `${value.replace(/-/g, '')}${'0'.repeat(32)}`.slice(0, 32); }
function otelSpanId(value: string): string { return `${value.replace(/-/g, '')}${'0'.repeat(16)}`.slice(0, 16); }
function otelTime(milliseconds: number): string { return String(Math.trunc(milliseconds * 1_000_000)); }
function otelValue(value: unknown): Record<string, unknown> {
  if (typeof value === 'boolean') return { boolValue: value };
  if (typeof value === 'number') return Number.isInteger(value) ? { intValue: String(value) } : { doubleValue: value };
  if (typeof value === 'string') return { stringValue: value };
  return { stringValue: JSON.stringify(value) };
}
function langfuseAttributeKey(key: string): string {
  switch (key) {
    case 'sessionId': return 'langfuse.session.id';
    case 'observationType': return 'langfuse.observation.type';
    case 'model': return 'langfuse.observation.model.name';
    case 'input': return 'langfuse.observation.input';
    case 'output': return 'langfuse.observation.output';
    default: return key;
  }
}
function otelAttributes(fields: Record<string, unknown>): Array<Record<string, unknown>> {
  return Object.entries(fields).filter(([, value]) => value !== undefined).map(([key, value]) => ({ key: langfuseAttributeKey(key), value: otelValue(value) }));
}
function otelSpan({ traceId, spanId, name, startedAt, endedAt, fields: spanFields, error }: { traceId: string; spanId: string; name: string; startedAt: number; endedAt: number; fields: Record<string, unknown>; error?: Record<string, unknown> }): Record<string, unknown> {
  const fields = { ...spanFields, ...(error ? { 'error.name': error.name, 'error.message': error.message, 'error.stack': error.stack } : {}) };
  return { traceId: otelTraceId(traceId), spanId: otelSpanId(spanId), name, kind: 1, startTimeUnixNano: otelTime(startedAt), endTimeUnixNano: otelTime(endedAt), attributes: otelAttributes(fields), status: error ? { code: 2, message: String(error.message || error) } : { code: 1 } };
}

class LangfuseExporter implements Exporter {
  private readonly pending: TelemetryRecord[] = [];
  private flushing: Promise<void> | null = null;
  private scheduled = false;
  constructor(private readonly options: { baseUrl: string; publicKey: string; secretKey: string; repository?: string; fetchFn: Fetch; onError(error: unknown): void }) {}
  record(record: TelemetryRecord): void {
    this.pending.push(record);
    if (this.scheduled) return;
    this.scheduled = true;
    queueMicrotask(() => { this.scheduled = false; void this.flush().catch(this.options.onError); });
  }
  private spanForRecord(record: TelemetryRecord): Record<string, unknown> {
    if (record.kind === 'span') return otelSpan(record);
    if (record.kind === 'event') return otelSpan({ traceId: record.traceId, spanId: record.spanId, name: `event:${record.name}`, startedAt: record.timestamp, endedAt: record.timestamp, fields: record.fields });
    return otelSpan({ traceId: record.traceId, spanId: id(), name: `log:${record.message}`, startedAt: record.timestamp, endedAt: record.timestamp, fields: { ...record.fields, level: record.level } });
  }
  async flush(): Promise<void> {
    if (this.flushing) return this.flushing;
    this.flushing = this.send().finally(() => { this.flushing = null; });
    return this.flushing;
  }
  private async send(): Promise<void> {
    while (this.pending.length > 0) {
      const records = this.pending.splice(0, 100);
      const spans = records.map((record) => this.spanForRecord(record));
      const resourceAttributes = [{ key: 'service.name', value: { stringValue: 'resonance' } }, { key: 'service.version', value: { stringValue: '0.1.0' } }, ...(this.options.repository ? [{ key: 'repository', value: { stringValue: this.options.repository } }] : [])];
      const body = { resourceSpans: [{ resource: { attributes: resourceAttributes }, scopeSpans: [{ scope: { name: 'resonance', version: '0.1.0' }, spans }] }] };
      const response = await this.options.fetchFn(`${this.options.baseUrl.replace(/\/$/, '')}/api/public/otel/v1/traces`, {
        method: 'POST',
        headers: {
          'content-type': 'application/json',
          authorization: `Basic ${Buffer.from(`${this.options.publicKey}:${this.options.secretKey}`).toString('base64')}`,
          'x-langfuse-ingestion-version': '4',
        },
        body: JSON.stringify(body),
      });
      if (!response.ok) throw new Error(`Langfuse trace ingestion failed with status ${response.status}.`);
    }
  }
}

class TelemetryImplementation implements TelemetryController {
  private disposed = false;
  private flushing: Promise<void> | null = null;
  constructor(private readonly context: { fields: Record<string, unknown>; sessionId?: string; minimum: TelemetryLevel; captureContent: boolean; console: TelemetryConsole | null; exporter: Exporter; now: () => number; traceId: string; flushTimeoutMs: number }) {}
  private export(record: TelemetryRecord): void {
    try { this.context.exporter.record(record); }
    catch (error) { this.context.console?.warn('Telemetry exporter record failed', { error: errorValue(error) || String(error) }); }
  }
  child(childFields: TelemetryFields = {}): Telemetry { return new TelemetryImplementation({ ...this.context, fields: { ...this.context.fields, ...fields(childFields, this.context.captureContent) }, traceId: id() }); }
  session(sessionId: string, sessionFields: TelemetryFields = {}): Telemetry {
    const normalizedSessionId = sessionId.trim();
    if (!normalizedSessionId) throw new Error('Telemetry session id must not be empty.');
    return new TelemetryImplementation({ ...this.context, fields: { ...this.context.fields, ...fields(sessionFields, this.context.captureContent) }, sessionId: normalizedSessionId, traceId: id() });
  }
  private safeFields(recordFields?: TelemetryFields): Record<string, unknown> {
    return { ...this.context.fields, ...fields(recordFields, this.context.captureContent), ...(this.context.sessionId ? { sessionId: this.context.sessionId } : {}) };
  }
  private record(level: TelemetryLevel, message: string, recordFields?: TelemetryFields): void {
    if (this.disposed || !levelEnabled(level, this.context.minimum)) return;
    const safeFields = this.safeFields(recordFields);
    const record: LogRecord = { kind: 'log', timestamp: this.context.now(), traceId: this.context.traceId, level, message, fields: safeFields };
    this.export(record);
    this.context.console?.[level](message, safeFields);
  }
  debug(message: string, recordFields?: TelemetryFields): void { this.record('debug', message, recordFields); }
  info(message: string, recordFields?: TelemetryFields): void { this.record('info', message, recordFields); }
  warn(message: string, recordFields?: TelemetryFields): void { this.record('warn', message, recordFields); }
  error(message: string, recordFields?: TelemetryFields): void { this.record('error', message, recordFields); }
  span(name: string, spanFields: TelemetryFields = {}): TelemetrySpan {
    const startedAt = this.context.now(); const spanId = id(); const traceFields = this.safeFields(spanFields); let complete = false;
    const event = (eventName: string, eventFields: TelemetryFields = {}) => { if (complete || this.disposed) return; this.export({ kind: 'event', timestamp: this.context.now(), traceId: this.context.traceId, spanId, name: eventName, fields: this.safeFields({ ...traceFields, ...eventFields }) }); };
    const end = (endFields: TelemetryFields = {}) => { if (complete || this.disposed) return; complete = true; this.export({ kind: 'span', timestamp: this.context.now(), traceId: this.context.traceId, spanId, name, startedAt, endedAt: this.context.now(), fields: this.safeFields({ ...traceFields, ...endFields }) }); };
    const fail = (error: unknown, errorFields: TelemetryFields = {}) => { if (complete || this.disposed) return; complete = true; const serialized = (sanitize(errorValue(error) || { message: String(error) }, this.context.captureContent, 'error') || {}) as Record<string, unknown>; this.export({ kind: 'span', timestamp: this.context.now(), traceId: this.context.traceId, spanId, name, startedAt, endedAt: this.context.now(), fields: this.safeFields({ ...traceFields, ...errorFields }), error: serialized }); this.record('error', `${name} failed`, { ...errorFields, error }); };
    return { event, end, fail };
  }
  async flush(): Promise<void> {
    if (this.flushing) return this.flushing;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const pending = this.context.exporter.flush();
    const timeout = new Promise<never>((_resolve, reject) => { timer = setTimeout(() => reject(new Error(`Telemetry flush timed out after ${this.context.flushTimeoutMs}ms.`)), this.context.flushTimeoutMs); });
    this.flushing = Promise.race([pending, timeout]).finally(() => { if (timer) clearTimeout(timer); this.flushing = null; });
    return this.flushing;
  }
  async dispose(): Promise<void> { if (this.disposed) return; this.disposed = true; await this.flush().catch((error) => this.context.console?.warn('Telemetry flush failed', { error: errorValue(error) || String(error) })); }
}

export function createTelemetry({ root, config, console: output = globalThis.console, fetchFn = fetch, now = Date.now, exporter, flushTimeoutMs = 2000 }: { root?: string | URL; config?: TelemetryConfig; console?: TelemetryConsole | null; fetchFn?: Fetch; now?: () => number; exporter?: Exporter; flushTimeoutMs?: number } = {}): TelemetryController {
  const resolvedConfig = config || environmentConfig(root);
  const mode = resolvedConfig.mode || 'console';
  const minimum = resolvedConfig.level || 'info';
  const repository = repositoryName(root);
  let selected: Exporter = exporter || new NullExporter();
  if (!exporter && mode === 'langfuse' && resolvedConfig.publicKey && resolvedConfig.secretKey) selected = new LangfuseExporter({ baseUrl: resolvedConfig.baseUrl || 'http://127.0.0.1:13000', publicKey: resolvedConfig.publicKey, secretKey: resolvedConfig.secretKey, repository, fetchFn, onError: (error) => output?.warn('Langfuse export failed', { error: errorValue(error) || String(error) }) });
  const telemetry = new TelemetryImplementation({ fields: repository ? { repository } : {}, minimum, captureContent: resolvedConfig.captureContent === true, console: mode === 'off' ? null : output, exporter: mode === 'off' ? new NullExporter() : selected, now, traceId: id(), flushTimeoutMs });
  if (mode === 'langfuse' && !exporter && (!resolvedConfig.publicKey || !resolvedConfig.secretKey)) telemetry.warn('Langfuse telemetry is not configured; using console telemetry.', { baseUrl: resolvedConfig.baseUrl || 'http://127.0.0.1:13000' });
  return telemetry;
}
