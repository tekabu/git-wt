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
if printf '%s' "$allco" | grep -q "All local branches are already checked out"; then
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

# --- merge ------------------------------------------------------------------
# Self-contained repo: merges move branches, so keep them off the shared fixture.
#   main    base.txt, shared.txt="0"
#   feat-a  adds a.txt          (clean merge into main)
#   cb1/cb2/cb3 shared.txt A vs B vs C  (each conflicts with the others)
MRG="$ROOT/mrg/app"; mkdir -p "$MRG"
( cd "$MRG"
  git init -q; git config user.email t@t; git config user.name t
  echo base > base.txt; echo 0 > shared.txt
  git add .; git commit -q -m init
  git branch feat-a; git branch cb1; git branch cb2; git branch cb3; git branch ffbr; git branch dirtybr
  git checkout -q feat-a; echo a > a.txt; git add a.txt; git commit -q -m a
  git checkout -q cb1; echo A > shared.txt; git commit -q -am A
  git checkout -q cb2; echo B > shared.txt; git commit -q -am B
  git checkout -q cb3; echo C > shared.txt; git commit -q -am C
  git checkout -q main 2>/dev/null || git checkout -q master
  "$BIN" add feat-a  --dirname w-feat  >/dev/null 2>&1
  "$BIN" add cb1     --dirname w-cb1   >/dev/null 2>&1
  "$BIN" add cb2     --dirname w-cb2   >/dev/null 2>&1
  "$BIN" add dirtybr --dirname w-dirty >/dev/null 2>&1 )

cd "$MRG" || exit 1
midx() { "$BIN" list | awk -v b="$1" '$2==b{print $1}'; }
A="$(midx feat-a)"; C1="$(midx cb1)"; C2="$(midx cb2)"; D="$(midx dirtybr)"

# Errors before any state changes.
check "merge needs a source"         exit=1 err="merge needs a source" -- 1 merge
check "merge unknown source"         exit=1 err="no worktree or branch 'zzz'" -- 1 merge zzz
check "merge self refused"           exit=1 err="already checked out in worktree 1" -- 1 merge 1
check "merge too many args"          exit=1 err="too many arguments" -- 1 merge "$A" "$C1"
check "merge unknown option"         exit=1 err="unknown option '--rebase'" -- 1 merge "$A" --rebase
check "merge --continue takes no arg" exit=1 err="--continue takes no argument" -- 1 merge --continue 2
check "merge continue w/o merge"     exit=1 err="no merge in progress" -- 1 merge --continue
check "merge abort w/o merge"        exit=1 err="no merge in progress" -- 1 merge abort
check "legacy merge hint"            exit=1 err="use 'git-wt 1 merge 2'" -- merge 2

# Dirty destination takes -f; untracked files alone do not count as dirty.
touch "$ROOT/mrg/w-dirty/untracked.txt"
check "merge with untracked only ok"  exit=0 err="Merged feat-a into dirtybr" -- "$D" merge "$A"
git -C "$ROOT/mrg/w-dirty" merge -q --abort 2>/dev/null; git -C "$ROOT/mrg/w-dirty" reset -q --hard HEAD~1
# Tracked edits AND untracked files together: still refused (porcelain reports
# both, and the untracked lines must not mask the tracked ones).
echo edit >> "$ROOT/mrg/w-dirty/base.txt"
check "merge into dirty+untracked refused" exit=1 err="uncommitted changes" -- "$D" merge "$A"
rm -f "$ROOT/mrg/w-dirty/untracked.txt"
check "merge into dirty refused"     exit=1 err="uncommitted changes" -- "$D" merge "$A"
check "merge into dirty with -f"     exit=0 err="Merged feat-a into dirtybr" -- "$D" merge "$A" -f

# Clean merge by worktree number: worktree A's branch moves into worktree 1.
check "merge by number"              exit=0 err="Merged feat-a into" -- 1 merge "$A"
if [ -f "$MRG/a.txt" ]; then
  printf '  \033[32mPASS\033[0m  %s\n' "merge by number moved the files"; pass=$((pass+1))
else
  printf '  \033[31mFAIL\033[0m  %s\n' "merge by number moved the files"; fail=$((fail+1))
fi
check "merge prints no stdout"       exit=0 out="" -- "$C1" merge "$A"

# A branch name works where a number does, and --ff-only refuses a real merge.
check "merge by branch name"         exit=0 err="Merged feat-a into cb2" -- "$C2" merge feat-a
check "merge --ff-only refuses"      exit=1 err="Not possible to fast-forward" -- "$C2" merge cb1 --ff-only

# Conflict -> continue.  cb1 and cb2 both rewrote shared.txt.
check "merge conflict reports files" exit=1 err="shared.txt" -- "$C1" merge cb2
check "merge conflict hints continue" exit=1 err="merge --continue" -- "$C1" merge --continue
check "second merge while stuck"     exit=1 err="already in progress" -- "$C1" merge feat-a
check "continue with unresolved"     exit=1 err="merge conflict in" -- "$C1" merge --continue
echo resolved > "$ROOT/mrg/w-cb1/shared.txt"
git -C "$ROOT/mrg/w-cb1" add shared.txt
check "continue after resolve"       exit=0 err="Completed merge" -- "$C1" merge continue

# Conflict -> abort restores the pre-merge state. cb1 has swallowed cb2 by now,
# so cb3 is the branch that still genuinely conflicts with cb2.
"$BIN" "$C2" merge cb3 >/dev/null 2>&1
check "abort a conflicted merge"     exit=0 err="Aborted merge" -- "$C2" merge --abort
if git -C "$ROOT/mrg/w-cb2" rev-parse --verify -q MERGE_HEAD >/dev/null; then
  printf '  \033[31mFAIL\033[0m  %s\n' "abort clears MERGE_HEAD"; fail=$((fail+1))
else
  printf '  \033[32mPASS\033[0m  %s\n' "abort clears MERGE_HEAD"; pass=$((pass+1))
fi

# --squash stages the merge without committing it.
FF="$(cd "$MRG" && "$BIN" add ffbr --dirname w-ff >/dev/null 2>&1; midx ffbr)"
check "merge --squash stages only"   exit=0 err="Squashed feat-a into ffbr" -- "$FF" merge feat-a --squash
if [ -n "$(git -C "$ROOT/mrg/w-ff" diff --cached --name-only)" ]; then
  printf '  \033[32mPASS\033[0m  %s\n' "--squash leaves changes staged"; pass=$((pass+1))
else
  printf '  \033[31mFAIL\033[0m  %s\n' "--squash leaves changes staged"; fail=$((fail+1))
fi

cd "$APP" || exit 1

# --- meta -------------------------------------------------------------------
check "version"                      exit=0 out="git-wt" -- version
check "--help"                       exit=0 out="USAGE" -- --help

echo "----------------------------------------------------------------------"
echo "Result: $pass passed, $fail failed"
[ "$fail" = 0 ]
