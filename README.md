# resonance

Resonance is an **Integrated Application Environment** for understanding, evolving, and operating a software application. It brings architecture, documentation, workflows, agents, skills, plans, operational context, runtime information, developer tooling, and team knowledge into one environment around the application itself.

The repository defines the team’s shared understanding; each developer extends it with their own experience and tools, and the runtime brings those layers together. Resonance raises the level at which humans spend their attention—from individual source files toward intent, constraints, relationships, evidence, operational state, and system behaviour. Resonance is the environment; **resonate** is the action.

Resonance is composed from package folders under `src/packages/<package-id>`:

- **Shell** owns navigation, package mounts, the fixed browser bootstrap, and shared layout.
- **Home** renders the configured repository landing source (`README.md` by default, or repository-owned Markdown/HTML).
- **Docs** owns Markdown discovery, tree navigation, and document rendering.
- **Pi Agent** owns one developer-specific server-side Pi ACP session, prompt submission, incremental activity events, and local recovery controls.

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

The command reads `.resonance/config.json` from the current repository. If it is absent, version-one defaults select the built-in Shell, Home, Docs, and Pi Agent modules, use `README.md` for Home, and discover `.md`/`.markdown` files for Docs while ignoring `.git` and `node_modules`. Pi Agent is local-only and requires an installed `pi` executable plus configured model credentials. The server starts at port 4317, moves to the next available port if needed, and opens the selected URL.

## Repository configuration

The config is optional. Each package entry explicitly names an app-root-relative server module. Package modules own routes and assets; remaining fields are passed to that package as inputs.

```json
{
  "version": 1,
  "packages": {
    "shell": { "module": "src/packages/shell/index.ts" },
    "home": { "module": "src/packages/home/index.ts", "source": "README.md" },
    "docs": {
      "module": "src/packages/docs/index.ts",
      "extensions": [".md", ".markdown"],
      "ignoredDirectories": [".git", "node_modules"]
    },
    "pi-agent": { "module": "src/packages/pi-agent/index.ts" }
  }
}
```

Set `enabled` to `false` to omit an optional package. Shell is required because it owns `/` and the browser bootstrap. Invalid optional module entries are skipped with a warning; an invalid Shell module prevents startup. A stale legacy manifest is not read or migrated.

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

Home accepts relative `.md`, `.markdown`, `.html`, and `.htm` sources. Markdown is rendered safely; HTML is trusted repository-owned markup inserted unchanged. Package responsibilities are documented in `src/packages/shell/README.md`, `src/packages/home/README.md`, `src/packages/docs/README.md`, and `src/packages/pi-agent/README.md`.

Package routes are canonical under `/api/<package-id>/...`; Docs uses `/api/docs/tree` and `/api/docs/document`, while Pi Agent uses `POST /api/pi-agent/prompt`, `GET /api/pi-agent/events`, `GET /api/pi-agent/state`, and `POST /api/pi-agent/reset`. Package assets retain `/assets/<package-id>/...` public URLs while their physical files live inside package folders.

## Develop resonance

Pi Agent uses the root-owned `pi-acp@0.0.33` adapter and the ACP SDK; Resonance starts that adapter from the launch repository and does not use `npx` or `bunx`. The external Pi runtime remains a local prerequisite. Prompts are streamed over Server-Sent Events, and the in-memory transcript remains available while navigating between Pi Agent and Docs.

```sh
bun install
bun test
```

The local Bun script delegates to the installed CLI:

```sh
bun run resonate
```
