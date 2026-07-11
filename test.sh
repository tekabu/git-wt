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

pass=0
fail=0

# check NAME -- run the binary, then assert on exit code, stdout, stderr.
#   expect_exit=<n>  expect_out=<substr>  expect_err=<substr>  (each optional)
# Trailing args after the option block are passed to git-wt (stdin = /dev/null,
# so any confirm() prompt sees EOF and answers No).
check() {
  local name="$1"; shift
  local exit_want="" out_want="" err_want=""
  while [ $# -gt 0 ]; do
    case "$1" in
      exit=*) exit_want="${1#exit=}"; shift ;;
      out=*)  out_want="${1#out=}"; shift ;;
      err=*)  err_want="${1#err=}"; shift ;;
      --) shift; break ;;
      *) break ;;
    esac
  done

  local out err code
  out="$("$BIN" "$@" 2>"$ROOT/err" </dev/null)"; code=$?
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
check "add existing local branch"    exit=0 err="myapp-feature-login" -- add feature/login
# worktree now exists at index 2
check "list shows new worktree"      exit=0 out="feature/login" -- list
check "list filter keeps index"      exit=0 out="2  feature/login" -- list logi
check "add --name suffix"            exit=0 err="myapp-review" -- add feature/logout --name review
check "add --dirname whole leaf"     exit=0 err="$CODE/scratch2" -- add feature/api --dirname scratch2
check "add dup dir refused"          exit=1 err="already exists" -- add feature/login
check "add name+dirname conflict"    exit=1 err="--name and --dirname conflict" -- add x -n a --dirname b
check "add --name empty"             exit=1 err="--name cannot be empty" -- add x -n ""
check "add --from needs ref"         exit=1 err="--from needs a ref" -- add x --from

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
check "remove worktree 2"            exit=0 out="$APP" -- 2 remove -y

# --- meta -------------------------------------------------------------------
check "version"                      exit=0 out="git-wt" -- version
check "--help"                       exit=0 out="USAGE" -- --help

echo "----------------------------------------------------------------------"
echo "Result: $pass passed, $fail failed"
[ "$fail" = 0 ]
