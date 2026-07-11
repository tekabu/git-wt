#!/usr/bin/env bash
# git-wt one-file installer (self-contained; no repo, no toolchain).
#
# build.sh generates the real installer from this template: it injects the
# shared alias helper at the @ALIAS_FN@ line and appends the gzipped binary as
# base64 below the __GITWT_PAYLOAD__ marker. Do not run this template directly.
#
#   ./git-wt-<version>-<os>-<arch>.install.sh            # install the binary
#   ./git-wt-<version>-<os>-<arch>.install.sh --alias wt # + a `wt` shell fn
#
# Installs to $GITWT_PREFIX/bin (default ~/.local/bin).
set -euo pipefail

self="${BASH_SOURCE[0]}"

alias_name=""
while [ $# -gt 0 ]; do
  case "$1" in
    --alias)   alias_name="${2:-}"; shift 2 ;;
    --alias=*) alias_name="${1#--alias=}"; shift ;;
    *) echo "error: unknown argument '$1'" >&2; exit 1 ;;
  esac
done

# base64 decode differs across coreutils (GNU: -d) and BSD/macOS (-D).
b64d() { if printf '' | base64 -d >/dev/null 2>&1; then base64 -d; else base64 -D; fi; }

# The payload is everything below the marker: base64 of the gzipped binary.
marker_line="$(grep -n '^__GITWT_PAYLOAD__$' "$self" | head -1 | cut -d: -f1 || true)"
[ -n "$marker_line" ] || { echo "error: installer is corrupt (no payload)" >&2; exit 1; }

destdir="${GITWT_PREFIX:-$HOME/.local}/bin"
bin="$destdir/git-wt"
echo "Installing git-wt to $bin..."
mkdir -p "$destdir"
tail -n +"$((marker_line + 1))" "$self" | b64d | gzip -dc > "$bin"
chmod 0755 "$bin"

# Warn if another git-wt earlier on PATH will shadow the installed one.
active="$(command -v git-wt || true)"
if [ -n "$active" ] && [ "$active" != "$bin" ]; then
  echo "warning: '$active' shadows '$bin' (earlier on PATH)." >&2
  echo "         remove it, or: ln -sf '$bin' '$active'" >&2
fi

echo "Installed $bin"
echo "Ensure $(dirname "$bin") is on your PATH."

if ! command -v fzf >/dev/null; then
  case "$(uname -s)" in
    Darwin) hint="brew install fzf" ;;
    Linux)  hint="your package manager, e.g. 'apt install fzf' or 'dnf install fzf'" ;;
    *)      hint="https://github.com/junegunn/fzf" ;;
  esac
  echo "Tip: install fzf for a fuzzy branch picker ($hint). Optional."
fi

# --- optional shell alias ---------------------------------------------------
if [ -n "$alias_name" ]; then
# @ALIAS_FN@  (build.sh injects _alias.sh here)
  gitwt_write_alias "$alias_name"
else
  echo "Done."
fi

# Stop before the appended payload so bash never executes the base64 data.
exit 0
__GITWT_PAYLOAD__
