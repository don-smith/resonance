# Docs package

The Docs package provides the repository Markdown workspace. It discovers configured document types, projects them into a tree, reads selected files, renders Markdown, and exposes canonical namespaced routes.

## Responsibilities

- Discover files using configured `extensions` and `ignoredDirectories` inputs.
- Build the sorted folder/file tree shown in the Docs mount.
- Read and render selected Markdown documents.
- Enforce the Markdown extension policy after host repository containment.
- Serve `/api/docs/tree` and `/api/docs/document`.
- Serve the Docs browser entrypoint and stylesheet.

## Configuration

Configure Docs as an entry in the repository manifest’s `packages` object:

```json
{
  "version": 1,
  "packages": {
    "docs": {
      "module": "src/packages/docs/index.ts",
      "extensions": [".md", ".markdown"],
      "ignoredDirectories": [".git", "node_modules"]
    }
  }
}
```

- `module` is required at load time and must be a non-empty path relative to the Resonance application root.
- `enabled` is an optional common package flag; `false` omits Docs from the host.
- `extensions` is optional and defaults to `[".md", ".markdown"]`. Every value must be a dotted string.
- `ignoredDirectories` is optional and defaults to `[".git", "node_modules"]`. Every value must be a non-empty directory name.

Docs uses `extensions` when discovering and reading documents, and applies `ignoredDirectories` during discovery and document access. Omit either option to use its default; provide both when the repository uses a different Markdown extension policy or needs additional directories excluded.

## Ownership boundary

`src/packages/docs/` contains reusable package implementation. Discovery and document reads continue from the viewed repository root supplied by `HostContext`; shared `src/content.ts` and `src/markdown.ts` remain outside this package because Home uses them too.
