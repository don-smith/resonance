# Home package

The Home package owns the repository landing page. It reads the configured source from the repository, returns its original content and browser-ready HTML, and mounts that content inside a private Home surface.

## Responsibilities

- Read `packages.home.source` from `.theview.json`.
- Fall back to `README.md` when no manifest is present.
- Accept relative `.md`, `.markdown`, `.html`, and `.htm` sources.
- Render Markdown with the safe shared Markdown renderer.
- Insert repository-owned HTML sources unchanged so a repository can provide a distinct landing page with scoped styles.
- Serve `/api/home` and the Home browser assets.

## Files

- `src/packages/home.ts` — source validation, containment handoff, loading, and response shaping.
- `public/home.js` — private Home mount and activation lifecycle.
- `public/home.css` — Home surface styles.
- `.theview.json` — this repository’s Home source selection.
- `.theview/home.html` — this repository’s repository-owned landing page.

HTML sources are inserted as trusted local markup. Point the manifest only at files owned by the repository, and scope page-specific selectors below a page root such as `.repository-home`.
