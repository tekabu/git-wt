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
    # 'esac' must be quoted: unquoted inside a pattern list bash reads it as
    # the case terminator and dies with a syntax error.
    if|then|else|elif|fi|for|while|until|do|done|case|"esac"|function|select|in|time)
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

  # Wrapper for the verb-first grammar: every command is
  # `git-wt <VERB> [TARGET_LIST] [-b/--branch TARGET_LIST] [FLAGS...]`.
  # Verbs that don't change the shell's cwd (list/version/path/show/fetch/
  # pull/push/diff/meld/commits/log/merge/merged/doctor, plus their aliases)
  # pass straight through — they're listed explicitly so a single-word
  # invocation isn't mistaken for a bare switch target below.
  # `add`/`a` creates a worktree and cd's into the printed path unless --stay.
  # `remove`/`rm` removes the named (or current) worktree and cd's back to the
  # main worktree if git-wt prints one. `switch`/`cd`/`s` switches worktrees
  # and cd's. A bare alias call with no args passes straight through to
  # git-wt, which defaults to the worktree list. A single bare token that
  # looks like a target (number, branch name, or comma list) is rewritten as
  # `git-wt switch <tok>` so `wt <N>` still works for convenience, but the
  # preferred form is `wt switch <N>`.
  cat >> "$tmp" <<EOF

# >>> git-wt alias >>>
# Managed by git-wt; edits here are overwritten on reinstall.
$alias_name() {
  # Help and version flags are terminal anywhere on the line; let git-wt
  # print clap's generated message and return without trying to cd.
  local a
  for a in "\$@"; do
    case "\$a" in
      -h|--help|-V|--version) git-wt "\$@"; return \$? ;;
    esac
  done

  case "\${1:-}" in
    ""|help|list|ls|version|path|show|fetch|pull|p|push|diff|meld|commits|c|log|l|merge|merged|m|doctor)
      # No args at all defaults to the worktree list (git-wt's own default);
      # never treated as a switch, so it never tries to cd.
      git-wt "\$@"; return \$? ;;
    add|a)
      # Create, then cd into the new worktree — unless --stay was passed.
      local stay=0 arg
      for arg in "\$@"; do [ "\$arg" = "--stay" ] && stay=1; done
      local d; d="\$(git-wt "\$@")" || return \$?
      [ "\$stay" = 0 ] && [ -n "\$d" ] && cd "\$d"
      return 0 ;;
    remove|rm)
      # Remove prints the main worktree path when it removed the tree you
      # were standing in; cd there if it printed one.
      local d; d="\$(git-wt "\$@")" || return \$?
      [ -n "\$d" ] && cd "\$d"
      return 0 ;;
    switch|cd|s)
      # Switch verb: switch to the worktree and cd.
      local d; d="\$(git-wt "\$@")" || return \$?
      [ -n "\$d" ] && cd "\$d"
      return 0 ;;
  esac

  # A lone bare target (number, branch, or comma list) without a verb is
  # treated as "git-wt switch <tok>" so the wrapper still cd's on "wt <N>".
  # Anything else (multiple tokens, a leading flag, or an unknown verb)
  # passes straight through so git-wt can report its own errors.
  if [ "\$#" -eq 1 ]; then
    case "\$1" in
      -*)
        git-wt "\$@"; return \$? ;;
      *)
        local d; d="\$(git-wt switch "\$1")" || return \$?
        [ -n "\$d" ] && cd "\$d"
        return 0 ;;
    esac
  fi

  git-wt "\$@"; return \$?
}
# <<< git-wt alias <<<
EOF

  # Write through rather than `mv`: the rc is often a symlink into a dotfile
  # manager (chezmoi/stow/yadm), and `mv` would replace the link with a plain
  # file. This also preserves the rc's existing permissions.
  # The redirect truncates $rc before the copy starts, so an interrupted write
  # would leave the user with an empty shell rc. Keep a backup until it lands.
  # `touch` above guarantees $rc exists, so the backup is always taken.
  local bak="$rc.git-wt.bak"
  cp "$rc" "$bak" || {
    echo "error: could not back up $rc; leaving it untouched" >&2
    rm -f "$tmp"
    return 1
  }
  # A failed write is the case the backup exists for: restore it rather than
  # leaving the truncated rc behind, and say so instead of reporting success.
  if ! cat "$tmp" > "$rc"; then
    # Whatever stopped the write (a full disk, a read-only file) is likely to
    # stop the restore too, so the restore is checked before it is claimed.
    if cp "$bak" "$rc" 2>/dev/null; then
      echo "error: could not write $rc; restored from $bak" >&2
    else
      echo "error: could not write $rc, and the restore failed; your original is at $bak" >&2
    fi
    rm -f "$tmp"
    return 1
  fi
  rm -f "$tmp"
  if [ "$had_block" = yes ]; then
    echo "Refreshed '$alias_name' in $rc"
  else
    echo "Added '$alias_name' to $rc"
  fi
  echo "Previous $rc saved as $bak"

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
