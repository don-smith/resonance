#!/usr/bin/env bash
set -euo pipefail

repo_root="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
# RESONANCE_RUNTIME_ROOT is used by packaged distributions; the checkout remains the default development runtime.
runtime_root="${RESONANCE_RUNTIME_ROOT:-$repo_root}"
bin_dir="${RESONANCE_BIN_DIR:-${HOME}/.local/bin}"
mkdir -p "$bin_dir"
ln -sfn "$runtime_root/bin/resonate" "$bin_dir/resonate"

case "${SHELL##*/}" in
  bash) default_rc="${HOME}/.bashrc" ;;
  *) default_rc="${HOME}/.zshrc" ;;
esac
shell_rc="${RESONANCE_SHELL_RC:-$default_rc}"
path_line="export PATH=\"${bin_dir}:\$PATH\""

if [[ ":${PATH}:" != *":${bin_dir}:"* ]]; then
  touch "$shell_rc"
  if ! grep -Fqx "$path_line" "$shell_rc"; then
    {
      printf '\n# resonate local CLI\n'
      printf '%s\n' "$path_line"
    } >> "$shell_rc"
  fi
  printf 'Installed resonate at %s\n' "$bin_dir/resonate"
  printf 'Added %s to %s. Open a new shell or run: source %s\n' "$bin_dir" "$shell_rc" "$shell_rc"
else
  printf 'Installed resonate at %s (already on PATH)\n' "$bin_dir/resonate"
fi
