import test from 'node:test';
import assert from 'node:assert/strict';
import { DEFAULT_MARKDOWN_EXTENSIONS, buildMarkdownTree, discoverMarkdownFiles } from '../src/content.ts';
import { createMarkdownRenderer, renderMarkdown } from '../src/markdown.ts';

test('supports configurable discovery filters while preserving defaults', async () => {
  const fixture = new URL('./fixtures/repository/', import.meta.url);
  const files = await discoverMarkdownFiles(fixture);
  assert.deepEqual(files, [
    'README.md',
    'docs/architecture.md',
    'docs/guides/getting-started.md',
    'home.md',
  ]);
  assert.deepEqual(await discoverMarkdownFiles(fixture, { extensions: ['.md'], ignoredDirectories: ['.git', 'node_modules'] }), files);
  assert.deepEqual(await discoverMarkdownFiles(fixture, { extensions: ['.markdown'], ignoredDirectories: ['.git', 'node_modules'] }), []);
  assert.deepEqual(DEFAULT_MARKDOWN_EXTENSIONS, ['.md', '.markdown']);
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

test('creates independently configured safe Markdown renderers', () => {
  const html = createMarkdownRenderer().render('# Heading');
  assert.match(html, /<h1[^>]*>Heading<\/h1>/);
  assert.doesNotMatch(renderMarkdown('<script>alert(1)</script>'), /<script>/);
});
