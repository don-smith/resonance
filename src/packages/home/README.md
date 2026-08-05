# Home package

The Home package owns the repository landing page. It reads the configured source from `.resonance/config.json`, returns its original content and browser-ready HTML, and mounts that content inside a private Home surface.

## Responsibilities

- Read `packages.home.source` from `.resonance/config.json`.
- Fall back to `README.md` when no config is present.
- Accept relative `.md`, `.markdown`, `.html`, and `.htm` sources.
- Render Markdown with the safe shared Markdown renderer.
- Insert repository-owned HTML sources unchanged so a repository can provide a distinct landing page with scoped styles.
- Serve `/api/home` and the Home browser assets.

## Ownership boundary

`src/packages/home/` contains reusable package implementation. Configured sources such as `.resonance/home.html`, `README.md`, and repository Markdown remain under the viewed repository root and are resolved through the host containment API, never relative to this package folder.

HTML sources are inserted as trusted local markup. Point the manifest only at files owned by the repository, and scope page-specific selectors below a page root such as `.repository-home`.
