# resonance

Resonance is an **Integrated Application Environment** for understanding, evolving, and operating a software application. It brings architecture, documentation, workflows, agents, skills, plans, operational context, runtime information, developer tooling, and team knowledge into one environment around the application itself.

The repository defines the team’s shared understanding; each developer extends it with their own experience and tools, and the runtime brings those layers together. Resonance raises the level at which humans spend their attention—from individual source files toward intent, constraints, relationships, evidence, operational state, and system behaviour. Resonance is the environment; **resonate** is the action.

Resonance is composed from package folders under `src/packages/<package-id>`:

- **Shell** owns navigation, workspace mounts, the fixed browser bootstrap, and shared layout.
- **Home** renders the configured repository landing source (`README.md` by default, or repository-owned Markdown/HTML).
- **Docs** owns Markdown discovery, tree navigation, and document rendering.

A **package** is Resonance's general extensibility and implementation unit. A **workspace** is a user-visible surface mounted and navigated by Shell. Home, Docs, and Backlog are packages that provide workspaces; Shell is infrastructure and is not a workspace. Member packages are external packages selected per developer. Package terminology remains authoritative for source folders, manifest entries, package IDs, routes, assets, contracts, and the authoring CLI.

## Try it

Install this checkout into a user-local bin directory:

```sh
./scripts/install-local.sh
source ~/.zshrc
```

Then run resonate from any repository:

```sh
cd /path/to/another/repository
resonate
```

The command reads `.resonance/config.json` from the current repository. If it is absent, Resonance reports that the repository is not installed and asks whether to install it. Approval runs the same setup as `resonate install`: Shell is always installed, while Home and Docs can be selected interactively. Existing config files are authoritative: omitted packages are not imported or registered. Member packages are never added by team installation. Select them from an external member repository with `resonate member install /path/to/member-packages`; the ignored `.resonance/member-config.json` selects them for the current repository. The server starts at port 4317, moves to the next available port if needed, and opens the selected URL.

## Telemetry

Resonance logs structured request and package events to the console by default. Telemetry includes the viewed repository's Git origin name (or its directory name when no origin is available) as the `repository` field, so traces from multiple repositories can be distinguished in one Langfuse instance. Agent consumers can group related traces with `telemetry.session(sessionId)`; the Backlog agent uses its conversation thread as the session. Copy `.resonance/.env.example` to `.resonance/.env` in the viewed repository and fill in the local telemetry settings. Resonance loads that file at startup; it is gitignored. Configure process telemetry with `RESONANCE_TELEMETRY=off`, `console`, or `langfuse`; use `RESONANCE_TELEMETRY_LEVEL=debug|info|warn|error` for the console threshold. Langfuse uses `LANGFUSE_BASE_URL` (default `http://127.0.0.1:13000`), `LANGFUSE_PUBLIC_KEY`, and `LANGFUSE_SECRET_KEY`. Set `RESONANCE_TELEMETRY_CAPTURE_CONTENT=true` to export complete agent-turn and model-stream request/response content. The checked-in example enables this for local Langfuse analysis; set it to `false` to retain only structural metadata. Secret-looking fields and values remain redacted in either mode.

## Repository configuration

Use `resonate install` to create `.resonance/config.json`. Installation records the repository name and leaves its optional tagline empty for later curation. Each package entry explicitly names an app-root-relative server module, and the entries form the authoritative package allowlist. Package modules own routes and assets; remaining fields are passed to that package as inputs. Package code receives Resonance-owned structured telemetry through its host context and does not read telemetry credentials directly.

```json
{
  "version": 1,
  "repository": { "name": "my-project", "tagline": "A short description." },
  "packages": {
    "shell": { "module": "src/packages/shell/index.ts" },
    "home": { "module": "src/packages/home/index.ts", "source": "README.md" },
    "docs": {
      "module": "src/packages/docs/index.ts",
      "extensions": [".md", ".markdown"],
      "ignoredDirectories": [".git", "node_modules"]
    },
    "backlog": { "module": "src/packages/backlog/index.ts" }
  }
}
```

Member packages use a separate external manifest, for example `member-packages.json`:

```json
{
  "version": 1,
  "packages": {
    "personal-tools": { "module": "src/packages/personal-tools/index.ts" }
  }
}
```

Create a self-contained member-package repository with:

```sh
resonate member init ~/resonance-member-packages
cd ~/resonance-member-packages
resonate member package create personal-tools
cd /path/to/viewed-repository
resonate member install ~/resonance-member-packages
```

The initializer creates the manifest, Bun package metadata, local package contract, Git repository, and a copy of the package-authoring skill.

Set `enabled` to `false` to omit an optional team package. Shell is required because it owns `/` and the browser bootstrap. Invalid optional module entries are skipped with a warning; an invalid Shell module prevents startup. A stale legacy manifest is not read or migrated. Team packages load before member packages and win conflicts.

### Create a custom Home page

Add a repository-owned HTML fragment such as `.resonance/home.html`, scope its selectors below a root class, and point Home at it:

```json
{
  "version": 1,
  "packages": {
    "home": { "module": "src/packages/home/index.ts", "source": ".resonance/home.html" }
  }
}
```

Home accepts relative `.md`, `.markdown`, `.html`, and `.htm` sources. Markdown is rendered safely; HTML is trusted repository-owned markup inserted unchanged. Package responsibilities are documented in `src/packages/shell/README.md`, `src/packages/home/README.md`, and `src/packages/docs/README.md`.

Package routes are canonical under `/api/<package-id>/...`; Docs uses `/api/docs/tree` and `/api/docs/document`. Package assets retain `/assets/<package-id>/...` public URLs while their physical files resolve from the owning team or member repository. Shell presents team navigation before personal member navigation.

## Develop resonance

Member packages run from their live external checkout and use the same package contract as team packages. Package routes use the package contract's request/response capabilities for JSON bodies and SSE rather than importing Node HTTP objects.

```sh
bun install
bun test
```

The local Bun script delegates to the installed CLI:

```sh
bun run resonate
```
