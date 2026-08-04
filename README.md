# theview

A local cockpit for seeing the shape of the application you are building.

Theview is composed from a small, deterministic set of packages:

- **Shell** owns navigation and package mount regions.
- **Home** renders the configured repository landing source (`README.md` by default, or repository-owned Markdown/HTML).
- **Docs** owns Markdown discovery, tree navigation, and document rendering.

## Try it

Install this checkout into a user-local bin directory:

```sh
./scripts/install-local.sh
source ~/.zshrc
```

Then run theview from any repository:

```sh
cd /path/to/another/repository
theview
```

The command reads `.theview.json` from the current repository. If it is absent, version-one defaults use `README.md` for Home and `.md`/`.markdown` files for Docs. The server starts at port 4317, moves to the next available port if needed, and opens the selected URL in the default browser.

## Repository configuration

The manifest is optional. Without `.theview.json`, Home reads `README.md` and Docs discovers `.md`/`.markdown` files while ignoring `.git` and `node_modules`.

```json
{
  "version": 1,
  "packages": {
    "home": { "source": "README.md" },
    "docs": {
      "extensions": [".md", ".markdown"],
      "ignoredDirectories": [".git", "node_modules"]
    }
  }
}
```

### Create a custom Home page

1. Add a repository-owned HTML fragment, for example `.theview/home.html`.
2. Put page-specific selectors below one root class such as `.repository-home`; inline `<style>` blocks are supported for scoped page styling.
3. Point the Home package at the file in `.theview.json`:

   ```json
   {
     "version": 1,
     "packages": {
       "home": { "source": ".theview/home.html" }
     }
   }
   ```

Home accepts relative `.md`, `.markdown`, `.html`, and `.htm` sources. Markdown is rendered safely by the server. HTML is repository-owned markup inserted unchanged into the Home mount, so only configure files you trust. If the source is absent, Home shows a package-local error while Docs remains available.

This repository uses that setup in `.theview.json` and `.theview/home.html`. The package responsibilities are documented in `docs/packages/shell.md`, `docs/packages/home.md`, and `docs/packages/docs.md`.

Package routes are canonical under `/api/<package-id>/...`; Docs also keeps `/api/tree` and `/api/document` as compatibility aliases. Package assets are served under `/assets/<package-id>/...`.

## Develop theview

Install dependencies and run the tests with Bun:

```sh
bun install
bun test
```

The local Bun script delegates to the installed CLI:

```sh
bun run theview
```

The installer keeps checkout symlinks for development. Packaged runtimes can set `THEVIEW_RUNTIME_ROOT` while retaining the same CLI entrypoint.
