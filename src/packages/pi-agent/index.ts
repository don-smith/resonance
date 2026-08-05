import type { HostResponse, PackageDefinition, PackageRegistration } from '../../package-contract.ts';
import { createPiAgentSession, createPiAcpFactory } from './session.ts';

const metadata = { id: 'pi-agent', version: '1.0.0', hostVersion: '1', label: 'Pi Agent', order: 30 } as const;
const isRecord = (value: unknown): value is Record<string, unknown> => Boolean(value) && typeof value === 'object' && !Array.isArray(value);
function sendError(response: HostResponse, error: unknown): void {
  const status = isRecord(error) && typeof error.status === 'number' ? error.status : 500;
  response.json(status, { error: error instanceof Error ? error.message : String(error) });
}

export function createPiAgentPackage(adapterFactory = createPiAcpFactory()): PackageDefinition {
  return {
    metadata,
    register(context): PackageRegistration {
      const session = createPiAgentSession({ repositoryRoot: context.repositoryRoot, adapterFactory });
      const activeStreams = new Set<() => void>();
      return {
        metadata,
        routes: [
          { method: 'GET', path: '/api/pi-agent/state', handler: async (_request, response) => response.json(200, session.snapshot()) },
          { method: 'GET', path: '/api/pi-agent/events', handler: async (request, response) => {
            const stream = response.sse();
            let closed = false;
            let unsubscribe: (() => void) | null = null;
            let cleanupBeforeSubscribe = false;
            let resolveClosed = () => {};
            const closedPromise = new Promise<void>((resolve) => { resolveClosed = resolve; });
            const close = () => {
              if (closed) return;
              closed = true;
              if (unsubscribe) unsubscribe();
              else cleanupBeforeSubscribe = true;
              activeStreams.delete(close);
              stream.close();
              resolveClosed();
            };
            activeStreams.add(close);
            request.onAbort(close);
            response.onClose(close);
            unsubscribe = session.subscribe((event) => {
              stream.write(event);
              if (response.closed) close();
            });
            if (cleanupBeforeSubscribe) unsubscribe();
            await closedPromise;
          } },
          { method: 'POST', path: '/api/pi-agent/prompt', handler: async (request, response) => {
            try {
              const body = await request.readJson<{ prompt?: unknown }>(32 * 1024);
              if (!isRecord(body) || typeof body.prompt !== 'string' || !body.prompt.trim()) {
                response.json(400, { error: 'Prompt must be a non-empty string.' });
                return;
              }
              await session.submitPrompt(body.prompt);
              response.json(202, { accepted: true });
            } catch (error) { sendError(response, error); }
          } },
          { method: 'POST', path: '/api/pi-agent/reset', handler: async (_request, response) => {
            try { response.json(200, { ok: true, state: await session.reset() }); }
            catch (error) { sendError(response, error); }
          } },
        ],
        assets: [
          { path: '/assets/pi-agent/pi-agent.js', file: 'src/packages/pi-agent/pi-agent.js', contentType: 'text/javascript; charset=utf-8' },
          { path: '/assets/pi-agent/pi-agent.css', file: 'src/packages/pi-agent/pi-agent.css', contentType: 'text/css; charset=utf-8' },
        ],
        navigation: [{ id: 'pi-agent', label: 'Pi Agent', order: metadata.order }],
        browser: { id: 'pi-agent', entry: '/assets/pi-agent/pi-agent.js', stylesheet: '/assets/pi-agent/pi-agent.css' },
        dispose: async () => {
          [...activeStreams].forEach((close) => close());
          await session.dispose();
        },
      };
    },
  };
}

export const piAgentPackage = createPiAgentPackage();
export default piAgentPackage;
