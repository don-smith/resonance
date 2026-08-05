import path from 'node:path';
import { buildMarkdownTree, discoverMarkdownFiles, readMarkdown } from '../../content.ts';
import { createMarkdownRenderer } from '../../markdown.ts';
import type { HostContext, PackageDefinition, PackageInput, PackageRegistration } from '../../package-contract.ts';

const metadata = { id: 'docs', version: '1.0.0', hostVersion: '1', label: 'Docs', order: 20 } as const;
const ROUTES = ['/api/docs/tree', '/api/docs/document'] as const;

type DocsOptions = { extensions: string[]; ignoredDirectories: string[] };

function isStringArray(value: unknown, predicate: (item: string) => boolean): value is string[] {
  return Array.isArray(value) && value.every((item): item is string => typeof item === 'string' && predicate(item));
}

export function docsInput(input: PackageInput): DocsOptions {
  const extensions = input.extensions === undefined ? ['.md', '.markdown'] : input.extensions;
  const ignoredDirectories = input.ignoredDirectories === undefined ? ['.git', 'node_modules'] : input.ignoredDirectories;
  if (!isStringArray(extensions, (value) => value.startsWith('.'))) {
    throw new Error('Docs extensions must be an array of dotted strings.');
  }
  if (!isStringArray(ignoredDirectories, (value) => value.length > 0)) {
    throw new Error('Docs ignoredDirectories must be an array of non-empty strings.');
  }
  return { extensions: [...extensions], ignoredDirectories: [...ignoredDirectories] };
}

function createRouteHandler(options: DocsOptions, kind: 'tree' | 'document') {
  const renderer = createMarkdownRenderer();
  return async (request, response, hostContext: HostContext) => {
    const requestUrl = new URL(request.url, 'http://127.0.0.1');
    if (kind === 'tree') {
      const documents = await discoverMarkdownFiles(hostContext.repositoryRoot, options);
      response.json(200, {
        rootName: path.basename(path.resolve(hostContext.repositoryRoot)),
        documents,
        tree: buildMarkdownTree(documents),
      });
      return;
    }

    const relativePath = hostContext.resolveRepositoryPath(requestUrl.searchParams.get('path') || '');
    const ignored = relativePath?.split(path.sep).some((segment) => options.ignoredDirectories.includes(segment));
    if (!relativePath || ignored || !options.extensions.some((extension) => relativePath.toLowerCase().endsWith(extension.toLowerCase()))) {
      response.json(404, { error: 'Markdown document not found' });
      return;
    }
    try {
      const content = await readMarkdown(hostContext.repositoryRoot, relativePath);
      response.json(200, {
        path: relativePath.split(path.sep).join('/'),
        content,
        html: renderer.render(content),
      });
    } catch {
      response.json(404, { error: 'Markdown document not found' });
    }
  };
}

function register(_context: HostContext, input: PackageInput): PackageRegistration {
  const options = docsInput(input);
  return {
    metadata,
    routes: [
      { method: 'GET', path: ROUTES[0], handler: createRouteHandler(options, 'tree') },
      { method: 'GET', path: ROUTES[1], handler: createRouteHandler(options, 'document') },
    ],
    assets: [
      { path: '/assets/docs/docs.js', file: 'src/packages/docs/docs.js', contentType: 'text/javascript; charset=utf-8' },
      { path: '/assets/docs/docs.css', file: 'src/packages/docs/docs.css', contentType: 'text/css; charset=utf-8' },
    ],
    navigation: [{ id: 'docs', label: 'Docs', order: metadata.order }],
    browser: { id: 'docs', entry: '/assets/docs/docs.js', stylesheet: '/assets/docs/docs.css' },
  };
}

export const docsPackage: PackageDefinition = { metadata, register };
export default docsPackage;
