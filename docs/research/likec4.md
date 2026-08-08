# LikeC4 integration notes

Researched from LikeC4's first-party documentation, repository, and npm packages.

## Findings

- The `likec4` package documents `.c4`/`.likec4` files as the source format and exposes `LikeC4.fromWorkspace()` and `LikeC4.fromSource()`. It exposes `layoutedModel()`/`diagrams()` for renderer-ready views and `getErrors()` for source validation.
- `@likec4/diagram` is the official React component library. Its documented `ReactLikeC4` component accepts a view ID through `LikeC4ModelProvider`, supports pannable/zoomable diagrams, and provides node/navigation callbacks.
- `@likec4/core` exposes `LikeC4Model.fromDump()`, allowing a server-produced layouted model data object to be reconstructed in the browser.
- LikeC4's own package documentation recommends its generated/Vite integrations for larger applications. Resonance has no browser build pipeline, so this package uses a checked-in Bun browser bundle generated from a small TSX adapter and keeps the LikeC4 source model server-owned.

## Sources

- https://likec4.dev
- https://github.com/likec4/likec4/blob/main/packages/likec4/README.md
- https://github.com/likec4/likec4/blob/main/packages/core/README.md
- https://github.com/likec4/likec4/blob/main/packages/diagram/README.md
- https://www.npmjs.com/package/likec4
- https://www.npmjs.com/package/@likec4/core
- https://www.npmjs.com/package/@likec4/diagram
