#!/usr/bin/env bash
# Install the git-wt binary via `cargo install`. Does not touch any shell
# profile. Invoke as `git-wt ...` (also reachable as `git wt ...`).
#
#   ./install.sh
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

command -v cargo >/dev/null || { echo "error: cargo not found; install Rust" >&2; exit 1; }

echo "Installing git-wt binary via cargo..."
cargo install --path "$here" --force

bin="${CARGO_HOME:-$HOME/.cargo}/bin/git-wt"

# Warn if another git-wt earlier on PATH will shadow the installed one.
active="$(command -v git-wt || true)"
if [ -n "$active" ] && [ "$active" != "$bin" ]; then
  echo "warning: '$active' shadows '$bin' (earlier on PATH)." >&2
  echo "         remove it, or: ln -sf '$bin' '$active'" >&2
fi

echo "Done. Installed $bin"
echo "Ensure ${CARGO_HOME:-\$HOME/.cargo}/bin is on your PATH."
