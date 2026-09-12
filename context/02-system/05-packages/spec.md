# Packages — Spec

## Status

Active.

This spec records the bundled content host, manifest contract, semantic capability boundary, and standard event vocabulary authorized by Decision 0010. Runtime-loaded member and repository packages remain deferred.

## 1. Bundled manifest and catalog

A bundled package has a direct `packages/*/manifest.json` file that conforms to `manifestVersion: 2`. Its `source` is `bundled`, and `content.entry` names a package-relative TypeScript file. The entry must exist inside the package. Absolute paths, traversal, symlink escapes, duplicate IDs, unknown fields, and unsupported semantic declarations are invalid.

One deterministic generator sorts packages by ID and writes both build inputs. The TypeScript catalog contains literal lazy imports for Vite. The Rust catalog contains the same normalized manifests. Both outputs are committed, end with a newline, and must match generator check mode.

Manifest version and semantic capability versions are independent. A manifest may declare `workspace-files:v1` or `conversations:v1` without tying that capability's evolution to the manifest contract.

## 2. Shared-webview content lifecycle

The desktop shell owns package discovery, navigation, mount elements, active selection, and package-local error rendering. A bundled package mounts into a host-owned element at most once and returns a retained instance with `activate`, `deactivate`, and idempotent `dispose` methods. Deactivation preserves package state. Disposal releases resources created by the package.

Import, mount, activation, deactivation, and disposal failures stay inside that package's content region. They do not replace onboarding, membership controls, peer state, navigation, or another package mount.

Bundled modules in the main webview are reviewed code in one trust domain. Source checks and explicit Tauri command permissions restrict accidental dependencies but do not isolate modules from each other. Member-loaded packages require separately labelled webviews before implementation.

## 3. Package context and semantic capabilities

The host constructs `PackageContext` from the validated manifest. The context contains package identity, declared-event access, shell design-token names, and only declared semantic capability adapters. It does not expose mutable manifest authority, desktop state, a generic command function, Tauri transport objects, local paths, persistence details, identity keys, workspace tokens, signed operations, blob locations, watcher state, SQL details, or Iroh handles.

Privileged operations use request and response methods on a versioned semantic capability. Events announce state changes but do not carry authoritative snapshots. The adapter fetches current state after invalidation, suppresses stale responses, validates wire values, and owns listener cleanup. Rust validates every operation and maps internal failures to finite safe errors. `conversations:v1` follows this boundary for channel lifecycle, message queries, posting, unread state, and synchronization status without exposing chat keys, mesh handles, addresses, or archive details.

## 4. Authoring enforcement

The package scaffold writes a complete manifest, package metadata, lifecycle entry, scoped styles, and lifecycle test, then regenerates the catalog. A direct package appears in the next development build without a Rust source edit.

Repository checks reject stale catalogs, `@tauri-apps/api`, desktop and runtime implementation imports, absolute or escaping imports, undeclared capability use, forbidden host dependencies, and unscoped first-party package CSS. Package-owned third-party library styles are the only selector exception.

`resonance.workspace-files` is the reference capability-backed package. It owns the files tree, root controls, Markdown editor and draft state, previews, conflicts, subscriptions, object URLs, listeners, timers, and styles. `resonance.conversations` follows the same host boundary for public channels, attributed Markdown, local unread state, and finite synchronization labels through `conversations:v1`; it has no reply, thread, removal-approval, read-receipt, or transport UI. The shell retains onboarding, membership, peers, package navigation, package mounts, and package-local error regions.

## 5. Runtime event vocabulary

The runtime owns standard event names. Packages may consume declared standard events but cannot receive installation or recipient private keys, epoch keys, workspace tokens, raw Iroh or Commonware handles, exact signed records, filesystem paths, SQL values, or unvalidated membership data. Conversation refresh uses `conversations:changed` with workspace ID, channel ID, and optional message ID only.

The peer lifecycle vocabulary is `peer:joined`, `peer:left`, and `peer:connection`. `peer:connection` carries only a public member identifier and a secret-free connection or presence state. The runtime derives every event from the active workspace session. Packages do not infer membership from gossip traffic.
