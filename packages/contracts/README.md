# Resonance package contracts

`schema/manifest.v2.json` defines reviewed bundled packages. `schema/workspace-files.v1.json` independently defines the secret-free workspace-files capability wire contract. TypeScript and Rust use hand-written types and validate the same fixtures under `fixtures/`.

## Manifest v2

A manifest declares `source: "bundled"`, a namespaced ID, display and navigation metadata, a package-relative `content.entry`, events, minimum role, optional semantic capabilities, and optional agent configuration. The catalog generator validates every direct `packages/*/manifest.json`, bounds its entry inside the package, sorts by ID, and writes the TypeScript and Rust catalogs.

Roles are `viewer`, `contributor`, and `developer`. Standard events are `repo:changed`, `doc:updated`, `doc:opened`, `message:received`, `peer:joined`, `peer:left`, `peer:connection`, `workspace:member-added`, and `workspace:member-removed`. Packages may also declare lowercase domain events and `agent-context:*` events.

Current semantic capabilities are `documents:read`, `documents:write`, `workspace:read`, `repository:read`, `telemetry:write`, and `workspace-files:v1`. A capability version does not imply a manifest version change.

## Workspace-files v1

Workspace-files v1 covers snapshots, local-root actions through host-owned dialogs, Markdown revisions, previews, creation and replacement, conflict resolution, payload-free invalidation, and finite safe errors. Unknown fields and private runtime details are invalid. TypeScript validates requests and results before transport; Rust validates them again before calling `WorkspaceFileRuntime`.

## Scaffold and checks

```sh
pnpm --filter @resonance/contracts generate -- \
  --id resonance.my-package --output packages/my-package
pnpm packages:generate
pnpm packages:check
pnpm --filter @resonance/contracts test
```

The scaffold writes a complete lifecycle package and regenerates both catalogs when the output is a direct package directory. See [`../../docs/package-authoring.md`](../../docs/package-authoring.md), [`../reference-package`](../reference-package), and [`../workspace-files`](../workspace-files).
