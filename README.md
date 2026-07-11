# git-wt

Create git worktrees in a sibling directory named `<repo-folder>-<branch>`.

Installed on PATH as `git-wt`, so it also works as `git wt`.

```
~/code/myapp  +  feature/login  ->  ~/code/myapp-feature-login
```

## Install

Needs Rust (`cargo`).

```sh
./install.sh                 # binary only, no profile changes
./install.sh --alias wt      # binary + a `wt` shell function that cd's for you
```

Runs `cargo install`, dropping the binary in `~/.cargo/bin`. Make sure
`~/.cargo/bin` is on your `PATH`. If another `git-wt` earlier on `PATH` shadows
the installed one, the script warns and suggests removing or symlinking it.

`--alias <name>` installs a shell function of that name into your rc
(`~/.zshrc`, `~/.bashrc`, or `~/.profile`) inside a managed block, refreshed on
reinstall. The function runs `git-wt` for you and `cd`s into the worktree on
create/`show`, and back to the main worktree after `remove` — something a bare
binary cannot do (a child process can't change its parent shell's directory).

## Usage

```
git-wt <branch> [base-ref]   Create a worktree for <branch>
git-wt                       Pick a branch interactively (fzf, else a list)
git-wt list                  List worktrees, numbered from 1
git-wt show <N>              Print worktree #N's path (for cd)
git-wt remove <N|branch>     Remove a worktree; N is from 'list'
git-wt --help
git-wt --version
```

Aliases: `ls` = `list`, `rm` = `remove`, `go` / `cd` = `show`.

### Options

```
-n, --name <dir>   Create: override the worktree directory name
-s, --show         Create: print the new worktree path on stdout (for cd)
-f, --force        Remove: discard uncommitted changes
-h, --help         Show this help
-V, --version      Show version
```

`--name` and `--show` apply to create only; `--force` to remove only.

Prompts appear `[y/N]` when creating a branch that does not exist, and before
`remove`. Type `y` (or `yes`) and press Enter to proceed; anything else —
including a bare Enter — aborts.

## Create

Directory is a sibling of the repo root, named `<repo-folder>-<branch>`, with
`/`, ` `, `:` and `\` collapsed to `-`. `--name` overrides it; a bare name is
still a sibling, a name containing `/` is a path as given.

```sh
git-wt feature/login                       # -> ../myapp-feature-login
git-wt feature/login --name myapp-review
git-wt feature/login --name /tmp/scratch
git-wt feature/login --show                # print the path so a shell can cd
```

Branch resolution, in order:

1. Local branch exists — check it out
2. `origin/<branch>` exists — create a tracking branch from it
3. Neither — prompt `[y/N]`, then create it from `[base-ref]` (default `HEAD`)

Create refuses when:

- the **target directory already exists**, or
- the **branch is already checked out** in another worktree (git shares one
  ref between worktrees, so the two HEADs would drift).

### Pick a branch

With no `<branch>`, git-wt opens an interactive picker over your local
branches. It uses [`fzf`](https://github.com/junegunn/fzf) for a live search
filter when installed, otherwise prints a numbered list and reads a number.

```sh
git-wt          # choose a branch, then it creates the worktree
```

## Remove

```sh
git-wt remove 2                 # by number from 'git-wt list'
git-wt remove feature/login     # by branch (only if a worktree has it)
git-wt remove --force 2         # also discard uncommitted changes
```

Removes the worktree directory and prunes git's admin data; the branch is left
alone. An invalid number, or a branch with no worktree, is an error. It prompts
before removing, and on success prints the **main worktree path** on stdout so
a shell can cd back to it — handy when you just removed the tree you were in.

## cd into a worktree

Only `show <N>`, `create --show`, and `remove` print a path — alone, on stdout
— so you can `cd`. All status text goes to stderr.

```sh
cd "$(git-wt show 2)"
cd "$(git-wt feature/login --show)"
cd "$(git-wt remove 2)"          # back to the main worktree
```

The easiest way is to let `install.sh --alias <name>` install a shell function
that does the `cd` for you (see [Install](#install)). It is equivalent to:

```sh
wt() {
  case "${1:-}" in
    -h|--help|-V|--version|list|ls|-l|--list) git-wt "$@"; return $? ;;
    show|go|cd|remove|rm) local d; d="$(git-wt "$@")" || return $?; [ -n "$d" ] && cd "$d" ;;
    *) local d; d="$(git-wt --show "$@")" || return $?; [ -n "$d" ] && cd "$d" ;;
  esac
}
```

A binary cannot change its parent shell's directory, so the `cd` must live in
a shell function like this.

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
