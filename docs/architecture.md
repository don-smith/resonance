# First-slice architecture

The CLI lives in this repository, but the server reads the **current working directory** as its content root. That is what lets theview be developed here and exercised against another repository.

The server exposes two small JSON endpoints:

- `/api/tree` discovers Markdown files and returns the navigation tree;
- `/api/document?path=...` returns the selected document and rendered HTML.

The browser shell stays deliberately plain: HTML, CSS, and JavaScript served by Bun. The first-level workspace navigation separates Home from Docs; Docs owns the second-level file tree, while the main pane renders the selected document. The file tree is structural, while Markdown rendering is handled on the server.
