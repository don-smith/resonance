import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const IGNORED_DIRECTORIES = new Set(['.git', 'node_modules']);

function toPath(value) {
  return value instanceof URL ? fileURLToPath(value) : value;
}

export async function discoverMarkdownFiles(root) {
  const rootPath = toPath(root);
  const paths = [];

  async function visit(directory, relativeDirectory = '') {
    const entries = await readdir(directory, { withFileTypes: true });

    for (const entry of entries) {
      if (entry.isDirectory() && IGNORED_DIRECTORIES.has(entry.name)) continue;

      const relativePath = path.posix.join(relativeDirectory, entry.name);
      const absolutePath = path.join(directory, entry.name);

      if (entry.isDirectory()) {
        await visit(absolutePath, relativePath);
      } else if (/\.(md|markdown)$/i.test(entry.name)) {
        paths.push(relativePath);
      }
    }
  }

  await visit(rootPath);
  return paths.sort((left, right) => left < right ? -1 : left > right ? 1 : 0);
}

export function buildMarkdownTree(paths) {
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

      if (existing) {
        children = existing.children;
      } else {
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
    .map((node) => node.type === 'folder'
      ? { ...node, children: sortNodes(node.children) }
      : node);

  return sortNodes(root);
}

export async function readMarkdown(root, relativePath) {
  return readFile(path.join(toPath(root), relativePath), 'utf8');
}
