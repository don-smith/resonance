# Resonance

Resonance is a local-first, peer-to-peer team workspace built on Tauri, Iroh,
and Yjs. This repository currently provides the Phase 1 runtime foundation: a
minimal desktop shell and the boundaries on which local persistence, packages,
and verified update delivery will be built. It intentionally does **not** ship
identity, collaboration, content packages, conversations, repository loading,
or agent execution.

## Prerequisites

- [Node.js](https://nodejs.org/) 22 or newer and pnpm 10.14.0 (activate it with
  Corepack if your Node distribution includes Corepack)
- Rust 1.98.0 with `rustfmt` and `clippy` (the pinned toolchain is declared in
  `rust-toolchain.toml`)
- Platform prerequisites for [Tauri v2](https://v2.tauri.app/start/prerequisites/)
  — on macOS, Xcode command-line tools; on Windows, WebView2 and the Microsoft
  C++ Build Tools

No secret, signing key, update endpoint, or cloud account is needed to develop
this shell.

## Start from a clean checkout

```sh
corepack pnpm install --frozen-lockfile
pnpm desktop:dev
```

The shell opens with navigation, local-workspace bootstrap status, and no
content surface.

## Validate and build

```sh
pnpm check
pnpm build:desktop
```

`pnpm check` is the CI-equivalent entry point. It runs formatting, TypeScript,
Vitest (including VRS structural validation), Rust formatting/check/test/Clippy,
and package-contract gates. The GitHub Actions quality workflow invokes that
same command.

## Package authors

The versioned package-manifest schema, generator, shared fixtures, and API
vocabulary live in [`packages/contracts/`](./packages/contracts/). Start with
the [package authoring guide](./docs/package-authoring.md) and the worked
[`reference package`](./packages/reference-package/). Only bundled, reviewed
team packages are supported in this foundation; no package content view or
agent execution ships yet. Development workspace storage, migration, and recovery
behavior are documented in [local data](./docs/local-data.md).

## Fork release delivery

The updater is a shell-only, default-deny seam. Development has no update
configuration; the checked-in release example is deliberately invalid and
`pnpm release:validate -- --config config/release.example.json` fails closed.
A fork owner must provision an HTTPS manifest endpoint, public updater key, CI
signing secret, signed artifacts, and a static `latest.json` manifest before a
release can work. Follow the [fork and release guide](./docs/fork-guide.md) for
first release, two-custodian recovery, compromise response, and old-key bridge
rotation. Live signing, hosting, and installation remain intentionally outside
this foundation phase.

## Design documentation

All vision, requirements, architecture decisions, and open questions live in
[`context/`](./context/). That directory is the authoritative source of record
for the project and uses the LiveStore VRS convention (vision → requirements →
spec → decisions → delta).
