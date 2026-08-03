import { createReadStream } from 'node:fs';
import { access, readFile } from 'node:fs/promises';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { buildMarkdownTree, discoverMarkdownFiles, readMarkdown } from './content.ts';
import { renderMarkdown } from './markdown.ts';

const projectRoot = fileURLToPath(new URL('..', import.meta.url));
const publicRoot = path.join(projectRoot, 'public');

function toPath(value) {
  return value instanceof URL ? fileURLToPath(value) : value;
}

function sendJson(response, status, body) {
  response.writeHead(status, {
    'content-type': 'application/json; charset=utf-8',
    'cache-control': 'no-store',
  });
  response.end(JSON.stringify(body));
}

function safeDocumentPath(root, requestedPath) {
  if (!requestedPath || path.posix.isAbsolute(requestedPath) || /\\/.test(requestedPath)) return null;

  const rootPath = path.resolve(toPath(root));
  const absolutePath = path.resolve(rootPath, requestedPath);
  const relativePath = path.relative(rootPath, absolutePath);

  if (relativePath.startsWith('..' + path.sep) || relativePath === '..' || path.isAbsolute(relativePath)) return null;
  if (!/\.(md|markdown)$/i.test(relativePath)) return null;
  return relativePath;
}

async function serveFile(response, filename, contentType) {
  try {
    await access(filename);
    response.writeHead(200, { 'content-type': contentType, 'cache-control': 'no-cache' });
    createReadStream(filename).pipe(response);
  } catch {
    response.writeHead(404);
    response.end('Not found');
  }
}

export function createApp({ root = process.cwd(), appRoot = projectRoot } = {}) {
  const repositoryRoot = toPath(root);
  const assetsRoot = path.join(toPath(appRoot), 'public');

  return http.createServer(async (request, response) => {
    if (request.method !== 'GET') {
      response.writeHead(405, { allow: 'GET' });
      response.end('Method not allowed');
      return;
    }

    const requestUrl = new URL(request.url, 'http://127.0.0.1');

    try {
      if (requestUrl.pathname === '/api/tree') {
        const documents = await discoverMarkdownFiles(repositoryRoot);
        sendJson(response, 200, {
          rootName: path.basename(path.resolve(repositoryRoot)),
          documents,
          tree: buildMarkdownTree(documents),
        });
        return;
      }

      if (requestUrl.pathname === '/api/document') {
        const relativePath = safeDocumentPath(repositoryRoot, requestUrl.searchParams.get('path'));
        if (!relativePath) {
          sendJson(response, 404, { error: 'Markdown document not found' });
          return;
        }

        try {
          const content = await readMarkdown(repositoryRoot, relativePath);
          sendJson(response, 200, {
            path: relativePath.split(path.sep).join('/'),
            content,
            html: renderMarkdown(content),
          });
        } catch {
          sendJson(response, 404, { error: 'Markdown document not found' });
        }
        return;
      }

      const assets = {
        '/': ['index.html', 'text/html; charset=utf-8'],
        '/assets/app.js': ['app.js', 'text/javascript; charset=utf-8'],
        '/assets/styles.css': ['styles.css', 'text/css; charset=utf-8'],
      };
      const asset = assets[requestUrl.pathname];
      if (asset) {
        await serveFile(response, path.join(assetsRoot, asset[0]), asset[1]);
        return;
      }

      response.writeHead(404);
      response.end('Not found');
    } catch (error) {
      console.error(error);
      sendJson(response, 500, { error: 'Internal server error' });
    }
  });
}

export async function startServer({
  root = process.cwd(),
  appRoot = projectRoot,
  host = '127.0.0.1',
  port = 4317,
  maxPortAttempts = 100,
} = {}) {
  const attempts = port === 0 ? 1 : maxPortAttempts;

  for (let attempt = 0; attempt < attempts; attempt += 1) {
    const candidatePort = port + attempt;
    const server = createApp({ root, appRoot });

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
