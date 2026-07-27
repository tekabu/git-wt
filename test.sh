#!/usr/bin/env bash
# Live end-to-end test for git-wt. Auto-detects the host OS and runs natively;
# --docker/--shell/--build-install cross-test Linux from any host via Docker.
#
# Builds the binary, spins up a dummy repo under /tmp, drives every command in
# the verb-first grammar, and prints a PASS/FAIL report. Exits non-zero if any
# case fails. Cleans up the /tmp scratch dir on exit.
#
#   ./test.sh                  # native: release build, run here
#   ./test.sh --debug          # native: debug build (faster compile)
#   ./test.sh --md             # native: also write docs/test-report.md
#   ./test.sh --md out.md      # native: ...to out.md instead
#
#   ./test.sh --docker         # build a Debian image, run unit + live tests there
#   ./test.sh --shell          # build the image, drop into a shell
#   ./test.sh --build-install  # verify build.sh + install.sh on Linux, via Docker
#   ./test.sh --rebuild        # force image rebuild (no cache); combine with the above
set -u

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

os="$(uname -s)"
case "$os" in
  Darwin) os_name="macOS" ;;
  Linux)  os_name="Linux" ;;
  *)      os_name="$os" ;;
esac

profile="release"
MD=""
docker_mode=""
docker_build_args=()
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
    --docker)        docker_mode="test"; shift ;;
    --shell)         docker_mode="shell"; shift ;;
    --build-install) docker_mode="build-install"; shift ;;
    --rebuild)       docker_build_args+=(--no-cache); shift ;;
    *) echo "unknown option '$1'" >&2; exit 2 ;;
  esac
done

echo "Detected OS: $os_name"

if [ -n "$docker_mode" ]; then
  image="git-wt-test"
  command -v docker >/dev/null 2>&1 || {
    echo "error: docker not found on PATH" >&2; exit 1
  }
  [ "$os" = "Linux" ] && echo "note: already on Linux — Docker still gives an isolated, throwaway environment."

  echo "Building image '$image' (Linux)..."
  docker build ${docker_build_args[@]+"${docker_build_args[@]}"} -t "$image" "$here"

  if [ "$docker_mode" = "shell" ]; then
    echo "Dropping into container shell. Run: cargo test --release && ./test.sh"
    exec docker run --rm -it "$image" bash
  fi

  if [ "$docker_mode" = "build-install" ]; then
    echo "Verifying build.sh, one-file installer, and install.sh (source) on Linux..."
    exec docker run --rm -e SHELL=/bin/bash "$image" bash -euc '
      echo "=== build.sh: version + compile + one file ==="
      ./build.sh
      ls -1 dist/

      echo "=== one-file installer (no repo, no toolchain) ==="
      # Copy ONLY the self-installing script to an empty dir to prove isolation.
      inst="$(ls dist/git-wt-*.install.sh)"
      mkdir -p /tmp/only && cp "$inst" /tmp/only/
      cd /tmp/only && ./git-wt-*.install.sh --alias wt
      export PATH="$HOME/.local/bin:$PATH"
      echo "-- binary on PATH:"; command -v git-wt; git-wt version
      grep -q "# >>> git-wt alias >>>" "$HOME/.bashrc" || { echo "alias block missing" >&2; exit 1; }
      eval "$(sed -n "/# >>> git-wt alias >>>/,/# <<< git-wt alias <<</p" "$HOME/.bashrc")"
      mkdir -p /tmp/r/app && cd /tmp/r/app && git init -q && git commit -q --allow-empty -m i
      echo "-- wt list via alias:"; wt list

      echo "=== install.sh: from source (cargo) ==="
      cd /work && ./install.sh
      "${CARGO_HOME:-$HOME/.cargo}/bin/git-wt" version

      echo "OK: build.sh + one-file installer + source install verified"
    '
  fi

  echo "Running unit + live tests on Linux (Docker)..."
  exec docker run --rm "$image"
fi

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
WT="$CODE/myapp-worktrees"
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
git branch bulk-a
git branch bulk-b
git branch bulk-c
git branch bulk-d

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
check "list with no args shows main"  exit=0 out="myapp" -- list
check "list shows main"              exit=0 out="main" -- list
check "ls alias"                     exit=0 out="main" -- ls
check "list no-match errors"         exit=0 out="" -- list zzz

# --- add --------------------------------------------------------------------
# The created path is printed on stdout (so scripts can capture it).
check "add existing local branch"    exit=0 out="$WT/feature-login" -- add feature/login
# worktree now exists at index 2
check "list shows new worktree"      exit=0 out="feature/login" -- list
check "list filter keeps index"      exit=0 out="2  feature/login" -- list logi
check "list --col branch only"       exit=0 out="feature/login" -- list --col 2 logi
check "list --col id+branch"         exit=0 out="2  feature/login" -- list --col 1,2 logi
check "list --col reorder"           exit=0 out="feature/login  2" -- list --col 2,1 logi
check "list --col bad number"        exit=1 err="no column 11" -- list --col 11
check "list --col non-numeric"       exit=1 err="bad column 'x'" -- list --col x
check "bare --col means list"        exit=0 out="main" -- list --col 2
check "bare -c short flag means list" exit=0 out="main" -- list -c 1,2
check "add --name suffix"            exit=0 out="$WT/review" -- add feature/logout --name review
check "add --dirname whole leaf"     exit=0 out="$WT/scratch2" -- add feature/api --dirname scratch2
check "add tracks remote-only"       exit=0 out="$WT/remote-only" err="Tracking remote branch 'origin/remote-only'" -- add remote-only
check "add --dirname as path"        exit=0 out="$WT/sub/deep" -- add pathtest --dirname sub/deep
check "add --from a ref (new branch)" exit=0 out="$WT/ff1" err="Creating new branch 'newfrom' from 'feature/login'" in=y -- add newfrom --from feature/login --dirname ff1
check "add dup dir refused"          exit=1 err="already exists" -- add feature/login
check "add name+dirname conflict"    exit=1 err="--name and --dirname conflict" -- add x -n a --dirname b
check "add --name empty"             exit=1 err="--name cannot be empty" -- add x -n ""
check "add --from needs ref"         exit=2 err="a value is required for '--from <FROM>'" -- add x --from
check "add new branch declined"      exit=0 err="Aborted." in=n -- add nope --dirname np1
check "add --stay accepted"          exit=0 out="$WT/stay1" -- add staybr --dirname stay1 --stay
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
ffhead="$(git -C "$WT/ff1" rev-parse HEAD)"
ffwant="$(git -C "$APP" rev-parse feature/login)"
if [ "$ffhead" = "$ffwant" ]; then
  report PASS HAPPY "add --from base commit matches ref" "git rev-parse HEAD  # in ff1"
else
  report FAIL HAPPY "add --from base commit matches ref" "git rev-parse HEAD  # in ff1" \
    "HEAD $ffhead != feature/login $ffwant"
fi

# --- switch / path ----------------------------------------------------------
check "switch N prints path"           exit=0 out="myapp" -- switch 1
check "path N prints path"             exit=0 out="$APP" -- path 1
check "switch N too many args"         exit=2 err="unexpected argument 'path' found" -- switch 1 path
check "index 0 errors"                 exit=1 err="no worktree #0" -- switch 0
check "index over range errors"        exit=1 err="there are" -- switch 99
check "extra arg after switch"         exit=2 err="unexpected argument 'bogus' found" -- switch 1 bogus
check "flag on bare switch"            exit=2 err="unexpected argument '-n' found" -- switch 1 -n x
check "long flag on bare switch"       exit=2 err="unexpected argument '--stat' found" -- switch 1 --stat

# --- legacy / unknown -------------------------------------------------------
# 'show' is retired; 'path' is the verb. The word now reads as a target list,
# so the *number* after it is the surprise clap reports.
check "retired show verb rejected"     exit=2 err="unexpected argument '1' found" -- show 1
check "legacy remove order rejected"   exit=1 err="unexpected argument for the verb" -- 1 remove
check "bare branch name rejected"      exit=1 err="no worktree named 'feat/x'" -- feat/x
check "typo verb rejected"             exit=1 err="no worktree named 'lsit'" -- lsit

# --- diff -------------------------------------------------------------------
# Give the two sides real, divergent commits: main-only 'onlymain.txt' vs
# login-only 'onlylogin.txt'. That split is what tells '..' from '...'.
didx="$("$BIN" list | awk '$2=="feature/login"{print $1}')"
( cd "$APP" && echo m > onlymain.txt && git add -A && git commit -qm mainside )
( cd "$WT/feature-login" && echo l > onlylogin.txt && git add -A && git commit -qm loginside )

check "diff --name-only shows adds"  exit=0 out="onlylogin.txt" -- diff "$didx" --name-only
# '..' is both directions: main's file shows as a deletion, login's as an add.
check "diff .. keeps main-only file" exit=0 out="onlymain.txt" -- diff "$didx" .. --name-only
# The default is '...' -- "since the fork" -- so main's own later commit drops
# out, and the listing matches what '1,N merge' would actually bring in. A '..'
# default would report main's commit as a deletion the merge never makes.
for spelling in "" "..."; do
  # shellcheck disable=SC2086 # empty spelling must vanish, not pass ''
  dots3="$("$BIN" diff "$didx" $spelling --name-only 2>/dev/null)"
  dcmd="$(fmt_cmd diff "$didx" $spelling --name-only)"
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
check "diff --stat"                  exit=0 out="1 +" -- diff "$didx" --stat
check "diff --name-status"           exit=0 out="A" -- diff "$didx" --name-status
# Exact output, not a substring: the unfiltered diff also *contains*
# 'onlylogin.txt', so a substring assertion here would pass even if the
# path were dropped on the floor. "Limits" means the other files are gone.
pspec="$("$BIN" diff "$didx" -p onlylogin.txt --name-only 2>/dev/null)"
pcmd="$(fmt_cmd diff "$didx" -p onlylogin.txt --name-only)"
if [ "$pspec" = "onlylogin.txt" ]; then
  report PASS HAPPY "diff -p limits to the path" "$pcmd"
else
  report FAIL HAPPY "diff -p limits to the path" "$pcmd" "wanted exactly 'onlylogin.txt', got '$pspec'"
fi
# git's trailing spelling is gone; it errors, with no hint offered.
# clap eats the literal '--' itself (its own end-of-options marker); the path
# after it lands as diff's one stray 'range' token, and cmd_diff recognizes it
# as a real file and gives the same '-p' hint a bare path gets.
check "diff -- is retired"           exit=1 err="paths go in '-p/--path'" -- diff "$didx" --name-only -- onlylogin.txt
check "bare path points at -p"       exit=1 err="paths go in '-p/--path'" -- diff "$didx" onlylogin.txt
# -A/-B keep the files one side has and the other lacks. Under the default
# '...' range main's side does not exist at all, so 'onlymain.txt' needs '..'
# to appear -- and there it is a delete, which is what -A keeps.
aonly="$("$BIN" diff "$didx" -A .. --name-only 2>/dev/null)"
acmd="$(fmt_cmd diff "$didx" -A .. --name-only)"
if [ "$aonly" = "onlymain.txt" ]; then
  report PASS HAPPY "diff -A keeps main-only file" "$acmd"
else
  report FAIL HAPPY "diff -A keeps main-only file" "$acmd" "wanted exactly 'onlymain.txt', got '$aonly'"
fi
bonly="$("$BIN" diff "$didx" -B .. --name-only 2>/dev/null)"
bcmd="$(fmt_cmd diff "$didx" -B .. --name-only)"
if [ "$bonly" = "onlylogin.txt" ]; then
  report PASS HAPPY "diff -B keeps login-only file" "$bcmd"
else
  report FAIL HAPPY "diff -B keeps login-only file" "$bcmd" "wanted exactly 'onlylogin.txt', got '$bonly'"
fi
check "diff -A names the side"       exit=0 out="only in main" -- diff "$didx" -A --hunks ..
check "diff -A -B cannot combine"    exit=1 err="cannot combine" -- diff "$didx" -A -B
check "diff --a-only long form"      exit=0 out="onlymain.txt" -- diff "$didx" --a-only .. --name-only
# Grammar mirrors merge's: the positional/-s is always the source, never the
# destination -- the destination is only -d/--destination, defaulting to the
# current worktree (worktree 1, main, throughout this section).
check "diff needs a source"          exit=1 err="diff needs a source: 'git-wt diff <SOURCE>' (or '-s <SOURCE>')" -- diff
check "diff self via default dest"   exit=1 err="worktree #1 against itself is always empty" -- diff 1
# The old 'N diff M' grammar: the trailing target is now junk in the action slot.
check "diff old form errors"         exit=2 err="unexpected argument 'diff' found" -- switch 1 diff "$didx"
check "diff non-numeric target"      exit=1 err="no worktree on branch 'x'" -- diff x
check "diff bad index errors"        exit=1 err="no worktree #99" -- diff 99
check "diff -d takes exactly one target" exit=1 err="diff's '-d/--destination' takes exactly one target" -- diff "$didx" -d "1,$didx"
check "diff -s takes exactly one source" exit=1 err="diff's '-s/--source' takes exactly one source branch" -- diff -s "1,$didx"
check "diff rejects a source list"   exit=1 err="diff takes one source, got the list '1,$didx'; the destination is '-d/--destination'" -- diff "1,$didx"
check "diff source given twice"      exit=1 err="source given twice" -- diff "$didx" -s "$didx"
# -w is a real git flag but not one of diff's own; clap rejects it directly
# now that diff has no catch-all tail to swallow it into.
check "diff rejects other git flags" exit=2 err="unexpected argument '-w' found" -- diff "$didx" -w

# Uncommitted work is invisible to a ref diff, so it must be called out.
echo scratch > "$WT/feature-login/uncommitted.txt"
check "diff warns on dirty worktree" exit=0 err="has uncommitted changes" -- diff "$didx" --name-only
rm -f "$WT/feature-login/uncommitted.txt"

# Not inside a worktree at all, with no -d given: the destination has nothing
# to default to. GIT_DIR points git-wt at the repo from a cwd ($ROOT, the
# scratch dir holding every worktree) that is outside every worktree's own
# path, so current_worktree_index() finds nothing to default to.
nowt_err="$(cd "$ROOT" && GIT_DIR="$APP/.git" "$BIN" diff "$didx" 2>&1 >/dev/null)"
nowt_cmd="$(fmt_cmd diff "$didx")  # cwd outside any worktree"
case "$nowt_err" in
  *"not inside a worktree; use 'git-wt diff <SOURCE> -d <DEST>'"*)
    report PASS UNHAPPY "diff outside a worktree needs -d" "$nowt_cmd" ;;
  *)
    report FAIL UNHAPPY "diff outside a worktree needs -d" "$nowt_cmd" "got '$nowt_err'" ;;
esac
nowt_out="$(cd "$ROOT" && GIT_DIR="$APP/.git" "$BIN" diff "$didx" -d 1 --name-only 2>/dev/null)"
nowt_okcmd="$(fmt_cmd diff "$didx" -d 1 --name-only)  # cwd outside any worktree"
if [ "$nowt_out" = "onlylogin.txt" ]; then
  report PASS HAPPY "diff -d from outside works" "$nowt_okcmd"
else
  report FAIL HAPPY "diff -d from outside works" "$nowt_okcmd" "wanted exactly 'onlylogin.txt', got '$nowt_out'"
fi

# --- commits ----------------------------------------------------------------
# Same divergence the diff cases built: 'mainside' on main, 'loginside' on
# feature/login, 'init' shared by both. That makes every cell predictable --
# one commit per column, and a shared one that must not appear at all.
# A single target is that worktree alone: its own log, no check columns and
# nothing to be ahead of. The worktree you are standing in is not pulled in.
check "commits single target"        exit=0 out="loginside" -- commits "$didx"
# No second branch means no mark columns, so the glyph legend goes with them.
solo="$("$BIN" commits "$didx" 2>/dev/null | grep -c "has commit")"
soloc="$(fmt_cmd commits "$didx")"
if [ "$solo" = 0 ]; then
  report PASS HAPPY "commits single drops the legend" "$soloc"
else
  report FAIL HAPPY "commits single drops the legend" "$soloc" "legend printed with no columns"
fi
check "commits single takes flags"   exit=0 out="loginside" -- commits "$didx" --author Test
# Naming the one you are standing in is a log like any other, not an error.
check "commits single self is a log" exit=0 out="mainside" -- commits 1

check "commits heads the columns"    exit=0 out="feature/login" -- commits "1,$didx"
check "commits lists its own side"   exit=0 out="mainside" -- commits "1,$didx"
# login's own commit is not a row by default: naming a worktree adds a column.
lonly="$("$BIN" commits "1,$didx" 2>/dev/null)"
lcmd="$(fmt_cmd commits "1,$didx")"
case "$lonly" in
  *loginside*) report FAIL HAPPY "commits anchors on the first" "$lcmd" "'loginside' is not main's: '$lonly'" ;;
  *)           report PASS HAPPY "commits anchors on the first" "$lcmd" ;;
esac
check "commits --union adds the rest" exit=0 out="loginside" -- commits "1,$didx" --union
check "commits heads the author col" exit=0 out="author" -- commits "1,$didx"
check "commits names the author"     exit=0 out="Test" -- commits "1,$didx"
check "commits heads the subject col" exit=0 out="subject" -- commits "1,$didx"
# ISO by default: what the table prints is what --date-since takes.
check "commits dates the rows"       exit=0 out="$(date +%F)" -- commits "1,$didx"
check "commits --date-human"         exit=0 out=", $(date +%Y)" -- commits "1,$didx" --date-human
# 24-hour h:m:s, appended to whichever day spelling is in play.
check "commits --time"          exit=0 out="$(date +%F) " -- commits "1,$didx" --time
check "commits --time is 24h"   exit=0 out=":" -- commits "1,$didx" --time
check "commits human + time"         exit=0 out=", $(date +%Y) " -- commits "1,$didx" --date-human --time
# A date read off the table pastes back into a filter unchanged.
check "commits ISO round-trips"      exit=0 out="mainside" -- commits "1,$didx" --date-since "$(date +%F)"

# The default is a merge-request slice: only branch 1's commits the others
# miss. 'mainside' is main's alone, so it shows; the shared 'init' is cut.
check "commits default shows slice"  exit=0 out="mainside" -- commits "1,$didx"
# --all brings back the whole first-branch log, so the shared root reappears.
check "commits --all shows full log" exit=0 out="init" -- commits "1,$didx" --all
# --all and --union are different row sources and cannot combine.
check "commits --all vs --union"     exit=1 err="two different row sources" -- commits "1,$didx" --all --union
# Short flags, alone and bundled under one dash.
check "commits -a aliases --all"     exit=0 out="init" -- commits "1,$didx" -a
check "commits -f aliases --files"   exit=0 out="A  onlymain.txt" -- commits "1,$didx" -a -f
check "commits -af bundles both"     exit=0 out="A  onlymain.txt" -- commits "1,$didx" -af
check "commits -fa order-free"       exit=0 out="A  onlymain.txt" -- commits "1,$didx" -fa
check "commits -fn takes a value"    exit=0 out="init" -- commits "1,$didx" -afn 20
# A value-taking flag mid-bundle is refused: '-nf' reads as '-n f', so 'f'
# lands as --limit's value and fails there, with the 20 left over. The
# assertion is that it is rejected, not which half is blamed.
check "commits -nf refused"          exit=2 err="bad count 'f'" -- commits "1,$didx" -nf 20
# A bundle of letters that name nothing: the first unknown short is the one
# reported.
check "commits -zy names the first"  exit=2 err="unexpected argument '-z' found" -- commits "1,$didx" -zy
# The default really drops the shared root -- not merely 'not asserted'.
droot="$("$BIN" commits "1,$didx" 2>/dev/null | grep -cw init || true)"
dcmd2="$(fmt_cmd commits "1,$didx")"
if [ "$droot" = 0 ]; then
  report PASS HAPPY "commits default drops shared root" "$dcmd2  # no init row"
else
  report FAIL HAPPY "commits default drops shared root" "$dcmd2" "init present in default range"
fi

# A row is checked only where the branch really has the commit. The subject is
# the last field now, so login's mark -- the last column -- is the one before
# it: 'mainside' is main's alone.
mrow="$("$BIN" commits "1,$didx" 2>/dev/null | awk '/mainside/{print $(NF-1)}')"
mcmd="$(fmt_cmd commits "1,$didx")"
if [ "$mrow" = "·" ]; then
  report PASS HAPPY "commits leaves foreign cell" "$mcmd  # mainside unchecked on login"
else
  report FAIL HAPPY "commits leaves foreign cell" "$mcmd" "wanted '·' in login's column, got '$mrow'"
fi

# Three columns: what diff cannot do, and the reason the command exists.
oidx="$("$BIN" list | awk '$2=="feature/logout"{print $1}')"
check "commits takes three worktrees" exit=0 out="feature/logout" -- commits "1,$didx,$oidx"

# --topo reorders; it never drops or invents rows. Same set, same count.
check "commits --topo keeps the rows" exit=0 out="mainside" -- commits "1,$didx" --topo
check "commits --topo-order spelling" exit=0 out="loginside" -- commits "1,$didx" --union --topo-order
dcount="$("$BIN" commits "1,$didx" --union 2>/dev/null | grep -cE 'mainside|loginside')"
tcount="$("$BIN" commits "1,$didx" --union --topo 2>/dev/null | grep -cE 'mainside|loginside')"
tcmd="$(fmt_cmd commits "1,$didx" --union --topo)"
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
  ncnt="$("$BIN" commits "1,$didx" $spelling 2>/dev/null | grep -cE 'mainside|loginside')"
  ncmd="$(fmt_cmd commits "1,$didx" $spelling)"
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
erow="$("$BIN" commits "1,$didx" 2>/dev/null | awk '/emojisubject/{print $(NF-2)"|"$(NF-1)}')"
ecmd="$(fmt_cmd commits "1,$didx")"
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

# --date is one exact day. The bounds are --date-since/--date-until.
check "commits --date exact day"      exit=0 out="mainside" -- commits "1,$didx" --date "$today"
check "commits --date other day"      exit=0 err="no commits match those filters" -- commits "1,$didx" --date "$tomorrow"
check "commits --date-since today"     exit=0 out="mainside" -- commits "1,$didx" --date-since "$today"
check "commits --date-until today"       exit=0 out="mainside" -- commits "1,$didx" --date-until "$today"
# Two bounds are an AND, which is how a range is spelled.
check "commits date range brackets"   exit=0 out="mainside" -- commits "1,$didx" --date-since "$yesterday" --date-until "$tomorrow"
# A bound that keeps nothing reports the filter, not an empty history.
check "commits --date-since tomorrow"  exit=0 err="no commits match those filters" -- commits "1,$didx" --date-since "$tomorrow"
check "commits --date-until yesterday"   exit=0 err="no commits match those filters" -- commits "1,$didx" --date-until "$yesterday"

# No operators in --date at all: each one names a bound that has its own flag,
# and the error says which -- with the day carried into the hint.
#
# The wording is ours (parse_date_filter), but it reaches the user inside clap's
# "invalid value 'X' for '--date <DATE>': ..." wrapper, and a value_parser
# rejection is a clap usage error -- exit 2, not the 1 a run-time failure gets.
check "commits --date rejects >="     exit=2 err="no '>' in --date" -- commits "1,$didx" --date ">=$today"
check "commits --date rejects <="     exit=2 err="no '<' in --date" -- commits "1,$didx" --date "<=$today"
check "commits --date rejects ="      exit=2 err="no '=' in --date" -- commits "1,$didx" --date "=$today"
check "commits --date bad shape"      exit=2 err="want YYYY-MM-DD" -- commits "1,$didx" --date "2026-1-1"
check "commits --date impossible"     exit=2 err="no such date" -- commits "1,$didx" --date "2026-13-01"
# A flag left without its value is clap's own message: one generic wording for
# every flag, rather than a bespoke closure per flag.
check "commits --date needs a value"  exit=2 err="a value is required for '--date <DATE>'" -- commits "1,$didx" --date
# An unquoted '>' is eaten by the shell, so the value arrives bare: say why.
check "commits --date eaten by shell" exit=2 err="no '>' in --date" -- commits "1,$didx" --date ">="

# A commit or a date filter widens the source to the full log by itself: it
# names something in the history, not something in the default slice. 'init' is
# the shared root, which the default slice always cuts.
rootsha="$(cd "$APP" && git rev-list --max-parents=0 --abbrev-commit HEAD)"
check "--commits implies --all"       exit=0 out="init" -- commits "1,$didx" --commits "$rootsha"
check "--date implies --all"          exit=0 out="init" -- commits "1,$didx" --date "$today"
check "--date-since implies --all"    exit=0 out="init" -- commits "1,$didx" --date-since "$yesterday"
# An upper bound only trims the top, which the slice already ends at, so it
# stays a post-filter over the default rows: 'init' is still cut.
uonly="$("$BIN" commits "1,$didx" --date-until "$tomorrow" 2>/dev/null | grep -cw init || true)"
ucmd="$(fmt_cmd commits "1,$didx" --date-until "$tomorrow")"
if [ "$uonly" = 0 ]; then
  report PASS HAPPY "--date-until keeps the slice" "$ucmd  # no init row"
else
  report FAIL HAPPY "--date-until keeps the slice" "$ucmd" "init leaked: an upper bound widened the source"
fi
check "--date-until --all widens"     exit=0 out="init" -- commits "1,$didx" --date-until "$tomorrow" --all
# ...and a range still widens, because its lower bound does.
check "a date range implies --all"    exit=0 out="init" -- commits "1,$didx" --date-since "$yesterday" --date-until "$tomorrow"
# A filter that kept nothing over the slice says the slice is what it read.
check "empty filter names the slice"  exit=0 err="only the rows ahead of the other branches" -- commits "1,$didx" --date-until "$yesterday"
# --commit-until is the same kind of post-filter. It needs history spread over
# more than one day, so it gets a self-contained repo: 'old' is below the
# slice's floor, 'new' is the slice.
CUR="$ROOT/cu/app"; mkdir -p "$CUR"
( cd "$CUR"
  git init -q; git checkout -q -b main
  git config user.email t@t; git config user.name t
  export GIT_AUTHOR_DATE="2026-01-01T12:00:00+0000"
  export GIT_COMMITTER_DATE="$GIT_AUTHOR_DATE"
  git commit -q --allow-empty -m old-shared
  git branch cuside
  export GIT_AUTHOR_DATE="2026-06-01T12:00:00+0000"
  export GIT_COMMITTER_DATE="$GIT_AUTHOR_DATE"
  git commit -q --allow-empty -m new-on-main )
( cd "$CUR" && printf 'y\n' | "$BIN" add cuside --dirname cuwt >/dev/null 2>&1 )
oldsha="$(cd "$CUR" && git rev-list --max-parents=0 --abbrev-commit HEAD)"
# The default slice is 'new-on-main' alone, so a bound at 'old' keeps nothing.
uc="$( cd "$CUR" && "$BIN" commits 1,2 --commit-until "$oldsha" 2>&1 )"
uccmd="$(fmt_cmd commits 1,2 --commit-until "$oldsha")"
case "$uc" in
  *"none kept"*) report PASS HAPPY "--commit-until keeps nothing" "$uccmd" ;;
  *) report FAIL HAPPY "--commit-until keeps nothing" "$uccmd" "got '$uc'" ;;
esac
# ...and with --all it reaches the older commit it named.
ucall="$( cd "$CUR" && "$BIN" commits 1,2 --commit-until "$oldsha" --all 2>&1 )"
ucacmd="$(fmt_cmd commits 1,2 --commit-until "$oldsha" --all)"
case "$ucall" in
  *old-shared*) report PASS HAPPY "--commit-until --all reaches back" "$ucacmd" ;;
  *) report FAIL HAPPY "--commit-until --all reaches back" "$ucacmd" "got '$ucall'" ;;
esac
# --author does not: it matches many commits and named none of them.
aonly="$("$BIN" commits "1,$didx" --author Test 2>/dev/null | grep -cw init || true)"
acmd="$(fmt_cmd commits "1,$didx" --author Test)"
if [ "$aonly" = 0 ]; then
  report PASS HAPPY "--author keeps the default slice" "$acmd  # no init row"
else
  report FAIL HAPPY "--author keeps the default slice" "$acmd" "init leaked: --author widened the source"
fi
check "--author --all is the full log" exit=0 out="init" -- commits "1,$didx" --author Test --all
# Neither text filter widens either, for the same reason: they match many
# commits and named none of them.
monly="$("$BIN" commits "1,$didx" --message init 2>/dev/null | grep -cw init || true)"
mcmd="$(fmt_cmd commits "1,$didx" --message init)"
if [ "$monly" = 0 ]; then
  report PASS HAPPY "--message keeps the default slice" "$mcmd  # no init row"
else
  report FAIL HAPPY "--message keeps the default slice" "$mcmd" "init leaked: --message widened the source"
fi
check "--message --all is the full log" exit=0 out="init" -- commits "1,$didx" --message init --all
# A --union the user typed still wins over the implied --all.
check "--union survives a date filter" exit=0 out="loginside" -- commits "1,$didx" --union --date "$today"

# Highlighting: a filtered table is all matches, so the color says WHERE the
# answer lives. CLICOLOR_FORCE makes the pipe emit ANSI the way a terminal does.
# The highlight is bold amber (1;38;5;214); 2 = dim, the unlit default.
hl() { ( cd "$APP" && CLICOLOR_FORCE=1 "$BIN" "$@" 2>/dev/null ); }
hlsha="$(cd "$APP" && git rev-parse --short HEAD)"
hlcheck() {
  local name="$1" want="$2" cmd="$3"; shift 3
  local out; out="$(hl "$@")"
  if printf '%s' "$out" | grep -q "$want"; then
    report PASS HAPPY "$name" "$cmd"
  else
    report FAIL HAPPY "$name" "$cmd" "no match for '$want'"
  fi
}
# The named commit's sha cell is lit.
hlcheck "--commits lights its sha" "$(printf '\033')\[1;38;5;214m$hlsha" \
  "$(fmt_cmd commits '1,$didx' --commits "$hlsha")" commits "1,$didx" --commits "$hlsha"
# A date filter lights the date column it read.
hlcheck "--date lights the date" "$(printf '\033')\[1;38;5;214m$today" \
  "$(fmt_cmd commits '1,$didx' --date "$today")" commits "1,$didx" --date "$today"
# --author lights the author column instead.
hlcheck "--author lights the author" "$(printf '\033')\[1;38;5;214mTest" \
  "$(fmt_cmd commits '1,$didx' --author Test)" commits "1,$didx" --author Test
# A commit bound is a date bound underneath, but the flag named a COMMIT: the
# sha lights and the date column stays dim, or most of the table would be lit.
cbhl="$(hl commits "1,$didx" --commit-until "$hlsha")"
cbcmd="$(fmt_cmd commits "1,$didx" --commit-until "$hlsha")"
if printf '%s' "$cbhl" | grep -q "$(printf '\033')\[1;38;5;214m$today"; then
  report FAIL HAPPY "--commit-until leaves the date dim" "$cbcmd" "the date column was lit by a commit flag"
else
  report PASS HAPPY "--commit-until leaves the date dim" "$cbcmd"
fi
hlcheck "--commit-until lights its sha" "$(printf '\033')\[1;38;5;214m$hlsha" \
  "$cbcmd" commits "1,$didx" --commit-until "$hlsha"

# The text filters go further than a cell: they light the matched CHARACTERS,
# since a subject is a sentence and the term is a few letters of it.
hlcheck "--message lights the matched word" "$(printf '\033')\[1;38;5;214mmainside" \
  "$(fmt_cmd commits '1,$didx' --message mainside)" commits "1,$didx" --message mainside
# A path match is lit inside the file block, which stays dim around it.
hlcheck "--path lights the path" "$(printf '\033')\[1;38;5;214monlymain.txt" \
  "$(fmt_cmd commits '1,$didx' --path onlymain.txt --all)" \
  commits "1,$didx" --path onlymain.txt --all

# Nothing is lit without a filter: the table is not an answer to anything.
plainhl="$(hl commits "1,$didx")"
phcmd="$(fmt_cmd commits "1,$didx")"
if printf '%s' "$plainhl" | grep -q "$(printf '\033')\[1;38;5;214m"; then
  report FAIL HAPPY "no filter lights nothing" "$phcmd" "a cell was highlighted with no filter"
else
  report PASS HAPPY "no filter lights nothing" "$phcmd"
fi

# --commit-since/--commit-until include the commit they name -- the point of the flags.
mainsha="$(cd "$APP" && git rev-parse --short HEAD)"
check "commits --commit-since keeps its own" exit=0 out="$mainsha" -- commits "1,$didx" --commit-since "$mainsha"
check "commits --commit-until keeps its own"   exit=0 out="$mainsha" -- commits "1,$didx" --commit-until "$mainsha"
# ...and the bound is that commit's DATE, not its ancestry: 'loginside' is on a
# branch main cannot reach, and it stays, because it was authored the same day.
# That is the whole difference from the --to-id these flags replaced.
toid="$("$BIN" commits "1,$didx" --union --commit-until "$mainsha" 2>/dev/null)"
tcmd="$(fmt_cmd commits "1,$didx" --union --commit-until "$mainsha")"
case "$toid" in
  *loginside*) report PASS HAPPY "commit bound is a date, not ancestry" "$tcmd" ;;
  *)           report FAIL HAPPY "commit bound is a date, not ancestry" "$tcmd" "loginside dropped; the bound read as ancestry: '$toid'" ;;
esac
check "commits --commit-since bad commit"  exit=1 err="--commit-since: no commit 'zzz9'" -- commits "1,$didx" --commit-since zzz9
check "commits --commit-until needs a value" exit=2 err="a value is required for '--commit-until <COMMIT>'" -- commits "1,$didx" --commit-until
# --commits names the rows outright, and resolves every id before filtering.
check "commits --commits one sha"     exit=0 out="$mainsha" -- commits "1,$didx" --commits "$mainsha"
check "commits -i short flag"         exit=0 out="$mainsha" -- commits "1,$didx" -i "$mainsha"
check "commits --commits bundled -ai" exit=0 out="$mainsha" -- commits "1,$didx" -ai "$mainsha"
check "commits --commits bad sha"     exit=1 err="--commits: no commit 'zzz9'" -- commits "1,$didx" --commits zzz9
# clap's value_delimiter splits the list, so an empty part is never seen as a
# malformed list -- each part is just resolved in turn, and the first one that
# is not a commit is the error.
check "commits --commits empty id"    exit=1 err="--commits: no commit 'a'" -- commits "1,$didx" --commits "a,,b"
# One named commit is one row, whatever else the range holds.
conly="$("$BIN" commits "1,$didx" --all --commits "$mainsha" 2>/dev/null | grep -c "^$mainsha" || true)"
call="$("$BIN" commits "1,$didx" --all 2>/dev/null | grep -cE "^[0-9a-f]{7}" || true)"
ccmd="$(fmt_cmd commits "1,$didx" --all --commits "$mainsha")"
if [ "$conly" = 1 ] && [ "$call" -gt 1 ]; then
  report PASS HAPPY "commits --commits keeps only those" "$ccmd  # 1 of $call rows"
else
  report FAIL HAPPY "commits --commits keeps only those" "$ccmd" "wanted 1 row of $call, got $conly"
fi

# Merges are dropped by default and --merges puts them back. Needs a merge with
# something to merge: a branch main already contains is "Already up to date"
# and --no-ff writes no commit at all.
( cd "$APP" \
  && git checkout -q -b nm-src \
  && git commit -q --allow-empty -m "nm-work" \
  && git checkout -q main \
  && git merge --no-ff -q -m "merge-for-nomerges" nm-src )
check "commits --merges shows them"  exit=0 out="merge-for-nomerges" -- commits "1,$didx" --merges
check "commits drops merges"         exit=0 err="" -- commits "1,$didx"
nm="$("$BIN" commits "1,$didx" 2>/dev/null | grep -c "merge-for-nomerges")"
nmc="$(fmt_cmd commits "1,$didx")"
if [ "$nm" = 0 ]; then
  report PASS HAPPY "commits hides merges by default" "$nmc"
else
  report FAIL HAPPY "commits hides merges by default" "$nmc" "merge row survived the default"
fi
# The work the merge joined must survive: only the merge row goes.
check "commits default keeps work"   exit=0 out="mainside" -- commits "1,$didx"

# --reverse flips the display, and only the display: '-n 2 --reverse' is the
# same two commits as '-n 2', read bottom-up -- not the two oldest.
# Match the sha rows rather than counting header lines off the top: the table
# is preceded by both a legend and a column header, and a row is the only line
# that starts with a short sha.
fwd="$("$BIN" commits "1,$didx" -n 2 2>/dev/null | awk '$1 ~ /^[0-9a-f]{7,}$/{print $1}')"
rev="$("$BIN" commits "1,$didx" -n 2 --reverse 2>/dev/null | awk '$1 ~ /^[0-9a-f]{7,}$/{print $1}')"
rcmd="$(fmt_cmd commits "1,$didx" -n 2 --reverse)"
want="$(printf '%s\n' "$fwd" | tail -r 2>/dev/null || printf '%s\n' "$fwd" | tac)"
if [ "$rev" = "$want" ]; then
  report PASS HAPPY "commits --reverse flips rows" "$rcmd"
else
  report FAIL HAPPY "commits --reverse flips rows" "$rcmd" "wanted '$want', got '$rev'"
fi
check "commits --oldest-first alias"  exit=0 out="mainside" -- commits "1,$didx" --oldest-first

# --md writes a file and prints nothing on stdout: the table is the file.
mdout="$ROOT/table.md"
check "commits --md names the file"  exit=0 err="Wrote $mdout" -- commits "1,$didx" --md "$mdout"
check "commits --md=PATH spelling"   exit=0 err="Wrote $ROOT/eq.md" -- commits "1,$didx" --md="$ROOT/eq.md"
mdcmd="$(fmt_cmd commits "1,$didx" --md "$mdout")"
if [ -f "$mdout" ] && grep -q '^| commit | author | date |' "$mdout" && grep -q 'mainside' "$mdout"; then
  report PASS HAPPY "commits --md writes a table" "$mdcmd"
else
  report FAIL HAPPY "commits --md writes a table" "$mdcmd" "no markdown table in $mdout"
fi
# A '|' in a subject must not become a column break.
( cd "$APP" && git commit -q --allow-empty -m "md: a|piped|subject" )
"$BIN" commits "1,$didx" --md "$ROOT/pipes.md">/dev/null 2>&1
pcmd="$(fmt_cmd commits "1,$didx" --md "$ROOT/pipes.md")"
if grep -q 'a\\|piped\\|subject' "$ROOT/pipes.md"; then
  report PASS HAPPY "commits --md escapes pipes" "$pcmd"
else
  report FAIL HAPPY "commits --md escapes pipes" "$pcmd" "pipe left unescaped: $(grep piped "$ROOT/pipes.md")"
fi
# The path is optional: a flag after --md is a flag, not a filename. The
# default name lands in the cwd, so this has to run somewhere inside the repo.
( cd "$APP" && "$BIN" commits "1,$didx" --md --topo>/dev/null 2>&1 )
dcmd="$(fmt_cmd commits "1,$didx" --md --topo)"
if ls "$APP"/commits_*.md >/dev/null 2>&1; then
  report PASS HAPPY "commits --md default name" "$dcmd  # commits_<stamp>.md"
else
  report FAIL HAPPY "commits --md default name" "$dcmd" "no commits_<stamp>.md written"
fi
rm -f "$APP"/commits_*.md
check "commits --md bad dir errors"  exit=1 err="cannot write" -- commits "1,$didx" --md /nope/nope/x.md

# A cherry-picked patch is neither present nor missing: '≈', not '·'. main
# already has work of its own, so the pick lands on a different parent and is
# a real copy -- picked onto the same parent, every input matches and git
# reproduces the original's sha instead.
( cd "$WT/feature-login" && echo picked > picked.txt && git add -A && git commit -q -m "cherrypicked-work" )
psha="$(cd "$WT/feature-login" && git rev-parse HEAD)"
( cd "$APP" && git cherry-pick "$psha" >/dev/null 2>&1 )
# LC_ALL=C, or sort collates '✓' and '≈' as equal -- they are symbols, which a
# UTF-8 locale ignores when comparing -- and -u folds the two rows into one.
# --union, or login's original is not a row and only main's copy can be seen.
crow="$("$BIN" commits "1,$didx" --union 2>/dev/null | awk '/cherrypicked-work/{print $(NF-2)"|"$(NF-1)}' | LC_ALL=C sort -u | tr '\n' ' ')"
ccmd="$(fmt_cmd commits "1,$didx" --union)"
# Two rows now carry that patch: main's copy (✓ ≈) and login's original (≈ ✓).
if [ "$crow" = "≈|✓ ✓|≈ " ]; then
  report PASS HAPPY "commits marks a cherry-pick" "$ccmd  # ≈ = same patch, other sha"
else
  report FAIL HAPPY "commits marks a cherry-pick" "$ccmd" "wanted '≈|✓ ✓|≈ ', got '$crow'"
fi
check "commits --no-cherry drops ≈"   exit=0 err="" -- commits "1,$didx" --union --no-cherry
nc="$("$BIN" commits "1,$didx" --union --no-cherry 2>/dev/null | grep -c "≈")"
nccmd="$(fmt_cmd commits "1,$didx" --union --no-cherry)"
if [ "$nc" = 0 ]; then
  report PASS HAPPY "commits --no-cherry is plain" "$nccmd  # no ≈ without the walk"
else
  report FAIL HAPPY "commits --no-cherry is plain" "$nccmd" "$nc rows still marked ≈"
fi
# '≈' must mean something: work nobody picked still reads as absent.
lrow="$("$BIN" commits "1,$didx" --union 2>/dev/null | awk '/loginside/{print $(NF-1)}')"
lcmd="$(fmt_cmd commits "1,$didx" --union)"
if [ "$lrow" = "✓" ]; then
  report PASS HAPPY "commits leaves unpicked work" "$lcmd  # loginside: not on main, not ≈"
else
  report FAIL HAPPY "commits leaves unpicked work" "$lcmd" "wanted login's '✓', got '$lrow'"
fi

# --author is the same fuzzy subsequence 'list' filters with.
check "commits --author exact"        exit=0 out="mainside" -- commits "1,$didx" --author Test
check "commits --author fuzzy"        exit=0 out="mainside" -- commits "1,$didx" --author tst
check "commits --author case-folds"   exit=0 out="mainside" -- commits "1,$didx" --author TEST
check "commits --author no match"     exit=0 err="no commits match those filters" -- commits "1,$didx" --author zzzz
check "commits --author needs a name" exit=2 err="a value is required for '--author <NAME>'" -- commits "1,$didx" --author

# --message is a substring over the subject and the body; --path is a
# substring over the paths a commit touched.
check "commits --message subject"     exit=0 out="mainside" -- commits "1,$didx" --message mainside
check "commits --message case-folds"  exit=0 out="mainside" -- commits "1,$didx" --message MAINSIDE
check "commits -m short form"         exit=0 out="mainside" -- commits "1,$didx" -m mainside
check "commits --message no match"    exit=0 err="no commits match those filters" -- commits "1,$didx" --message zzzz
check "commits --message needs a term" exit=2 err="a value is required for '--message <MESSAGE>'" -- commits "1,$didx" --message
# An *empty* term is a different case from a missing one, and nothing rejects
# it: clap is satisfied by the empty string, and an empty substring matches
# every subject, so the filter is a no-op rather than an error. Asserted as it
# behaves; if it should be refused, that is a src change, not a test one.
check "commits --message empty is a no-op" exit=0 out="mainside" -- commits "1,$didx" --message ""
check "commits --path needs a term" exit=2 err="a value is required for '--path <PATH_LIST>" -- commits "1,$didx" --path
# The block is cut to the matched paths by default; --all-files widens it back.
#
# Two files in one commit, only one of them matching the term: without the
# second file both modes print the same block, and every assertion below would
# hold just as well if the flag did the opposite of what it says.
( cd "$APP" \
  && echo a > blockmatch.txt && echo b > blockother.txt \
  && git add -A && git commit -qm twofileblock )

# Default: the matched path is kept and the other one is cut.
check "commits --path trims block" exit=0 out="blockmatch.txt" -- commits "1,$didx" --path blockmatch --all
trim="$("$BIN" commits "1,$didx" --path blockmatch --all 2>/dev/null | grep -c "blockother.txt")"
tcmd="$(fmt_cmd commits "1,$didx" --path blockmatch --all)"
if [ "$trim" = 0 ]; then
  report PASS HAPPY "commits --path cuts the rest" "$tcmd"
else
  report FAIL HAPPY "commits --path cuts the rest" "$tcmd" "unmatched file survived the trim"
fi
# --all-files widens: the unmatched file comes back. This is the assertion the
# one-file fixture could not make -- it fails if the flag is inverted.
check "commits --all-files widens"     exit=0 out="blockother.txt" -- commits "1,$didx" --path blockmatch --all --all-files
check "commits --all-files keeps match" exit=0 out="blockmatch.txt" -- commits "1,$didx" --path blockmatch --all --all-files
check "commits --all-files alone errors" exit=1 err="--all-files needs" -- commits "1,$didx" --all-files

check "commits rejects a dup target" exit=1 err="listed twice" -- commits "1,1"
check "commits bad index errors"     exit=1 err="no worktree #99" -- commits "1,99"
check "commits -n needs a count"     exit=2 err="a value is required for '--limit <LIMIT>'" -- commits "1,$didx" -n
# Same as --date: our wording, clap's wrapper, clap's exit 2.
check "commits -n 0 errors"          exit=2 err="would show nothing" -- commits "1,$didx" -n 0
check "commits -n non-numeric"       exit=2 err="bad count 'x'" -- commits "1,$didx" -n x
check "bare commits uses current"    exit=0 out="mainside" -- commits

# --- diff live --------------------------------------------------------------
# The case no ref diff can answer: put BOTH worktrees on the same commit, then
# change one on disk only. 'git diff <a>..<b>' is provably empty here -- both
# refs resolve to the same tree -- so any output at all proves live read disk.
LIVE="$WT/myapp-live"
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
refout="$("$BIN" diff "$lidx" --name-only 2>/dev/null)"
rcmd="$(fmt_cmd diff "$lidx" --name-only)"
if [ -z "$refout" ]; then
  report PASS HAPPY "ref diff blind on same commit" "$rcmd  # empty, as designed"
else
  report FAIL HAPPY "ref diff blind on same commit" "$rcmd" "wanted empty, got '$refout'"
fi

check "live sees uncommitted edit"   exit=0 out="shared.txt" -- diff "$lidx" --live --name-only
check "live sees untracked as add"   exit=0 out="A	untracked.txt" -- diff "$lidx" --live --name-status
check "live counts hunks"            exit=0 out="+2" -- diff "$lidx" --live
check "live summary counts lines"    exit=0 out="2 files changed, 3 insertions(+), 1 deletion(-)" -- diff "$lidx" --live
check "live hunks show line numbers" exit=0 out="modified 1" -- diff "$lidx" --live --hunks
check "live --stat still works"      exit=0 out="3 ++-" -- diff "$lidx" --live --stat
check "live suppresses dirty warn"   exit=0 err="" -- diff "$lidx" --live --name-only

# The pairing this whole flag exists for: literal on-disk files, one side only.
# 'untracked.txt' has no commit anywhere, so no ref diff can report it.
lbonly="$("$BIN" diff "$lidx" --live -B --name-only 2>/dev/null)"
lbcmd="$(fmt_cmd diff "$lidx" --live -B --name-only)"
if [ "$lbonly" = "untracked.txt" ]; then
  report PASS HAPPY "live -B keeps side-only add" "$lbcmd"
else
  report FAIL HAPPY "live -B keeps side-only add" "$lbcmd" "wanted exactly 'untracked.txt', got '$lbonly'"
fi
# Nothing is missing from the live side, and that answer belongs on stderr so
# a pipe sees an empty list rather than prose.
check "live -A empty says so"        exit=0 out="" err="no files only in" -- diff "$lidx" --live -A --name-only

# .gitignore is the reason live can't just be 'diff -rq': without it, build
# output drowns the signal.
ligr="$("$BIN" diff "$lidx" --live --name-only 2>/dev/null)"
gcmd="$(fmt_cmd diff "$lidx" --live --name-only)"
case "$ligr" in
  *ignoreme*) report FAIL HAPPY "live honors .gitignore" "$gcmd" "ignored file listed: '$ligr'" ;;
  *)          report PASS HAPPY "live honors .gitignore" "$gcmd" ;;
esac

lspec="$("$BIN" diff "$lidx" --live -p shared.txt --name-only 2>/dev/null)"
lpcmd="$(fmt_cmd diff "$lidx" --live -p shared.txt --name-only)"
if [ "$lspec" = "shared.txt" ]; then
  report PASS HAPPY "live -p limits to the path" "$lpcmd"
else
  report FAIL HAPPY "live -p limits to the path" "$lpcmd" "wanted exactly 'shared.txt', got '$lspec'"
fi

# A deleted-on-disk file is a delete, and its hunk must not read as '+0'.
rm "$LIVE/shared.txt"
check "live reports on-disk delete"  exit=0 out="D	shared.txt" -- diff "$lidx" --live --name-status
check "live delete is not an add"    exit=0 out="deleted 3" -- diff "$lidx" --live --hunks
printf 'one\nTWO\nthree\nfour\n' > "$LIVE/shared.txt"

# Identical contents: stdout stays empty so a pipe sees nothing, and the note
# that this is a real answer (not the empty-ref-diff bug) goes to stderr.
check "live identical is empty out"  exit=0 out="" err="no differences" -- diff "$lidx" --live -p .gitignore

check "live identical says so once"  exit=0 err="no differences" -- diff "$lidx" --live -p .gitignore --name-only
# -w is not one of diff's own flags; clap rejects it directly.
check "live bad flag rejected"       exit=2 err="unexpected argument '-w' found" -- diff "$lidx" --live -w
# 'live' and 'hunks' are plain words that name no file, so diff's one 'range'
# positional catches them and rejects them as neither a range nor a path.
check "bare 'live' word rejected"    exit=1 err="range must be '..' or '...'" -- diff "$lidx" live --name-only
check "bare 'hunks' word rejected"   exit=1 err="range must be '..' or '...'" -- diff "$didx" hunks

# git's trailing '-- PATH...' has no catch-all to land in anymore: clap eats
# the literal '--' as its own end-of-options marker, and the path after it
# becomes diff's one 'range' token, same as a bare path with no '--' at all --
# so both spellings get the same '-p' hint now.
check "live -- is retired"           exit=1 err="paths go in '-p/--path'" -- diff "$lidx" --live --name-only -- shared.txt
check "bare path after --live"       exit=1 err="paths go in '-p/--path'" -- diff "$lidx" --live --name-only shared.txt
# '-p/--path' is that limit in plain words: a flag, so it can sit anywhere a
# flag can, and a comma list instead of a trailing run of words.
check "-p limits the diff"           exit=0 out="shared.txt" -- diff "$lidx" --live -p shared.txt --name-only
check "--path long form"             exit=0 out="shared.txt" -- diff "$lidx" --live --path shared.txt --name-only
check "-p takes a comma list"        exit=0 out="untracked.txt" -- diff "$lidx" --live -p shared.txt,untracked.txt --name-only
check "-p checks the path too"       exit=1 err="no file matches 'nope/'" -- diff "$lidx" --live -p nope/ --name-only
check "-p empty element errors"      exit=1 err="bad path list" -- diff "$lidx" --live -p "shared.txt," --name-only
check "-p with a retired -- too"     exit=1 err="paths go in '-p/--path'" -- diff "$lidx" --live -p shared.txt --name-only -- shared.txt
# A glob is git's own matching, and one that hits nothing is still a mistake.
check "glob path matches"            exit=0 out="shared.txt" -- diff "$lidx" --live -p "*.txt" --name-only
check "glob with no hit errors"      exit=1 err="no file matches '*.zzz'" -- diff "$lidx" --live -p "*.zzz" --name-only

check "live rejects .. range"        exit=1 err="'--live' and '..' cannot combine" -- diff "$lidx" --live ..
check "live rejects ... range"       exit=1 err="'--live' and '...' cannot combine" -- diff "$lidx" --live ...
check "hunks rejects --stat"         exit=1 err="cannot combine" -- diff "$lidx" --hunks --stat
check "hunks works without live"     exit=0 out="committed state" -- diff "$didx" --hunks

# --- list --files -----------------------------------------------------------
# The live worktree is already dirty in all three interesting ways: a tracked
# edit, an untracked file, and an ignored one. --files must show the first two
# under the branch row and never the third.
check "list --files shows edit"      exit=0 out="M  shared.txt" -- list --files
check "list --files counts lines"    exit=0 out="+2  -1" -- list --files
check "list --files shows untracked" exit=0 out="?  untracked.txt" -- list --files
check "list --files counts untracked" exit=0 out="+1  -0" -- list --files
check "bare --files means list"      exit=0 out="M  shared.txt" -- list --files
check "bare -f short flag means list"  exit=0 out="M  shared.txt" -- list -f
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
lpath="$("$BIN" path "$lidx" 2>/dev/null)"
rpath="$("$BIN" path "$ridx" 2>/dev/null)"
mpath="$("$BIN" path 1 2>/dev/null)"

check "meld 2 trees passes both dirs" exit=0 out="ARGV: $mpath $lpath" -- meld "1,$lidx"
check "meld 3 trees, listed order"    exit=0 out="ARGV: $lpath $mpath $rpath" -- meld "$lidx,1,$ridx"
check "meld one tree errors"          exit=1 err="meld needs 2 or 3 worktrees" -- meld 1
check "meld over 3 errors"            exit=1 err="at most 3 worktrees, got 4" -- meld 1,2,3,4
check "meld dup tree errors"          exit=1 err="worktree #1 listed twice" -- meld 1,1
check "meld bad index errors"         exit=1 err="no worktree #99" -- meld 1,99
check "meld non-numeric list errors"  exit=1 err="no worktree on branch 'x'" -- meld 1,x
check "meld takes no options"         exit=2 err="unexpected argument '-z' found" -- meld 1,2 -z
check "meld --diff needs 2 trees"     exit=1 err="takes exactly 2 worktrees" -- meld 1,2,3 --diff
check "meld --diff ... range works"   exit=0 out="onlylogin.txt" -- meld "1,$didx" --diff ...
check "meld --diff ... omits main"    exit=0 err="" -- meld "1,$didx" --diff ...
check "meld --diff 2-way works"       exit=0 out="onlymain.txt" -- meld "1,$didx" --diff
check "meld --diff 2-way has both"    exit=0 out="onlylogin.txt" -- meld "1,$didx" --diff
check "meld --diff empty diff"        exit=0 err="no files differ" -- meld "1,1" --diff
check "meld --3way and --base clash"  exit=1 err="alternatives" -- meld 1,2 --diff --3way --base main

# --left/--right/--center: an alternative to naming worktrees by position in
# a list. Each takes a worktree number or a raw filesystem path.
check "meld --left/--right two-way"     exit=0 out="ARGV: $mpath $lpath" -- meld --left 1 --right "$lidx"
check "meld --left/--right/--center 3-way" exit=0 out="ARGV: $lpath $mpath $rpath" -- meld --left "$lidx" --center 1 --right "$ridx"
# --center without --right folds into a two-way, the center slot doubling as
# the right side.
check "meld --center folds into two-way" exit=0 out="ARGV: $mpath $lpath" -- meld --left 1 --center "$lidx"
check "meld --left alone errors"        exit=1 err="meld needs --right or --center along with --left" -- meld --left 1
# --left/--right/--center and the worktree list are alternatives.
check "meld --left + positional errors" exit=1 err="meld's '--left'/'--right'/'--center' and the worktree list" -- meld --left 1 --right "$lidx" "1,$lidx"
# A raw filesystem path is as valid a slot as a worktree number.
check "meld --left takes a raw path"    exit=0 out="ARGV: $FAKEBIN $lpath" -- meld --left "$FAKEBIN" --right "$lidx"

check "bare target list rejected"       exit=1 err="switch takes a single worktree, not '1,2'" -- 1,2

# meld missing from PATH: a PATH holding only git proves the check fires
# regardless of whether the host has a real meld installed.
GITONLY="$ROOT/gitonly"
mkdir -p "$GITONLY"
ln -sf "$(command -v git)" "$GITONLY/git"
meld_err="$(PATH="$GITONLY" "$BIN" meld 1,2 2>&1 >/dev/null; )"
mcmd="$(fmt_cmd meld 1,2)  # PATH without meld"
case "$meld_err" in
  *"meld is not installed"*)
    report PASS UNHAPPY "meld missing gives install hint" "$mcmd" ;;
  *)
    report FAIL UNHAPPY "meld missing gives install hint" "$mcmd" "got '$meld_err'" ;;
esac

# --- compare ----------------------------------------------------------------
# compare works on cwd alone -- the suite's cwd is the main worktree -- so the
# cases below need no target list, only a ref. They reuse 'onlymain.txt', which
# the diff section committed on main, and everything they dirty is restored at
# the end of the section so the later suites still see a clean tree.
#
# The meld stub installed above is still on PATH, which is what '-M' asserts on.
cmp_sha="$(git rev-parse --short HEAD~1)"
git tag cmptag HEAD~1
# The binary reports its cwd resolved (/tmp is a symlink to /private/tmp on
# macOS), so the '-m' assertions below must compare against the resolved form.
cmp_cwd="$(pwd -P)"

# An uncommitted edit is what every ref shape below has to show: the file is
# committed on main, so without this the diff would be empty at all of them and
# the assertions would prove nothing. It also proves compare reads the file on
# disk rather than HEAD.
echo edited >> onlymain.txt

# '-x/--reference' (alias '--ref') takes every ref shape, which is the whole
# point of it not being a branch-only flag.
check "compare -x branch"            exit=0 out="+edited" -- compare -p onlymain.txt -x main
check "compare -x HEAD~1"            exit=0 out="+edited" -- compare -p onlymain.txt -x HEAD~1
check "compare -x sha"               exit=0 out="+edited" -- compare -p onlymain.txt -x "$cmp_sha"
check "compare -x tag"               exit=0 out="+edited" -- compare -p onlymain.txt -x cmptag
check "compare -x remote ref"        exit=0 out="+edited" -- compare -p onlymain.txt -x origin/remote-only
check "compare --ref long form"      exit=0 out="+edited" -- compare --path onlymain.txt --ref main
check "compare names the file"       exit=0 out="onlymain.txt" -- compare -p onlymain.txt -x main

# A comma list is many files, one invocation. cmp2.txt is staged rather than
# committed so main's history stays exactly as the merge/merged suites expect;
# staging is enough for 'git diff <ref>' to see it.
echo two > cmp2.txt && git add cmp2.txt
check "compare -p takes a file list" exit=0 out="cmp2.txt" -- compare -p onlymain.txt,cmp2.txt -x main
check "compare file list keeps both" exit=0 out="onlymain.txt" -- compare -p onlymain.txt,cmp2.txt -x main

# -m hands meld one '--diff local ref-copy' pair per file, local side first.
check "compare -m pairs local first" exit=0 out="ARGV: --diff $cmp_cwd/onlymain.txt" -- compare -p onlymain.txt -x main -M
check "compare -m pairs every file"  exit=0 out="--diff $cmp_cwd/cmp2.txt" -- compare -p onlymain.txt,cmp2.txt -x main -M
check "compare -m names the ref"     exit=0 err="compare main" -- compare -p onlymain.txt -x main -M

git rm -q --cached cmp2.txt && rm -f cmp2.txt
git checkout -q -- onlymain.txt

# Back to a clean tree: comparing against HEAD is no diff at all, and still a
# success. Exact, not a substring -- 'out=' with an empty value asserts nothing.
cmp_same="$("$BIN" compare -p onlymain.txt -x HEAD 2>/dev/null)"
cmp_cmd="$(fmt_cmd compare -p onlymain.txt -x HEAD)"
if [ -z "$cmp_same" ]; then
  report PASS HAPPY "compare at HEAD prints nothing" "$cmp_cmd"
else
  report FAIL HAPPY "compare at HEAD prints nothing" "$cmp_cmd" "wanted no output, got '$cmp_same'"
fi

check "compare bad ref errors"       exit=1 err="no such ref 'nope'" -- compare -p onlymain.txt -x nope
check "compare bad file errors"      exit=1 err="no such file 'nope.txt'" -- compare -p nope.txt -x main
check "compare empty list part"      exit=1 err="bad file list" -- compare -p "onlymain.txt,,cmp2.txt" -x main
check "compare needs a ref"          exit=1 err="compare needs a ref: '-x/--reference <REF>'" -- compare -p onlymain.txt
check "compare needs a file"         exit=1 err="compare needs at least one file: '-p/--path <FILE_LIST>'" -- compare -x main
check "compare -x takes exactly one" exit=1 err="compare takes exactly one ref, got 2" -- compare -p onlymain.txt -x main,cmptag
# -c/--commit is retired: -x/--reference is the only ref flag.
check "compare -c is retired"        exit=2 err="unexpected argument '-c'" -- compare -p onlymain.txt -c main
check "compare --commit is retired"  exit=2 err="unexpected argument '--commit'" -- compare -p onlymain.txt --commit main
# compare declares no -b/--branch at all now, so clap itself rejects it --
# pre-verb (no longer a global) or post-verb (not one of compare's own flags).
check "compare rejects pre-verb -b"  exit=2 err="unexpected argument '-b' found" -- -b main compare -p onlymain.txt -x main
check "compare rejects post-verb -b" exit=2 err="unexpected argument '-b' found" -- compare -p onlymain.txt -x main -b main

git tag -d cmptag >/dev/null 2>&1

# --- remove -----------------------------------------------------------------
check "remove main refused"          exit=1 err="refusing to remove the main worktree" -- remove 1 -y
# Removing a tree you are NOT standing in prints nothing (wrapper stays put).
check "remove other prints nothing"  exit=0 out="" -- remove 2 -y

# Standing INSIDE the removed tree: it prints main so the wrapper cd's back.
"$BIN" add insidebr --dirname insidewt >/dev/null 2>&1
iidx="$("$BIN" list | awk '$2=="insidebr"{print $1}')"
inside_out="$(cd "$WT/insidewt" && "$BIN" remove "$iidx" -y</dev/null 2>/dev/null)"
app_phys="$(cd "$APP" && pwd -P)"
if [ "$inside_out" = "$app_phys" ]; then
  report PASS HAPPY "remove-from-inside prints main" "$(fmt_cmd remove "$iidx" -y)  # cwd inside it"
else
  report FAIL HAPPY "remove-from-inside prints main" "$(fmt_cmd remove "$iidx" -y)" \
    "wanted main '$app_phys', got '$inside_out'"
fi

# -f: a worktree with an untracked file is refused without -f, removed with it.
"$BIN" add dirty --dirname dirtywt >/dev/null 2>&1
touch "$WT/dirtywt/junk.txt"
didx="$("$BIN" list | awk '$2=="dirty"{print $1}')"
check "remove dirty refused (no -f)" exit=1 err="modified or untracked files" -- remove "$didx" -y
check "remove dirty with -f"         exit=0 -- remove "$didx" -y -f

# Bulk remove: the positional list takes several targets at once, comma- or
# space-separated, one `worktree remove` per target.
"$BIN" add bulk-a --dirname bulkA >/dev/null 2>&1
"$BIN" add bulk-b --dirname bulkB >/dev/null 2>&1
baidx="$("$BIN" list | awk '$2=="bulk-a"{print $1}')"
bbidx="$("$BIN" list | awk '$2=="bulk-b"{print $1}')"
check "remove comma list removes both" exit=0 -- remove "$baidx,$bbidx" -y
check "removed comma target is gone"   exit=1 err="no worktree #$baidx" -- path "$baidx"
check "removed comma target 2 is gone" exit=1 err="no worktree #$bbidx" -- path "$bbidx"

"$BIN" add bulk-c --dirname bulkC >/dev/null 2>&1
"$BIN" add bulk-d --dirname bulkD >/dev/null 2>&1
bcidx="$("$BIN" list | awk '$2=="bulk-c"{print $1}')"
bdidx="$("$BIN" list | awk '$2=="bulk-d"{print $1}')"
check "remove space list removes both" exit=0 -- remove "$bcidx" "$bdidx" -y
check "removed space target is gone"   exit=1 err="no worktree #$bcidx" -- path "$bcidx"
check "removed space target 2 is gone" exit=1 err="no worktree #$bdidx" -- path "$bdidx"

check "remove dup target errors" exit=1 err="worktree #$didx listed twice" -- remove "$didx,$didx" -y

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
# The lone positional is the source, exactly as 'git merge <thing>' reads it,
# whether it is a worktree number, a branch with a worktree, or a plain branch
# name. The destination is '-d/--destination' and defaults to the current
# worktree; cwd here is the main worktree, #1.
check "merge takes one source"       exit=2 err="unexpected argument" -- merge "$A" "$C1"
check "merge old target-first order rejected" exit=2 err="unexpected argument 'merge' found" -- switch 1 merge 2
check "merge unknown source"         exit=1 err="no worktree or branch 'zzz'" -- merge -s zzz
check "merge source out of range"    exit=1 err="no worktree #99" -- merge 99
check "merge source twice refused"   exit=1 err="source given twice" -- merge "$A" -s feat-a
check "merge -d takes one target"    exit=1 err="takes exactly one target" -- merge "$A" -d "1,$C1"
check "merge rejects a source list"  exit=1 err="merge takes one source" -- merge "1,$A"
# Source and destination naming one worktree is refused, by either spelling.
check "merge self refused"           exit=1 err="worktree #1 is both the source and the target" -- merge 1 -d 1
check "merge self by default target" exit=1 err="worktree #1 is both the source and the target" -- merge 1
# merge's options are clap-declared (MergeOptions), so an unrecognized flag
# and every pairwise/start-only conflict (ours/theirs, continue/abort, an
# option alongside continue/abort) is clap's declarative conflicts_with: clap's
# wording, its Usage block underneath, and its exit 2 -- `Cli::parse()` renders
# and exits itself, before main's `error: `-prefixed one-liner path can run.
check "merge unknown option"         exit=2 err="unexpected argument '--rebase'" -- merge "$A" --rebase
check "merge ours+theirs conflict"   exit=2 err="'--ours' cannot be used with '--theirs'" -- merge "$A" --ours --theirs
# '--dry-run' is compatible with ours/theirs (unlike the other start-only
# flags), so its "takes no merge options" check is the hand-written accumulator
# this list feeds.
check "merge dry-run + --no-ff"      exit=1 err="dry-run takes no merge options (got --no-ff)" -- merge "$A" --dry-run --no-ff
# A resume word takes no source at all: it finishes what is already started in
# the destination, so '-d' is how it reaches a worktree other than the current.
check "merge continue takes no source" exit=2 err="'[SOURCE]' cannot be used with '--continue'" -- merge "$A" --continue
check "merge abort takes no source"  exit=2 err="'[SOURCE]' cannot be used with '--abort'" -- merge "$A" --abort
check "merge continue takes no -s"   exit=1 err="'--continue' cannot be used with '[SOURCE]'" -- merge -s feat-a --continue
check "merge continue with a side"   exit=2 err="'--theirs' cannot be used with '--continue'" -- merge --theirs --continue
check "merge continue+abort"         exit=2 err="'--continue' cannot be used with '--abort'" -- merge --continue --abort
check "rejection names the flag"     exit=2 err="'--abort' cannot be used with" -- merge --abort -m x --squash
check "merge continue w/o merge"     exit=1 err="no merge in progress" -- merge --continue -d 1
check "merge abort w/o merge"        exit=1 err="no merge in progress" -- merge --abort -d 1

# Dirty destination takes -f; untracked files alone do not count as dirty.
touch "$ROOT/mrg/app-worktrees/w-dirty/untracked.txt"
check "merge with untracked only ok"  exit=0 err="Merged feat-a into dirtybr" in=y -- merge "$A" -d "$D"
git -C "$ROOT/mrg/app-worktrees/w-dirty" merge -q --abort 2>/dev/null; git -C "$ROOT/mrg/app-worktrees/w-dirty" reset -q --hard HEAD~1
# Tracked edits must still be refused even when untracked files are also
# present; the porcelain reports both, and the untracked lines must not mask
# the tracked ones. Re-create the untracked file so the combined case is real.
touch "$ROOT/mrg/app-worktrees/w-dirty/untracked.txt"
echo edit >> "$ROOT/mrg/app-worktrees/w-dirty/base.txt"
check "merge into dirty+untracked refused" exit=1 err="uncommitted changes" -- merge "$A" -d "$D"
rm -f "$ROOT/mrg/app-worktrees/w-dirty/untracked.txt"
check "merge into dirty refused"     exit=1 err="uncommitted changes" -- merge "$A" -d "$D"
check "merge into dirty with -F"     exit=0 err="Merged feat-a into dirtybr" in=y -- merge "$A" -d "$D" -F

# Clean merge by worktree number: worktree A's branch moves into worktree 1.
check "merge by number"              exit=0 err="Merged feat-a into" in=y -- merge "$A" -d "1"
if [ -f "$MRG/a.txt" ]; then
  report PASS HAPPY "merge by number moved the files" "test -f a.txt  # in worktree 1"
else
  report FAIL HAPPY "merge by number moved the files" "test -f a.txt  # in worktree 1" \
    "a.txt absent from $MRG after merge"
fi
check "merge prints no stdout"       exit=0 out="" in=y -- merge "$A" -d "$C1"

# A branch name works where a number does, and --ff-only refuses a real merge.
check "merge by branch name"         exit=0 err="Merged feat-a into cb2" in=y -- merge -s feat-a -d "$C2"
check "merge --ff-only refuses"      exit=1 err="Not possible to fast-forward" in=y -- merge "$C1" -d "$C2" --ff-only

# Conflict -> continue.  cb1 and cb2 both rewrote shared.txt.
check "merge conflict reports files" exit=1 err="shared.txt" in=y -- merge "$C2" -d "$C1"
check "second merge while stuck"     exit=1 err="already in progress" in=y -- merge -s feat-a -d "$C1"
check "continue with unresolved"     exit=1 err="merge conflict in" -- merge --continue -d "$C1"
echo resolved > "$ROOT/mrg/app-worktrees/w-cb1/shared.txt"
git -C "$ROOT/mrg/app-worktrees/w-cb1" add shared.txt
check "continue after resolve"       exit=0 err="Completed merge" -- merge --continue -d "$C1"

# Conflict -> abort restores the pre-merge state. cb1 has swallowed cb2 by now,
# so cb3 is the branch that still genuinely conflicts with cb2.
printf "y\\n" | "$BIN" merge "$M3" -d "$C2" >/dev/null 2>&1
check "abort a conflicted merge"     exit=0 err="Aborted merge" -- merge --abort -d "$C2"
if git -C "$ROOT/mrg/app-worktrees/w-cb2" rev-parse --verify -q MERGE_HEAD >/dev/null; then
  report FAIL HAPPY "abort clears MERGE_HEAD" "git rev-parse MERGE_HEAD  # in w-cb2" \
    "MERGE_HEAD still present after --abort"
else
  report PASS HAPPY "abort clears MERGE_HEAD" "git rev-parse MERGE_HEAD  # in w-cb2"
fi

# dry-run: answers the question, writes nothing. cb3 still collides with cb2.
check "dry-run clean merge"          exit=0 err="merges into" -- merge "$A" -d "$C2" --dry-run
check "dry-run reports a conflict"   exit=1 err="does NOT merge" -- merge "$M3" -d "$C2" --dry-run
check "dry-run names the file"       exit=1 err="shared.txt" -- merge "$M3" -d "$C2" --dry-run
check "dry-run says it touched none" exit=1 err="nothing was changed" -- merge "$M3" -d "$C2" --dry-run
# '--dry-run' has no short form any more -- '-d' is the destination flag now
# -- so ours/theirs under a dry run are only worth checking long-form.
check "theirs + --dry-run"           exit=0 err="merges into" -- merge "$A" -d "$C2" --theirs --dry-run
check "ours + --dry-run"             exit=0 err="merges into" -- merge "$A" -d "$C2" --ours --dry-run
# Proof it wrote nothing: a dry run that predicted a conflict left no merge
# behind, so a real merge can still start cleanly afterwards.
if git -C "$ROOT/mrg/app-worktrees/w-cb2" rev-parse --verify -q MERGE_HEAD >/dev/null; then
  report FAIL HAPPY "dry-run leaves no merge state" "git rev-parse MERGE_HEAD  # in w-cb2" \
    "MERGE_HEAD exists after a dry run"
else
  report PASS HAPPY "dry-run leaves no merge state" "git rev-parse MERGE_HEAD  # in w-cb2"
fi

# 'review' is a verb of its own: it answers what a merge would bring over and
# whether it would be clean, writes nothing, and carries the verdict in its
# exit code the way 'merge --dry-run' does -- so these mirror the dry-run cases
# above. Its source/dest grammar is merge's: positional source, '-d' dest.
check "review clean merge"           exit=0 err="merges cleanly" -- review "$A" -d "$LM"
# Nothing to bring: there is no merge to have a verdict about, so it says the
# one true thing in 'merged's words rather than "0 commits, merges cleanly"
# above an empty table -- which reads as though a merge just ran.
check "review of an empty range"     exit=0 err="is already in" -- review "$FF2" -d "$C2"
# ...and says only that: no verdict line, no count, no table. `check` asserts
# what output contains, so the absence is checked here.
emptyrev="$("$BIN" review "$FF2" -t "$C2" 2>&1)"
emptycmd="git-wt review $FF2 -t $C2"
if [[ "$emptyrev" == *"merges cleanly"* || "$emptyrev" == *"0 commits"* ]]; then
  report FAIL HAPPY "review empty says it once" "$emptycmd" \
    "an empty range still printed a verdict (got '$emptyrev')"
else
  report PASS HAPPY "review empty says it once" "$emptycmd"
fi
# An empty range is still parsed: the flags are rejected on their own terms,
# not skipped because there happened to be nothing to report.
check "review empty still parses"    exit=2 err="unexpected argument '--bogus'" -- review "$FF2" -d "$C2" --bogus
check "review empty refuses --all"   exit=1 err="no '--all' under 'review'" -- review "$FF2" -d "$C2" --all
check "review names both branches"   exit=0 err="->" -- review "$A" -d "$LM"
check "review reports a conflict"    exit=1 err="does NOT merge cleanly" -- review "$M3" -d "$C2"
check "review lists the conflict"    exit=1 err="shared.txt" -- review "$M3" -d "$C2"
check "review says it touched none"  exit=1 err="nothing was changed" -- review "$M3" -d "$C2"
# review owns a whole flag vocabulary, the 'commits' table's. Several of these
# letters are merge options under 'merge' -- '-f' is force there and files
# here, '-a' is abort there and all here -- which is exactly what a separate
# verb buys: no collision to arbitrate.
check "review -f is files"           exit=0 err="merges cleanly" -- review "$A" -d "$LM" -f
check "review takes -n"              exit=0 err="merges cleanly" -- review "$A" -d "$LM" -n 1
check "review takes --author"        exit=0 err="merges cleanly" -- review "$A" -d "$LM" --author t
# --no-merges is a hard error in 'commits' and a real flag here: a review keeps
# merges by default, so the word that turns them off has something to do.
check "review takes --no-merges"     exit=0 err="merges cleanly" -- review "$A" -d "$LM" --no-merges
check "commits still rejects it"     exit=2 err="unexpected argument '--no-merges'" -- commits "$LM,$A" --no-merges
# A merge option is not a word review knows, in either order.
check "review rejects -F"            exit=2 err="unexpected argument '-F'" -- review "$A" -d "$LM" -F
check "review rejects --dry-run"     exit=2 err="unexpected argument '--dry-run'" -- review "$A" -d "$LM" --dry-run
check "review rejects --ff-only"     exit=2 err="unexpected argument '--ff-only'" -- review "$A" -d "$LM" --ff-only
# ...and '--review' is not a word merge knows any more either.
check "merge rejects --review"       exit=2 err="unexpected argument '--review'" -- merge "$A" -d "$LM" --review
check "merge rejects --meld"         exit=2 err="unexpected argument '--meld'" -- merge "$A" -d "$LM" --meld
# '--squash' is a commits flag (the consolidated file block), so it shapes the
# table rather than colliding with anything.
check "review + squash consolidates" exit=0 err="merges cleanly" -- review "$A" -d "$LM" --squash
check "review keeps typo errors"     exit=2 err="unexpected argument '--bogus'" -- review "$A" -d "$LM" --bogus
# --all/--union are commits flags, and refused: both name a row source, and a
# review's is the range. '-a' is '--all' here, so it goes under that name.
check "review refuses --all"         exit=1 err="no '--all' under 'review'" -- review "$A" -d "$LM" --all
check "review refuses -a as --all"   exit=1 err="no '--all' under 'review'" -- review "$A" -d "$LM" -a
check "review refuses --union"       exit=1 err="no '--union' under 'review'" -- review "$A" -d "$LM" --union
# The source/dest grammar, same shape merge's block asserts.
check "review needs a source"        exit=1 err="review needs a source" -- review -d "$LM"
check "review source twice refused"  exit=1 err="source given twice" -- review "$A" -s feat-a
check "review -d takes one target"   exit=1 err="takes exactly one target" -- review "$A" -d "1,$C1"
check "review rejects a source list" exit=1 err="review takes one source" -- review "1,$A"
check "review self refused"          exit=1 err="worktree #1 is both the source and the target" -- review 1 -d 1
check "review -s names the source"   exit=0 err="merges cleanly" -- review -s feat-a -d "$LM"
# Proof it wrote nothing, exactly as the dry-run block above proves it.
if git -C "$ROOT/mrg/app-worktrees/w-cb2" rev-parse --verify -q MERGE_HEAD >/dev/null; then
  report FAIL HAPPY "review leaves no merge state" "git rev-parse MERGE_HEAD  # in w-cb2" \
    "MERGE_HEAD exists after a review"
else
  report PASS HAPPY "review leaves no merge state" "git rev-parse MERGE_HEAD  # in w-cb2"
fi

# --- merged -----------------------------------------------------------------
# Read-only ancestor check; state is unchanged. Uses the same fixture as merge.
# cb3 has one commit (C) that is not in main, so it is a stable "ahead" case.
# stuckbr was branched from cb2, so it is cleanly contained in cb2.
# Assumes the harness is standing on 'main' (worktree 1), so a self-check reads
# "Merged main is already in main".
check "merged current in itself"     exit=0 out="Merged main is already in main" -- merged 1
check "merged branch not in main"    exit=1 err="Ahead cb3 is NOT in main (ahead 1)" -- merged 1 cb3
check "merged branch is in cb2"       exit=0 out="Merged stuckbr is already in cb2" -- merged "$C2" stuckbr
check "merged list form dest-first"  exit=1 err="Ahead cb3 is NOT in main (ahead 1)" -- merged "1,$M3"
check "merged list form reversed"    exit=0 out="Merged stuckbr is already in cb2" -- merged "$C2,$FF2"
check "merged too many args"         exit=2 err="unexpected argument 'extra'" -- merged 1 cb3 extra
check "merged unknown source"        exit=1 err="no worktree or branch 'zzz'" -- merged 1 zzz
check "merged self single form"      exit=1 err="already checked out in worktree 1" -- merged 1 1
# A worktree-number source takes the list form, as merge and diff do. A source
# equal to the destination is left to the self-check above, which says more.
check "merged single target with number source" exit=1 err="Ahead cb3 is NOT in main (ahead 1)" -- merged 1 "$M3"
check "merged list too many"         exit=1 err="merged takes one or two worktrees, got 3" -- merged "1,$M3,$C2"
check "merged list form extra arg"   exit=1 err="merged takes no arguments" -- merged "1,$M3" extra
check "merged list form dup"         exit=1 err="worktree #1 listed twice" -- merged "1,1"
check "merged N self-check"           exit=0 out="Merged main is already in cb1" -- merged "$C1"

# A detached worktree has no branch to name, so the list form is the only way to
# ask about one: 'merged' only tests containment, so it answers by sha.
# Torn down right after: 'git worktree list' orders by path, so leaving this one
# in would renumber the worktrees the hardcoded indices below depend on.
git worktree add --detach "$ROOT/mrg/w-det" main >/dev/null 2>&1
DET="$(midx '(detached)')"
check "merged detached list form"    exit=0 out="is already in main" -- merged "1,$DET"
check "merged detached number source needs branch" exit=1 err="no worktree or branch" -- merged 1 "$DET"
git worktree remove --force "$ROOT/mrg/w-det" >/dev/null 2>&1

# Column 6 in list: shows merged/ahead relative to the current branch (main).
check "list --col 6"                 exit=0 out="ahead 1" -- list --col 1,2,6

# ours/theirs settle the collision that stopped the plain merge above.
# cb3 (shared.txt=C) vs w-cb2 (shared.txt=B): theirs takes C, ours keeps B.
check "merge theirs resolves"        exit=0 err="theirs won conflicts" in=y -- merge "$M3" -d "$C2" --theirs
if [ "$(cat "$ROOT/mrg/app-worktrees/w-cb2/shared.txt")" = "C" ]; then
  report PASS HAPPY "theirs took the source's side" "cat shared.txt  # in w-cb2"
else
  report FAIL HAPPY "theirs took the source's side" "cat shared.txt  # in w-cb2" \
    "shared.txt is '$(cat "$ROOT/mrg/app-worktrees/w-cb2/shared.txt")', want C"
fi

# cb4 (shared.txt=D) collides with whatever w-cb1 settled on earlier.
before="$(cat "$ROOT/mrg/app-worktrees/w-cb1/shared.txt")"
check "merge ours keeps our side"    exit=0 err="ours won conflicts" in=y -- merge -s cb4 -d "$C1" --ours
if [ "$(cat "$ROOT/mrg/app-worktrees/w-cb1/shared.txt")" = "$before" ]; then
  report PASS HAPPY "ours kept worktree N's side" "cat shared.txt  # in w-cb1"
else
  report FAIL HAPPY "ours kept worktree N's side" "cat shared.txt  # in w-cb1" \
    "shared.txt changed to '$(cat "$ROOT/mrg/app-worktrees/w-cb1/shared.txt")', want '$before'"
fi

# 'theirs' on a merge that already stopped: it can't join one, so git-wt offers
# to abort and redo. Declining must leave the stopped merge exactly as it was.
printf "y\\n" | "$BIN" merge "$M3" -d "$FF2" >/dev/null 2>&1   # conflict in w-ff2
check "stuck+theirs declined"        exit=0 err="Aborted." in=n -- merge "$M3" -d "$FF2" --theirs
if git -C "$ROOT/mrg/app-worktrees/w-ff2" rev-parse --verify -q MERGE_HEAD >/dev/null; then
  report PASS UNHAPPY "declining keeps the stopped merge" "git rev-parse MERGE_HEAD  # in w-ff2"
else
  report FAIL UNHAPPY "declining keeps the stopped merge" "git rev-parse MERGE_HEAD  # in w-ff2" \
    "MERGE_HEAD gone — the merge was aborted despite answering n"
fi
# Accepting redoes it from clean, and the source wins.
check "stuck+theirs accepted"       exit=0 err="theirs won conflicts" in=$'y\ny' -- merge "$M3" -d "$FF2" --theirs
if [ "$(cat "$ROOT/mrg/app-worktrees/w-ff2/shared.txt")" = "C" ]; then
  report PASS HAPPY "redo let theirs win" "cat shared.txt  # in w-ff2"
else
  report FAIL HAPPY "redo let theirs win" "cat shared.txt  # in w-ff2" \
    "shared.txt is '$(cat "$ROOT/mrg/app-worktrees/w-ff2/shared.txt")', want C"
fi

# Explicit '-d' sanity checks: it takes an option tail like any other form, and
# a source list is not a target.
check "explicit -d dry-run clean"    exit=0 err="merges into" -- merge "$A" -d "1" --dry-run
check "explicit -d takes options"    exit=1 err="does NOT merge" -- merge "$M3" -d "$C1" --dry-run
check "-d rejects 3 targets"         exit=1 err="takes exactly one target" -- merge "$A" -d "1,$C1,$M3"
check "bare list without verb rejected" exit=1 err="switch takes a single worktree, not '1,$A'" -- "1,$A"
check "malformed list with verb rejected" exit=1 err="unexpected argument for the verb" -- "1," merge
check "list form + verb order rejected" exit=1 err="unexpected argument for the verb" -- "1,$A" merge continue
check "list form + short flag order rejected" exit=1 err="unexpected argument for the verb" -- "1,$A" merge -a
check "bad list + verb order rejected" exit=1 err="unexpected argument for the verb" -- "1,x" merge
# The real thing: worktree M's branch lands in worktree N.
check "-d merges the source into it" exit=0 err="Merged feat-a into" in=y -- merge "$A" -d "$LM"
if [ -f "$ROOT/mrg/app-worktrees/w-lm/a.txt" ]; then
  report PASS HAPPY "-d moved the files" "test -f a.txt  # in w-lm"
else
  report FAIL HAPPY "-d moved the files" "test -f a.txt  # in w-lm" \
    "a.txt absent from w-lm after 'merge $A -d $LM'"
fi

# --squash stages the merge without committing it.
FF="$(cd "$MRG" && "$BIN" add ffbr --dirname w-ff >/dev/null 2>&1; midx ffbr)"
check "merge --squash stages only"   exit=0 err="Squashed feat-a into ffbr" in=y -- merge "$A" -d "$FF" --squash
if [ -n "$(git -C "$ROOT/mrg/app-worktrees/w-ff" diff --cached --name-only)" ]; then
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
( set -e
  git init -q --bare "$SR"
  # A bare repo's HEAD is init.defaultBranch: 'master' on stock Linux. Only
  # 'main' is ever pushed here, so a later clone of this origin would resolve
  # HEAD to a ref that does not exist, check out nothing, and leave the second
  # clone empty. Pin the bare HEAD to the branch that will actually be there
  # -- with -C, or it retargets the fixture repo we happen to be standing in.
  git -C "$SR" symbolic-ref HEAD refs/heads/main
  git clone -q "$SR" "$SA" 2>/dev/null
  cd "$SA"
  git config user.email t@t; git config user.name t
  # The clone of an empty repo names its unborn branch from the *local*
  # init.defaultBranch, so it can still land on 'master'. Renaming an unborn
  # HEAD is a symbolic-ref, not a checkout.
  git symbolic-ref HEAD refs/heads/main
  echo s > s.txt; git add s.txt; git commit -q -m init; git push -q -u origin main
  git branch feat-s; git branch lonely
  "$BIN" add feat-s --dirname w-feat-s >/dev/null 2>&1
  git -C "$ROOT/syn/app-worktrees/w-feat-s" push -q -u origin feat-s
  # lonely is never pushed, so it has no upstream: pull/push fail on it.
  "$BIN" add lonely --dirname w-lonely >/dev/null 2>&1
  git worktree add -q --detach "$ROOT/syn/w-det" HEAD
  # A separate clone is the "someone else pushed" the sweep then pulls.
  git clone -q "$SR" "$ROOT/syn/other" 2>/dev/null
  cd "$ROOT/syn/other"
  git config user.email t@t; git config user.name t
  echo upstream >> s.txt; git commit -q -am "upstream work"; git push -q origin main ) ||
  { echo "fixture: sync setup failed -- the pull/push checks below cannot mean anything" >&2; exit 1; }

cd "$SA" || exit 1
sidx() { "$BIN" list | awk -v b="$1" '$2==b{print $1}'; }
SF="$(sidx feat-s)"; SL="$(sidx lonely)"; SD="$(sidx '(detached)')"

# Grammar, before anything moves.
check "sync fetch bare verb defaults to current" exit=0 err="fetch main" -- fetch
check "sync target + --all"          exit=1 err="'--all' is every worktree, so a target list has nothing to add" -- pull 1 --all
check "sync list + --all"            exit=1 err="'--all' is every worktree, so a target list has nothing to add" -- push "1,$SF" --all
check "sync list dup"                exit=1 err="worktree #1 listed twice" -- fetch "1,1"
# fetch/pull/push are now three separate clap-declared structs (not a shared
# catch-all tail), so a flag foreign to the verb is clap's own rejection, not
# a hand-written "unknown option for X" -- and per-verb vocabularies (and the
# rebase/no-rebase, rebase/ff-only contradictions) are enforced by
# conflicts_with at parse time instead of a manual check afterward.
check "sync unknown flag"            exit=2 err="unexpected argument '--depth' found" -- pull 1 --depth=1
check "sync flags are per verb"      exit=2 err="unexpected argument '--rebase' found" -- fetch 1 --rebase
check "sync push has no --rebase"    exit=2 err="unexpected argument '--rebase' found" -- push 1 --rebase
check "sync pull has no -u"          exit=2 err="unexpected argument '-u' found" -- pull 1 -u
# push's own '-F/--force' stays a declared flag purely so this keeps its
# explanatory rejection instead of becoming clap's generic "unexpected
# argument" -- it is a real word on the other two verbs, so a safety note
# beats silence.
check "sync push --force refused"    exit=1 err="no '--force' for push" -- push 1 --force
check "sync push -F refused"         exit=1 err="no '--force' for push" -- push 1 -F
check "sync contradiction"           exit=2 err="'--rebase' cannot be used with '--no-rebase'" -- pull 1 --rebase --no-rebase
check "sync rebase vs ff-only"       exit=2 err="'--rebase' cannot be used with '--ff-only'" -- pull 1 --rebase --ff-only

# fetch works everywhere: it moves remote-tracking refs, so even a detached
# HEAD has something to do.
check "fetch one worktree"           exit=0 -- fetch 1
check "fetch takes --prune"          exit=0 -- fetch 1 --prune
check "fetch list form"              exit=0 err="fetch: 2 ok, 0 failed, 0 skipped" -- fetch "1,$SF"
check "fetch --all sweeps"           exit=0 err="fetch: 4 ok, 0 failed, 0 skipped" -- fetch --all
check "fetch detached is not skipped" exit=0 err="fetch: 2 ok, 0 failed, 0 skipped" -- fetch "1,$SD"

# pull: main is behind by the other clone's commit, so this is a real update.
check "pull one worktree"            exit=0 err="Fast-forward" -- pull 1

# The target list defaults to the current worktree when omitted entirely, so a
# bare verb now runs there instead of erroring -- that defaulting is the point
# of the verb-first rework. Run after the update above so it does not steal
# the fast-forward "pull one worktree" checks for.
check "sync bare verb defaults to current" exit=0 err="pull main" -- pull
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
check "pull single error is git's"   exit=1 err="no tracking information" -- pull "$SL"
check "pull takes --ff-only"         exit=0 -- pull 1 --ff-only

# push -u on a branch with no upstream: a bare 'git push -u' has none to read
# the remote off of, so git-wt names 'origin lonely' the way you would.
check "push -u sets the upstream"    exit=0 err="set up to track 'origin/lonely'" -- push "$SL" -u
if [ "$(git -C "$ROOT/syn/app-worktrees/w-lonely" rev-parse --abbrev-ref '@{upstream}' 2>&1)" = "origin/lonely" ]; then
  report PASS HAPPY "push -u left an upstream" "git rev-parse --abbrev-ref @{u}  # in w-lonely"
else
  report FAIL HAPPY "push -u left an upstream" "git rev-parse --abbrev-ref @{u}  # in w-lonely" \
    "upstream is '$(git -C "$ROOT/syn/app-worktrees/w-lonely" rev-parse --abbrev-ref '@{upstream}' 2>&1)'"
fi
check "push -u again is fine"        exit=0 -- push "$SL" -u
check "push --all sweeps"            exit=0 err="push: 3 ok, 0 failed, 1 skipped" -- push --all
check "push takes --dry-run"         exit=0 -- push 1 --dry-run

cd "$APP" || exit 1

# --- meta -------------------------------------------------------------------
check "version"                      exit=0 out="git-wt" -- version
check "--help"                       exit=0 out="Usage:" -- --help

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
