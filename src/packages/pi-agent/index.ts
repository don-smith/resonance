import type { HostContext, PackageDefinition, PackageRegistration } from '../../package-contract.ts';
import { HttpError, openSse, readJsonBody } from '../../http.ts';
import { PiAgentBusyError, PiAgentUnavailableError, createPiAgentSession, createPiAcpFactory } from './session.ts';

const metadata = { id: 'pi-agent', version: '1.0.0', hostVersion: '1', label: 'Pi Agent', order: 30 } as const;
const isRecord = (value: unknown): value is Record<string, unknown> => Boolean(value) && typeof value === 'object' && !Array.isArray(value);
function sendError(response: Parameters<HostContext['sendJson']>[0], context: HostContext, error: unknown): void {
  const status = error instanceof HttpError || error instanceof PiAgentBusyError || error instanceof PiAgentUnavailableError ? error.status : 500;
  context.sendJson(response, status, { error: error instanceof Error ? error.message : String(error) });
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
          { method: 'GET', path: '/api/pi-agent/state', handler: async (_request, response, hostContext) => hostContext.sendJson(response, 200, session.snapshot()) },
          { method: 'GET', path: '/api/pi-agent/events', handler: async (request, response) => {
            const stream = openSse(response);
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
            request.once('aborted', close);
            response.once('close', close);
            unsubscribe = session.subscribe((event) => {
              stream.write(event);
              if (response.destroyed || response.writableEnded) close();
            });
            if (cleanupBeforeSubscribe) unsubscribe();
            await closedPromise;
          } },
          { method: 'POST', path: '/api/pi-agent/prompt', handler: async (request, response, hostContext) => {
            try {
              const body = await readJsonBody<{ prompt?: unknown }>(request, 32 * 1024);
              if (!isRecord(body) || typeof body.prompt !== 'string' || !body.prompt.trim()) throw new HttpError(400, 'Prompt must be a non-empty string.');
              await session.submitPrompt(body.prompt);
              hostContext.sendJson(response, 202, { accepted: true });
            } catch (error) { sendError(response, hostContext, error); }
          } },
          { method: 'POST', path: '/api/pi-agent/reset', handler: async (_request, response, hostContext) => {
            try { hostContext.sendJson(response, 200, { ok: true, state: await session.reset() }); }
            catch (error) { sendError(response, hostContext, error); }
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
