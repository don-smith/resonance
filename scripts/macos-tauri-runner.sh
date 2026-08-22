#!/usr/bin/env bash
set -euo pipefail

# Tauri's default dev command runs a bare Mach-O executable. macOS does not
# register that process as an application, which leaves its webview unable to
# receive keyboard focus under window managers. Build the same dev binary and
# launch it through a small local app bundle instead.

if [[ "${1:-}" != "run" ]]; then
  exec cargo "$@"
fi
shift

keychain="$HOME/Library/Keychains/login.keychain-db"
identity="${RESONANCE_MACOS_SIGNING_IDENTITY:-}"
if [[ ! "$identity" =~ ^[[:xdigit:]]{40}$ ]]; then
  echo "macOS development signing is not configured. Run scripts/setup-macos-development-signing.sh." >&2
  exit 1
fi
identity=$(printf '%s' "$identity" | tr '[:lower:]' '[:upper:]')
identity_record=$(security find-identity -v -p codesigning "$keychain" | awk -v identity="$identity" 'toupper($2) == identity { print; exit }')
if [[ "$identity_record" != *'"Apple Development:'* ]]; then
  echo "The configured Apple Development signing identity is unavailable: $identity" >&2
  echo "Run scripts/setup-macos-development-signing.sh to repair the local configuration." >&2
  exit 1
fi

cargo_args=()
for argument in "$@"; do
  [[ "$argument" == "--" ]] && break
  cargo_args+=("$argument")
done
cargo build "${cargo_args[@]}"

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bundle="$root/target/debug/Resonance Dev.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cat >"$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleDisplayName</key><string>Resonance Dev</string>
  <key>CFBundleExecutable</key><string>resonance-desktop</string>
  <key>CFBundleIdentifier</key><string>com.resonance.desktop.dev</string>
  <key>CFBundleName</key><string>Resonance Dev</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
</dict></plist>
PLIST
binary="$bundle/Contents/MacOS/resonance-desktop"
existing_pids=$(pgrep -f "^$binary$" || true)
if [[ -n "$existing_pids" ]]; then
  kill $existing_pids 2>/dev/null || true
  sleep 0.2
fi
rm -f "$binary"
cp "$root/target/debug/resonance-desktop" "$binary"

codesign --force --sign "$identity" --timestamp=none --identifier com.resonance.desktop.dev "$binary"
codesign --force --sign "$identity" --timestamp=none --identifier com.resonance.desktop.dev "$bundle"
signed_team=$(codesign -dvv "$bundle" 2>&1 | awk -F= '/^TeamIdentifier=/{print $2; exit}')
if [[ -z "$signed_team" ]]; then
  echo "Resonance Dev was signed without an Apple Team ID; refusing to launch." >&2
  exit 1
fi

open -n -a "$bundle"

for _ in {1..20}; do
  app_pid=$(pgrep -f "^$binary$" | tail -n 1 || true)
  [[ -n "$app_pid" ]] && break
  sleep 0.1
done
[[ -n "${app_pid:-}" ]] || {
  echo "Resonance Dev did not start." >&2
  exit 1
}

cleanup() {
  kill "$app_pid" 2>/dev/null || true
  wait "$app_pid" 2>/dev/null || true
}
trap cleanup EXIT INT TERM
while kill -0 "$app_pid" 2>/dev/null; do
  sleep 1
done
