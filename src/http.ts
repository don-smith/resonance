import type { IncomingMessage, ServerResponse } from 'node:http';
import type { HostRequest, HostResponse } from './package-contract.ts';

export class HttpError extends Error {
  constructor(public readonly status: number, message: string) {
    super(message);
    this.name = 'HttpError';
  }
}

export async function readJsonBody<T>(request: IncomingMessage, maxBytes = 1024 * 1024): Promise<T> {
  const contentType = request.headers['content-type'] || '';
  if (!contentType.toLowerCase().includes('application/json')) {
    throw new HttpError(415, 'Request body must use application/json.');
  }
  return new Promise<T>((resolve, reject) => {
    const chunks: Buffer[] = [];
    let size = 0;
    let settled = false;
    const onData = (chunk: Buffer | string) => {
      const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
      size += buffer.byteLength;
      if (size > maxBytes) {
        request.resume();
        settle(() => reject(new HttpError(413, 'Request body is too large.')));
        return;
      }
      chunks.push(buffer);
    };
    const onEnd = () => settle(() => {
      try {
        resolve(JSON.parse(Buffer.concat(chunks).toString('utf8')) as T);
      } catch {
        reject(new HttpError(400, 'Request body must be valid JSON.'));
      }
    });
    const onError = (error: Error) => settle(() => reject(error));
    const onAborted = () => settle(() => reject(new HttpError(400, 'Request body was aborted.')));
    const cleanup = () => {
      request.off('data', onData);
      request.off('end', onEnd);
      request.off('error', onError);
      request.off('aborted', onAborted);
    };
    const settle = (callback: () => void) => {
      if (settled) return;
      settled = true;
      cleanup();
      callback();
    };
    request.on('data', onData);
    request.once('end', onEnd);
    request.once('error', onError);
    request.once('aborted', onAborted);
  });
}

export function openSse(response: ServerResponse): { write(event: unknown): boolean; close(): void } {
  response.writeHead(200, {
    'content-type': 'text/event-stream; charset=utf-8',
    'cache-control': 'no-cache, no-store',
    connection: 'keep-alive',
    'x-accel-buffering': 'no',
  });
  response.flushHeaders?.();
  return {
    write(event) {
      if (response.destroyed || response.writableEnded) return false;
      return response.write(`data: ${JSON.stringify(event)}\n\n`);
    },
    close() {
      if (!response.writableEnded && !response.destroyed) response.end();
    },
  };
}

export function createHostRequest(request: IncomingMessage): HostRequest {
  return {
    url: request.url || '/',
    headers: request.headers,
    readJson: <T>(maxBytes?: number) => readJsonBody<T>(request, maxBytes),
    onAbort(listener) { request.once('aborted', listener); },
  };
}

export function createHostResponse(response: ServerResponse): HostResponse {
  return {
    json(status, body) {
      response.writeHead(status, { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store' });
      response.end(JSON.stringify(body));
    },
    sse() { return openSse(response); },
    onClose(listener) { response.once('close', listener); },
    get closed() { return Boolean(response.destroyed || response.writableEnded); },
  };
}
