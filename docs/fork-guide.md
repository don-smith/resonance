# Fork and release guide

This foundation deliberately cannot publish an update until the fork owner
provisions public configuration and CI secrets. Development never needs a
release host or key: without `config/release.json` the desktop starts with its
update seam disabled.

## Provision a fork

1. Fork the repository and enable GitHub Actions and GitHub Pages (or replace
   the Pages deployment job with the team's HTTPS static host).
2. Generate a Tauri updater signing key with the Tauri CLI. Keep the private
   key and its password out of the repository and developer machines.
3. Put the private key in the `TAURI_SIGNING_PRIVATE_KEY` CI secret and its
   password in `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` when the key is encrypted.
   Set `RESONANCE_RELEASE_CONFIGURATION` to the complete JSON configuration
   described below. It is public build configuration, but treating it as a CI
   value prevents a fork from accidentally shipping example values.
4. Copy `config/release.example.json` to `config/release.json` locally only to
   validate a real configuration. Supply an HTTPS `manifestEndpoint`, HTTPS
   `artifactBaseUrl`, the updater **public** key, and filenames for the Apple
   Silicon, Intel macOS, and Windows updater artifacts. CI bundles this public
   configuration into a release; never commit the local file.
5. Confirm the release guard fails closed before trusting it:

   ```sh
   pnpm release:validate -- --config config/release.json --require-signing-secret
   ```

   The signing secret must be present in CI; it should not be exported in a
   local development shell. The checked-in example intentionally fails this
   command.

## First release

1. Match the release configuration filenames to the signed updater artifacts
   emitted by the fork's Tauri build. The update-manifest generator rejects a
   missing/empty artifact or `.sig` sidecar.
2. Push a `vX.Y.Z` tag. The template validates configuration and signing
   material, builds macOS Apple Silicon, macOS Intel, and Windows targets, then
   collects the signed artifacts.
3. The template generates `latest.json` using:

   ```sh
   pnpm release:manifest -- --config config/release.json \
     --artifacts release-artifacts --version X.Y.Z --output pages/latest.json
   ```

   The template copies the signed artifacts, their `.sig` files, and
   `latest.json` to GitHub Pages. Set `artifactBaseUrl` and `manifestEndpoint`
   to that Pages location (or replace that job with an equivalent HTTPS static
   host) so every generated manifest URL resolves.

4. Before telling users to update, verify that the hosted URL is HTTPS, the
   manifest URLs resolve to the signed artifacts, and a separately installed
   test build discovers and installs the update. That live hosted check is an
   operational follow-up, not evidence in this foundation phase.

## Key custody, compromise, and rotation

- Assign **two recovery custodians**. Each must be able to access the CI secret
  recovery process, not necessarily possess a copied private key. Record their
  identities and handoff procedure in the fork's private operations record.
- On suspected compromise, disable release credentials, generate a new key,
  replace CI secrets and the embedded public configuration, and release a
  signed bridge update while the old key still signs the installed version.
  That bridge can introduce the new public key; then publish a normal release
  signed by the new key.
- If ownership changes, follow the same bridge rotation rather than handing a
  private key through chat or source control. Revoke former CI access after
  the bridge is verified.
- If the old private key is lost before a bridge update reaches users, existing
  installations cannot authenticate a new key. Publish recovery instructions
  and require a **manual reinstall** from a verified distribution channel.

## Team packages

Only bundled, reviewed team packages are available in this phase. Follow the
[package authoring guide](./package-authoring.md), validate packages in CI, and
ship them with a desktop release. Do not add a member/repository package loader
or give a package updater, filesystem, shell, or credential access.
