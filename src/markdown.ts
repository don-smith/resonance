import MarkdownIt from 'markdown-it';

const DEFAULT_OPTIONS = { html: false, linkify: false, typographer: false };

export function createMarkdownRenderer(options = {}) {
  return new MarkdownIt({ ...DEFAULT_OPTIONS, ...options });
}

export function renderMarkdown(source: string): string {
  return createMarkdownRenderer().render(source);
}
