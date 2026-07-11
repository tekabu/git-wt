#!/usr/bin/env bash
# Install the git-wt binary via `cargo install`.
#
#   ./install.sh                 # binary only, no profile changes
#   ./install.sh --alias wt      # also add a `wt` shell function that cd's
#                                #   into the worktree (name is yours to pick)
#
# A binary cannot change its parent shell's directory, so the cd-on-switch /
# cd-on-remove behaviour needs a shell function. --alias installs one, named as
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

case "$alias_name" in
  ""|*[!A-Za-z0-9_]*) echo "error: invalid alias name '$alias_name'" >&2; exit 1 ;;
  [0-9]*) echo "error: alias name '$alias_name' cannot start with a digit" >&2; exit 1 ;;
  if|then|else|elif|fi|for|while|until|do|done|case|esac|function|select|in|time)
    echo "error: alias name '$alias_name' is a shell reserved word" >&2; exit 1 ;;
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

# Wrapper for the target-first grammar. Two names, one behavioral difference:
# cd. For `<N>` with switch/cd (or nothing) it cd's into the worktree; for
# `<N> remove` it cd's back to the main worktree git prints; path/show and every
# non-target verb (list/add/help/...) pass straight through, printing only. A
# declined prompt or an error prints nothing, so the guard skips the cd.
cat >> "$tmp" <<EOF

# >>> git-wt alias >>>
# Managed by git-wt install.sh; edits here are overwritten on reinstall.
$alias_name() {
  case "\${1:-}" in
    ""|-h|--help|-V|--version|version|help|list|ls)
      git-wt "\$@"; return \$? ;;
    add)
      # Create, then cd into the new worktree — unless --stay was passed.
      local stay=0 a
      for a in "\$@"; do [ "\$a" = "--stay" ] && stay=1; done
      local d; d="\$(git-wt "\$@")" || return \$?
      [ "\$stay" = 0 ] && [ -n "\$d" ] && cd "\$d"
      return 0 ;;
  esac
  # A leading token that is not a numeric target index (e.g. an unknown verb
  # or a stray flag) passes straight through, so git-wt prints its own error
  # instead of the wrapper building a confusing 'git-wt <tok> path'.
  case "\${1:-}" in
    *[!0-9]*) git-wt "\$@"; return \$? ;;
  esac
  # <N> [action]: switch (default) & remove cd the shell; path/show print.
  case "\${2:-}" in
    ""|switch|cd)
      local d; d="\$(git-wt "\$1" path)" || return \$?
      [ -n "\$d" ] && cd "\$d" ;;
    remove|rm)
      local d; d="\$(git-wt "\$@")" || return \$?
      [ -n "\$d" ] && cd "\$d" ;;
    *)  # path/show and anything else: pass through
      git-wt "\$@" ;;
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

# The binary is already live (the function just calls it). Only the shell
# function itself needs reloading, and a child process cannot touch its parent
# shell — so define it in THIS shell with the copy-paste line below.
cat <<EOF
Done.

The git-wt binary is active now. To load the '$alias_name' function into your
current shell without opening a new one, run:

    eval "\$(sed -n '/# >>> git-wt alias >>>/,/# <<< git-wt alias <<</p' '$rc')"

New shells pick it up automatically. (Or reload everything: exec ${SHELL:-sh})
EOF
