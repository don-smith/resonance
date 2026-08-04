# Shell package

The Shell package owns the application frame. It provides the stable page document, primary workspace navigation, package mount region, and the browser coordinator that switches between installed package mounts.

## Responsibilities

- Render the shared sidebar and navigation controls.
- Create one private DOM mount for each browser package.
- Activate and deactivate package instances without owning their internal state.
- Serve the shell entrypoint and shared shell stylesheet.
- Expose composition metadata through the host manifest without owning package routes.

## Files

- `public/index.html` — shell document and mount points.
- `public/app.js` — browser coordinator and manifest bootstrap.
- `public/shell.js` — navigation, mount creation, activation, and rollback behavior.
- `public/styles.css` — shared design tokens and shell layout.
- `src/packages/shell.ts` — shell route-free registration and compatibility assets.

The shell intentionally does not discover Markdown, render repository content, or select a Home source. Those policies belong to the Home and Docs packages.
