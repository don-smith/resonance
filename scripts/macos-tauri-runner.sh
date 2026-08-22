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
run_args=()
after_separator=false
for argument in "$@"; do
  if [[ "$argument" == "--" ]]; then
    after_separator=true
  elif [[ "$after_separator" == true ]]; then
    run_args+=("$argument")
  else
    cargo_args+=("$argument")
  fi
done
cargo build "${cargo_args[@]}"

profile_name="${RESONANCE_DEBUG_PROFILE_NAME:-}"
if [[ -n "$profile_name" && ( ! "$profile_name" =~ ^[a-z][a-z0-9-]{0,31}$ || "$profile_name" == *- ) ]]; then
  echo "The debug profile bundle name is invalid." >&2
  exit 1
fi
if [[ -n "$profile_name" ]]; then
  bundle_name="Resonance Debug $profile_name"
  bundle_identifier="com.resonance.desktop.debug.$profile_name"
else
  bundle_name="Resonance Dev"
  bundle_identifier="com.resonance.desktop.dev"
fi

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bundle="$root/target/debug/$bundle_name.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cat >"$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleDisplayName</key><string>$bundle_name</string>
  <key>CFBundleExecutable</key><string>resonance-desktop</string>
  <key>CFBundleIdentifier</key><string>$bundle_identifier</string>
  <key>CFBundleName</key><string>$bundle_name</string>
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

codesign --force --sign "$identity" --timestamp=none --identifier "$bundle_identifier" "$binary"
codesign --force --sign "$identity" --timestamp=none --identifier "$bundle_identifier" "$bundle"
signed_team=$(codesign -dvv "$bundle" 2>&1 | awk -F= '/^TeamIdentifier=/{print $2; exit}')
if [[ -z "$signed_team" ]]; then
  echo "Resonance Dev was signed without an Apple Team ID; refusing to launch." >&2
  exit 1
fi

open -n -a "$bundle" --args "${run_args[@]}"

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
