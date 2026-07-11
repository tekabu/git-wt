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

Needs Rust (`cargo`).

```sh
./install.sh                 # binary only, no profile changes
./install.sh --alias wt      # binary + a `wt` shell function that cd's for you
```

Runs `cargo install`, dropping the binary in `~/.cargo/bin`. Make sure
`~/.cargo/bin` is on your `PATH`. If another `git-wt` earlier on `PATH` shadows
the installed one, the script warns and suggests removing or symlinking it.

### `git-wt` (binary) vs `wt` (wrapper)

Two names, one tool, **one behavioral difference: `cd`.** A binary cannot change
its parent shell's directory, so the binary always *prints* a path; the `wt`
shell function *cd's* using that path.

|  | `git-wt` (binary) | `wt` (shell function) |
|---|---|---|
| `switch` / bare `N` | prints the path | **cd's** into it |
| `remove` | prints main path | cd's back to main after removal |
| everything else | identical | identical (pass-through) |
| Use for | **scripting** / `$(...)` | **interactive** daily use |

- **Interactive:** run `./install.sh --alias wt`, then use `wt`. `wt 1` drops
  you in the worktree; `wt 1 remove` returns you to main.
- **Scripts:** call `git-wt` directly. `dir=$(git-wt 1 path)`.

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
    --from REF          Base ref for a NEW branch (default: current HEAD)
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
git-wt add feature/login                 # -> ../myapp-feature-login
git-wt add feature/login --name review   # -> ../myapp-review
git-wt add feature/login --dirname scratch   # -> ../scratch
git-wt add feature/login -p ~/work       # -> ~/work/myapp-feature-login
git-wt add feature/login --from develop  # new branch off develop
git-wt add                               # pick a branch interactively
```

Directory is a sibling of the repo root, named `<repo-folder>-<branch>`, with
`/`, ` `, `:` and `\` collapsed to `-`. `--name` replaces the suffix only;
`--dirname` replaces the whole leaf; `--dirname` containing `/` is taken as a
path. `--name` and `--dirname` together is an error.

Branch resolution, in order:

1. Local branch exists — check it out
2. `origin/<branch>` exists — create a tracking branch from it
3. Neither — prompt `Branch '<b>' does not exist. Create it from '<from>'?
   [y/N]`, then create from `--from` (default `HEAD`)

Create refuses when the target directory already exists, or the branch is
already checked out in another worktree.

### Pick a branch

With no `<branch>`, `add` opens an interactive picker over your local branches:
[`fzf`](https://github.com/junegunn/fzf) for a live search filter when
installed, otherwise a numbered list read from stdin. Flags still apply, so
`git-wt add --name review` picks a branch and gives the worktree a custom
suffix.

## Remove

```sh
git-wt 2 remove             # prompt, then remove worktree 2
git-wt 2 remove -y          # no prompt
git-wt 2 remove -f          # also discard uncommitted changes
```

Removes the worktree directory and prunes git's admin data; the branch is left
alone. Refuses to remove the main/bare worktree. On success prints the main
worktree path (so the `wt` wrapper can cd you back — handy when you just removed
the tree you were standing in).

## Build

```sh
./build.sh          # release build at current version
./build.sh 1.2.3    # set version to 1.2.3, then build
./build.sh patch    # bump x.y.Z, then build
./build.sh minor    # bump x.Y.0, then build
./build.sh major    # bump X.0.0, then build
```

The chosen version is written to `Cargo.toml`, so it flows into `--version`.
Release binary lands at `target/release/git-wt`.

Typical release: `./build.sh patch && ./install.sh`.
