#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This setup command only supports macOS." >&2
  exit 1
fi

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
keychain="$HOME/Library/Keychains/login.keychain-db"
requested_identity="${RESONANCE_MACOS_SIGNING_IDENTITY:-}"

if [[ -n "$requested_identity" && ! "$requested_identity" =~ ^[[:xdigit:]]{40}$ ]]; then
  echo "RESONANCE_MACOS_SIGNING_IDENTITY must be a 40-character certificate hash." >&2
  exit 1
fi
requested_identity=$(printf '%s' "$requested_identity" | tr '[:lower:]' '[:upper:]')

identity_records=$(security find-identity -v -p codesigning "$keychain" | awk '/"Apple Development:/{print}')
if [[ -n "$requested_identity" ]]; then
  identity_record=$(printf '%s\n' "$identity_records" | awk -v identity="$requested_identity" 'toupper($2) == identity { print; exit }')
else
  identity_count=$(printf '%s\n' "$identity_records" | awk 'NF { count++ } END { print count + 0 }')
  if [[ "$identity_count" -ne 1 ]]; then
    echo "Expected one valid Apple Development identity, found $identity_count." >&2
    if [[ -n "$identity_records" ]]; then
      printf '%s\n' "$identity_records" >&2
      echo "Set RESONANCE_MACOS_SIGNING_IDENTITY to the certificate hash to choose one." >&2
    else
      echo "Create an Apple Development certificate in Xcode Settings > Accounts > Manage Certificates." >&2
      echo "If Xcode created one but it is not valid, install the current WWDR intermediate from https://www.apple.com/certificateauthority/." >&2
    fi
    exit 1
  fi
  identity_record="$identity_records"
fi

if [[ -z "${identity_record:-}" ]]; then
  echo "The requested Apple Development identity is unavailable: $requested_identity" >&2
  exit 1
fi

identity=$(printf '%s\n' "$identity_record" | awk '{print toupper($2)}')
certificate_name=$(printf '%s\n' "$identity_record" | sed -E 's/^[^"]*"(.*)"$/\1/')

temporary_directory=$(mktemp -d)
keychain_password=""
cleanup() {
  unset keychain_password
  rm -rf "$temporary_directory"
}
trap cleanup EXIT

certificate="$temporary_directory/development-certificate.pem"
security find-certificate -p -c "$certificate_name" "$keychain" >"$certificate"
team_id=$(openssl x509 -in "$certificate" -noout -subject -nameopt sep_multiline | awk -F= '/^[[:space:]]*OU=/{gsub(/[[:space:]]/, "", $2); print $2; exit}')
subject_key_id=$(openssl x509 -in "$certificate" -noout -ext subjectKeyIdentifier | tail -n 1 | tr -d ' :\r\n' | tr '[:lower:]' '[:upper:]')
if [[ -z "$team_id" || ! "$subject_key_id" =~ ^[[:xdigit:]]{40}$ ]]; then
  echo "Could not read the Team ID or private-key identifier from $certificate_name." >&2
  exit 1
fi

key_label=$(security find-key -t private "$keychain" | awk -v target="$subject_key_id" '
  /0x00000001 <blob>=/ {
    label = $0
    sub(/^.*<blob>=/, "", label)
    sub(/^"/, "", label)
    sub(/"$/, "", label)
  }
  /0x00000006 <blob>=0x/ {
    hash = $0
    sub(/^.*0x/, "", hash)
    sub(/[[:space:]].*$/, "", hash)
    if (toupper(hash) == target) {
      print label
      exit
    }
  }
')
if [[ -z "$key_label" ]]; then
  echo "Could not locate the private key for $certificate_name." >&2
  exit 1
fi

has_installation_identity=false
if security find-generic-password -a installation-identity -s dev.resonance.desktop "$keychain" >/dev/null 2>&1; then
  has_installation_identity=true
fi

printf 'Apple Development identity: %s\n' "$certificate_name"
printf 'Team ID: %s\n' "$team_id"
printf 'Login Keychain password: '
IFS= read -r -s keychain_password </dev/tty
printf '\n'

cat >"$temporary_directory/keychain-password.exp" <<'EXPECT'
set timeout -1
set password [gets stdin]
if {[lindex $argv 0] eq "--"} {
  set argv [lrange $argv 1 end]
}
spawn -noecho {*}$argv
expect -re {(?i)password[^:]*:}
send -- "$password\r"
expect eof
set result [wait]
exit [lindex $result 3]
EXPECT

run_with_keychain_password() {
  printf '%s\n' "$keychain_password" | /usr/bin/expect -f "$temporary_directory/keychain-password.exp" -- "$@" >/dev/null
}

run_with_keychain_password security set-key-partition-list \
  -S apple-tool:,apple:,codesign: \
  -t private -s -l "$key_label" \
  "$keychain"

if [[ "$has_installation_identity" == true ]]; then
  run_with_keychain_password security set-generic-password-partition-list \
    -a installation-identity \
    -s dev.resonance.desktop \
    -S "teamid:$team_id" \
    "$keychain"
fi
unset keychain_password

executable_probe="$temporary_directory/signing-probe"
printf '#!/usr/bin/env bash\nexit 0\n' >"$executable_probe"
chmod +x "$executable_probe"
codesign --force --sign "$identity" --timestamp=none "$executable_probe"
probe_team=$(codesign -dvv "$executable_probe" 2>&1 | awk -F= '/^TeamIdentifier=/{print $2; exit}')
if [[ "$probe_team" != "$team_id" ]]; then
  echo "The signing probe reported Team ID ${probe_team:-none}, expected $team_id." >&2
  exit 1
fi
"$executable_probe"

if [[ "$has_installation_identity" == true ]]; then
  cat >"$temporary_directory/keychain-probe.swift" <<'SWIFT'
import Foundation
import Security

let query: [CFString: Any] = [
  kSecClass: kSecClassGenericPassword,
  kSecAttrService: "dev.resonance.desktop",
  kSecAttrAccount: "installation-identity",
  kSecMatchLimit: kSecMatchLimitOne,
  kSecReturnData: true,
]
var result: CFTypeRef?
let status = SecItemCopyMatching(query as CFDictionary, &result)
if status != errSecSuccess {
  FileHandle.standardError.write(Data("Installation identity probe failed with OSStatus \(status).\n".utf8))
  exit(1)
}
SWIFT
  keychain_probe="$temporary_directory/keychain-probe"
  xcrun swiftc -framework Security "$temporary_directory/keychain-probe.swift" -o "$keychain_probe"
  codesign --force --sign "$identity" --timestamp=none --identifier com.resonance.desktop.dev "$keychain_probe"
  echo "An existing installation identity may ask once about the new Apple signer. Choose Always Allow."
  "$keychain_probe"
  "$keychain_probe"
fi

local_environment="$root/.resonance/.env"
mkdir -p "$(dirname "$local_environment")"
touch "$local_environment"
environment_update="$temporary_directory/local.env"
awk -v identity="$identity" '
  /^RESONANCE_MACOS_SIGNING_IDENTITY=/ {
    if (!written) {
      print "RESONANCE_MACOS_SIGNING_IDENTITY=" identity
      written = 1
    }
    next
  }
  { print }
  END {
    if (!written) print "RESONANCE_MACOS_SIGNING_IDENTITY=" identity
  }
' "$local_environment" >"$environment_update"
mv "$environment_update" "$local_environment"
chmod 600 "$local_environment"

printf 'Configured Apple Development signing for Team ID %s.\n' "$team_id"
if [[ "$has_installation_identity" == false ]]; then
  echo "No existing installation identity needed repair; Resonance will create it on first launch."
fi
