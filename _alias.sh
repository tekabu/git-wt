#!/usr/bin/env bash
# Shared helper: write git-wt's `wt` shell wrapper into the user's rc file.
#
# Single source of truth for both the wrapper body and the managed-block
# plumbing. install.sh sources this at runtime; build.sh bakes it into the
# one-file installer. Do not add `set -e` here — it is meant to be sourced.
#
# Usage: gitwt_write_alias <name>   (git-wt must already be installed)

gitwt_write_alias() {
  local alias_name="$1"

  case "$alias_name" in
    ""|*[!A-Za-z0-9_]*) echo "error: invalid alias name '$alias_name'" >&2; return 1 ;;
    [0-9]*) echo "error: alias name '$alias_name' cannot start with a digit" >&2; return 1 ;;
    if|then|else|elif|fi|for|while|until|do|done|case|esac|function|select|in|time)
      echo "error: alias name '$alias_name' is a shell reserved word" >&2; return 1 ;;
  esac

  local rc
  case "$(basename "${SHELL:-}")" in
    zsh)  rc="${ZDOTDIR:-$HOME}/.zshrc" ;;
    bash) rc="$HOME/.bashrc" ;;
    *)    rc="$HOME/.profile" ;;
  esac
  touch "$rc"

  # Note whether a managed block already exists, then strip it. The fresh block
  # is appended below with a plain heredoc (not inside $(...)), so macOS bash 3.2
  # doesn't mis-parse the parens in the wrapper body.
  local had_block=no
  grep -q '# >>> git-wt alias >>>' "$rc" && had_block=yes

  local tmp; tmp="$(mktemp)"
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
# Managed by git-wt; edits here are overwritten on reinstall.
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

  # Write through rather than `mv`: the rc is often a symlink into a dotfile
  # manager (chezmoi/stow/yadm), and `mv` would replace the link with a plain
  # file. This also preserves the rc's existing permissions.
  cat "$tmp" > "$rc" && rm -f "$tmp"
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
}
