# git-wt

Create and manage git worktrees in sibling directories named
`<repo-folder>-<branch>`.

Installed on PATH as `git-wt`, so it also works as `git wt`.

```
~/code/myapp  +  feature/login  ->  ~/code/myapp-feature-login
```

## What this gives you (the non-technical version)

Ever needed to quickly check something on another branch, fix an urgent bug, or
try an experiment — but your current work isn’t ready to commit? Normally you
have to stop, save everything, switch branches, and remember where you left off.

`git-wt` gives you **separate project folders for each branch**, all sharing one
Git history so nothing is duplicated. Jump between them instantly, keep your
main work untouched, and throw away experiments safely.

```
Before git-wt (one folder, lots of switching)
┌─────────────────┐
│  Your project   │  switch to bugfix ──► save changes in stash
│     folder      │  switch back     ──► restore stash
│                 │  switch again    ──► "wait, where was I?"
└─────────────────┘

After git-wt (a folder per branch)
┌───────────────┐      ┌──────────────────┐      ┌─────────────────┐
│     myapp     │◄────►│ myapp-bugfix-123 │      │ myapp-try-new-ui│
│  (main work)  │ add  │  (urgent fix)    │ jump │  (experiment)   │
│               │◄────►│                  │◄────►│                 │
└───────────────┘ rm   └──────────────────┘      └─────────────────┘

Replace branch switching with separate folders next door.
```

### What you can do

| You want to… | Just type | What happens |
|---|---|---|
| Start a new task on its own branch | `wt add feature/login` | A new folder appears next door, already on that branch |
| Jump back to another task | `wt 1` | Your terminal moves to that task’s folder |
| Peek at a branch without disturbing your current work | `git-wt add bugfix/123 --stay` | The folder is created; you stay where you are |
| Fold one task's work into another | `wt 1 merge 2` | Task 2's branch is merged into task 1's, without leaving your shell |
| Clean up a finished task | `wt 1 remove` | The extra folder disappears; the branch stays in Git |

No more stashing, no more “wait, which branch was I on?”, no more half-finished
work blocking a quick fix.

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
git-wt <N> merge <M|BRANCH>  Merge M (or BRANCH) into worktree N
git-wt <N>,<M> merge         Same thing: merge M into N
git-wt <N> merge continue|abort
git-wt <N>,<M> diff [flags]  Diff worktree N against worktree M
git-wt <N>,<N>[,<N>] meld    Diff 2-3 worktrees side by side in meld
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

## Diff

Diff two worktrees in the terminal, through git's own pager:

```sh
git-wt 1,2 diff             # git diff <branch 1>..<branch 2>
git-wt 1,2 diff ...         # git diff <branch 1>...<branch 2>
git-wt 1,2 diff --name-only
git-wt 1,2 diff --stat -- src/
```

It compares the two worktrees' **branches**, not their directories — a directory
diff would drag in `target/`, `node_modules` and everything else `.gitignore`
exists to hide. Detached worktrees diff by HEAD sha.

`..` (the default) is everything that differs between the two. `...` is only what
worktree M added since it forked from worktree N — the review view, which hides
N's own newer commits.

| Flag | Shows |
|---|---|
| `--name-only` | File names |
| `--name-status` | File names with `A`/`M`/`D` |
| `--stat` | File names with a churn summary |
| `-- PATH...` | Limit to those paths |

That is the whole flag set on purpose. Anything else git diff can do, get from
git diff — the error for an unknown flag prints the exact `git diff <A>..<B>`
command to run instead.

Because the comparison is committed state, uncommitted work is invisible to it.
When either side is dirty, git-wt says so on stderr and points you at `meld`,
which does compare working trees.

## Meld

Compare worktrees side by side in [meld](https://meldmerge.org/):

```sh
git-wt 1,3 meld             # 2-way: meld <dir 1> <dir 3>
git-wt 2,1,3 meld           # 3-way: meld <dir 2> <dir 1> <dir 3>
```

The list order is the pane order, so `2,1,3` puts worktree 2 on the left. Takes
2 or 3 worktrees — meld itself decides 2-way vs 3-way from how many it gets.
git-wt waits until you close meld; background it with `&` if you'd rather not.

Requires `meld` on PATH; without it you get an error and an install hint
(`brew install --cask meld`, `apt install meld`, `dnf install meld`) rather than
a silent no-op. Listing the same worktree twice is refused — comparing a
directory against itself is never what you meant.

## Merge

```sh
git-wt 1 merge 2            # worktree 2's branch -> worktree 1's branch
git-wt 1,2 merge            # the same thing, list-style like meld
git-wt 1 merge feat/x       # a branch name works too
git-wt 1 merge 2 dry-run    # would it conflict? nothing is touched
git-wt 1 merge 2 theirs     # let 2 win every collision
```

The merge runs **inside worktree N**, so N's branch is the one that moves — the
source is only read. `git-wt 1 merge 2` reads as "worktree 1, merge 2 into you",
and you never have to `cd` anywhere to do it.

The source can be a worktree number or any branch/ref. A number that names a
worktree wins over a branch of the same name.

### Two ways to name the targets

`git-wt 1,2 merge` is the list form, spelled like `meld`'s. Both forms read
**dest-first**, so these are identical:

```sh
git-wt 1 merge 2     # spelled out
git-wt 1,2 merge     # list-style
```

Options work the same either way — `git-wt 1,2 merge theirs dry-run`.

Two differences from `meld`, both because a merge is directional:

- The list takes **exactly two** worktrees. `meld` diffs 2–3; a merge has one
  destination and one source, so `git-wt 1,2,3 merge` is an error.
- Both sides are worktree **numbers**. To merge a branch that has no worktree,
  use the spelled-out form: `git-wt 1 merge feat/x`.

The list already names the source, so it can't be combined with
`continue`/`abort` — those take a single target (`git-wt 1 merge continue`), and
asking otherwise says so.

### Words and options

The five verb-ish words take an **optional `--`**, plus a short form — `abort`,
`--abort` and `-a` are the same thing:

| Word | Short | Does |
|---|---|---|
| `continue` | `-c` | Conclude a conflicted merge |
| `abort` | `-a` | Undo a conflicted merge |
| `ours` | `-o` | On a conflicting hunk, keep worktree N's side |
| `theirs` | `-t` | On a conflicting hunk, take the source's side |
| `dry-run` | `-d` | Report whether it would merge; change nothing |

These words win over a branch of the same name, so to merge a branch actually
called `theirs`, spell it `heads/theirs`.

The flags that mirror git's own spelling keep their dashes, so muscle memory
carries over:

```
-m, --message MSG      Merge commit message
    --no-ff            Always create a merge commit
    --ff-only          Refuse anything but a fast-forward
    --squash           Stage the merge without committing
-f, --force            Merge even when worktree N has uncommitted changes
```

### `ours` / `theirs`

These are git's `-X` **strategy options**, so they settle only the hunks that
actually collide — every non-conflicting change from both sides still merges.
They are deliberately not `-s ours`, which would drop the source's changes
wholesale and still record a merge that claims to have taken them.

```sh
git-wt 1 merge 2 theirs   # collisions resolve to 2's side
git-wt 1 merge 2 ours     # collisions resolve to 1's side
```

A side is chosen while the merge is **computed**, so it cannot be bolted onto a
merge that has already stopped — git offers no way to do that. Ask for one
anyway and `git-wt` explains, then offers the only route that honors it:

```
$ git-wt 1 merge 2 theirs
A merge is already in progress in /Users/me/code/myapp, and 'theirs' only applies when a merge starts.
Abort it and re-merge '2' with 'theirs'? Any conflict resolution already done there is discarded. [y/N]
```

Answering `n` leaves the stopped merge exactly as it was. The prompt exists
because the redo discards resolution work — and if the merge was started with
`-f` over a dirty tree, the abort unwinds that uncommitted work too.

The tempting shortcut while stuck, `git checkout --theirs -- <file>`, is a
**different and lossier** operation: it takes the source's *whole file*,
throwing away your own non-conflicting edits to it. Abort-and-redo keeps them.

### `dry-run`

Answers "would this merge?" without touching anything — no index, no checkout,
nothing to clean up. It uses `git merge-tree --write-tree` (**git 2.38+**),
which resolves the merge in memory.

```sh
git-wt 1 merge 2 dry-run
```

It exits 0 when clean and 1 when it would conflict, so it drives a script:

```sh
if git-wt 1 merge 2 dry-run; then git-wt 1 merge 2; fi
```

`dry-run` takes none of the flags that need a real merge to run — `-m`,
`--no-ff`, `--squash`, `--ff-only`, `-f`. Some shape the resulting commit and
there is no commit; `--ff-only` and `-f` gate whether the merge may run at all,
and nothing runs. (In particular `merge-tree` resolves in memory and never
fast-forwards, so `--ff-only` could not be honored even in principle.) A side is
allowed, since it changes the answer.

Merges never open an editor: without `-m`, git's default message is taken as-is.
A destination with uncommitted changes to **tracked** files is refused without
`-f`, since a merge into uncommitted work can leave your own edits tangled in
conflict markers. Untracked files don't count — git refuses on its own rather
than clobber one — so a tree that is merely untracked-dirty merges fine.

A **detached** worktree is a valid destination: the merge lands on the detached
HEAD, exactly as `git merge` run there would. Only a bare worktree is refused.

### Conflicts

A conflict exits 1 and names the files:

```
$ git-wt 1 merge 2
error: merge conflict in /Users/me/code/myapp
  src/auth.rs
hint: resolve them there, 'git add' each, then 'git-wt 1 merge continue'
hint: or undo the merge with 'git-wt 1 merge abort'
hint: or redo it letting one side win: 'git-wt 1 merge abort', then 'git-wt 1 merge <M|BRANCH> theirs'
```

Fix the files in worktree N, `git add` them, then:

```sh
git-wt 1 merge continue   # conclude it ('--continue' and '-c' also work)
git-wt 1 merge abort      # or undo the whole merge
```

Both also accept the bare words `continue` / `abort`. They take no source and no
merge options — those were settled when the merge started. `--continue` with
unresolved files re-prints the conflict list rather than failing obscurely.

Merge prints nothing on stdout; all status goes to stderr.

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
| `git-wt 1 merge 2` | Merge worktree 2's branch into worktree 1's |
| `git-wt 1,2 merge` | The same, list-style (exactly two, dest first) |
| `git-wt 1 merge feat/x` | Merge branch `feat/x` into worktree 1's branch |
| `git-wt 1 merge 2 --no-ff -m "sync"` | Merge commit with a message |
| `git-wt 1 merge 2 --squash` | Stage the merge, don't commit |
| `git-wt 1 merge 2 -f` | Merge even though worktree 1 is dirty |
| `git-wt 1 merge 2 theirs` | Merge, letting 2 win every collision (`-X theirs`) |
| `git-wt 1 merge 2 ours` | Merge, letting 1 win every collision (`-X ours`) |
| `git-wt 1 merge 2 dry-run` | Report whether it would merge; change nothing |
| `git-wt 1 merge continue` | Conclude a conflicted merge (`--continue`, `-c`) |
| `git-wt 1 merge abort` | Undo a conflicted merge (`--abort`, `-a`) |

### Diff — `git-wt <N>,<M> diff [flags]`

| Command | Effect |
|---|---|
| `git-wt 1,2 diff` | `git diff <branch 1>..<branch 2>` — everything that differs |
| `git-wt 1,2 diff ..` | Same, spelled out |
| `git-wt 1,2 diff ...` | `git diff <branch 1>...<branch 2>` — only 2's own commits |
| `git-wt 1,2 diff --name-only` | File names only (also `--name-status`, `--stat`) |
| `git-wt 1,2 diff -- src/` | Limit to `src/`; combines with the flags above |
| `git-wt 1,1 diff` | Error `worktree #1 against itself is always empty` |
| `git-wt 1 diff` | Error — `diff` takes a worktree list |
| `git-wt 1,2 diff -w` | Error — unknown flag, with the `git diff` command to run instead |
| `git-wt 1,2,3 diff` | Error — `diff` takes exactly two worktrees; `meld` compares three |

### Multi-target — `git-wt <N>,<N>[,<N>] meld`

| Command | Effect |
|---|---|
| `git-wt 1,2 meld` | 2-way diff of worktrees 1 and 2, in that pane order |
| `git-wt 2,1,3 meld` | 3-way diff; worktree 2 on the left |
| `git-wt 1 meld` | Error `meld needs 2 or 3 worktrees` |
| `git-wt 1,2,3,4 meld` | Error `meld takes at most 3 worktrees, got 4` |
| `git-wt 1,1 meld` | Error `worktree #1 listed twice` |
| `git-wt 1,2 remove` | Error — only `diff` and `meld` take a list |

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
| `git-wt 1 bogus` | `unknown action 'bogus' (switch, path, remove, merge)` |
| `git-wt 1 switch path` | `too many arguments` |
| `git-wt 1 -n x` | `switch/path/remove take no --name` |
| `git-wt 1 remove` on main/bare | `refusing to remove the main worktree` |
| `git-wt foo` (branch-like: has `/` or `-`) | `unknown command 'foo'; did you mean 'add foo'?` |
| `git-wt lsit` (not branch-like) | `unknown command 'lsit'` |
| `git-wt show 1` (legacy) | `unknown command 'show'; use 'git-wt 1 path'` |
| `git-wt remove 1` (legacy) | `unknown command 'remove'; use 'git-wt 1 remove'` |
| `git-wt merge 2` (target missing) | `unknown command 'merge'; use 'git-wt 1 merge 2' or 'git-wt 1,2 merge'` |
| `git-wt 1,2,3 merge` | `merge takes exactly two worktrees, not 3` |
| `git-wt 1,2 merge continue` | `'continue' takes no source, so a worktree list has nothing to name` |
| `git-wt 1,x merge` | `bad worktree list '1,x'; want numbers, e.g. '1,2'` |
| `git-wt 1,2` (no action) | `a worktree list needs an action, e.g. 'git-wt 1,2 meld'` |
| `git-wt 1 merge` | `merge needs a source: 'git-wt <N> merge <M\|BRANCH>', or continue/abort` |
| `git-wt 1 merge zzz` | `no worktree or branch 'zzz' (see 'git-wt list')` |
| `git-wt 1 merge 1` | `'main' is already checked out in worktree 1` |
| `git-wt 1 merge 2` (worktree 1 dirty) | `worktree 1 has uncommitted changes` + `-f` hint |
| `git-wt 1 merge 2` (merge in progress) | `a merge is already in progress` + continue/abort hint |
| `git-wt 1 merge 2 theirs` (merge in progress) | Explains, then prompts to abort and redo `[y/N]` |
| `git-wt 1 merge continue` (none started) | `no merge in progress in <path>` |
| `git-wt 1 merge continue 2` | `continue takes no argument (got '2')` |
| `git-wt 1 merge 2 ours theirs` | `ours and theirs conflict` |
| `git-wt 1 merge theirs continue` | `continue takes no merge options` + why, and the abort/redo route |
| `git-wt 1 merge 2 dry-run --no-ff` | `dry-run takes no merge options` |
| `git-wt 1 merge 2 dry-run` (git < 2.38) | `dry-run needs git 2.38 or newer (git merge-tree --write-tree)` |
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
