#!/usr/bin/env bash
# Live end-to-end test for git-wt.
#
# Builds the binary, spins up a dummy repo under /tmp, drives every command in
# the target-first grammar, and prints a PASS/FAIL report. Exits non-zero if any
# case fails. Cleans up the /tmp scratch dir on exit.
#
#   ./test.sh            # release build (cargo build --release)
#   ./test.sh --debug    # debug build (faster compile)
set -u

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

profile="release"
[ "${1:-}" = "--debug" ] && profile="debug"

echo "Building git-wt ($profile)..."
if [ "$profile" = "release" ]; then
  cargo build --release --manifest-path "$here/Cargo.toml" >/dev/null 2>&1 || { echo "build failed" >&2; exit 1; }
  BIN="$here/target/release/git-wt"
else
  cargo build --manifest-path "$here/Cargo.toml" >/dev/null 2>&1 || { echo "build failed" >&2; exit 1; }
  BIN="$here/target/debug/git-wt"
fi

# --- scratch repo under /tmp ------------------------------------------------
ROOT="$(mktemp -d "/tmp/git-wt-test.XXXXXX")"
CODE="$ROOT/code"
APP="$CODE/myapp"
trap 'rm -rf "$ROOT"' EXIT

mkdir -p "$APP"
cd "$APP" || exit 1
git init -q
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

  if [ "$ok" = 1 ]; then
    printf '  \033[32mPASS\033[0m  %s\n' "$name"
    pass=$((pass+1))
  else
    printf '  \033[31mFAIL\033[0m  %s  (%s)\n' "$name" "${why# ; }"
    fail=$((fail+1))
  fi
}

echo
echo "Running live tests in $APP"
echo "----------------------------------------------------------------------"

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
check "list --col bad number"        exit=1 err="no column 6" -- list --col 6
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
  git init -q; git config user.email t@t; git config user.name t
  git commit -q --allow-empty -m i; git branch only
  "$BIN" add only >/dev/null 2>&1 )          # main + only both checked out now
allco="$(cd "$FULL" && printf '\n' | "$BIN" add 2>&1)"
if printf '%s' "$allco" | grep -q "all are checked out"; then
  printf '  \033[32mPASS\033[0m  %s\n' "picker errors when all checked out"; pass=$((pass+1))
else
  printf '  \033[31mFAIL\033[0m  %s\n' "picker errors when all checked out"; fail=$((fail+1))
fi

# --from actually based the new branch on the given ref, not current HEAD.
if [ "$(git -C "$CODE/ff1" rev-parse HEAD)" = "$(git -C "$APP" rev-parse feature/login)" ]; then
  printf '  \033[32mPASS\033[0m  %s\n' "add --from base commit matches ref"; pass=$((pass+1))
else
  printf '  \033[31mFAIL\033[0m  %s\n' "add --from base commit matches ref"; fail=$((fail+1))
fi

# --- target: switch / path --------------------------------------------------
check "bare N prints path"           exit=0 out="myapp" -- 1
check "N path prints path"           exit=0 out="$APP" -- 1 path
check "N show alias"                 exit=0 out="$APP" -- 1 show
check "N switch too many args"       exit=1 err="too many arguments" -- 1 switch path
check "index 0 errors"               exit=1 err="no worktree #0" -- 0
check "index over range errors"      exit=1 err="there are" -- 99
check "unknown action errors"        exit=1 err="unknown action 'bogus'" -- 1 bogus
check "flag on target errors"        exit=1 err="switch/path/remove take no --name" -- 1 -n x

# --- legacy / unknown -------------------------------------------------------
check "legacy show hint"             exit=1 err="use 'git-wt 1 path'" -- show 1
check "legacy remove hint"           exit=1 err="use 'git-wt 1 remove'" -- remove 1
check "branch-like suggests add"     exit=1 err="did you mean 'add feat/x'" -- feat/x
check "plain unknown no suggest"     exit=1 err="unknown command 'lsit'" -- lsit

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
  printf '  \033[32mPASS\033[0m  %s\n' "remove-from-inside prints main"; pass=$((pass+1))
else
  printf '  \033[31mFAIL\033[0m  %s  (got '\''%s'\'')\n' "remove-from-inside prints main" "$inside_out"; fail=$((fail+1))
fi

# -f: a worktree with an untracked file is refused without -f, removed with it.
"$BIN" add dirty --dirname dirtywt >/dev/null 2>&1
touch "$CODE/dirtywt/junk.txt"
didx="$("$BIN" list | awk '$2=="dirty"{print $1}')"
check "remove dirty refused (no -f)" exit=1 err="modified or untracked files" -- "$didx" remove -y
check "remove dirty with -f"         exit=0 -- "$didx" remove -y -f

# --- meta -------------------------------------------------------------------
check "version"                      exit=0 out="git-wt" -- version
check "--help"                       exit=0 out="USAGE" -- --help

echo "----------------------------------------------------------------------"
echo "Result: $pass passed, $fail failed"
[ "$fail" = 0 ]
