# Shell package

The Shell package owns the application frame. It provides the stable page document, primary workspace navigation, workspace mount region, and browser coordinator that switches between installed package surfaces.

## Responsibilities

- Render the shared sidebar and navigation controls.
- Present the viewed repository's name, optional version, and tagline from the host manifest.
- Create one private DOM mount for each browser package that provides a workspace.
- Activate and deactivate package instances without owning their internal state.
- Serve the Shell entrypoint and shared stylesheet.
- Expose composition metadata through the host manifest without owning package routes.

## Configuration

Configure Shell as an entry in the repository manifest’s `packages` object:

```json
{
  "version": 1,
  "packages": {
    "shell": { "module": "src/packages/shell/index.ts" }
  }
}
```

- `module` is required at load time and must be a non-empty path relative to the Resonance application root.
- `enabled` is an optional common package flag, but Shell cannot be disabled because it owns `/` and the browser bootstrap.
- Shell has no package-specific configuration fields; other package inputs are ignored. Repository presentation metadata is configured at the manifest root, not as Shell input:

```json
{
  "version": 1,
  "repository": { "name": "my-project", "tagline": "A short description." },
  "packages": { "shell": { "module": "src/packages/shell/index.ts" } }
}
```

The configured module must load successfully and default-export the Shell package. Shell provides the application frame; it does not discover Markdown, render repository content, or select a Home source.

## Files

- `index.html` — Shell document and mount points.
- `app.js` — browser coordinator and manifest bootstrap.
- `shell.js` — navigation, mount creation, activation, and rollback behavior.
- `styles.css` — shared design tokens and Shell layout.
- `index.ts` — Shell route-free registration and compatibility assets.

The Shell intentionally does not discover Markdown, render repository content, or select a Home source. Those policies belong to the Home and Docs packages.
