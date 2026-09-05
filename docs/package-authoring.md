# Package authoring

Resonance builds reviewed TypeScript packages from `packages/*`. A bundled package contributes content to the shared desktop webview. It does not register Rust commands or import desktop implementation code.

## Create a package

Run the scaffold from the repository root:

```sh
pnpm --filter @resonance/contracts generate -- \
  --id resonance.my-package --output packages/my-package
```

The scaffold writes:

- `packages/my-package/manifest.json`, the manifest v2 declaration;
- `packages/my-package/package.json`, including the SDK dependency;
- `packages/my-package/src/index.ts`, the lifecycle entry;
- `packages/my-package/src/styles.css`, scoped package styles;
- `packages/my-package/src/index.test.ts`, a lifecycle test.

It also regenerates `apps/desktop/src/generated/bundled-package-catalog.ts` and `apps/desktop/src-tauri/generated/bundled-package-manifests.json`. The next Vite development build includes the package. No Rust source edit is needed.

## Manifest v2

A bundled manifest has these fields:

- `manifestVersion: 2` and `source: "bundled"`;
- a lowercase `namespace.name` `id`;
- `name`, `description`, and `nav.label`/`nav.icon` display metadata;
- `content.entry`, a package-relative `.ts` file inside the package;
- `events.emits` and `events.consumes`;
- `minRole`, one of `viewer`, `contributor`, or `developer`;
- optional semantic `capabilities` and agent configuration.

Manifest and capability versions are independent. For example, `workspace-files:v1` and `conversations:v1` can evolve without changing manifest v2. The catalog rejects duplicate IDs, absolute entries, traversal, symlink escapes, missing entries, and unknown manifest fields.

## Lifecycle

The entry exports `mount(root, context)`. The host calls it at most once and retains the returned instance:

```ts
export const mount: PackageContentModule["mount"] = (root, context) => ({
  activate() {},
  deactivate() {},
  dispose() {},
});
```

The order is `mount`, `activate`, then any number of `deactivate`/`activate` transitions, followed by one effective `dispose`. Normal navigation deactivates the package without discarding its DOM or state. `dispose` must be idempotent and release every package-owned listener, capability subscription, timer, editor, worker, and object URL.

If `mount` allocates a resource and then fails, clean it before throwing or throw `PackageMountError` with a cleanup callback. Import, mount, activation, deactivation, and disposal failures stay inside that package's content region.

## SDK and capabilities

Import package interfaces from `@resonance/package-sdk`. `PackageContext` contains immutable package identity, declared-event access, shell design-token names, and only the capabilities declared by the validated manifest.

A package may emit and consume only declared events. Privileged work uses a semantic capability such as `context.capabilities.workspaceFilesV1` or `context.capabilities.conversationsV1`; packages never receive command names, raw Tauri transport, desktop state, local paths, keys, tokens, persistence details, signed operations, blobs, watcher state, SQL details, Iroh handles, or Commonware handles.

Request a new runtime capability only when the operation needs host or Rust authority and cannot be implemented from existing SDK methods. The proposal must define one versioned semantic interface, bounded secret-free request and result shapes, finite safe errors, production and test adapters, shared TypeScript/Rust fixtures, cleanup, and authorization behavior. Do not add a generic invoke escape hatch.

## Dependency and style rules

Bundled source may depend on the SDK, browser libraries, and package-owned libraries. It must not import:

- `@tauri-apps/api`;
- `apps/desktop` or `@resonance/desktop`;
- Rust runtime internals;
- absolute files or relative paths that leave its package.

Declare every dependency in the package's `package.json`. First-party CSS selectors must begin with `[data-package-id="<manifest-id>"]` or use CSS Modules. Use the design-token custom properties exposed by the shell for colors, type, spacing, and borders. A package-owned third-party stylesheet may keep its library selectors, but package overrides must remain below the package root.

## Development loop

Use ordinary Vite rebuilds while editing an existing package:

```sh
pnpm desktop:dev
```

After adding or renaming a package, regenerate and check the catalogs:

```sh
pnpm packages:generate
pnpm packages:check
```

Run package tests and the repository gates before delivery:

```sh
pnpm exec vitest run packages/my-package/src
pnpm typecheck
pnpm check
pnpm build:desktop
```

`pnpm packages:check` rejects stale catalogs, forbidden imports and dependencies, undeclared capability use, and unscoped first-party CSS.

## Trust boundary

Bundled packages are reviewed code in the main webview. Import checks and the explicit application-command allowlist prevent accidental coupling; they do not sandbox JavaScript modules from each other. Do not ship member-loaded or otherwise unreviewed code through this host. A future member-package loader must use separately labelled webviews with separate Tauri capabilities while preserving the semantic SDK interface.

See `packages/reference-package` for a small package and `packages/workspace-files` for a complete capability-backed package.
