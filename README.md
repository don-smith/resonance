# Resonance

Resonance is a local-first, peer-to-peer team workspace built on Tauri and
Iroh. Workspace content uses signed file-tree operations and verified immutable
blobs; each member may bind the shared logical tree to a separate private local
root. The desktop can choose or repair that root, browse the workspace tree,
create and edit rendered Markdown files, resolve file conflicts, and exchange
attributed Markdown in public workspace channels. The runtime owns filesystem
polling, authorized file-history recovery, encrypted conversation recovery, and
verified private blob storage. The conversations, workspace-files, and reference
views load as reviewed bundled TypeScript packages. Repository loading, member
packages, and agent execution do not ship yet.

## Prerequisites

- [Node.js](https://nodejs.org/) 22 or newer and pnpm 10.14.0 (activate it with
  Corepack if your Node distribution includes Corepack)
- Rust 1.98.0 with `rustfmt` and `clippy` (the pinned toolchain is declared in
  `rust-toolchain.toml`)
- Platform prerequisites for [Tauri v2](https://v2.tauri.app/start/prerequisites/)
  — on macOS, full Xcode for initial signing setup; on Windows, WebView2 and the
  Microsoft C++ Build Tools

No release secret, updater key, endpoint, or paid cloud account is needed to
develop this shell. macOS development uses an Apple Development certificate
from the developer's own Apple Account and free Xcode Personal Team.

## Start from a clean checkout

```sh
corepack pnpm install --frozen-lockfile
pnpm desktop:dev
```

On macOS, first sign in under Xcode Settings > Accounts and use Manage
Certificates to create an Apple Development certificate. Then run:

```sh
scripts/setup-macos-development-signing.sh
pnpm desktop:dev
```

The setup command asks once for the login Keychain password. If an existing
Resonance installation identity still trusts an older development signer,
choose Always Allow in the one-time Keychain dialog. The command stores only
the selected certificate hash in the ignored `.resonance/.env`; it does not
print or persist the installation identity or Keychain password. If Xcode has
created a certificate but the command cannot find a valid identity, install the
current WWDR intermediate certificate from [Apple PKI](https://www.apple.com/certificateauthority/)
and rerun it.

Each Mac developer should provision a separate Apple Development certificate.
Do not share its private key. Rerun the setup after replacing or renewing the
certificate. The development launcher fails before launch if the configured
identity is missing instead of falling back to ad-hoc or self-signed code.

For a real two-peer local collaboration demonstration on macOS, use two valid
lowercase profile names:

```sh
pnpm desktop:profiles -- alice bob
# later, only after closing alice:
pnpm desktop:profiles -- --reset alice
```

For an offline/reconnect check where one profile must remain running, start each
profile from a separate terminal with its stable port:

```sh
pnpm desktop:profile -- alice 1421
pnpm desktop:profile -- bob 1422
```

Closing Bob now leaves Alice running. Restart `bob` with the same command to
exercise durable offline authorship and direct recovery. Keep the two profile
names and ports distinct.

This command builds debug-only profile peers with separate signed app bundles.
It stores their ignored state under `.resonance/debug-profiles/` and needs the
same Apple Development signing setup. `pnpm desktop:dev` remains the ordinary
single-app launcher and uses native Keychain custody; it does not accept a
profile argument.

The shell opens with workspace bootstrap status. After membership is ready, use
Files to choose a new or empty folder outside Git management. The bound root
contains ordinary shared files beginning with `plans` and no Resonance control
metadata. Markdown editing and conflict resolution remain authority-mediated;
the webview never receives the private root or blob-store location.

## User documentation

Start with the [HTML user documentation](./docs/html/index.html) or its
[equivalent Markdown index](./docs/index.md). The
[workspace files guide](./docs/workspace-files.md) covers root selection,
rendered and external editing, offline synchronization, conflict resolution,
recovery, and current limits. The [conversations guide](./docs/conversations.md)
explains public channels, attributed Markdown, local unread state, direct-network
synchronization, and current exclusions.

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

The manifest and capability schemas, scaffold, shared fixtures, SDK, and package
checks live under [`packages/contracts/`](./packages/contracts/) and
[`packages/sdk/`](./packages/sdk/). Start with the
[package authoring guide](./docs/package-authoring.md), the small
[`reference package`](./packages/reference-package/), and the complete
[`workspace-files package`](./packages/workspace-files/), and the
[`conversations package`](./packages/conversations/). `pnpm packages:check`
checks catalogs, imports, capabilities, dependencies, and CSS scope. Bundled
packages are reviewed code in one webview; member packages remain deferred to
separately labelled webviews. Development workspace storage and recovery are
documented in [local data](./docs/local-data.md).

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
