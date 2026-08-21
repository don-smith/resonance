# Package authoring

A Resonance package starts with a versioned manifest, not direct runtime
imports. Phase 1 supports reviewed, bundled team packages only; member and
repository loading are intentionally unavailable until they can run in separate
least-privilege webviews.

## Scaffold

```sh
pnpm --filter @resonance/contracts generate -- \
  --id resonance.my-package --output packages/my-package
```

Edit the generated `manifest.json`, then run:

```sh
pnpm --filter @resonance/contracts test
cargo test -p resonance-runtime --test package_registry_conformance
cargo test -p resonance-runtime --test package_bus
```

The TypeScript authoring adapter and Rust registry validate the same conformance
fixtures. The worked [`reference package`](../packages/reference-package/) is
the complete manifest example.

## Phase 1 boundary

Generation creates a valid manifest contract; it does not yet make a new
package appear in the desktop shell. Phase 1 bundles and validates only the
reference manifest, and ships neither package webview loading, tabs, nor agent
execution. Keep generated packages alongside the runtime and validate them in
CI while the content-package surface is designed in a later workstream.

## Manifest rules

- `manifestVersion` is `1`, and `source` is `bundled-team`.
- IDs are lowercase `namespace.name`; reference packages use `resonance.*`.
- `minRole` is one of `viewer`, `contributor`, or `developer`.
- Declare every event emitted or consumed. The runtime routes a declared emit
  without interpreting its payload. An undeclared emit is rejected with a
  warning in development and silently dropped in production.
- Capabilities and agent permissions describe semantic operations, not Tauri
  command names. The finite vocabularies are listed in the contract README.
- An optional `agent` has `systemPrompt`, `permissions`, and
  `contextProviders`. It has no `agentPanel` field: packages supply context,
  while the runtime owns the one shared panel UI.

A package uses `@resonance/package-sdk` for its event transport. It must not
hold a reference to another package or access filesystem, signing, or updater
APIs directly.
