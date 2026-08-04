# Docs package

The Docs package owns the repository’s Markdown workspace. It discovers configured document types, projects them into a tree, reads selected files, renders Markdown, and keeps the original Docs API aliases working.

## Responsibilities

- Discover files using the configured `extensions` and `ignoredDirectories` inputs.
- Build the sorted folder/file tree shown in the Docs mount.
- Read and render selected Markdown documents.
- Enforce the Markdown extension policy after the host validates repository containment.
- Serve canonical `/api/docs/tree` and `/api/docs/document` routes.
- Preserve `/api/tree` and `/api/document` as compatibility aliases.
- Serve the Docs browser entrypoint and stylesheet.

## Files

- `src/packages/docs.ts` — Docs inputs, handlers, routes, and registration.
- `src/content.ts` — generic discovery, tree projection, and file reading helpers.
- `src/markdown.ts` — safe Markdown renderer factory and default renderer.
- `public/docs.js` — private tree, document, selection, loading, and error lifecycle.
- `public/docs.css` — Docs sidebar, tree, document, and responsive styles.

The host supplies transport and repository path containment; Docs owns the policy that determines which repository files count as documents.
