# git-wt

Create and manage git worktrees in sibling directories named
`<repo-folder>-<branch>`.

Installed on PATH as `git-wt`, so it also works as `git wt`.

```
~/code/myapp  +  feature/login  ->  ~/code/myapp-feature-login
```

Grammar is **target-first** for existing worktrees (`git-wt <N> <action>`) with
an explicit `add` verb for creation.

## Install

### From the one-file installer (no Rust needed)

Download the single self-installing script for your platform —
`git-wt-<version>-<os>-<arch>.install.sh` — and run it. Nothing else: no repo,
no tarball, no toolchain. The binary is embedded inside the script.

```sh
chmod +x git-wt-1.0.9-linux-x86_64.install.sh
./git-wt-1.0.9-linux-x86_64.install.sh            # binary only
./git-wt-1.0.9-linux-x86_64.install.sh --alias wt # + a `wt` shell function
```

Installs to `~/.local/bin` (override with `GITWT_PREFIX=/usr/local`). Make sure
that `bin` dir is on your `PATH`. If another `git-wt` earlier on `PATH` shadows
the installed one, the script warns and suggests removing or symlinking it.

### From source

Needs Rust (`cargo`). Builds and installs in one step with `cargo install`,
dropping the binary in `~/.cargo/bin`:

```sh
./install.sh                 # build + install from source
./install.sh --alias wt      # + a `wt` shell function that cd's for you
```

`install.sh` only ever installs from source. The shareable one-file installer
above is produced by `build.sh` (see [Build](#build)).

### `git-wt` (binary) vs `wt` (wrapper)

Two names, one tool, **one behavioral difference: `cd`.** A binary cannot change
its parent shell's directory, so the binary always *prints* a path; the `wt`
shell function *cd's* using that path.

|  | `git-wt` (binary) | `wt` (shell function) |
|---|---|---|
| `switch` / bare `N` | prints the path | **cd's** into it |
| `add` | prints the new path | **cd's** into it (unless `--stay`) |
| `remove` | prints main path | cd's back to main after removal |
| everything else | identical | identical (pass-through) |
| Use for | **scripting** / `$(...)` | **interactive** daily use |

- **Interactive:** run `./install.sh --alias wt`, then use `wt`. `wt 1` drops
  you in the worktree; `wt add feat/x` creates it and drops you in; `wt 1
  remove` returns you to main.
- **Scripts:** call `git-wt` directly. `dir=$(git-wt 1 path)`,
  `dir=$(git-wt add feat/x)`.

`--alias <name>` installs a shell function of that name into your rc
(`~/.zshrc`, `~/.bashrc`, or `~/.profile`) inside a managed block, refreshed on
reinstall.

## Usage

```
git-wt                       List worktrees, numbered from 1
git-wt list [SEARCH]         List, optionally fuzzy-filtered (indices stay put)
git-wt <N>                   == git-wt <N> switch
git-wt <N> switch            cd into worktree N (alias: cd)
git-wt <N> path              Print worktree N's path only (alias: show)
git-wt <N> remove [-y] [-f]  Remove worktree N
git-wt add [BRANCH] [flags]  Create a worktree (picker when BRANCH omitted)
git-wt version
git-wt --help
```

Aliases: `ls` = `list`, `rm` = `remove`, `cd` = `switch`, `show` = `path`.

### Options (create)

```
-n, --name NAME        Suffix only -> leaf = <repo>-NAME
    --dirname DIR       Whole leaf, verbatim (sanitized); with '/' = a path
-p, --parentdir DIR    Parent dir (default: primary worktree's parent)
    --from REF          Base ref for a NEW branch
                        (default: the branch of the worktree you run from)
    --stay              wrapper: create but do NOT cd into the new worktree
```

`-y` skips the remove confirmation; `-f`/`--force` discards uncommitted changes.
They are independent.

Prompts appear `[y/N]` when creating a branch that does not exist, before
`remove`, and whenever a flag is silently overridden. Type `y` (or `yes`) and
Enter to proceed; anything else — including bare Enter — aborts.

## List

`git-wt` with no arguments lists worktrees numbered from 1. `git-wt list
SEARCH` fuzzy-filters that list; the **numbers stay the original indices**, so
`git-wt <N> remove` always means the same tree regardless of any filter. No
match is an error (exit 1).

`--col` picks and orders columns — `1`=id, `2`=branch, `3`=dir (full path):

```sh
git-wt list --col 2,3        # branch + path, no id
git-wt list --col 1,2        # id + branch, no path
git-wt list --col 2          # branch only
git-wt list --col 3,2,1      # reversed
git-wt list --col 1,2 feat   # combine with a filter
```

## Switch / path

```sh
wt 1                     # cd into worktree 1
git-wt 1 path            # just print its path
cd "$(git-wt 1 path)"    # equivalent by hand
```

`path` prints the path plus a single trailing newline, nothing else. All status
text goes to stderr.

## Create

```sh
wt add feature/login                  # create -> and cd into ../myapp-feature-login
wt add feature/login --stay           # create but stay put
git-wt add feature/login --name review    # -> ../myapp-review
git-wt add feature/login --dirname scratch  # -> ../scratch
git-wt add feature/login -p ~/work    # -> ~/work/myapp-feature-login
git-wt add feature/login --from develop   # new branch off develop
git-wt add                            # pick a branch interactively
```

Through the `wt` wrapper, `add` **cd's you into the new worktree** by default;
pass `--stay` to create it without moving. The bare `git-wt` binary always just
prints the new path (for `dir=$(git-wt add feat/x)`).

Directory is a sibling of the repo root, named `<repo-folder>-<branch>`, with
`/`, ` `, `:` and `\` collapsed to `-`. `--name` replaces the suffix only;
`--dirname` replaces the whole leaf; `--dirname` containing `/` is taken as a
path. `--name` and `--dirname` together is an error.

Branch resolution, in order:

1. Local branch exists — check it out
2. `origin/<branch>` exists — create a tracking branch from it
3. Neither — prompt `Branch '<b>' does not exist. Create it from '<from>'?
   [y/N]`, then create from `--from` (default: the branch checked out where you
   ran the command — the current worktree, not the primary)

Create refuses when the target directory already exists, or the branch is
already checked out in another worktree.

### Pick a branch

With no `<branch>`, `add` opens an interactive picker over your local branches:
[`fzf`](https://github.com/junegunn/fzf) for a live search filter when
installed, otherwise a numbered list read from stdin. Branches **already checked
out in a worktree** can't be added again, so they're dropped from the choices
and listed separately under `Already checked out (not selectable)`. If every
branch is checked out, the picker errors instead of offering nothing. Flags
still apply, so `git-wt add --name review` picks a branch and gives it a custom
suffix.

## Remove

```sh
git-wt 2 remove             # prompt, then remove worktree 2
git-wt 2 remove -y          # no prompt
git-wt 2 remove -f          # also discard uncommitted changes
```

Removes the worktree directory and prunes git's admin data; the branch is left
alone. Refuses to remove the main/bare worktree.

On success it prints the main worktree path **only when you were standing inside
the tree you just removed** (your cwd now dangles), so the `wt` wrapper cd's you
back to main. Remove some *other* worktree and it prints nothing — the wrapper
leaves you exactly where you are.

## Command reference (all combinations)

Every form the CLI accepts. Examples assume:

- repo root `~/code/myapp`, current branch `main`, parent `~/code`
- worktrees numbered `1..N` as printed by `git-wt list`
- `wt` = the shell wrapper (cd's); `git-wt` = the binary (prints paths)

### List

| Command | Effect |
|---|---|
| `git-wt` | List all worktrees, numbered from 1 |
| `git-wt list` | Same |
| `git-wt ls` | Alias → same |
| `git-wt list feat` | List fuzzy-filtered by `feat`; **indices stay original** |
| `git-wt list zzz` (no match) | Error `no worktree matches 'zzz'`, exit 1 |
| `git-wt list --col 2,3` | Show only branch + dir columns (1=id, 2=branch, 3=dir) |
| `git-wt list --col 3,2,1 feat` | Reorder columns; combines with a filter |

### Target — `git-wt <N> [action]`

| Command | Effect |
|---|---|
| `git-wt 1` | == `git-wt 1 switch` |
| `git-wt 1 switch` | Print worktree 1's path (wrapper cd's in); alias `cd` |
| `git-wt 1 path` | Print worktree 1's path only, never cd; alias `show` |
| `git-wt 1 remove` | Prompt, then remove |
| `git-wt 1 remove -y` | Remove, no prompt |
| `git-wt 1 remove -f` | Remove, discard dirty (still prompts) |
| `git-wt 1 remove -y -f` | Remove, no prompt, discard dirty |
| `git-wt 1 rm` | Alias → remove |

Through the wrapper: `wt 1` / `wt 1 switch` / `wt 1 cd` cd into it; `wt 1 remove`
cd's back to main; `wt 1 path` / `wt 1 show` only print.

### Create — explicit branch

Leaf = the last path component; full path = `<parent>/<leaf>`.

| Command | Leaf | Full path |
|---|---|---|
| `add feat/x` | `myapp-feat-x` | `~/code/myapp-feat-x` |
| `add feat/x -n test` | `myapp-test` | `~/code/myapp-test` |
| `add feat/x --dirname test` | `test` | `~/code/test` |
| `add feat/x -p P` | `myapp-feat-x` | `P/myapp-feat-x` |
| `add feat/x -n test -p P` | `myapp-test` | `P/myapp-test` |
| `add feat/x --dirname test -p P` | `test` | `P/test` |
| `add feat/x --dirname sub/test` | `sub/test` | `~/code/sub/test` (path; ignores `-p`) |
| `add feat/x --dirname /abs/test` | `/abs/test` | `/abs/test` (absolute; ignores `-p`) |
| `add feat/x --from develop` | `myapp-feat-x` | new branch off `develop` |
| `add feat/x --stay` | `myapp-feat-x` | create but wrapper does NOT cd in |
| `add brandnew` (no such branch) | `myapp-brandnew` | prompts, then creates from current branch |

Through the wrapper, `wt add …` cd's into the new worktree (unless `--stay`);
the `git-wt` binary only prints the path.

### Create — picker (BRANCH omitted)

Opens fzf (or a numbered prompt) over local branches; all flags still apply.

| Command | Leaf | Full path |
|---|---|---|
| `add` | `myapp-<picked>` | `~/code/myapp-<picked>` |
| `add -n test` | `myapp-test` | `~/code/myapp-test` |
| `add --dirname test` | `test` | `~/code/test` |
| `add -p P` | `myapp-<picked>` | `P/myapp-<picked>` |
| `add -n test -p P` | `myapp-test` | `P/myapp-test` |
| `add --dirname test -p P` | `test` | `P/test` |

### Meta

| Command | Effect |
|---|---|
| `git-wt version` / `-V` / `--version` | `git-wt X.Y.Z` |
| `git-wt help` / `-h` / `--help` | Help text |

### Errors & edge cases

| Command | Result |
|---|---|
| `git-wt` outside a repo | `not inside a git repository` |
| `git-wt 0` | `no worktree #0` |
| `git-wt 99` | `no worktree #99; there are N (see 'git-wt list')` |
| `git-wt 1 bogus` | `unknown action 'bogus' (switch, path, remove)` |
| `git-wt 1 switch path` | `too many arguments` |
| `git-wt 1 -n x` | `switch/path/remove take no --name` |
| `git-wt 1 remove` on main/bare | `refusing to remove the main worktree` |
| `git-wt foo` (branch-like: has `/` or `-`) | `unknown command 'foo'; did you mean 'add foo'?` |
| `git-wt lsit` (not branch-like) | `unknown command 'lsit'` |
| `git-wt show 1` (legacy) | `unknown command 'show'; use 'git-wt 1 path'` |
| `git-wt remove 1` (legacy) | `unknown command 'remove'; use 'git-wt 1 remove'` |
| `add feat/x -n a --dirname b` | `--name and --dirname conflict` |
| `add feat/x -n ""` | `--name cannot be empty` |
| `add feat/x --dirname ""` | `--dirname cannot be empty` |
| `add feat/x -n` (no value) | `--name needs a name` |
| `add feat/x -p` (no value) | `--parentdir needs a directory` |
| `add feat/x --from` (no value) | `--from needs a ref` |
| `add feat/x --from bogus` | `bad revision 'bogus'` (from git) |
| `add feat/x` when path exists | `<path> already exists` |
| `add feat/x` branch checked out elsewhere | `branch 'feat/x' already checked out at <path>` |

### Silent-override warnings

When a flag would be silently ignored, `add` warns and asks `[y/N]` (bare Enter,
EOF, or no-tty = No) instead of proceeding quietly:

| Trigger | Prompt |
|---|---|
| `-p` + `--dirname` with `/` | `--parentdir ignored because --dirname is a path. Continue? [y/N]` |
| `--from` + existing branch | `branch 'BRANCH' already exists; --from ignored. Continue? [y/N]` |

Hard conflicts (`--name` + `--dirname`) stay errors — no safe default to confirm.

## Build

`build.sh` does three things: set the version, compile, and bundle **one**
shareable installable file.

```sh
./build.sh          # build at current version
./build.sh 1.2.3    # set version to 1.2.3, then build
./build.sh patch    # bump x.y.Z, then build
./build.sh minor    # bump x.Y.0, then build
./build.sh major    # bump X.0.0, then build
```

The chosen version is written to `Cargo.toml`, so it flows into `--version`.
The release binary lands at `target/release/git-wt`, and the shareable file at:

```
dist/git-wt-<version>-<os>-<arch>.install.sh
```

That is the self-installing script from
[Install → one-file installer](#from-the-one-file-installer-no-rust-needed):
the binary is gzipped and embedded inside it, so a recipient downloads just that
one file and runs it — no repo, no toolchain. Build it once per target platform
you want to share to.

### Build on Linux (Docker)

`./linux-test.sh` builds and runs the unit + live tests in a throwaway Debian
container. `./linux-test.sh --build-install` additionally verifies the one-file
installer end-to-end on Linux. The Docker image's arch matches your host
(Apple Silicon → `aarch64`); for an x86_64 Linux artifact, build with
`docker build --platform linux/amd64 -t git-wt-test .` first.
