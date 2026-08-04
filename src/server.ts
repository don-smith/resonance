import { createReadStream } from 'node:fs';
import { access } from 'node:fs/promises';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defaultRepositoryConfig, loadRepositoryConfig } from './config.ts';
import { createHost, type HostRegistry } from './host.ts';
import { createDefaultPackages } from './packages/index.ts';

const projectRoot = fileURLToPath(new URL('..', import.meta.url));

function toPath(value: string | URL): string {
  return value instanceof URL ? fileURLToPath(value) : value;
}

async function serveFile(response, filename: string, contentType: string): Promise<void> {
  try {
    await access(filename);
    response.writeHead(200, { 'content-type': contentType, 'cache-control': 'no-cache' });
    createReadStream(filename).pipe(response);
  } catch {
    response.writeHead(404);
    response.end('Not found');
  }
}

export function createApp({
  root = process.cwd(),
  appRoot = projectRoot,
  config,
  registry = createHost({
    root,
    appRoot,
    config: config || defaultRepositoryConfig(),
    packages: createDefaultPackages(config || defaultRepositoryConfig()),
  }),
} : {
  root?: string | URL;
  appRoot?: string | URL;
  config?: ReturnType<typeof defaultRepositoryConfig>;
  registry?: HostRegistry;
} = {}) {
  const assetsRoot = path.join(toPath(appRoot), 'public');
  return http.createServer(async (request, response) => {
    if (request.method !== 'GET') {
      response.writeHead(405, { allow: 'GET' });
      response.end('Method not allowed');
      return;
    }

    const requestUrl = new URL(request.url, 'http://127.0.0.1');
    try {
      if (requestUrl.pathname === '/api/manifest') {
        registry.context.sendJson(response, 200, registry.manifest);
        return;
      }

      const route = registry.routes[requestUrl.pathname];
      if (route) {
        await route.handler(request, response, registry.context);
        return;
      }

      const asset = registry.assets[requestUrl.pathname];
      if (asset) {
        await serveFile(response, path.join(assetsRoot, asset.file), asset.contentType);
        return;
      }

      response.writeHead(404);
      response.end('Not found');
    } catch (error) {
      console.error(error);
      registry.context.sendJson(response, 500, { error: 'Internal server error' });
    }
  });
}

export async function startServer({
  root = process.cwd(),
  appRoot = projectRoot,
  host = '127.0.0.1',
  port = 4317,
  maxPortAttempts = 100,
  config,
  registry,
} : {
  root?: string | URL;
  appRoot?: string | URL;
  host?: string;
  port?: number;
  maxPortAttempts?: number;
  config?: ReturnType<typeof defaultRepositoryConfig>;
  registry?: HostRegistry;
} = {}) {
  const resolvedConfig = config || await loadRepositoryConfig(root);
  const resolvedRegistry = registry || createHost({ root, appRoot, config: resolvedConfig, packages: createDefaultPackages(resolvedConfig) });
  const attempts = port === 0 ? 1 : maxPortAttempts;

  for (let attempt = 0; attempt < attempts; attempt += 1) {
    const candidatePort = port + attempt;
    const server = createApp({ root, appRoot, registry: resolvedRegistry });
    try {
      await new Promise((resolve, reject) => {
        server.once('error', reject);
        server.listen(candidatePort, host, resolve);
      });
      return server;
    } catch (error) {
      if (error.code !== 'EADDRINUSE' || port === 0 || candidatePort >= 65535 || attempt === attempts - 1) {
        throw error;
      }
    }
  }

  throw new Error('Unable to find an available port.');
}
