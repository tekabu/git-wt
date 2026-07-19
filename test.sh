#!/usr/bin/env bash
# Live end-to-end test for git-wt.
#
# Builds the binary, spins up a dummy repo under /tmp, drives every command in
# the target-first grammar, and prints a PASS/FAIL report. Exits non-zero if any
# case fails. Cleans up the /tmp scratch dir on exit.
#
#   ./test.sh                  # release build (cargo build --release)
#   ./test.sh --debug          # debug build (faster compile)
#   ./test.sh --md             # also write docs/test-report.md
#   ./test.sh --md out.md      # ...to out.md instead
set -u

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

profile="release"
MD=""
while [ $# -gt 0 ]; do
  case "$1" in
    --debug) profile="debug"; shift ;;
    # --md takes an optional path: the next word is the file unless it's another
    # flag or the end of the line, in which case the default stands.
    --md)
      shift
      case "${1:-}" in
        ""|-*) MD="$here/docs/test-report.md" ;;
        *)     MD="$1"; shift ;;
      esac ;;
    *) echo "unknown option '$1'" >&2; exit 2 ;;
  esac
done

# Resolve before the suite cd's into the scratch repo, so a relative --md path
# means "relative to where the user ran this", not to /tmp.
if [ -n "$MD" ]; then
  case "$MD" in
    /*) ;;
    *)  MD="$PWD/$MD" ;;
  esac
  mkdir -p "$(dirname "$MD")" || exit 1
fi

echo "Building git-wt ($profile)..."
if [ "$profile" = "release" ]; then
  cargo build --release --manifest-path "$here/Cargo.toml" >/dev/null 2>&1 || { echo "build failed" >&2; exit 1; }
  BIN="$here/target/release/git-wt"
else
  cargo build --manifest-path "$here/Cargo.toml" >/dev/null 2>&1 || { echo "build failed" >&2; exit 1; }
  BIN="$here/target/debug/git-wt"
fi

# The branch picker prefers fzf, which reads /dev/tty directly and so ignores
# the empty stdin this suite feeds it — on a machine that has fzf installed the
# run blocks forever on the first picker test. Drop every PATH entry providing
# fzf so the numbered fallback, which is what these tests assert on, is what
# runs. (CI images have no fzf, which is why this only bites locally.)
nofzf=""
IFS=':' read -ra _parts <<< "$PATH"
for _p in "${_parts[@]}"; do
  [ -x "$_p/fzf" ] || nofzf="$nofzf${nofzf:+:}$_p"
done
export PATH="$nofzf"

# --- scratch repo under /tmp ------------------------------------------------
ROOT="$(mktemp -d "/tmp/git-wt-test.XXXXXX")"
CODE="$ROOT/code"
APP="$CODE/myapp"
trap 'rm -rf "$ROOT"' EXIT

mkdir -p "$APP"
cd "$APP" || exit 1
git init -q
# git picks the default branch name from init.defaultBranch, which is 'master'
# on a stock Linux install and 'main' only because macOS ships a system gitconfig
# saying so. The suite asserts on the name, so pin it here (works on an unborn
# HEAD, before the first commit).
git checkout -q -b main
git config user.email test@example.com
git config user.name "Test"
git commit -q --allow-empty -m "init"
git branch feature/login
git branch feature/logout
git branch feature/api
git branch dirty
git branch pathtest
git branch staybr
git branch insidebr

# A bare origin with a branch that exists ONLY on the remote, so `add` can
# exercise the tracking-branch path.
REMOTE="$ROOT/origin.git"
git init -q --bare "$REMOTE"
git remote add origin "$REMOTE"
git push -q origin "feature/api:refs/heads/remote-only"
git fetch -q origin

pass=0
fail=0

# report PASS|FAIL <tag> <name> <cmd> [why] -- the single place that prints a
# result line, so the bespoke cases below line up with check()'s.
# <tag> groups by intent: HAPPY expects success, UNHAPPY expects a rejection.
report() {
  local st="$1" tag="$2" name="$3" cmd="$4" why="${5:-}"
  local tcol='\033[2m'                      # HAPPY: dim, it's the quiet default
  [ "$tag" = UNHAPPY ] && tcol='\033[33m'   # UNHAPPY: yellow, a deliberate error
  if [ "$st" = PASS ]; then
    printf "  \033[32mPASS\033[0m  ${tcol}%-7s\033[0m %-34s \033[2m%s\033[0m\n" "$tag" "$name" "$cmd"
    pass=$((pass+1))
  else
    printf "  \033[31mFAIL\033[0m  ${tcol}%-7s\033[0m %-34s \033[2m%s\033[0m\n" "$tag" "$name" "$cmd"
    [ -n "$why" ] && printf '        \033[31m%s\033[0m\n' "$why"
    fail=$((fail+1))
  fi
  [ -n "$MD" ] && md_row "$st" "$tag" "$name" "$cmd" "$why"
  return 0
}

# One table row per result. A literal '|' or a newline would break the table, so
# both are neutralised; backticks in the cell would end the code span, so the
# command and reason are fenced with a doubled delimiter.
md_cell() {
  local s="$1"
  s="${s//|/\\|}"
  s="${s//$'\n'/ }"
  printf '%s' "$s"
}

md_row() {
  local st="$1" tag="$2" name="$3" cmd="$4" why="${5:-}"
  local mark='✅'
  [ "$st" = FAIL ] && mark='❌'
  [ "$tag" = "-" ] && tag=""
  printf '| %s | %s | %s | `` %s `` | %s |\n' \
    "$mark" "$tag" "$(md_cell "$name")" "$(md_cell "$cmd")" \
    "$([ -n "$why" ] && printf '`` %s ``' "$(md_cell "$why")")" >> "$MD"
}

# Render an invocation for the report: 'git-wt' plus each argument, with any
# long one (a scratch path, mostly) cut to its first 21 chars so the command
# column stays readable. Empty and space-bearing args are quoted, so the report
# shows the argument boundaries the shell actually passed.
ARG_MAX=24
fmt_cmd() {
  local out="" a shown
  for a in "$@"; do
    if [ "${#a}" -gt "$ARG_MAX" ]; then
      shown="${a:0:21}..."
    else
      shown="$a"
    fi
    case "$a" in
      "")   shown="''" ;;
      *" "*) shown="'$shown'" ;;
    esac
    out="$out $shown"
  done
  printf 'git-wt%s' "$out"
}

# check NAME -- run the binary, then assert on exit code, stdout, stderr.
#   exit=<n>  out=<substr>  err=<substr>  in=<stdin>   (each optional)
# Trailing args after the option block are passed to git-wt. Without in=, stdin
# is empty, so any confirm() prompt reads EOF/blank and answers No.
check() {
  local name="$1"; shift
  local exit_want="" out_want="" err_want="" in_data=""
  while [ $# -gt 0 ]; do
    case "$1" in
      exit=*) exit_want="${1#exit=}"; shift ;;
      out=*)  out_want="${1#out=}"; shift ;;
      err=*)  err_want="${1#err=}"; shift ;;
      in=*)   in_data="${1#in=}"; shift ;;
      --) shift; break ;;
      *) break ;;
    esac
  done

  local out err code
  out="$(printf '%s' "$in_data" | "$BIN" "$@" 2>"$ROOT/err")"; code=$?
  err="$(cat "$ROOT/err")"

  local ok=1 why=""
  if [ -n "$exit_want" ] && [ "$code" != "$exit_want" ]; then
    ok=0; why="exit $code != $exit_want"
  fi
  if [ -n "$out_want" ] && [[ "$out" != *"$out_want"* ]]; then
    ok=0; why="$why; stdout lacks '$out_want' (got '$out')"
  fi
  if [ -n "$err_want" ] && [[ "$err" != *"$err_want"* ]]; then
    ok=0; why="$why; stderr lacks '$err_want' (got '$err')"
  fi

  # The tag is derived, never passed: a test that declares exit=0 is a happy
  # path, anything else is a rejection we want to keep proving. No exit= at all
  # means the case only asserts on output, so it claims neither.
  local tag="-"
  case "$exit_want" in
    "") tag="-" ;;
    0)  tag="HAPPY" ;;
    *)  tag="UNHAPPY" ;;
  esac

  if [ "$ok" = 1 ]; then
    report PASS "$tag" "$name" "$(fmt_cmd "$@")"
  else
    report FAIL "$tag" "$name" "$(fmt_cmd "$@")" "${why#; }"
  fi
}

echo
echo "Running live tests in $APP"
echo "----------------------------------------------------------------------"

# The report is rewritten from scratch on every run: it describes this run, and
# a half-stale file would be worse than none.
if [ -n "$MD" ]; then
  {
    printf '# git-wt test report\n\n'
    printf -- '- Version: `%s`\n' "$("$BIN" version 2>/dev/null | head -n1)"
    printf -- '- Build: `%s`\n' "$profile"
    printf -- '- Date: `%s`\n\n' "$(date '+%Y-%m-%d %H:%M:%S %Z')"
    printf '## Results\n\n'
    printf '| | Tag | Test | Command | Failure |\n'
    printf '|---|---|---|---|---|\n'
  } > "$MD"
fi

# --- list / default ---------------------------------------------------------
check "no-args lists main"            exit=0 out="myapp" --
check "list shows main"              exit=0 out="main" -- list
check "ls alias"                     exit=0 out="main" -- ls
check "list no-match errors"         exit=1 err="no worktree matches 'zzz'" -- list zzz

# --- add --------------------------------------------------------------------
# The created path is printed on stdout (so scripts can capture it).
check "add existing local branch"    exit=0 out="$CODE/myapp-feature-login" -- add feature/login
# worktree now exists at index 2
check "list shows new worktree"      exit=0 out="feature/login" -- list
check "list filter keeps index"      exit=0 out="2  feature/login" -- list logi
check "list --col branch only"       exit=0 out="feature/login" -- list --col 2 logi
check "list --col id+branch"         exit=0 out="2  feature/login" -- list --col 1,2 logi
check "list --col reorder"           exit=0 out="feature/login  2" -- list --col 2,1 logi
check "list --col bad number"        exit=1 err="no column 11" -- list --col 11
check "list --col non-numeric"       exit=1 err="bad column 'x'" -- list --col x
check "bare --col (no list word)"    exit=0 out="main" -- --col 2
check "bare -c short flag"           exit=0 out="main" -- -c 1,2
check "add --name suffix"            exit=0 out="$CODE/myapp-review" -- add feature/logout --name review
check "add --dirname whole leaf"     exit=0 out="$CODE/scratch2" -- add feature/api --dirname scratch2
check "add tracks remote-only"       exit=0 out="$CODE/myapp-remote-only" err="Tracking remote branch 'origin/remote-only'" -- add remote-only
check "add --dirname as path"        exit=0 out="$CODE/sub/deep" -- add pathtest --dirname sub/deep
check "add --from a ref (new branch)" exit=0 out="$CODE/ff1" err="Creating new branch 'newfrom' from 'feature/login'" in=y -- add newfrom --from feature/login --dirname ff1
check "add dup dir refused"          exit=1 err="already exists" -- add feature/login
check "add name+dirname conflict"    exit=1 err="--name and --dirname conflict" -- add x -n a --dirname b
check "add --name empty"             exit=1 err="--name cannot be empty" -- add x -n ""
check "add --from needs ref"         exit=1 err="--from needs a ref" -- add x --from
check "add new branch declined"      exit=0 err="Aborted." in=n -- add nope --dirname np1
check "add --stay accepted"          exit=0 out="$CODE/stay1" -- add staybr --dirname stay1 --stay
# Picker hides checked-out branches under a separate section and offers the rest.
# Cancel the picker (empty stdin) so it prints the section but creates nothing.
check "picker lists checked-out sep"  exit=1 err="Already checked out (not selectable):" -- add
check "picker shows a checked-out br"  exit=1 err="feature/login" -- add

# Self-contained: a repo where every branch is checked out -> picker errors.
FULL="$ROOT/full/app"; mkdir -p "$FULL"
( cd "$FULL"
  git init -q; git checkout -q -b main
  git config user.email t@t; git config user.name t
  git commit -q --allow-empty -m i; git branch only
  "$BIN" add only >/dev/null 2>&1 )          # main + only both checked out now
allco="$(cd "$FULL" && printf '\n' | "$BIN" add 2>&1)"
if printf '%s' "$allco" | grep -q "All local branches are already checked out"; then
  report PASS UNHAPPY "picker errors when all checked out" "$(fmt_cmd add)  # in a fully-checked-out repo"
else
  report FAIL UNHAPPY "picker errors when all checked out" "$(fmt_cmd add)" "got '$allco'"
fi

# --from actually based the new branch on the given ref, not current HEAD.
ffhead="$(git -C "$CODE/ff1" rev-parse HEAD)"
ffwant="$(git -C "$APP" rev-parse feature/login)"
if [ "$ffhead" = "$ffwant" ]; then
  report PASS HAPPY "add --from base commit matches ref" "git rev-parse HEAD  # in ff1"
else
  report FAIL HAPPY "add --from base commit matches ref" "git rev-parse HEAD  # in ff1" \
    "HEAD $ffhead != feature/login $ffwant"
fi

# --- target: switch / path --------------------------------------------------
check "bare N prints path"           exit=0 out="myapp" -- 1
check "N path prints path"           exit=0 out="$APP" -- 1 path
check "N show alias"                 exit=0 out="$APP" -- 1 show
check "N switch too many args"       exit=1 err="too many arguments" -- 1 switch path
check "index 0 errors"               exit=1 err="no worktree #0" -- 0
check "index over range errors"      exit=1 err="there are" -- 99
# Pin the whole action list, not just the prefix: the README documents it, and
# a substring match let it drift silently when diff/meld were added.
check "unknown action errors"        exit=1 err="unknown action 'bogus' (switch, path, remove, diff, commits, merge, meld, merged, fetch, pull, push)" -- 1 bogus
check "flag on target errors"        exit=1 err="'-n' is an option, not an action" -- 1 -n x
# The message must not claim an action rejects flags that it actually takes.
check "flag on target hints actions" exit=1 err="options follow the action" -- 1 --stat

# --- legacy / unknown -------------------------------------------------------
check "legacy show hint"             exit=1 err="use 'git-wt 1 path'" -- show 1
check "legacy remove hint"           exit=1 err="use 'git-wt 1 remove'" -- remove 1
check "branch-like suggests add"     exit=1 err="did you mean 'add feat/x'" -- feat/x
check "plain unknown no suggest"     exit=1 err="unknown command 'lsit'" -- lsit

# --- diff -------------------------------------------------------------------
# Give the two sides real, divergent commits: main-only 'onlymain.txt' vs
# login-only 'onlylogin.txt'. That split is what tells '..' from '...'.
didx="$("$BIN" list | awk '$2=="feature/login"{print $1}')"
( cd "$APP" && echo m > onlymain.txt && git add -A && git commit -qm mainside )
( cd "$CODE/myapp-feature-login" && echo l > onlylogin.txt && git add -A && git commit -qm loginside )

check "diff --name-only shows adds"  exit=0 out="onlylogin.txt" -- "1,$didx" diff --name-only
# '..' is both directions: main's file shows as a deletion, login's as an add.
check "diff .. keeps main-only file" exit=0 out="onlymain.txt" -- "1,$didx" diff .. --name-only
# The default is '...' -- "since the fork" -- so main's own later commit drops
# out, and the listing matches what '1,N merge' would actually bring in. A '..'
# default would report main's commit as a deletion the merge never makes.
for spelling in "" "..."; do
  # shellcheck disable=SC2086 # empty spelling must vanish, not pass ''
  dots3="$("$BIN" "1,$didx" diff $spelling --name-only 2>/dev/null)"
  dcmd="$(fmt_cmd "1,$didx" diff $spelling --name-only)"
  name="diff ${spelling:-(default)} hides main-only file"
  case "$dots3" in
    *onlymain.txt*)
      report FAIL HAPPY "$name" "$dcmd" "main-only file still listed: '$dots3'" ;;
    *onlylogin.txt*)
      report PASS HAPPY "$name" "$dcmd" ;;
    *)
      report FAIL HAPPY "$name" "$dcmd" "wanted login's file, got '$dots3'" ;;
  esac
done
check "diff --stat"                  exit=0 out="1 +" -- "1,$didx" diff --stat
check "diff --name-status"           exit=0 out="A" -- "1,$didx" diff --name-status
# Exact output, not a substring: the unfiltered diff also *contains*
# 'onlylogin.txt', so a substring assertion here would pass even if the
# pathspec were dropped on the floor. "Limits" means the other files are gone.
pspec="$("$BIN" "1,$didx" diff --name-only -- onlylogin.txt 2>/dev/null)"
pcmd="$(fmt_cmd "1,$didx" diff --name-only -- onlylogin.txt)"
if [ "$pspec" = "onlylogin.txt" ]; then
  report PASS HAPPY "diff -- pathspec limits" "$pcmd"
else
  report FAIL HAPPY "diff -- pathspec limits" "$pcmd" "wanted exactly 'onlylogin.txt', got '$pspec'"
fi
check "diff needs two worktrees"     exit=1 err="diff takes a worktree list" -- 1 diff
# The old 'N diff M' grammar: the trailing target is now junk in the action slot.
check "diff old form errors"         exit=1 err="diff takes a worktree list" -- 1 diff "$didx"
check "diff non-numeric target"      exit=1 err="bad worktree list '1,x'" -- "1,x" diff
check "diff bad index errors"        exit=1 err="no worktree #99" -- "1,99" diff
check "diff against itself errors"   exit=1 err="against itself" -- "1,1" diff
# meld takes 3; diff cannot, since 'git diff' compares exactly two things.
check "diff rejects three targets"   exit=1 err="exactly two worktrees, got 3" -- "1,$didx,1" diff
check "diff rejects other git flags" exit=1 err="unexpected argument '-w' for diff" -- "1,$didx" diff -w
# The hint must name the real branches, not echo the offending flag back as a
# ref: 'git diff -w..feat -w' is what a shadowed loop variable looks like.
check "diff flag error hints git"    exit=1 err="run git itself: git diff main...feature/login -w" -- "1,$didx" diff -w

# Uncommitted work is invisible to a ref diff, so it must be called out.
echo scratch > "$CODE/myapp-feature-login/uncommitted.txt"
check "diff warns on dirty worktree" exit=0 err="has uncommitted changes" -- "1,$didx" diff --name-only
check "dirty warning points at live" exit=0 err="git-wt 1,$didx diff live" -- "1,$didx" diff --name-only
rm -f "$CODE/myapp-feature-login/uncommitted.txt"

# --- commits ----------------------------------------------------------------
# Same divergence the diff cases built: 'mainside' on main, 'loginside' on
# feature/login, 'init' shared by both. That makes every cell predictable --
# one commit per column, and a shared one that must not appear at all.
# A single target reads like 'merged' does: N against the worktree you are
# standing in. The suite runs from $APP, which is worktree 1 -- so the rows are
# main's log and login is the check column.
check "commits single target"        exit=0 out="mainside" -- "$didx" commits
check "commits single names both"    exit=0 out="feature/login" -- "$didx" commits
check "commits single takes flags"   exit=0 out="mainside" -- "$didx" commits --author Test
# Standing in the one you named, the table would compare it with itself.
check "commits single self errors"   exit=1 err="standing in" -- 1 commits

check "commits heads the columns"    exit=0 out="feature/login" -- "1,$didx" commits
check "commits lists its own side"   exit=0 out="mainside" -- "1,$didx" commits
# login's own commit is not a row by default: naming a worktree adds a column.
lonly="$("$BIN" "1,$didx" commits 2>/dev/null)"
lcmd="$(fmt_cmd "1,$didx" commits)"
case "$lonly" in
  *loginside*) report FAIL HAPPY "commits anchors on the first" "$lcmd" "'loginside' is not main's: '$lonly'" ;;
  *)           report PASS HAPPY "commits anchors on the first" "$lcmd" ;;
esac
check "commits --union adds the rest" exit=0 out="loginside" -- "1,$didx" commits --union
check "commits --any spells --union"  exit=0 out="loginside" -- "1,$didx" commits --any
check "commits heads the author col" exit=0 out="author" -- "1,$didx" commits
check "commits names the author"     exit=0 out="Test" -- "1,$didx" commits
check "commits heads the subject col" exit=0 out="subject" -- "1,$didx" commits
# ISO by default: what the table prints is what --from-date takes.
check "commits dates the rows"       exit=0 out="$(date +%F)" -- "1,$didx" commits
check "commits --date-human"         exit=0 out=", $(date +%Y)" -- "1,$didx" commits --date-human
# 24-hour h:m:s, appended to whichever day spelling is in play.
check "commits --show-time"          exit=0 out="$(date +%F) " -- "1,$didx" commits --show-time
check "commits --show-time is 24h"   exit=0 out=":" -- "1,$didx" commits --show-time
check "commits human + time"         exit=0 out=", $(date +%Y) " -- "1,$didx" commits --date-human --show-time
# A date read off the table pastes back into a filter unchanged.
check "commits ISO round-trips"      exit=0 out="mainside" -- "1,$didx" commits --from-date "$(date +%F)"

# The default is a merge-request slice: only branch 1's commits the others
# miss. 'mainside' is main's alone, so it shows; the shared 'init' is cut.
check "commits default shows slice"  exit=0 out="mainside" -- "1,$didx" commits
# --all brings back the whole first-branch log, so the shared root reappears.
check "commits --all shows full log" exit=0 out="init" -- "1,$didx" commits --all
# --all and --union are different row sources and cannot combine.
check "commits --all vs --union"     exit=1 err="two different row sources" -- "1,$didx" commits --all --union
# Short flags, alone and bundled under one dash.
check "commits -a aliases --all"     exit=0 out="init" -- "1,$didx" commits -a
check "commits -f aliases --files"   exit=0 out="A  onlymain.txt" -- "1,$didx" commits -a -f
check "commits -af bundles both"     exit=0 out="A  onlymain.txt" -- "1,$didx" commits -af
check "commits -fa order-free"       exit=0 out="A  onlymain.txt" -- "1,$didx" commits -fa
check "commits -fn takes a value"    exit=0 out="init" -- "1,$didx" commits -afn 20
# A value-taking flag mid-bundle would hand one value to two flags.
check "commits -nf refused"          exit=1 err="has to come last" -- "1,$didx" commits -nf 20
check "commits -nf names the fix"    exit=1 err="-fn <value>" -- "1,$didx" commits -nf 20
# A bundle of letters that name nothing is reported as typed, not split up.
check "commits -xz reported whole"   exit=1 err="'-xz'" -- "1,$didx" commits -xz
# The default really drops the shared root -- not merely 'not asserted'.
droot="$("$BIN" "1,$didx" commits 2>/dev/null | grep -cw init || true)"
dcmd2="$(fmt_cmd "1,$didx" commits)"
if [ "$droot" = 0 ]; then
  report PASS HAPPY "commits default drops shared root" "$dcmd2  # no init row"
else
  report FAIL HAPPY "commits default drops shared root" "$dcmd2" "init present in default range"
fi

# A row is checked only where the branch really has the commit. The subject is
# the last field now, so login's mark -- the last column -- is the one before
# it: 'mainside' is main's alone.
mrow="$("$BIN" "1,$didx" commits 2>/dev/null | awk '/mainside/{print $(NF-1)}')"
mcmd="$(fmt_cmd "1,$didx" commits)"
if [ "$mrow" = "·" ]; then
  report PASS HAPPY "commits leaves foreign cell" "$mcmd  # mainside unchecked on login"
else
  report FAIL HAPPY "commits leaves foreign cell" "$mcmd" "wanted '·' in login's column, got '$mrow'"
fi

# Three columns: what diff cannot do, and the reason the command exists.
oidx="$("$BIN" list | awk '$2=="feature/logout"{print $1}')"
check "commits takes three worktrees" exit=0 out="feature/logout" -- "1,$didx,$oidx" commits

# --topo reorders; it never drops or invents rows. Same set, same count.
check "commits --topo keeps the rows" exit=0 out="mainside" -- "1,$didx" commits --topo
check "commits --topo-order spelling" exit=0 out="loginside" -- "1,$didx" commits --union --topo-order
dcount="$("$BIN" "1,$didx" commits --union 2>/dev/null | grep -cE 'mainside|loginside')"
tcount="$("$BIN" "1,$didx" commits --union --topo 2>/dev/null | grep -cE 'mainside|loginside')"
tcmd="$(fmt_cmd "1,$didx" commits --union --topo)"
if [ "$dcount" = "$tcount" ]; then
  report PASS HAPPY "commits --topo same row set" "$tcmd  # $tcount rows either way"
else
  report FAIL HAPPY "commits --topo same row set" "$tcmd" "date order had $dcount rows, topo had $tcount"
fi

# Which row survives a cap is not asserted: the suite's commits land in the same
# second, so --date-order ties and settles it by ref order. The count is the
# contract -- '-n 1' means one row, whichever it is.
for spelling in "-n 1" "--limit=1"; do
  # shellcheck disable=SC2086 # both spellings must word-split into flag+value
  ncnt="$("$BIN" "1,$didx" commits $spelling 2>/dev/null | grep -cE 'mainside|loginside')"
  ncmd="$(fmt_cmd "1,$didx" commits $spelling)"
  if [ "$ncnt" = 1 ]; then
    report PASS HAPPY "commits $spelling caps the rows" "$ncmd"
  else
    report FAIL HAPPY "commits $spelling caps the rows" "$ncmd" "wanted 1 row, got $ncnt"
  fi
done

# A worktree against itself is a column of guaranteed checks: never meant.
# The subject is the last column precisely so an emoji -- two terminal columns
# wide, but one char -- cannot shift what follows it. Single-word subjects
# throughout, so awk's field numbers mean what they look like: the two marks are
# the fields before the subject. Left of the reorder, this row slid right by one.
# Last in the section: it adds a third diverged commit, which the row-count
# cases above are not expecting.
( cd "$APP" && git commit -q --allow-empty -m "🚀emojisubject" )
erow="$("$BIN" "1,$didx" commits 2>/dev/null | awk '/emojisubject/{print $(NF-2)"|"$(NF-1)}')"
ecmd="$(fmt_cmd "1,$didx" commits)"
if [ "$erow" = "✓|·" ]; then
  report PASS HAPPY "commits align past an emoji" "$ecmd  # emoji subject, marks hold"
else
  report FAIL HAPPY "commits align past an emoji" "$ecmd" "wanted '✓|·', got '$erow'"
fi

# --- commits filters ---------------------------------------------------------
# Every commit in the scratch repo is made now, so "today" brackets them all.
today="$(date +%F)"
tomorrow="$(date -v+1d +%F 2>/dev/null || date -d tomorrow +%F)"
yesterday="$(date -v-1d +%F 2>/dev/null || date -d yesterday +%F)"

check "commits --date = today"        exit=0 out="mainside" -- "1,$didx" commits --date "=$today"
check "commits --date >= today"       exit=0 out="mainside" -- "1,$didx" commits --date ">=$today"
check "commits --date <= today"       exit=0 out="mainside" -- "1,$didx" commits --date "<=$today"
check "commits --date bare = '='"     exit=0 out="mainside" -- "1,$didx" commits --date "$today"
check "commits --from-date today"     exit=0 out="mainside" -- "1,$didx" commits --from-date "$today"
check "commits --to-date today"       exit=0 out="mainside" -- "1,$didx" commits --to-date "$today"
# Two bounds are an AND, which is how a range is spelled.
check "commits date range brackets"   exit=0 out="mainside" -- "1,$didx" commits --from-date "$yesterday" --to-date "$tomorrow"
# A bound that keeps nothing reports the filter, not an empty history.
check "commits --from-date tomorrow"  exit=0 err="no commits match those filters" -- "1,$didx" commits --from-date "$tomorrow"
check "commits --to-date yesterday"   exit=0 err="no commits match those filters" -- "1,$didx" commits --to-date "$yesterday"

# Inclusive bounds only: a strict comparison names a day '>=' already reaches.
check "commits --date rejects >"      exit=1 err="no '>' comparison" -- "1,$didx" commits --date ">$today"
check "commits --date rejects <"      exit=1 err="no '<' comparison" -- "1,$didx" commits --date "<$today"
check "commits --date bad shape"      exit=1 err="want YYYY-MM-DD" -- "1,$didx" commits --date ">=2026-1-1"
check "commits --date impossible"     exit=1 err="no such date" -- "1,$didx" commits --date "2026-13-01"
check "commits --date needs a value"  exit=1 err="redirect" -- "1,$didx" commits --date
# An unquoted '>' is eaten by the shell, so the value arrives bare: say why.
check "commits --date eaten by shell" exit=1 err="redirect" -- "1,$didx" commits --date ">="

# --from-id/--to-id include the commit they name -- the point of the flags.
mainsha="$(cd "$APP" && git rev-parse --short HEAD)"
check "commits --from-id keeps its own" exit=0 out="$mainsha" -- "1,$didx" commits --from-id "$mainsha"
check "commits --to-id keeps its own"   exit=0 out="$mainsha" -- "1,$didx" commits --to-id "$mainsha"
# ...and --to-id drops what the named commit cannot reach: login's own branch.
toid="$("$BIN" "1,$didx" commits --union --to-id "$mainsha" 2>/dev/null)"
tcmd="$(fmt_cmd "1,$didx" commits --union --to-id "$mainsha")"
case "$toid" in
  *loginside*) report FAIL HAPPY "commits --to-id bounds the walk" "$tcmd" "loginside is unreachable from main: '$toid'" ;;
  *)           report PASS HAPPY "commits --to-id bounds the walk" "$tcmd" ;;
esac
check "commits --from-id bad commit"  exit=1 err="--from-id: no commit 'zzz9'" -- "1,$didx" commits --from-id zzz9
check "commits --to-id needs a value" exit=1 err="--to-id needs a commit" -- "1,$didx" commits --to-id
# A bare --from names neither bound; git's date words point at ours.
check "commits rejects bare --from"   exit=1 err="'--from-id' takes a commit" -- "1,$didx" commits --from x
check "commits rejects --since"       exit=1 err="use '--from-date" -- "1,$didx" commits --since 2026-01-01
check "commits rejects --until"       exit=1 err="use '--to-date" -- "1,$didx" commits --until 2026-01-01

# --no-merges drops the bookkeeping rows and keeps the work. Needs a merge with
# something to merge: a branch main already contains is "Already up to date"
# and --no-ff writes no commit at all.
( cd "$APP" \
  && git checkout -q -b nm-src \
  && git commit -q --allow-empty -m "nm-work" \
  && git checkout -q main \
  && git merge --no-ff -q -m "merge-for-nomerges" nm-src )
check "commits shows merge rows"     exit=0 out="merge-for-nomerges" -- "1,$didx" commits
check "commits --no-merges drops it" exit=0 err="" -- "1,$didx" commits --no-merges
nm="$("$BIN" "1,$didx" commits --no-merges 2>/dev/null | grep -c "merge-for-nomerges")"
nmc="$(fmt_cmd "1,$didx" commits --no-merges)"
if [ "$nm" = 0 ]; then
  report PASS HAPPY "commits --no-merges hides merges" "$nmc"
else
  report FAIL HAPPY "commits --no-merges hides merges" "$nmc" "merge row survived --no-merges"
fi
# The work the merge joined must survive: only the merge row goes.
check "commits --no-merges keeps work" exit=0 out="mainside" -- "1,$didx" commits --no-merges

# --reverse flips the display, and only the display: '-n 2 --reverse' is the
# same two commits as '-n 2', read bottom-up -- not the two oldest.
# Match the sha rows rather than counting header lines off the top: the table
# is preceded by both a legend and a column header, and a row is the only line
# that starts with a short sha.
fwd="$("$BIN" "1,$didx" commits -n 2 2>/dev/null | awk '$1 ~ /^[0-9a-f]{7,}$/{print $1}')"
rev="$("$BIN" "1,$didx" commits -n 2 --reverse 2>/dev/null | awk '$1 ~ /^[0-9a-f]{7,}$/{print $1}')"
rcmd="$(fmt_cmd "1,$didx" commits -n 2 --reverse)"
want="$(printf '%s\n' "$fwd" | tail -r 2>/dev/null || printf '%s\n' "$fwd" | tac)"
if [ "$rev" = "$want" ]; then
  report PASS HAPPY "commits --reverse flips rows" "$rcmd"
else
  report FAIL HAPPY "commits --reverse flips rows" "$rcmd" "wanted '$want', got '$rev'"
fi
check "commits --oldest-first alias"  exit=0 out="mainside" -- "1,$didx" commits --oldest-first

# --md writes a file and prints nothing on stdout: the table is the file.
mdout="$ROOT/table.md"
check "commits --md names the file"  exit=0 err="Wrote $mdout" -- "1,$didx" commits --md "$mdout"
check "commits --md=PATH spelling"   exit=0 err="Wrote $ROOT/eq.md" -- "1,$didx" commits --md="$ROOT/eq.md"
mdcmd="$(fmt_cmd "1,$didx" commits --md "$mdout")"
if [ -f "$mdout" ] && grep -q '^| commit | author | date |' "$mdout" && grep -q 'mainside' "$mdout"; then
  report PASS HAPPY "commits --md writes a table" "$mdcmd"
else
  report FAIL HAPPY "commits --md writes a table" "$mdcmd" "no markdown table in $mdout"
fi
# A '|' in a subject must not become a column break.
( cd "$APP" && git commit -q --allow-empty -m "md: a|piped|subject" )
"$BIN" "1,$didx" commits --md "$ROOT/pipes.md" >/dev/null 2>&1
pcmd="$(fmt_cmd "1,$didx" commits --md "$ROOT/pipes.md")"
if grep -q 'a\\|piped\\|subject' "$ROOT/pipes.md"; then
  report PASS HAPPY "commits --md escapes pipes" "$pcmd"
else
  report FAIL HAPPY "commits --md escapes pipes" "$pcmd" "pipe left unescaped: $(grep piped "$ROOT/pipes.md")"
fi
# The path is optional: a flag after --md is a flag, not a filename. The
# default name lands in the cwd, so this has to run somewhere inside the repo.
( cd "$APP" && "$BIN" "1,$didx" commits --md --topo >/dev/null 2>&1 )
dcmd="$(fmt_cmd "1,$didx" commits --md --topo)"
if ls "$APP"/commits_*.md >/dev/null 2>&1; then
  report PASS HAPPY "commits --md default name" "$dcmd  # commits_<stamp>.md"
else
  report FAIL HAPPY "commits --md default name" "$dcmd" "no commits_<stamp>.md written"
fi
rm -f "$APP"/commits_*.md
check "commits --md bad dir errors"  exit=1 err="cannot write" -- "1,$didx" commits --md /nope/nope/x.md

# A cherry-picked patch is neither present nor missing: '≈', not '·'. main
# already has work of its own, so the pick lands on a different parent and is
# a real copy -- picked onto the same parent, every input matches and git
# reproduces the original's sha instead.
( cd "$CODE/myapp-feature-login" && echo picked > picked.txt && git add -A && git commit -q -m "cherrypicked-work" )
psha="$(cd "$CODE/myapp-feature-login" && git rev-parse HEAD)"
( cd "$APP" && git cherry-pick "$psha" >/dev/null 2>&1 )
# LC_ALL=C, or sort collates '✓' and '≈' as equal -- they are symbols, which a
# UTF-8 locale ignores when comparing -- and -u folds the two rows into one.
# --union, or login's original is not a row and only main's copy can be seen.
crow="$("$BIN" "1,$didx" commits --union 2>/dev/null | awk '/cherrypicked-work/{print $(NF-2)"|"$(NF-1)}' | LC_ALL=C sort -u | tr '\n' ' ')"
ccmd="$(fmt_cmd "1,$didx" commits --union)"
# Two rows now carry that patch: main's copy (✓ ≈) and login's original (≈ ✓).
if [ "$crow" = "≈|✓ ✓|≈ " ]; then
  report PASS HAPPY "commits marks a cherry-pick" "$ccmd  # ≈ = same patch, other sha"
else
  report FAIL HAPPY "commits marks a cherry-pick" "$ccmd" "wanted '≈|✓ ✓|≈ ', got '$crow'"
fi
check "commits --no-cherry drops ≈"   exit=0 err="" -- "1,$didx" commits --union --no-cherry
nc="$("$BIN" "1,$didx" commits --union --no-cherry 2>/dev/null | grep -c "≈")"
nccmd="$(fmt_cmd "1,$didx" commits --union --no-cherry)"
if [ "$nc" = 0 ]; then
  report PASS HAPPY "commits --no-cherry is plain" "$nccmd  # no ≈ without the walk"
else
  report FAIL HAPPY "commits --no-cherry is plain" "$nccmd" "$nc rows still marked ≈"
fi
# '≈' must mean something: work nobody picked still reads as absent.
lrow="$("$BIN" "1,$didx" commits --union 2>/dev/null | awk '/loginside/{print $(NF-1)}')"
lcmd="$(fmt_cmd "1,$didx" commits --union)"
if [ "$lrow" = "✓" ]; then
  report PASS HAPPY "commits leaves unpicked work" "$lcmd  # loginside: not on main, not ≈"
else
  report FAIL HAPPY "commits leaves unpicked work" "$lcmd" "wanted login's '✓', got '$lrow'"
fi

# --author is the same fuzzy subsequence 'list' filters with.
check "commits --author exact"        exit=0 out="mainside" -- "1,$didx" commits --author Test
check "commits --author fuzzy"        exit=0 out="mainside" -- "1,$didx" commits --author tst
check "commits --author case-folds"   exit=0 out="mainside" -- "1,$didx" commits --author TEST
check "commits --author no match"     exit=0 err="no commits match those filters" -- "1,$didx" commits --author zzzz
check "commits --author needs a name" exit=1 err="--author needs a name" -- "1,$didx" commits --author

check "commits rejects a dup target" exit=1 err="listed twice" -- "1,1" commits
check "commits bad index errors"     exit=1 err="no worktree #99" -- "1,99" commits
check "commits rejects git flags"    exit=1 err="unexpected argument '--stat' for commits" -- "1,$didx" commits --stat
check "commits -n needs a count"     exit=1 err="-n needs a count" -- "1,$didx" commits -n
check "commits -n 0 errors"          exit=1 err="would show nothing" -- "1,$didx" commits -n 0
check "commits -n non-numeric"       exit=1 err="bad count 'x'" -- "1,$didx" commits -n x
check "bare commits hints the list"  exit=1 err="use 'git-wt 1,2 commits'" -- commits

# --- diff live --------------------------------------------------------------
# The case no ref diff can answer: put BOTH worktrees on the same commit, then
# change one on disk only. 'git diff <a>..<b>' is provably empty here -- both
# refs resolve to the same tree -- so any output at all proves live read disk.
LIVE="$CODE/myapp-live"
"$BIN" add livebr --dirname myapp-live --from main >/dev/null 2>&1 <<< y
( cd "$APP" && git checkout -q main )
lidx="$("$BIN" list | awk '$2=="livebr"{print $1}')"
printf 'one\ntwo\nthree\n' > "$APP/shared.txt"
( cd "$APP" && git add -A && git commit -qm shared )
( cd "$LIVE" && git merge -q main )     # same commit, same tree, both sides
echo 'ignoreme/' > "$APP/.gitignore"
( cd "$APP" && git add -A && git commit -qm ign )
( cd "$LIVE" && git merge -q main )

# Uncommitted-only divergence in the live worktree.
printf 'one\nTWO\nthree\nfour\n' > "$LIVE/shared.txt"   # 1 modified, 1 added
echo brandnew > "$LIVE/untracked.txt"                    # untracked -> a real add
mkdir -p "$LIVE/ignoreme" && echo junk > "$LIVE/ignoreme/x.o"

# Both on one commit: the ref diff is empty, and that is the whole problem.
refout="$("$BIN" "1,$lidx" diff --name-only 2>/dev/null)"
rcmd="$(fmt_cmd "1,$lidx" diff --name-only)"
if [ -z "$refout" ]; then
  report PASS HAPPY "ref diff blind on same commit" "$rcmd  # empty, as designed"
else
  report FAIL HAPPY "ref diff blind on same commit" "$rcmd" "wanted empty, got '$refout'"
fi

check "live sees uncommitted edit"   exit=0 out="shared.txt" -- "1,$lidx" diff live --name-only
check "live sees untracked as add"   exit=0 out="A	untracked.txt" -- "1,$lidx" diff live --name-status
check "live counts hunks"            exit=0 out="+2" -- "1,$lidx" diff live
check "live summary counts lines"    exit=0 out="2 files changed, 3 insertions(+), 1 deletion(-)" -- "1,$lidx" diff live
check "live hunks show line numbers" exit=0 out="modified 1" -- "1,$lidx" diff live hunks
check "live --stat still works"      exit=0 out="3 ++-" -- "1,$lidx" diff live --stat
check "live suppresses dirty warn"   exit=0 err="" -- "1,$lidx" diff live --name-only

# .gitignore is the reason live can't just be 'diff -rq': without it, build
# output drowns the signal.
ligr="$("$BIN" "1,$lidx" diff live --name-only 2>/dev/null)"
gcmd="$(fmt_cmd "1,$lidx" diff live --name-only)"
case "$ligr" in
  *ignoreme*) report FAIL HAPPY "live honors .gitignore" "$gcmd" "ignored file listed: '$ligr'" ;;
  *)          report PASS HAPPY "live honors .gitignore" "$gcmd" ;;
esac

lspec="$("$BIN" "1,$lidx" diff live --name-only -- shared.txt 2>/dev/null)"
lpcmd="$(fmt_cmd "1,$lidx" diff live --name-only -- shared.txt)"
if [ "$lspec" = "shared.txt" ]; then
  report PASS HAPPY "live -- pathspec limits" "$lpcmd"
else
  report FAIL HAPPY "live -- pathspec limits" "$lpcmd" "wanted exactly 'shared.txt', got '$lspec'"
fi

# A deleted-on-disk file is a delete, and its hunk must not read as '+0'.
rm "$LIVE/shared.txt"
check "live reports on-disk delete"  exit=0 out="D	shared.txt" -- "1,$lidx" diff live --name-status
check "live delete is not an add"    exit=0 out="deleted 3" -- "1,$lidx" diff live hunks
printf 'one\nTWO\nthree\nfour\n' > "$LIVE/shared.txt"

# Identical contents: stdout stays empty so a pipe sees nothing, and the note
# that this is a real answer (not the empty-ref-diff bug) goes to stderr.
check "live identical is empty out"  exit=0 out="" err="no differences" -- "1,$lidx" diff live -- .gitignore

check "live identical says so once"  exit=0 err="no differences" -- "1,$lidx" diff live --name-only -- .gitignore
# The ref-diff hint would contradict the mode the user is already in, and it
# must not reappear just because 'live' came after the offending flag.
check "live bad flag hints no-index"  exit=1 err="git diff --no-index" -- "1,$lidx" diff live -w
check "live hint survives word order" exit=1 err="git diff --no-index" -- "1,$lidx" diff -w live
livehint="$("$BIN" "1,$lidx" diff live -w 2>&1)"
lhcmd="$(fmt_cmd "1,$lidx" diff live -w)"
case "$livehint" in
  *"run git itself"*) report FAIL UNHAPPY "live bad flag drops ref hint" "$lhcmd" "ref-diff hint leaked: '$livehint'" ;;
  *)                  report PASS UNHAPPY "live bad flag drops ref hint" "$lhcmd" ;;
esac
# A pathspec named 'live' is a path, not the mode word.
check "pathspec 'live' not the mode"  exit=0 err="" -- "1,$lidx" diff --name-only -- live

check "live rejects .. range"        exit=1 err="'live' and '..' cannot combine" -- "1,$lidx" diff live ..
check "live rejects ... range"       exit=1 err="'live' and '...' cannot combine" -- "1,$lidx" diff live ...
check "hunks rejects --stat"         exit=1 err="cannot combine" -- "1,$lidx" diff hunks --stat
check "--live dashed form works"     exit=0 out="shared.txt" -- "1,$lidx" diff --live --name-only
check "hunks works without live"     exit=0 out="committed state" -- "1,$didx" diff hunks

# --- list --files -----------------------------------------------------------
# The live worktree is already dirty in all three interesting ways: a tracked
# edit, an untracked file, and an ignored one. --files must show the first two
# under the branch row and never the third.
check "list --files shows edit"      exit=0 out="M  shared.txt" -- list --files
check "list --files counts lines"    exit=0 out="+2  -1" -- list --files
check "list --files shows untracked" exit=0 out="?  untracked.txt" -- list --files
check "list --files counts untracked" exit=0 out="+1  -0" -- list --files
check "bare --files (no list word)"  exit=0 out="M  shared.txt" -- --files
check "list -f short flag"           exit=0 out="M  shared.txt" -- -f
check "--files combines with --col"  exit=0 out="M  shared.txt" -- list --col 2 --files

fign="$("$BIN" list --files 2>/dev/null)"
fcmd="$(fmt_cmd list --files)"
case "$fign" in
  *ignoreme*) report FAIL HAPPY "list --files honors .gitignore" "$fcmd" "ignored file listed" ;;
  *)          report PASS HAPPY "list --files honors .gitignore" "$fcmd" ;;
esac

# Without the flag the listing stays a table: no file block at all.
fplain="$("$BIN" list 2>/dev/null)"
pcmd="$(fmt_cmd list)"
case "$fplain" in
  *shared.txt*) report FAIL HAPPY "list without --files has no block" "$pcmd" "file block leaked" ;;
  *)            report PASS HAPPY "list without --files has no block" "$pcmd" ;;
esac

# --- meld -------------------------------------------------------------------
# A stub 'meld' on PATH echoes its argv and lists the files inside each
# directory it receives, so we can assert on both the pane order and the
# extracted contents. Real meld is never launched.
FAKEBIN="$ROOT/fakebin"
mkdir -p "$FAKEBIN"
cat > "$FAKEBIN/meld" <<'EOF'
#!/bin/sh
echo "ARGV: $@"
for d in "$@"; do
  if [ -d "$d" ]; then
    echo "DIR: $d"
    find "$d" -type f | sort
  fi
done
EOF
chmod +x "$FAKEBIN/meld"
PATH="$FAKEBIN:$PATH"

# Indices come from list (git's own order); paths from 'path', so the assertion
# compares what git-wt reports against what it handed the stub.
lidx="$("$BIN" list | awk '$2=="feature/login"{print $1}')"
ridx="$("$BIN" list | awk '$2=="logout"||$2=="feature/logout"{print $1}')"
lpath="$("$BIN" "$lidx" path 2>/dev/null)"
rpath="$("$BIN" "$ridx" path 2>/dev/null)"
mpath="$("$BIN" 1 path 2>/dev/null)"

check "meld 2 trees passes both dirs" exit=0 out="ARGV: $mpath $lpath" -- "1,$lidx" meld
check "meld 3 trees, listed order"    exit=0 out="ARGV: $lpath $mpath $rpath" -- "$lidx,1,$ridx" meld
check "meld one tree errors"          exit=1 err="meld needs 2 or 3 worktrees" -- 1 meld
check "meld over 3 errors"            exit=1 err="at most 3 worktrees, got 4" -- 1,2,3,4 meld
check "meld dup tree errors"          exit=1 err="worktree #1 listed twice" -- 1,1 meld
check "meld bad index errors"         exit=1 err="no worktree #99" -- 1,99 meld
check "meld non-numeric list errors"  exit=1 err="bad worktree list '1,x'" -- 1,x meld
check "meld takes no options"         exit=1 err="unknown option '-x'" -- 1,2 meld -x
check "meld --diff needs 2 trees"     exit=1 err="takes exactly 2 worktrees" -- 1,2,3 meld --diff
check "meld --diff ... range works"   exit=0 out="onlylogin.txt" -- "1,$didx" meld --diff ...
check "meld --diff ... omits main"    exit=0 err="" -- "1,$didx" meld --diff ...
check "meld --diff 2-way works"       exit=0 out="onlymain.txt" -- "1,$didx" meld --diff
check "meld --diff 2-way has shared"  exit=0 out="shared.txt" -- "1,$didx" meld --diff
check "meld --diff empty diff"        exit=0 err="no files differ" -- "1,1" meld --diff
check "meld --3way and --base clash"  exit=1 err="alternatives" -- 1,2 meld --diff --3way --base main
check "list needs an action"          exit=1 err="needs an action" -- 1,2
check "list rejects single-tree verb" exit=1 err="'remove' takes a single worktree" -- 1,2 remove

# meld missing from PATH: a PATH holding only git proves the check fires
# regardless of whether the host has a real meld installed.
GITONLY="$ROOT/gitonly"
mkdir -p "$GITONLY"
ln -sf "$(command -v git)" "$GITONLY/git"
meld_err="$(PATH="$GITONLY" "$BIN" 1,2 meld 2>&1 >/dev/null; )"
mcmd="$(fmt_cmd 1,2 meld)  # PATH without meld"
case "$meld_err" in
  *"meld is not installed"*)
    report PASS UNHAPPY "meld missing gives install hint" "$mcmd" ;;
  *)
    report FAIL UNHAPPY "meld missing gives install hint" "$mcmd" "got '$meld_err'" ;;
esac

# --- remove -----------------------------------------------------------------
check "remove main refused"          exit=1 err="refusing to remove the main worktree" -- 1 remove -y
# Removing a tree you are NOT standing in prints nothing (wrapper stays put).
check "remove other prints nothing"  exit=0 out="" -- 2 remove -y

# Standing INSIDE the removed tree: it prints main so the wrapper cd's back.
"$BIN" add insidebr --dirname insidewt >/dev/null 2>&1
iidx="$("$BIN" list | awk '$2=="insidebr"{print $1}')"
inside_out="$(cd "$CODE/insidewt" && "$BIN" "$iidx" remove -y </dev/null 2>/dev/null)"
app_phys="$(cd "$APP" && pwd -P)"
if [ "$inside_out" = "$app_phys" ]; then
  report PASS HAPPY "remove-from-inside prints main" "$(fmt_cmd "$iidx" remove -y)  # cwd inside it"
else
  report FAIL HAPPY "remove-from-inside prints main" "$(fmt_cmd "$iidx" remove -y)" \
    "wanted main '$app_phys', got '$inside_out'"
fi

# -f: a worktree with an untracked file is refused without -f, removed with it.
"$BIN" add dirty --dirname dirtywt >/dev/null 2>&1
touch "$CODE/dirtywt/junk.txt"
didx="$("$BIN" list | awk '$2=="dirty"{print $1}')"
check "remove dirty refused (no -f)" exit=1 err="modified or untracked files" -- "$didx" remove -y
check "remove dirty with -f"         exit=0 -- "$didx" remove -y -f

# --- merge ------------------------------------------------------------------
# Self-contained repo: merges move branches, so keep them off the shared fixture.
#   main    base.txt, shared.txt="0"
#   feat-a  adds a.txt          (clean merge into main)
#   cb1/cb2/cb3/cb4 shared.txt A vs B vs C vs D  (each conflicts with the others)
#   stuckbr  a second copy of B, kept aside so a merge can be left stopped
MRG="$ROOT/mrg/app"; mkdir -p "$MRG"
( cd "$MRG"
  git init -q; git checkout -q -b main
  git config user.email t@t; git config user.name t
  echo base > base.txt; echo 0 > shared.txt
  git add .; git commit -q -m init
  git branch feat-a; git branch cb1; git branch cb2; git branch cb3; git branch cb4
  git branch ffbr; git branch dirtybr; git branch lmbr
  git checkout -q feat-a; echo a > a.txt; git add a.txt; git commit -q -m a
  git checkout -q cb1; echo A > shared.txt; git commit -q -am A
  git checkout -q cb2; echo B > shared.txt; git commit -q -am B
  git checkout -q cb3; echo C > shared.txt; git commit -q -am C
  git checkout -q cb4; echo D > shared.txt; git commit -q -am D
  # stuckbr forks from cb2 *before* any test merges into it, so it still
  # genuinely collides with cb3 late in the run.
  git branch stuckbr cb2
  git checkout -q main
  "$BIN" add feat-a  --dirname w-feat  >/dev/null 2>&1
  "$BIN" add cb1     --dirname w-cb1   >/dev/null 2>&1
  "$BIN" add cb2     --dirname w-cb2   >/dev/null 2>&1
  "$BIN" add stuckbr --dirname w-ff2   >/dev/null 2>&1
  "$BIN" add dirtybr --dirname w-dirty >/dev/null 2>&1
  # The list form names both sides by number, so the source needs a worktree
  # too — cb3 gets one, and lmbr is a clean destination to merge into.
  "$BIN" add cb3     --dirname w-cb3   >/dev/null 2>&1
  "$BIN" add lmbr    --dirname w-lm    >/dev/null 2>&1 )

cd "$MRG" || exit 1
midx() { "$BIN" list | awk -v b="$1" '$2==b{print $1}'; }
A="$(midx feat-a)"; C1="$(midx cb1)"; C2="$(midx cb2)"; D="$(midx dirtybr)"
FF2="$(midx stuckbr)"; M3="$(midx cb3)"; LM="$(midx lmbr)"

# Errors before any state changes.
# Worktree-number sources use the list form; branch sources and resume words
# keep the single-target form.
check "merge needs a source"         exit=1 err="merge needs a source" -- 1 merge
check "merge unknown command"        exit=1 err="use 'git-wt 1,2 merge'" -- merge 2
check "merge old form errors"        exit=1 err="merge takes a worktree list: 'git-wt 1,2 merge' (or use 'heads/2' for a branch of the same name)" -- 1 merge 2
check "merge unknown source"         exit=1 err="no worktree or branch 'zzz'" -- 1 merge zzz
check "merge self refused"           exit=1 err="already checked out in worktree 1" -- "1,1" merge
check "merge too many args"          exit=1 err="too many arguments" -- "1,$A" merge "$C1"
check "merge unknown option"         exit=1 err="unknown option '--rebase'" -- "1,$A" merge --rebase
check "merge ours+theirs conflict"   exit=1 err="ours and theirs conflict" -- "1,$A" merge ours theirs
check "merge dry-run + --no-ff"      exit=1 err="dry-run takes no merge options (got --no-ff)" -- "1,$A" merge dry-run --no-ff
# The resume words keep the single-target form, so their parse errors are
# reachable only there.
check "merge continue takes no arg"  exit=1 err="continue takes no argument" -- 1 merge --continue 2
check "merge continue with a side"   exit=1 err="applied when a merge starts" -- 1 merge theirs continue
check "merge continue+abort"         exit=1 err="continue and abort conflict" -- 1 merge continue abort
check "rejection names the flag"     exit=1 err="(got -m, --squash)" -- 1 merge abort -m x --squash
check "merge continue w/o merge"     exit=1 err="no merge in progress" -- 1 merge --continue
check "merge abort w/o merge"        exit=1 err="no merge in progress" -- 1 merge abort

# Dirty destination takes -f; untracked files alone do not count as dirty.
touch "$ROOT/mrg/w-dirty/untracked.txt"
check "merge with untracked only ok"  exit=0 err="Merged feat-a into dirtybr" -- "$D,$A" merge
git -C "$ROOT/mrg/w-dirty" merge -q --abort 2>/dev/null; git -C "$ROOT/mrg/w-dirty" reset -q --hard HEAD~1
# Tracked edits must still be refused even when untracked files are also
# present; the porcelain reports both, and the untracked lines must not mask
# the tracked ones. Re-create the untracked file so the combined case is real.
touch "$ROOT/mrg/w-dirty/untracked.txt"
echo edit >> "$ROOT/mrg/w-dirty/base.txt"
check "merge into dirty+untracked refused" exit=1 err="uncommitted changes" -- "$D,$A" merge
rm -f "$ROOT/mrg/w-dirty/untracked.txt"
check "merge into dirty refused"     exit=1 err="uncommitted changes" -- "$D,$A" merge
check "merge into dirty with -f"     exit=0 err="Merged feat-a into dirtybr" -- "$D,$A" merge -f

# Clean merge by worktree number: worktree A's branch moves into worktree 1.
check "merge by number"              exit=0 err="Merged feat-a into" -- "1,$A" merge
if [ -f "$MRG/a.txt" ]; then
  report PASS HAPPY "merge by number moved the files" "test -f a.txt  # in worktree 1"
else
  report FAIL HAPPY "merge by number moved the files" "test -f a.txt  # in worktree 1" \
    "a.txt absent from $MRG after merge"
fi
check "merge prints no stdout"       exit=0 out="" -- "$C1,$A" merge

# A branch name works where a number does, and --ff-only refuses a real merge.
check "merge by branch name"         exit=0 err="Merged feat-a into cb2" -- "$C2" merge feat-a
check "merge --ff-only refuses"      exit=1 err="Not possible to fast-forward" -- "$C2,$C1" merge --ff-only

# Conflict -> continue.  cb1 and cb2 both rewrote shared.txt.
check "merge conflict reports files" exit=1 err="shared.txt" -- "$C1,$C2" merge
check "merge conflict hints continue" exit=1 err="merge continue" -- "$C1" merge --continue
check "second merge while stuck"     exit=1 err="already in progress" -- "$C1" merge feat-a
check "continue with unresolved"     exit=1 err="merge conflict in" -- "$C1" merge --continue
echo resolved > "$ROOT/mrg/w-cb1/shared.txt"
git -C "$ROOT/mrg/w-cb1" add shared.txt
check "continue after resolve"       exit=0 err="Completed merge" -- "$C1" merge continue

# Conflict -> abort restores the pre-merge state. cb1 has swallowed cb2 by now,
# so cb3 is the branch that still genuinely conflicts with cb2.
"$BIN" "$C2,$M3" merge >/dev/null 2>&1
check "abort a conflicted merge"     exit=0 err="Aborted merge" -- "$C2" merge --abort
if git -C "$ROOT/mrg/w-cb2" rev-parse --verify -q MERGE_HEAD >/dev/null; then
  report FAIL HAPPY "abort clears MERGE_HEAD" "git rev-parse MERGE_HEAD  # in w-cb2" \
    "MERGE_HEAD still present after --abort"
else
  report PASS HAPPY "abort clears MERGE_HEAD" "git rev-parse MERGE_HEAD  # in w-cb2"
fi

# dry-run: answers the question, writes nothing. cb3 still collides with cb2.
check "dry-run clean merge"          exit=0 err="merges into" -- "$C2,$A" merge dry-run
check "dry-run reports a conflict"   exit=1 err="does NOT merge" -- "$C2,$M3" merge dry-run
check "dry-run names the file"       exit=1 err="shared.txt" -- "$C2,$M3" merge dry-run
check "dry-run says it touched none" exit=1 err="nothing was changed" -- "$C2,$M3" merge dry-run
# The short form drives the same path end to end, not just the parser.
check "dry-run -d short form"        exit=1 err="does NOT merge" -- "$C2,$M3" merge -d
check "theirs -t short form"         exit=0 err="merges into" -- "$C2,$A" merge -t -d
# Proof it wrote nothing: a dry run that predicted a conflict left no merge
# behind, so a real merge can still start cleanly afterwards.
if git -C "$ROOT/mrg/w-cb2" rev-parse --verify -q MERGE_HEAD >/dev/null; then
  report FAIL HAPPY "dry-run leaves no merge state" "git rev-parse MERGE_HEAD  # in w-cb2" \
    "MERGE_HEAD exists after a dry run"
else
  report PASS HAPPY "dry-run leaves no merge state" "git rev-parse MERGE_HEAD  # in w-cb2"
fi

# --- merged -----------------------------------------------------------------
# Read-only ancestor check; state is unchanged. Uses the same fixture as merge.
# cb3 has one commit (C) that is not in main, so it is a stable "ahead" case.
# stuckbr was branched from cb2, so it is cleanly contained in cb2.
# Assumes the harness is standing on 'main' (worktree 1), so a self-check reads
# "Merged main is already in main".
check "merged current in itself"     exit=0 err="Merged main is already in main" -- 1 merged
check "merged branch not in main"    exit=1 err="Ahead cb3 is NOT in main (ahead 1)" -- 1 merged cb3
check "merged branch is in cb2"      exit=0 err="Merged stuckbr is already in cb2" -- "$C2" merged stuckbr
check "merged list form dest-first"  exit=1 err="Ahead cb3 is NOT in main (ahead 1)" -- "1,$M3" merged
check "merged list form reversed"    exit=0 err="Merged stuckbr is already in cb2" -- "$C2,$FF2" merged
check "merged too many args"         exit=1 err="too many arguments" -- 1 merged cb3 extra
check "merged unknown source"        exit=1 err="no worktree or branch 'zzz'" -- 1 merged zzz
check "merged self single form"      exit=1 err="already checked out in worktree 1" -- 1 merged 1
# A worktree-number source takes the list form, as merge and diff do. A source
# equal to the destination is left to the self-check above, which says more.
check "merged number wants a list"   exit=1 err="merged takes a worktree list: 'git-wt 1,$M3 merged'" -- 1 merged "$M3"
check "merged list too many"         exit=1 err="merged takes exactly two worktrees" -- "1,$M3,$C2" merged
check "merged list form extra arg"   exit=1 err="merged takes no arguments" -- "1,$M3" merged extra
check "merged list form dup"         exit=1 err="worktree #1 listed twice" -- "1,1" merged
check "merged legacy hint"           exit=1 err="use 'git-wt 1 merged' or 'git-wt 1,2 merged'" -- merged 2

# A detached worktree has no branch to name, so the list form is the only way to
# ask about one: 'merged' only tests containment, so it answers by sha.
# Torn down right after: 'git worktree list' orders by path, so leaving this one
# in would renumber the worktrees the hardcoded indices below depend on.
git worktree add --detach "$ROOT/mrg/w-det" main >/dev/null 2>&1
DET="$(midx '(detached)')"
check "merged detached list form"    exit=0 err="is already in main" -- "1,$DET" merged
check "merged detached wants a list" exit=1 err="merged takes a worktree list" -- 1 merged "$DET"
git worktree remove --force "$ROOT/mrg/w-det" >/dev/null 2>&1

# Column 6 in list: shows merged/ahead relative to the current branch (main).
check "list --col 6"                 exit=0 out="ahead 1" -- list --col 1,2,6

# ours/theirs settle the collision that stopped the plain merge above.
# cb3 (shared.txt=C) vs w-cb2 (shared.txt=B): theirs takes C, ours keeps B.
check "merge theirs resolves"        exit=0 err="theirs won conflicts" -- "$C2,$M3" merge theirs
if [ "$(cat "$ROOT/mrg/w-cb2/shared.txt")" = "C" ]; then
  report PASS HAPPY "theirs took the source's side" "cat shared.txt  # in w-cb2"
else
  report FAIL HAPPY "theirs took the source's side" "cat shared.txt  # in w-cb2" \
    "shared.txt is '$(cat "$ROOT/mrg/w-cb2/shared.txt")', want C"
fi

# cb4 (shared.txt=D) collides with whatever w-cb1 settled on earlier.
before="$(cat "$ROOT/mrg/w-cb1/shared.txt")"
check "merge ours keeps our side"    exit=0 err="ours won conflicts" -- "$C1" merge cb4 ours
if [ "$(cat "$ROOT/mrg/w-cb1/shared.txt")" = "$before" ]; then
  report PASS HAPPY "ours kept worktree N's side" "cat shared.txt  # in w-cb1"
else
  report FAIL HAPPY "ours kept worktree N's side" "cat shared.txt  # in w-cb1" \
    "shared.txt changed to '$(cat "$ROOT/mrg/w-cb1/shared.txt")', want '$before'"
fi

# 'theirs' on a merge that already stopped: it can't join one, so git-wt offers
# to abort and redo. Declining must leave the stopped merge exactly as it was.
"$BIN" "$FF2,$M3" merge >/dev/null 2>&1   # conflict in w-ff2
check "stuck+theirs declined"        exit=0 err="Aborted." in=n -- "$FF2,$M3" merge theirs
if git -C "$ROOT/mrg/w-ff2" rev-parse --verify -q MERGE_HEAD >/dev/null; then
  report PASS UNHAPPY "declining keeps the stopped merge" "git rev-parse MERGE_HEAD  # in w-ff2"
else
  report FAIL UNHAPPY "declining keeps the stopped merge" "git rev-parse MERGE_HEAD  # in w-ff2" \
    "MERGE_HEAD gone — the merge was aborted despite answering n"
fi
# Accepting redoes it from clean, and the source wins.
check "stuck+theirs accepted"        exit=0 err="theirs won conflicts" in=y -- "$FF2,$M3" merge theirs
if [ "$(cat "$ROOT/mrg/w-ff2/shared.txt")" = "C" ]; then
  report PASS HAPPY "redo let theirs win" "cat shared.txt  # in w-ff2"
else
  report FAIL HAPPY "redo let theirs win" "cat shared.txt  # in w-ff2" \
    "shared.txt is '$(cat "$ROOT/mrg/w-ff2/shared.txt")', want C"
fi

# List form sanity checks for the new grammar.
check "list form dry-run clean"      exit=0 err="merges into" -- "1,$A" merge dry-run
check "list form takes options"      exit=1 err="does NOT merge" -- "$C1,$M3" merge dry-run
check "list form rejects 3"          exit=1 err="exactly two worktrees" -- "1,$A,$C1" merge
check "list form needs an action"    exit=1 err="a worktree list needs an action" -- "1,$A"
check "list form malformed"          exit=1 err="bad worktree list '1,'" -- "1," merge
check "list form + continue"         exit=1 err="takes no source" -- "1,$A" merge continue
check "list form + abort short"      exit=1 err="takes no source" -- "1,$A" merge -a
check "list form bad number"         exit=1 err="bad worktree list" -- "1,x" merge
# The real thing: worktree M's branch lands in worktree N, list-style.
check "list form merges M into N"    exit=0 err="Merged feat-a into" -- "$LM,$A" merge
if [ -f "$ROOT/mrg/w-lm/a.txt" ]; then
  report PASS HAPPY "list form moved the files" "test -f a.txt  # in w-lm"
else
  report FAIL HAPPY "list form moved the files" "test -f a.txt  # in w-lm" \
    "a.txt absent from w-lm after '$LM,$A merge'"
fi

# --squash stages the merge without committing it.
FF="$(cd "$MRG" && "$BIN" add ffbr --dirname w-ff >/dev/null 2>&1; midx ffbr)"
check "merge --squash stages only"   exit=0 err="Squashed feat-a into ffbr" -- "$FF,$A" merge --squash
if [ -n "$(git -C "$ROOT/mrg/w-ff" diff --cached --name-only)" ]; then
  report PASS HAPPY "--squash leaves changes staged" "git diff --cached  # in w-ff"
else
  report FAIL HAPPY "--squash leaves changes staged" "git diff --cached  # in w-ff" \
    "nothing staged after --squash"
fi

cd "$APP" || exit 1

# --- sync: fetch / pull / push ----------------------------------------------
# Self-contained clone with a real remote: these verbs move refs on both sides,
# which the shared fixture's origin is not there to survive.
#   main    tracks origin/main; a second clone pushes a commit for it to pull
#   feat-s  tracks origin/feat-s, up to date
#   lonely  no upstream at all   (the failure a sweep must survive)
#   detached                     (skipped: no branch to sync)
SR="$ROOT/syn/origin.git"; SA="$ROOT/syn/app"; mkdir -p "$ROOT/syn"
( git init -q --bare "$SR"
  git clone -q "$SR" "$SA" 2>/dev/null
  cd "$SA"
  git config user.email t@t; git config user.name t
  git checkout -q -b main 2>/dev/null
  echo s > s.txt; git add .; git commit -q -m init; git push -q -u origin main
  git branch feat-s; git branch lonely
  "$BIN" add feat-s --dirname w-feat-s >/dev/null 2>&1
  git -C "$ROOT/syn/w-feat-s" push -q -u origin feat-s
  # lonely is never pushed, so it has no upstream: pull/push fail on it.
  "$BIN" add lonely --dirname w-lonely >/dev/null 2>&1
  git worktree add -q --detach "$ROOT/syn/w-det" HEAD
  # A separate clone is the "someone else pushed" the sweep then pulls.
  git clone -q "$SR" "$ROOT/syn/other" 2>/dev/null
  cd "$ROOT/syn/other"
  git config user.email t@t; git config user.name t
  echo upstream >> s.txt; git commit -q -am "upstream work"; git push -q origin main )

cd "$SA" || exit 1
sidx() { "$BIN" list | awk -v b="$1" '$2==b{print $1}'; }
SF="$(sidx feat-s)"; SL="$(sidx lonely)"; SD="$(sidx '(detached)')"

# Grammar, before anything moves.
check "sync bare verb needs target"  exit=1 err="'pull' needs a worktree" -- pull
check "sync fetch bare verb"         exit=1 err="'fetch' needs a worktree" -- fetch
check "sync target + --all"          exit=1 err="'--all' is every worktree, so worktree #1" -- 1 pull --all
check "sync list + --all"            exit=1 err="'--all' is every worktree, so a worktree list" -- "1,$SF" push --all
check "sync list dup"                exit=1 err="worktree #1 listed twice" -- "1,1" fetch
check "sync unknown flag"            exit=1 err="unknown option '--depth=1' for pull" -- 1 pull --depth=1
check "sync flag names git for you"  exit=1 err="git -C <dir> pull --depth=1" -- 1 pull --depth=1
check "sync flags are per verb"      exit=1 err="unknown option '--rebase' for fetch" -- 1 fetch --rebase
check "sync push has no --rebase"    exit=1 err="unknown option '--rebase' for push" -- 1 push --rebase
check "sync pull has no -u"          exit=1 err="unknown option '-u' for pull" -- 1 pull -u
check "sync push --force refused"    exit=1 err="no '--force' for push" -- 1 push --force
check "sync push -f refused"         exit=1 err="--force-with-lease" -- 1 push -f
check "sync contradiction"           exit=1 err="'--rebase' and '--no-rebase' contradict" -- 1 pull --rebase --no-rebase
check "sync rebase vs ff-only"       exit=1 err="'--rebase' and '--ff-only' contradict" -- 1 pull --rebase --ff-only

# fetch works everywhere: it moves remote-tracking refs, so even a detached
# HEAD has something to do.
check "fetch one worktree"           exit=0 -- 1 fetch
check "fetch takes --prune"          exit=0 -- 1 fetch --prune
check "fetch list form"              exit=0 err="fetch: 2 ok, 0 failed, 0 skipped" -- "1,$SF" fetch
check "fetch --all sweeps"           exit=0 err="fetch: 4 ok, 0 failed, 0 skipped" -- fetch --all
check "fetch detached is not skipped" exit=0 err="fetch: 2 ok, 0 failed, 0 skipped" -- "1,$SD" fetch

# pull: main is behind by the other clone's commit, so this is a real update.
check "pull one worktree"            exit=0 err="Fast-forward" -- 1 pull
if [ "$(git -C "$SA" log --oneline -1 --format=%s)" = "upstream work" ]; then
  report PASS HAPPY "pull moved the branch" "git log -1  # in syn/app"
else
  report FAIL HAPPY "pull moved the branch" "git log -1  # in syn/app" \
    "HEAD is '$(git -C "$SA" log --oneline -1 --format=%s)', want 'upstream work'"
fi
# The sweep: 1 and feat-s pull, detached is skipped, lonely has no upstream and
# fails — and the ones after it still ran.
check "pull --all keeps going"       exit=1 err="pull: 2 ok, 1 failed, 1 skipped" -- pull --all
check "pull --all names the failure" exit=1 err="pull failed in 1: lonely" -- pull --all
check "pull --all skips detached"    exit=1 err="detached HEAD, no branch to sync" -- pull --all
check "pull single error is git's"   exit=1 err="no tracking information" -- "$SL" pull
check "pull takes --ff-only"         exit=0 -- 1 pull --ff-only

# push -u on a branch with no upstream: a bare 'git push -u' has none to read
# the remote off of, so git-wt names 'origin lonely' the way you would.
check "push -u sets the upstream"    exit=0 err="set up to track 'origin/lonely'" -- "$SL" push -u
if [ "$(git -C "$ROOT/syn/w-lonely" rev-parse --abbrev-ref '@{upstream}' 2>&1)" = "origin/lonely" ]; then
  report PASS HAPPY "push -u left an upstream" "git rev-parse --abbrev-ref @{u}  # in w-lonely"
else
  report FAIL HAPPY "push -u left an upstream" "git rev-parse --abbrev-ref @{u}  # in w-lonely" \
    "upstream is '$(git -C "$ROOT/syn/w-lonely" rev-parse --abbrev-ref '@{upstream}' 2>&1)'"
fi
check "push -u again is fine"        exit=0 -- "$SL" push -u
check "push --all sweeps"            exit=0 err="push: 3 ok, 0 failed, 1 skipped" -- push --all
check "push takes --dry-run"         exit=0 -- 1 push --dry-run

cd "$APP" || exit 1

# --- meta -------------------------------------------------------------------
check "version"                      exit=0 out="git-wt" -- version
check "--help"                       exit=0 out="USAGE" -- --help

echo "----------------------------------------------------------------------"
echo "Result: $pass passed, $fail failed"

if [ -n "$MD" ]; then
  {
    printf '\n## Summary\n\n'
    printf -- '- Passed: **%s**\n' "$pass"
    printf -- '- Failed: **%s**\n' "$fail"
    printf -- '- Total: **%s**\n' "$((pass+fail))"
  } >> "$MD"
  echo "Report: $MD"
fi

[ "$fail" = 0 ]
