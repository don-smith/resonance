# Docs package

The Docs package owns the repository Markdown workspace. It discovers configured document types, projects them into a tree, reads selected files, renders Markdown, and exposes canonical namespaced routes.

## Responsibilities

- Discover files using configured `extensions` and `ignoredDirectories` inputs.
- Build the sorted folder/file tree shown in the Docs mount.
- Read and render selected Markdown documents.
- Enforce the Markdown extension policy after host repository containment.
- Serve `/api/docs/tree` and `/api/docs/document`.
- Serve the Docs browser entrypoint and stylesheet.

## Ownership boundary

`src/packages/docs/` contains reusable package implementation. Discovery and document reads continue from the viewed repository root supplied by `HostContext`; shared `src/content.ts` and `src/markdown.ts` remain outside this package because Home uses them too.

The old `/api/tree` and `/api/document` aliases are intentionally removed in this clean-break design.
