import MarkdownIt from 'markdown-it';

const markdown = new MarkdownIt({
  html: false,
  linkify: false,
  typographer: false,
});

export function renderMarkdown(source) {
  return markdown.render(source);
}
