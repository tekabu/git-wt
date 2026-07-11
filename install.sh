#!/usr/bin/env bash
# Install the git-wt binary via `cargo install`.
#
#   ./install.sh                 # binary only, no profile changes
#   ./install.sh --alias wt      # also add a `wt` shell function that cd's
#                                #   into the worktree (name is yours to pick)
#
# A binary cannot change its parent shell's directory, so the cd-on-create /
# cd-on-show behaviour needs a shell function. --alias installs one, named as
# you ask, into your shell rc inside a managed block.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

alias_name=""
while [ $# -gt 0 ]; do
  case "$1" in
    --alias)   alias_name="${2:-}"; shift 2 ;;
    --alias=*) alias_name="${1#--alias=}"; shift ;;
    *) echo "error: unknown argument '$1'" >&2; exit 1 ;;
  esac
done

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

echo "Installed $bin"
echo "Ensure ${CARGO_HOME:-\$HOME/.cargo}/bin is on your PATH."

# --- optional shell alias ---------------------------------------------------
[ -z "$alias_name" ] && { echo "Done."; exit 0; }

case "$alias_name" in
  ""|*[!A-Za-z0-9_]*) echo "error: invalid alias name '$alias_name'" >&2; exit 1 ;;
esac

case "$(basename "${SHELL:-}")" in
  zsh)  rc="${ZDOTDIR:-$HOME}/.zshrc" ;;
  bash) rc="$HOME/.bashrc" ;;
  *)    rc="$HOME/.profile" ;;
esac
touch "$rc"

# Note whether a managed block already exists, then strip it. The fresh block
# is appended below with a plain heredoc (not inside $(...)), so macOS bash 3.2
# doesn't mis-parse the parens in the wrapper body.
had_block=no
grep -q '# >>> git-wt alias >>>' "$rc" && had_block=yes

tmp="$(mktemp)"
awk 'BEGIN{s=0}
     /# >>> git-wt alias >>>/{s=1; next}
     /# <<< git-wt alias <<</{s=0; next}
     s==0{print}' "$rc" > "$tmp"

# Wrapper: create (or the branch picker) and show cd into the worktree;
# remove cds back to the main worktree; list/help pass straight through.
# create/bare use --show so a path is emitted to cd to; a declined prompt or
# an error prints nothing, so the guard skips the cd.
cat >> "$tmp" <<EOF

# >>> git-wt alias >>>
# Managed by git-wt install.sh; edits here are overwritten on reinstall.
$alias_name() {
  case "\${1:-}" in
    -h|--help|-V|--version|list|ls|-l|--list)
      git-wt "\$@"; return \$? ;;
    show|go|cd|remove|rm)
      local d; d="\$(git-wt "\$@")" || return \$?
      [ -n "\$d" ] && cd "\$d" ;;
    *)  # a branch to create, or nothing -> interactive branch picker
      local d; d="\$(git-wt --show "\$@")" || return \$?
      [ -n "\$d" ] && cd "\$d" ;;
  esac
}
# <<< git-wt alias <<<
EOF

mv "$tmp" "$rc"
if [ "$had_block" = yes ]; then
  echo "Refreshed '$alias_name' in $rc"
else
  echo "Added '$alias_name' to $rc"
fi

echo "Done. Reload your shell:  exec ${SHELL:-sh}"
