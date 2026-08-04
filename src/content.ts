import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const DEFAULT_MARKDOWN_EXTENSIONS = ['.md', '.markdown'];
export const DEFAULT_IGNORED_DIRECTORIES = ['.git', 'node_modules'];
export type DiscoveryOptions = { extensions?: string[]; ignoredDirectories?: string[] };

function toPath(value: string | URL): string {
  return value instanceof URL ? fileURLToPath(value) : value;
}

export async function discoverMarkdownFiles(
  root: string | URL,
  { extensions = DEFAULT_MARKDOWN_EXTENSIONS, ignoredDirectories = DEFAULT_IGNORED_DIRECTORIES }: DiscoveryOptions = {},
): Promise<string[]> {
  const rootPath = toPath(root);
  const extensionSet = new Set(extensions.map((extension) => extension.toLowerCase()));
  const ignored = new Set(ignoredDirectories);
  const paths: string[] = [];

  async function visit(directory: string, relativeDirectory = ''): Promise<void> {
    const entries = await readdir(directory, { withFileTypes: true });
    for (const entry of entries) {
      if (entry.isDirectory() && ignored.has(entry.name)) continue;
      const relativePath = path.posix.join(relativeDirectory, entry.name);
      const absolutePath = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        await visit(absolutePath, relativePath);
      } else if ([...extensionSet].some((extension) => entry.name.toLowerCase().endsWith(extension))) {
        paths.push(relativePath);
      }
    }
  }

  await visit(rootPath);
  return paths.sort((left, right) => left < right ? -1 : left > right ? 1 : 0);
}

export function buildMarkdownTree(paths: string[]) {
  const root = [];
  for (const documentPath of paths) {
    const segments = documentPath.split('/');
    let children = root;
    segments.forEach((segment, index) => {
      const isFile = index === segments.length - 1;
      const existing = children.find((node) => node.name === segment);
      if (isFile) {
        if (!existing) children.push({ type: 'file', name: segment, path: documentPath });
        return;
      }
      if (existing) children = existing.children;
      else {
        const folder = { type: 'folder', name: segment, children: [] };
        children.push(folder);
        children = folder.children;
      }
    });
  }

  const sortNodes = (nodes) => nodes
    .sort((left, right) => {
      if (left.type !== right.type) return left.type === 'file' ? -1 : 1;
      return left.name < right.name ? -1 : left.name > right.name ? 1 : 0;
    })
    .map((node) => node.type === 'folder' ? { ...node, children: sortNodes(node.children) } : node);
  return sortNodes(root);
}

export async function readMarkdown(root: string | URL, relativePath: string): Promise<string> {
  return readFile(path.join(toPath(root), relativePath), 'utf8');
}
