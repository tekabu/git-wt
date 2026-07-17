#!/usr/bin/env bash
# Install git-wt FROM SOURCE (requires Rust/`cargo`).
#
#   ./install-mac.sh             # build + install, adds default `wt` alias
#   ./install-mac.sh --alias xy  # use `xy` instead of `wt` for the shell fn
#   ./install-mac.sh --no-alias  # install the binary only, no shell function
#
# No toolchain? Use the one-file installer that build.sh produces
# (dist/git-wt-<version>-<os>-<arch>.install.sh) — see the README.
#
# A binary cannot change its parent shell's directory, so the cd-on-switch /
# cd-on-remove behaviour needs a shell function. --alias installs one (shared
# logic lives in _alias.sh), into your shell rc inside a managed block.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

alias_name="wt"
while [ $# -gt 0 ]; do
  case "$1" in
    --alias)    alias_name="${2:-}"; shift 2 ;;
    --alias=*)  alias_name="${1#--alias=}"; shift ;;
    --no-alias) alias_name=""; shift ;;
    *) echo "error: unknown argument '$1'" >&2; exit 1 ;;
  esac
done

command -v cargo >/dev/null || {
  echo "error: cargo not found; install Rust, or use the one-file installer (see README)" >&2
  exit 1
}

echo "Installing git-wt from source via cargo..."
cargo install --path "$here" --force
bin="${CARGO_HOME:-$HOME/.cargo}/bin/git-wt"

# Warn if another git-wt earlier on PATH will shadow the installed one.
active="$(command -v git-wt || true)"
if [ -n "$active" ] && [ "$active" != "$bin" ]; then
  echo "warning: '$active' shadows '$bin' (earlier on PATH)." >&2
  echo "         remove it, or: ln -sf '$bin' '$active'" >&2
fi

echo "Installed $bin"
echo "Ensure $(dirname "$bin") is on your PATH."

# fzf is an optional runtime helper: when present, the branch picker (git-wt add
# with no branch) uses fzf's fuzzy search instead of a numbered prompt. Not a
# build dependency and not required — just hint how to get the nicer picker.
if ! command -v fzf >/dev/null; then
  case "$(uname -s)" in
    Darwin) hint="brew install fzf" ;;
    Linux)  hint="your package manager, e.g. 'apt install fzf' or 'dnf install fzf'" ;;
    *)      hint="https://github.com/junegunn/fzf" ;;
  esac
  echo "Tip: install fzf for a fuzzy branch picker ($hint). Optional."
fi

# --- optional shell alias ---------------------------------------------------
[ -z "$alias_name" ] && { echo "Done."; exit 0; }

# shellcheck source=_alias.sh
. "$here/_alias.sh"
gitwt_write_alias "$alias_name"

# Load the alias function into the caller's shell right now, so `wt` works
# without opening a new terminal or re-sourcing the rc by hand.
eval "$(sed -n '/# >>> git-wt alias >>>/,/# <<< git-wt alias <<</p' "/Users/$USER/.zshrc")"
