# 0002 — Use Tauri as the app shell, not Electron

Status: accepted (2026-08-21, Don Smith).

## Context

The collaborative workspace app needs a desktop shell. The two practical options for a TypeScript/web-frontend app are Electron and Tauri. Copland used Electron; the Resonance CLI used a plain Bun HTTP server (not a desktop shell at all).

The P2P sync layer (Iroh) is written in Rust. The choice of shell determines whether Iroh runs in the same process as the shell or as a sidecar.

## Options

### Option A — Electron

Electron bundles Chromium and Node.js. The Holepunch stack (Hyperswarm, Hypercore) runs natively in Node. This was Copland's model.

Problems:
- Electron binaries are ~120 MB+. Updates are slow to download and install, which is a meaningful friction point for a team app that updates frequently.
- The P2P layer would remain the Holepunch stack (Node-native). To use Iroh instead, a separate Rust process would be needed, communicating over IPC — worse than Tauri's native integration.
- Slow startup time; noticeable on developer machines, more noticeable on management machines.

### Option B — Tauri — chosen

Tauri uses the system WebView (WebKit on macOS, WebView2 on Windows) and a Rust backend. Binaries are ~5–15 MB.

Advantages:
- Binary size makes updates fast to deliver and install — important for a frequently-updated team app.
- Iroh (Rust) integrates as a Rust crate in the Tauri backend. No sidecar, no cross-language IPC protocol.
- Tauri's plugin system gives packages a clean capability boundary.
- `tauri-plugin-updater` provides auto-update out of the box.
- Tauri's event system (Rust ↔ webview) serves as the event bus without additional infrastructure.

Tradeoffs accepted:
- System WebView differences across platforms (macOS WebKit vs Windows WebView2) can produce rendering inconsistencies. Mitigated by testing on both platforms and avoiding WebKit-only CSS features.
- Tauri is less mature than Electron. The plugin ecosystem is smaller. This is acceptable given that Resonance's Rust requirements are narrow: Iroh, keychain, file watching, auto-update.
- Rust knowledge is required for the runtime layer. Package authors do not need Rust; only runtime contributors do. This is acceptable given Don's intent to re-engage with Rust and the expectation that the runtime is stable and rarely touched by teams.

## Evidence

- Copland demonstrated that the Electron model works but introduced update friction and a large install footprint.
- Tauri's auto-updater is production-grade as of Tauri v2.
- Iroh provides a Rust crate that integrates cleanly with Tauri's async runtime (Tokio).

## Consequences

- The frontend web stack (HTML/CSS/TypeScript, any framework) is unchanged. Package authors carry over their skills.
- The runtime layer requires Rust. Contributors to the runtime need Rust knowledge; package authors do not.
- Platform testing must cover macOS and Windows WebView variants.
- The Holepunch stack (Hyperswarm, Hypercore, Autobase) is not used. Its lessons are applied to protocol design, not its code.
