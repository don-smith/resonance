# Delivery — Requirements

Role: owns the CI pipeline, update manifest publication, binary signing, platform targets, and the fork guide for teams adopting Resonance.

---

## Requirements

### Build and release

- **RS.DEL-R01 Builds are reproducible from CI.** The CI pipeline (GitHub Actions or equivalent) builds platform binaries from a tagged commit. No local build steps are required beyond what CI runs. `refines: RS-A04`

- **RS.DEL-R02 Binaries are signed before distribution.** Each platform binary is signed with the team's signing key before upload. The signing key is held in CI secrets, not committed to the repository. `refines: RS-R13`

- **RS.DEL-R03 Platform targets are macOS (Apple Silicon and Intel) and Windows.** Linux is a stretch goal for v1. The Tauri backend and Iroh are compatible with Linux; the primary constraint is QA resources.

### Update manifest

- **RS.DEL-R04 The update manifest is a static JSON file.** The manifest contains the latest version string, per-platform download URLs, and per-platform signatures. It is published to GitHub Pages on every release. `refines: RS-R14`

- **RS.DEL-R05 The manifest URL is configurable in the fork.** Teams self-hosting the manifest set the URL in their Tauri config. The runtime uses this URL for all update checks. The default URL points to the reference repository's GitHub Pages. `refines: RS-T06`

### Fork guide

- **RS.DEL-R06 The fork guide is a first-class deliverable.** The fork guide covers: forking the repository, generating a signing key, configuring CI, publishing the update manifest, adding team packages, and rolling out the first update. It targets a developer with GitHub and CI experience but no Rust knowledge. `refines: RS.PROD-R03`

- **RS.DEL-R07 The fork guide includes key rotation procedures.** The procedure for rotating the signing key (when the key is compromised or team ownership changes) is documented. It does not require code changes; it is an operational procedure. `refines: RS-R13`


