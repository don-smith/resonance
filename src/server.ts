import { createReadStream } from 'node:fs';
import { access } from 'node:fs/promises';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defaultRepositoryConfig, loadRepositoryConfig } from './config.ts';
import { createHost, type HostRegistry } from './host.ts';
import { loadConfiguredPackages } from './packages/index.ts';

const projectRoot = fileURLToPath(new URL('..', import.meta.url));
async function serveFile(response, filename: string, contentType: string): Promise<void> {
  try { await access(filename); response.writeHead(200, { 'content-type': contentType, 'cache-control': 'no-cache' }); createReadStream(filename).pipe(response); }
  catch { response.writeHead(404); response.end('Not found'); }
}

export async function createApp({ root = process.cwd(), appRoot = projectRoot, config, registry }: { root?: string | URL; appRoot?: string | URL; config?: ReturnType<typeof defaultRepositoryConfig>; registry?: HostRegistry } = {}) {
  const resolvedConfig = config || await loadRepositoryConfig(root);
  const resolvedRegistry = registry || createHost({ root, appRoot, config: resolvedConfig, packages: await loadConfiguredPackages({ config: resolvedConfig, appRoot }) });
  const assetsRoot = resolvedRegistry.context.appRoot;
  return http.createServer(async (request, response) => {
    if (request.method !== 'GET') { response.writeHead(405, { allow: 'GET' }); response.end('Method not allowed'); return; }
    const requestUrl = new URL(request.url, 'http://127.0.0.1');
    try {
      if (requestUrl.pathname === '/api/manifest') { resolvedRegistry.context.sendJson(response, 200, resolvedRegistry.manifest); return; }
      const route = resolvedRegistry.routes[requestUrl.pathname]; if (route) { await route.handler(request, response, resolvedRegistry.context); return; }
      const asset = resolvedRegistry.assets[requestUrl.pathname]; if (asset) { await serveFile(response, path.join(assetsRoot, asset.file), asset.contentType); return; }
      response.writeHead(404); response.end('Not found');
    } catch (error) { console.error(error); resolvedRegistry.context.sendJson(response, 500, { error: 'Internal server error' }); }
  });
}

export async function startServer({ root = process.cwd(), appRoot = projectRoot, host = '127.0.0.1', port = 4317, maxPortAttempts = 100, config, registry }: { root?: string | URL; appRoot?: string | URL; host?: string; port?: number; maxPortAttempts?: number; config?: ReturnType<typeof defaultRepositoryConfig>; registry?: HostRegistry } = {}) {
  const resolvedConfig = config || await loadRepositoryConfig(root);
  const resolvedRegistry = registry || createHost({ root, appRoot, config: resolvedConfig, packages: await loadConfiguredPackages({ config: resolvedConfig, appRoot }) });
  const attempts = port === 0 ? 1 : maxPortAttempts;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    const candidatePort = port + attempt; const server = await createApp({ root, appRoot, registry: resolvedRegistry });
    try { await new Promise((resolve, reject) => { server.once('error', reject); server.listen(candidatePort, host, resolve); }); return server; }
    catch (error) { if (error.code !== 'EADDRINUSE' || port === 0 || candidatePort >= 65535 || attempt === attempts - 1) throw error; }
  }
  throw new Error('Unable to find an available port.');
}
