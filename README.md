# theview

A local cockpit for seeing the shape of the application you are building.

Theview is composed from a small, deterministic set of packages:

- **Shell** owns navigation and package mount regions.
- **Home** renders the configured repository landing source.
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
