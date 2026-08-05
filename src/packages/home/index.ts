import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { readMarkdown } from '../../content.ts';
import { createMarkdownRenderer } from '../../markdown.ts';
import type { HostContext, PackageDefinition, PackageInput, PackageRegistration } from '../../package-contract.ts';

const metadata = { id: 'home', version: '1.0.0', hostVersion: '1', label: 'Home', order: 10 } as const;

export function homeInput(input: PackageInput) {
  const source = input.source === undefined ? 'README.md' : input.source;
  if (typeof source !== 'string' || !source || path.posix.isAbsolute(source) || path.win32.isAbsolute(source) || /\\/.test(source)) {
    throw new Error('Home source must be a non-empty relative path.');
  }
  if (!/\.(md|markdown|html|htm)$/i.test(source)) throw new Error('Home source must be a Markdown file or an HTML file.');
  return { source };
}

function createHomeHandler(input: PackageInput) {
  const { source } = homeInput(input);
  const renderer = createMarkdownRenderer();
  const isHtml = /\.(html|htm)$/i.test(source);
  return async (_request, response, context: HostContext) => {
    const relativePath = context.resolveRepositoryPath(source);
    if (!relativePath) {
      context.sendJson(response, 404, { error: 'Home source not found' });
      return;
    }
    try {
      const content = isHtml
        ? await readFile(path.join(context.repositoryRoot, relativePath), 'utf8')
        : await readMarkdown(context.repositoryRoot, relativePath);
      context.sendJson(response, 200, {
        path: relativePath.split(path.sep).join('/'),
        content,
        html: isHtml ? content : renderer.render(content),
      });
    } catch {
      context.sendJson(response, 404, { error: 'Home source not found' });
    }
  };
}

function register(_context: HostContext, input: PackageInput): PackageRegistration {
  homeInput(input);
  return {
    metadata,
    routes: [{ method: 'GET', path: '/api/home', handler: createHomeHandler(input) }],
    assets: [
      { path: '/assets/home/home.js', file: 'src/packages/home/home.js', contentType: 'text/javascript; charset=utf-8' },
      { path: '/assets/home/home.css', file: 'src/packages/home/home.css', contentType: 'text/css; charset=utf-8' },
    ],
    navigation: [{ id: 'home', label: 'Home', order: metadata.order }],
    browser: { id: 'home', entry: '/assets/home/home.js', stylesheet: '/assets/home/home.css' },
  };
}

export const homePackage: PackageDefinition = { metadata, register };
export default homePackage;
