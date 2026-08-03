import test from 'node:test';
import assert from 'node:assert/strict';
import { buildMarkdownTree, discoverMarkdownFiles } from '../src/content.ts';
import { renderMarkdown } from '../src/markdown.ts';

test('discovers Markdown files recursively while ignoring generated dependency directories', async () => {
  const files = await discoverMarkdownFiles(new URL('./fixtures/repository/', import.meta.url));

  assert.deepEqual(files, [
    'README.md',
    'docs/architecture.md',
    'docs/guides/getting-started.md',
  ]);
});

test('builds a navigable folder tree from Markdown paths', () => {
  assert.deepEqual(buildMarkdownTree([
    'README.md',
    'docs/architecture.md',
    'docs/guides/getting-started.md',
  ]), [
    { type: 'file', name: 'README.md', path: 'README.md' },
    {
      type: 'folder',
      name: 'docs',
      children: [
        { type: 'file', name: 'architecture.md', path: 'docs/architecture.md' },
        {
          type: 'folder',
          name: 'guides',
          children: [
            { type: 'file', name: 'getting-started.md', path: 'docs/guides/getting-started.md' },
          ],
        },
      ],
    },
  ]);
});

test('renders safe Markdown for the document pane', () => {
  const html = renderMarkdown('# Heading\n\nA [link](https://example.com).\n\n```js\nconst answer = 42;\n```\n\n<script>alert(1)</script>');

  assert.match(html, /<h1[^>]*>Heading<\/h1>/);
  assert.match(html, /<a href="https:\/\/example.com">link<\/a>/);
  assert.match(html, /<code class="language-js">/);
  assert.doesNotMatch(html, /<script>/);
});
