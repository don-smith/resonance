#!/usr/bin/env bash
set -euo pipefail

repo_root="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
bin_dir="${THEVIEW_BIN_DIR:-${HOME}/.local/bin}"
mkdir -p "$bin_dir"
ln -sfn "$repo_root/bin/theview" "$bin_dir/theview"

case "${SHELL##*/}" in
  bash) default_rc="${HOME}/.bashrc" ;;
  *) default_rc="${HOME}/.zshrc" ;;
esac
shell_rc="${THEVIEW_SHELL_RC:-$default_rc}"
path_line="export PATH=\"${bin_dir}:\$PATH\""

if [[ ":${PATH}:" != *":${bin_dir}:"* ]]; then
  touch "$shell_rc"
  if ! grep -Fqx "$path_line" "$shell_rc"; then
    {
      printf '\n# theview local CLI\n'
      printf '%s\n' "$path_line"
    } >> "$shell_rc"
  fi
  printf 'Installed theview at %s\n' "$bin_dir/theview"
  printf 'Added %s to %s. Open a new shell or run: source %s\n' "$bin_dir" "$shell_rc" "$shell_rc"
else
  printf 'Installed theview at %s (already on PATH)\n' "$bin_dir/theview"
fi
