# git-wt — UX Redesign (draft)

Grammar shift: **target-first** for existing worktrees (`git-wt <N> <action>`),
explicit **`add`** verb for creation. No-args now lists (was: create picker).

Assumptions for every example below:

- repo root: `~/code/myapp`
- current branch: `main`
- default parent: `~/code` (parent of the primary worktree)
- worktrees numbered `1..N` as printed by `git-wt list`

---

## 1. Command surface

```
git-wt                       list worktrees, numbered from 1
git-wt list [SEARCH]         list, optional fuzzy filter (indices stay original)
git-wt <N>                   == git-wt <N> switch (wrapper cd's into it)
git-wt <N> switch            cd into worktree N (alias: cd)
git-wt <N> path              print worktree N's path only (alias: show)
git-wt <N> remove [-y] [-f]  remove worktree N
git-wt add [BRANCH] [flags]  create a worktree (picker when BRANCH omitted)
git-wt version               print version
git-wt --help
```

Aliases: `ls` = `list`, `rm` = `remove`, `cd` = `switch`, `show` = `path`.

Meta flags: `-h`/`--help`, `-V`/`--version`, plus bare `version`/`help` subcommands.

---

## 2. List / default

| Command | Effect |
|---|---|
| `git-wt` | list all worktrees, numbered from 1 |
| `git-wt list` | same |
| `git-wt ls` | alias → same |
| `git-wt list feat` | list filtered by fuzzy `feat`; **indices stay original** |
| `git-wt list zzz` (no match) | error: `no worktree matches 'zzz'`, **exit 1** |

Search is a **display filter only**. Index shown is the worktree's position in
the full list, so `git-wt <N> remove` always means the same tree regardless of
any prior filter.

---

## 3. Target (number-first)

| Command | Effect |
|---|---|
| `git-wt 1` | == `git-wt 1 switch` → cd into worktree 1 (via wrapper) |
| `git-wt 1 switch` | cd into worktree 1; alias `cd` |
| `git-wt 1 path` | print worktree 1's path only, no cd; alias `show` |
| `git-wt 1 remove` | prompt, then remove |
| `git-wt 1 remove -y` | remove, no prompt |
| `git-wt 1 remove -f` | remove, discard dirty (still prompts) |
| `git-wt 1 remove -y -f` | remove, no prompt, discard dirty |
| `git-wt 1 rm` | alias → remove |

**Binary vs wrapper.** A binary cannot cd its parent shell, so the binary
`git-wt` *always* just prints the path. The `wt` shell function decides:

- `wt 1`, `wt 1 switch`, `wt 1 cd` → `cd "$(git-wt 1 path)"`
- `wt 1 path`, `wt 1 show` → pass through, print only (scripting)

`path` prints the path + a **single trailing newline**, nothing else, so
`cd "$(git-wt 1 path)"` is safe. Default action is **`switch`**.

`-y` = skip confirmation. `-f`/`--force` = discard uncommitted/untracked files
(git refuses removal otherwise). They are independent.

---

## 4. Create (`add`)

Flags:

```
-n, --name NAME        suffix only → leaf = <repo>-NAME
    --dirname DIR       whole leaf, verbatim (sanitized)
-p, --parentdir DIR    parent dir (default: primary worktree's parent)
    --from REF          base ref for a NEW branch (default: current HEAD)
```

`BRANCH` optional — omit to open the interactive picker. All flags apply
equally to picker and explicit-branch forms.

**Picker fallback:**

- **fzf present:** live fuzzy search, arrow keys / typing to filter, Enter
  selects, Esc/Ctrl-C aborts (→ `no branch selected`, exit 1).
- **no fzf:** print a numbered list of local branches to stderr, prompt
  `Select a branch number (Enter to cancel): `. Type a number + Enter.
  Empty = cancel; out-of-range = error.

### 4a. Explicit branch

| Command | Leaf | Full path |
|---|---|---|
| `add feat/x` | `myapp-feat-x` | `~/code/myapp-feat-x` |
| `add feat/x -n test` | `myapp-test` | `~/code/myapp-test` |
| `add feat/x --dirname test` | `test` | `~/code/test` |
| `add feat/x -p P` | `myapp-feat-x` | `P/myapp-feat-x` |
| `add feat/x -n test -p P` | `myapp-test` | `P/myapp-test` |
| `add feat/x --dirname test -p P` | `test` | `P/test` |
| `add feat/x --dirname sub/test` | `sub/test` | `sub/test` (path, ignores `-p`) |
| `add feat/x --from develop` | `myapp-feat-x` | `~/code/myapp-feat-x`, new branch off `develop` |
| `add brandnew` (no such branch) | `myapp-brandnew` | prompts first (see below), then creates |

### 4b. Picker (BRANCH omitted)

| Command | Leaf | Full path |
|---|---|---|
| `add` | `myapp-<picked>` | `~/code/myapp-<picked>` |
| `add -n test` | `myapp-test` | `~/code/myapp-test` |
| `add --dirname test` | `test` | `~/code/test` |
| `add -p P` | `myapp-<picked>` | `P/myapp-<picked>` |
| `add -n test -p P` | `myapp-test` | `P/myapp-test` |
| `add --dirname test -p P` | `test` | `P/test` |

### 4c. Naming rules

- `--name` and `--dirname` values are **sanitized**: `/`, space, `:`, `\`
  collapse to `-`.
- Exception: `--dirname` containing `/` is treated as a **path**
  (parent-relative or absolute), sanitize skipped, `-p` ignored. When `-p` was
  also passed, warn + confirm:
  `--parentdir ignored because --dirname is a path. Continue? [y/N]`
- `--name` decouples the leaf suffix from the branch. Example: pick
  `feature/x`, pass `-n review` → dir `~/code/myapp-review`, branch
  `feature/x`. `git-wt list` still shows the real branch in its column.

### 4d. Branch resolution (after path resolved)

| Branch state | Effect |
|---|---|
| local exists | checkout into leaf |
| `origin/BRANCH` exists | create tracking branch, checkout |
| neither | prompt `Branch 'BRANCH' does not exist. Create it from '<from>'? [y/N]` → yes: create from `--from` (default current HEAD); no/Enter: `Aborted.`, exit 0 |

`--from` only affects the "neither" case (creating a new branch). Ignored when
the branch already exists local/remote — warn + confirm:
`branch 'BRANCH' already exists; --from ignored. Continue? [y/N]`

### 4e. Create refuses when

| Condition | Error |
|---|---|
| target path exists | `<path> already exists` |
| branch already checked out elsewhere | `branch 'BRANCH' already checked out at <path>` |

---

## 5. Meta

| Command | Effect |
|---|---|
| `git-wt version` | `git-wt X.Y.Z` |
| `git-wt -V` / `--version` | same |
| `git-wt help` | help text |
| `git-wt -h` / `--help` | same |

---

## 6. Errors & edge cases

| Command | Error |
|---|---|
| `git-wt` outside a repo | `not inside a git repository` |
| `git-wt 99` | `no worktree #99; there are N (see 'git-wt list')` |
| `git-wt 0` | `no worktree #0` |
| `git-wt 99 remove` | same `no worktree #99` |
| `git-wt 1 bogus` | `unknown action 'bogus' (switch, path, remove)` |
| `git-wt 1 switch path` | `too many arguments` |
| `git-wt 1 -n x` | `switch/path/remove take no --name` |
| `git-wt foo` (branch-like: has `/` or `-`, no spaces) | `unknown command 'foo'; did you mean 'add foo'?` |
| `git-wt lsit` (not branch-like) | `unknown command 'lsit'` (no `add` suggestion) |
| `git-wt show 1` (legacy verb-first) | `unknown command 'show'; use 'git-wt 1 path'` |
| `git-wt remove 1` (legacy verb-first) | `unknown command 'remove'; use 'git-wt 1 remove'` |
| `git-wt 1 remove` on main/bare worktree | `refusing to remove the main worktree` |
| `add feat/x -n a --dirname b` | `--name and --dirname conflict` |
| `add -n a --dirname b` | `--name and --dirname conflict` |
| `add feat/x -n ""` | `--name cannot be empty` |
| `add feat/x --dirname ""` | `--dirname cannot be empty` |
| `add feat/x -p` (no value) | `--parentdir needs a directory` |
| `add feat/x -n` (no value) | `--name needs a name` |
| `add feat/x --from` (no value) | `--from needs a ref` |
| `add feat/x --from bogus` | `bad revision 'bogus'` (git) |

---

## 6a. Warning convention

Any situation where a flag is **silently overridden** does not proceed
silently — it warns and asks `[y/N]`. Same prompt semantics as create/remove
(`y`/`yes` proceeds; anything else, incl. bare Enter, aborts; EOF/no-tty = No).

| Trigger | Prompt |
|---|---|
| `-p` + `--dirname` with `/` | `--parentdir ignored because --dirname is a path. Continue? [y/N]` |
| `--from` + existing branch | `branch 'BRANCH' already exists; --from ignored. Continue? [y/N]` |

Hard conflicts (`--name` + `--dirname`) stay **errors**, not prompts — no safe
default to confirm.

---

## 7. Resolved design decisions

1. **No-args = list.** Interactive create moved to `add` (no branch → picker).
2. **Legacy verb-first rejected.** Single grammar: `git-wt <N> <action>`.
   `git-wt show 1` / `git-wt remove 1` error with a migration hint.
3. **`list SEARCH` no-match = error**, not silent empty.
4. **`-y` (skip prompt) and `-f` (discard dirty) are separate flags.**
5. **`--name` = suffix, `--dirname` = whole leaf**, mutually exclusive.
6. **Both `--name`/`--dirname` sanitized**; `--dirname` with `/` = path.
7. **`--from REF` reinstated** — base ref for new branches (was cut, back in).
8. **`add` suggestion only for branch-like input** (has `/`/`-`, no spaces).
9. **Silent overrides warn + `[y/N]`** (see §6a); hard conflicts stay errors.
10. **`list SEARCH` no-match exits non-zero.**

---

## 8. Changes vs current binary

| Area | Now | Redesign |
|---|---|---|
| no-args | create picker | list |
| create | `git-wt <branch> [base-ref]` | `git-wt add <branch>` |
| cd into wt | `git-wt show <N>` (print only) | bare `git-wt <N>` = `switch` (cd); `path` prints only |
| remove | `git-wt remove <N>` | `git-wt <N> remove` |
| dir override | `--name` = whole dir | `--name` = suffix, `--dirname` = whole |
| parent dir | (none) | `-p/--parentdir` |
| list filter | (none) | `git-wt list SEARCH` |
| skip remove prompt | (none) | `-y` |
| base-ref | `[base-ref]` positional | `--from REF` flag (default current HEAD) |

---

## 9. `git-wt` (binary) vs `wt` (wrapper)

Two names, one tool, **one behavioral difference: `cd`.**

| | `git-wt` (binary) | `wt` (shell function) |
|---|---|---|
| Installed by | `install.sh` → `~/.cargo/bin` | `install.sh --alias wt` → your rc |
| `switch` / bare `N` | **prints** the path (can't cd) | **cd's** into it |
| `remove` | prints main path | cd's back to main after removal |
| `path`, `list`, `add`, etc. | identical | identical (pass-through) |
| Use for | **scripting** / `$(...)` capture | **interactive** daily use |

Why: a child process cannot change its parent shell's directory. The binary
does the work + prints a path; the `wt` function `cd`s using that path. Every
non-cd command behaves identically through either name.

**Install guidance (make explicit in README):**

- Interactive users: run `install.sh --alias wt`, then use **`wt`** — `wt 1`
  drops you in the worktree, `wt 1 remove` returns you to main.
- Scripts / capture: call **`git-wt`** directly. `dir=$(git-wt 1 path)`.
- Lead all examples with `switch`; keep `cd` only as a quiet alias (it's a
  shell builtin, so don't feature it).

Wrapper dispatch (updated for the new grammar):

```sh
wt() {
  case "$1" in
    -h|--help|-V|--version|version|help|list|ls|add) git-wt "$@"; return $? ;;
  esac
  # <N> [action]: switch (default) & remove cd the shell; path/show print.
  case "$2" in
    ""|switch|cd) local d; d="$(git-wt "$1" path)" || return $?; [ -n "$d" ] && cd "$d" ;;
    remove|rm)    local d; d="$(git-wt "$@")"      || return $?; [ -n "$d" ] && cd "$d" ;;
    *)            git-wt "$@" ;;   # path/show and anything else: pass through
  esac
}
```
