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
| Fold one task's work into another | `wt 1,2 merge` | Task 2's branch is merged into task 1's, without leaving your shell |
| Check what a merge would bring first | `wt 1,2 merge --review` | The commits about to land, and whether they'd conflict — nothing is changed |
| See which tasks have which commits | `wt 1,2,3 commits` | A table: one row per commit, a check under every task that has it |
| Catch every task up with the server | `wt pull --all` | Each folder pulls its own branch; the last line counts what worked |
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

#### Installing Rust and cargo

If `cargo --version` fails, you don't have Rust yet. The official installer is
[rustup](https://rustup.rs), which puts both `rustc` and `cargo` in
`~/.cargo/bin`:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

macOS/Linux only. On Windows, download and run
[`rustup-init.exe`](https://rustup.rs) instead.

Accept the default (`1`) when it asks; it edits your shell rc so `~/.cargo/bin`
is on `PATH`. Either open a new shell or source it now:

```sh
. "$HOME/.cargo/env"
cargo --version              # confirm
```

**rustup does not install a linker.** Rust links through the system C toolchain,
so a fresh machine with only rustup fails at the last step of the first build:

```
error: linker `cc` not found
```

Install the platform's build tools once:

| Platform | Command |
|---|---|
| Debian/Ubuntu | `sudo apt install -y build-essential` |
| Fedora/RHEL | `sudo dnf groupinstall "Development Tools"` |
| Arch | `sudo pacman -S base-devel` |
| Alpine | `sudo apk add build-base` |
| macOS | `xcode-select --install` |

Only the source install needs this — the [one-file installer](#from-the-one-file-installer-no-rust-needed)
ships a prebuilt binary and needs no toolchain at all.

Package managers work too — `brew install rust`, `apt install cargo`,
`dnf install cargo` — but they pin whatever version the distro ships, and
updating means waiting on them. `rustup` updates on your word:

```sh
rustup update                # newest stable
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
git-wt <N>,<M> merge         Merge M into N
git-wt <N> merge <BRANCH>    Merge BRANCH into worktree N
git-wt <N>,<M> merge review  What would that merge bring over?
git-wt <N> merge continue|abort
git-wt <N>,<M> merged        Is M's branch already in N's branch?
git-wt <N> merged <BRANCH>   Is BRANCH already in worktree N's branch?
git-wt <N> merged            Is N's branch already in the current branch?
git-wt <N>,<M> diff [flags]  Diff worktree N against worktree M
git-wt <N>[,<M>...] commits  Table: which commit is on which branch
git-wt <N> commits           One worktree's own log, nothing compared
git-wt <N>,<N>[,<N>] meld    Diff 2-3 worktrees side by side in meld
git-wt <N> fetch|pull|push   Run it in worktree N
git-wt <N>,<M> pull          Run it in each worktree listed
git-wt fetch|pull|push --all Run it in every worktree
git-wt add [BRANCH] [flags]  Create a worktree (picker when BRANCH omitted)
git-wt version
git-wt --help
```

Aliases: `ls` = `list`, `rm` = `remove`, `cd` = `switch`, `show` = `path`.

### Branch names instead of numbers

Anywhere `<N>` or a `<N>,<M>` list appears above, a worktree may be named by
the branch it holds instead of its number, and the two spellings mix freely:

```
git-wt main commits
git-wt main,2 diff
git-wt main,feat/login merge
git-wt main,feat/login,feat/api commits
```

The branch has to be checked out in a worktree — a list action diffs, melds or
sweeps real directories, so a branch nobody has checked out has no path to
give. To merge such a branch, name it in the single-target form instead:
`git-wt 1 merge some-branch`.

A bare number is always read as a worktree number, even when a branch shares
that name; write `heads/2` for a branch that is itself called `2`. This is the
same rule `merge` and `merged` already follow.

A command word likewise wins over a branch of the same name: `git-wt list` is
the listing, never a worktree on a branch called `list`. Reach that one as
`git-wt heads/list`.

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

`--col` picks and orders columns — `1`=id, `2`=branch, `3`=dir (full path),
`6`=merged (relative to the branch you are standing in):

```sh
git-wt list --col 2,3        # branch + path, no id
git-wt list --col 1,2        # id + branch, no path
git-wt list --col 2          # branch only
git-wt list --col 3,2,1      # reversed
git-wt list --col 1,2,6      # id + branch + merged status
git-wt list --col 1,2 feat   # combine with a filter
```

### `--files`

`--files` (`-f`) prints a file block under each worktree row, in the same shape
`git-wt <N>,<M> commits --files` prints under a commit: status, path, added
lines, removed lines. **Every worktree that is not clean gets one** -- staged and
unstaged changes counted together, untracked files listed with `?` (ignored ones
never are). A clean worktree keeps its row and adds no block, so the flag reads
as "show me what is in flight, everywhere".

```
1  main                  /code/myapp

2  feat/files-flag       /code/myapp-feat-files-flag

	?  scratch.txt   +2   -0
	M  src/list.rs  +19   -2

3  feat/windows-support  /code/myapp-feat-windows-support

	?  docs/PLAN.md  +130   -0
```

It combines with everything else `list` takes:

```sh
git-wt --files                # the default listing, plus the file blocks
git-wt list --files           # same
git-wt ls -f feat             # combines with a filter
git-wt list --col 2 --files   # and with --col
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
git-wt 1,2 diff             # git diff <branch 1>...<branch 2>
git-wt 1,2 diff ..          # git diff <branch 1>..<branch 2>
git-wt 1,2 diff --name-only
git-wt 1,2 diff --stat -- src/
```

It compares the two worktrees' **branches**, not their directories — a directory
diff would drag in `target/`, `node_modules` and everything else `.gitignore`
exists to hide. Detached worktrees diff by HEAD sha.

`...` (the default) is only what worktree M added since it forked from worktree N
— the review view, and exactly what `git-wt N,M merge` would bring in. `..`
compares the two tips instead: it also reports N's own newer commits, inverted,
as if M had deleted them, which on diverged branches reads as a huge diff that no
merge would ever apply.

| Flag | Shows |
|---|---|
| `--name-only` | File names |
| `--name-status` | File names with `A`/`M`/`D` |
| `--stat` | File names with a churn summary |
| `-- PATH...` | Limit to those paths |
| `live` | Compare the files on disk, not the commits (see below) |
| `hunks` | Each file's changed line numbers |

That is the whole flag set on purpose. Anything else git diff can do, get from
git diff — the error for an unknown flag prints the exact `git diff <A>...<B>`
command to run instead.

Because the comparison is committed state, uncommitted work is invisible to it.
When either side is dirty, git-wt says so on stderr and points you at `live`.

## Diff live — `git-wt <N>,<M> diff live`

`live` compares the **literal bytes on disk** instead of the commits:

```sh
git-wt 1,2 diff live        # what actually differs between the directories
git-wt 1,2 diff live hunks  # + the changed line numbers
```

This is the answer to the case no ref diff can reach: two worktrees on the same
commit, one of them dirty. `git diff <a>..<b>` there is *provably* empty — both
refs resolve to the same tree — while the directories differ by hundreds of
lines. One git process has one working tree, so no single `git diff` can ever
show both worktrees' uncommitted work.

```
diff main ↔ merge   live — literal contents, .gitignore honored

M README.md     +90  −10
M src/main.rs   +345 −38
M test.sh       +73  −4

3 files changed, 508 insertions(+), 52 deletions(-)
```

`.gitignore` is honored — the candidate paths come from `git ls-files --cached
--others --exclude-standard` on both sides, so `target/` never enters. Paths are
byte-compared first, and only the survivors cost a `git diff --no-index`.

`hunks` adds each file's changed line numbers, on the `+` side (worktree M),
which is the side you'd jump to:

```
M README.md     +90 −10
      119  modified 1
      290  added 2
```

`live` takes no range: `..`/`...` compare commits, which is the opposite
question, so combining them is an error rather than a silently ignored word.
`--name-only`, `--name-status`, `--stat` and `-- PATH...` all still apply, and
`hunks` works without `live` — line numbers are just as useful against commits.

Two blind spots, both shared with plain git: ignored-but-differing files stay
invisible, and `live` compares exactly two worktrees (`git diff --no-index`
takes two paths, and there is no three-path form). For three, use `meld`.

## Commits

The first worktree's log, counter-checked against any number of others:

```sh
git-wt 1,2,3 commits    # worktree 1's log; 2 and 3 are check columns
git-wt 2 commits        # worktree 2's own log, nothing compared
```

A single target is that worktree alone: its whole log, no check columns, and
nothing it has to be ahead of — for when the question is about one branch's
history rather than two branches' difference. Name a second worktree and the
comparison comes back.

```
commit   author  date        main  feat/login  bugfix-123  subject
a1b2c3d  Nino    2026-09-15   ✓        ✓           ·       fix token expiry
9f8e7d6  Jhon    2026-04-02   ✓        ·           ·       🚀 add oauth scopes
4c5d6e7  Nino    2026-01-31   ✓        ✓           ✓       bump serde
```

The rows are exactly `git log --oneline <first worktree's branch>` — sha,
author, date — then one column per worktree, checked where that branch has it
too, and the subject last. This is the question
`diff` cannot answer: `diff` compares exactly two branches, and compares their
*content*. `commits` compares any number, by *commit*, so "who already has the
oauth fix?" is one glance instead of three `merged` calls.

Authors come from `%aN`, so a `.mailmap` is honored and a contributor who has
committed under two names or addresses shows up as one. Dates are *author*
dates — when the work was written, not when it landed here.

**Ancestry outranks the dates.** A parent is never listed above its child, no
matter what the two dates say, so the history you read down the table is the
real one. The date only orders commits that don't descend from each other. A
consequence worth knowing: if a commit was authored before its own parent —
a rebase, a cherry-pick, or just a laptop with a wrong clock — the date column
will look out of order at that row. It isn't; the story is.

The subject sits last for a reason worth knowing: it is the only free-form cell,
and an emoji like 🚀 occupies two terminal columns while being a single
character. Any column padded to a "length" measured in characters would shift
by one on exactly those rows. Nothing is padded after the subject, so the marks
line up whatever anyone puts in a commit message — no Unicode width tables, and
no dependencies.

The table is one-sided on purpose: naming another worktree adds a **column,
never a row**, so the rows don't move when you add one. A commit only worktree 2
carries is no row at all — it's worktree 2's business until it lands. Read the
default as *what of mine has landed elsewhere*.

### The other question: `--union`

`--union` asks *who is out of sync with who*. Every worktree
listed contributes rows, so the table becomes the union of their logs and a
commit the first one lacks gets a row with a `·` under it:

```sh
git-wt 1,2 commits -n 20      # newest 20 rows
git-wt 1,2,3 commits --union  # every branch's commits as rows
```

The tradeoff is that rows grow with each worktree you name — the same filter on
`1,2` and on `1,2,3` will not print the same commits, because the third branch
brought its own. Anchored, the rows are stable.

`--union` does not cut at the fork point: rows are whole logs, shared history
included, and a row checked in every column is the answer *everyone has this*
rather than noise. The default view does cut — it stops at the first branch's
earliest divergent commit, keeping the shared commits above that floor — and
`--all` turns the cut off for the first branch's whole log. `-n` keeps any of
them short.

### Spelling the date

The date column is **ISO** by default — the same shape the filters take, so a
date you read off the table pastes straight back into `--date-since`. It also
sorts, greps, and is one width on every row.

```sh
git-wt 1,2 commits                          # 2026-01-31
git-wt 1,2 commits --time              # 2026-01-31 14:30:05
git-wt 1,2 commits --date-human             # Jan. 31, 2026
git-wt 1,2 commits --date-human --time # Jan. 31, 2026 14:30:05
```

`--date-human` is easier to read a date *out* of; the cost is the round-trip,
since it isn't what `--date-since` accepts. What `--date` compares never changes
shape, whatever the column is spelled as — the two are independent.

### Writing a report

```sh
git-wt 1,2 commits --md              # -> commits_2026-07-17_14-30-05.md
git-wt 1,2 commits --md report.md    # -> that path
git-wt 1,2,3 commits --merges --md      # filters apply as usual
```

`--md` writes the table as markdown instead of printing it. The file records
the command that produced it, so a report pasted into an issue says how to
reproduce itself:

```markdown
# git-wt commits

- Command: `git-wt 1,4 commits -n 3 --md`
- Worktrees: `main`, `live-diff`
- Commits: 3

| commit | author | date | main | live-diff | subject |
|---|---|:-:|:-:|---|
| `b1eb4c5` | Nino | 2026-07-17 | ✓ | · | build: bump version to 1.2.4 |
```

`merge --review --md` writes through the same code, and says so: the heading
reads `# git-wt merge --review` and the label is `Merging:` rather than
`Worktrees:`, since a review's subject is one branch coming over rather than a
set of worktrees being compared.

The default name is stamped to the second, so a re-run never eats the last
report; a name you pass is yours, and is overwritten. The path is optional, so
a flag may follow `--md` — `commits --md --topo` writes the default name *and*
groups by branch, rather than saving to a file called `--topo`.

Subjects are whole in a file — no right edge to run out of, so nothing is
truncated — and a `|` in a subject is escaped rather than left to end the cell
and shift every column after it.

### Newest last

```sh
git-wt 1,2 commits --reverse        # alias: --oldest-first
```

Applied after the `-n` cap, so `-n 10 --reverse` is the same ten commits as
`-n 10`, read bottom-up — not the ten *oldest*.

### Merge commits

```sh
git-wt 1,2 commits --merges
```

Merge commits carry no work of their own, and on a branch that merges often
they're most of the table — so they're dropped by default. The commits they
joined all stay either way: only the merge's own row goes, and the marks are
untouched. `--merges` puts those rows back.

`--no-merges` does not exist **here** — it named the drop back when merges were
kept by default, so in `commits` it has nothing left to ask for. Typing it is an
unknown-argument error, like any flag that was never there.

It is a real flag under [`merge --review`](#--review), where the default is the
other way round: a review range is bounded by the merge about to happen, so a
merge inside it is the cargo. The guard follows the actual default rather than
being unconditional, so the message never claims a drop that isn't happening.

### Filtering the rows

Filters narrow which commits are listed; the columns stay whatever your
worktree list named. They AND together, and `-n` counts what survives them.

```sh
git-wt 1,2 commits --author nino                    # fuzzy, like list's SEARCH
git-wt 1,2 commits --date-since 2026-01-01          # that day and after
git-wt 1,2 commits --date-since 2026-01-01 --date-until 2026-06-30
git-wt 1,2 commits --date 2026-01-31                # exactly that day
git-wt 1,2 commits --commit-since 5568a21           # 5568a21's day, and after
git-wt 1,2 commits --commits af48509,f9e2427        # just these rows
git-wt 1,2 commits --message oauth                  # subject or body
git-wt 1,2 commits --filename api.php               # commits touching that path
```

Two vocabularies, one shape. The `--commit-` bounds take a **commit** — a sha, a
branch, a tag, `HEAD~3` — and the `--date-` bounds take a **YYYY-MM-DD**. Both
end up as the same date filter: a commit bound is read for its *day*, nothing
more.

| Flag | Means |
|---|---|
| `--author NAME` | Only NAME's commits; fuzzy subsequence, case-folded (`nes` → `Nino Escalera`) |
| `--date D` (`-d`) | Commits on exactly day D |
| `--date-since D` | Day D and after |
| `--date-until D` | Day D and before |
| `--commit-since C` | The day C was authored, and after |
| `--commit-until C` | The day C was authored, and before |
| `--commits A,B` (`-c`) | Only these commits, named by sha |
| `--message TERM` (`-m`) | Only commits with TERM in the subject **or** the body; plain substring, case-folded |
| `--filename TERM` | Only commits touching a path containing TERM, case-folded |

**A lower bound widens the rows, an upper bound does not.** The default rows
are cut at the bottom, at the earliest divergent commit — so `--commits`,
`--date`, `--date-since` and `--commit-since` imply `--all`, because what they
name can sit below that cut. `--date-until`/`--commit-until` only trim the top,
which the rows already end at, so they stay a post-filter over the default
slice. A range widens through its lower bound, and `--author` never widens.
When a filter keeps nothing, the message names the flags that reach further
back.

**A filter highlights what it read.** A filtered table is all matches by
definition, so the color says *where* the answer lives rather than which rows
survived. It follows the flag you typed, not the filter it became: `--author`
lights the author column, `--date`/`--date-since`/`--date-until` light the date
column, and `--commits`/`--commit-since`/`--commit-until` light only the sha of
the commit they name — the row that was asked for, as opposed to the rows that
merely fall on the right side of it. A commit bound *is* a date bound
underneath, but you named a commit, so the date column stays dim. Bold amber — yellow is the family the eye
finds first, and plain yellow is already spent on `≈`.

**Both ends include what they name.** `--commit-since 5568a21` takes that
commit's whole day, and `--date-since 2026-01-01` takes that whole day. That's
why there's no `>` or `<`: the day either side of a bound is just the inclusive bound next
door, and a strict comparison would be a second spelling of the same thing —
costing a character the shell wants for itself.

### Searching the message — `--message`

```sh
git-wt 1,2 commits --message oauth   # alias: -m
```

A plain substring, case-folded, matched against the subject **and** the body —
a term you remember from a commit is as likely to be in the explanation as in
the one line summarizing it. Not the fuzzy subsequence `--author` uses: a name
is one word typed from memory, where a message is prose, and a subsequence over
prose matches nearly all of it.

When the match is in the body, the matching lines are printed under the row —
otherwise the table would assert a match it never shows. `--message` also
implies `--wrap full`, since a row kept for a word past the terminal's cut has
to show that word.

```
commit   author  date        main  feat  subject
a1b2c3d  Nino    2026-09-15   ✓     ·    fix token expiry

                                         Refresh tokens were compared with the
                                         oauth scope table before expiry.
```

### Searching the files — `--filename`

```sh
git-wt 1,2 commits --filename api.php               # rows that touched it
git-wt 1,2 commits --filename api.php --all-files   # + every other file they touched
```

Keeps only commits touching a path that contains the term, case-folded. It
implies `--files`, for the same reason `--message` shows body lines: a row kept
for a file it touched has to name that file. The matched paths are highlighted
where they sit.

**The block is cut to the matches.** A merge can carry a hundred files and match
on three, and the whole list buries the answer you asked for. `--all-files`
widens it back to everything each commit touched — which is also the only way
the `+`/`-` counts sum to the commit again.

```
$ git-wt 1 commits --filename Cargo.toml -n 1
commit   author        date  subject
c43f151  Nino    2026-07-19  fix: only a lower bound widens the rows

	M  Cargo.toml  +1  -1

$ git-wt 1 commits --filename Cargo.toml -n 1 --all-files
commit   author        date  subject
c43f151  Nino    2026-07-19  fix: only a lower bound widens the rows

	M  Cargo.lock                +1   -1
	M  Cargo.toml                +1   -1
	M  README.md                +10   -0
	M  src/cmd/commits/args.rs  +32  -17
	M  src/cmd/commits/mod.rs   +19   -1
	M  src/main.rs               +8   -5
	M  test.sh                  +46   -0
```

Merges are the reason this is not just `git log -- <path>`: git prunes merges
from a pathspec walk, so a merge that brought a whole feature in would list none
of its files. `commits` keeps them and attributes each merge's files against its
first parent.

### How many branches can it compare?

No cap — unlike `diff` (exactly two) and `meld` (two or three), `commits` takes
as many columns as you name. Comparing all six of your worktrees at once is a
supported thing to type.

Your **terminal** is the real limit. Each column costs its branch name plus two
spaces, and the fixed columns (sha, author, date) take about 35 more. Once a
row no longer fits, the subject wraps to the next line — the marks never do,
since they sit to its left. So a too-wide table degrades into a shaggy right
edge rather than a broken grid.

Rough guide at 100 columns, with names about 10 characters:

| Columns | Fits at 100 cols? |
|---|---|
| 2–3 branches | Comfortably, with room for the subject |
| 4–5 branches | Yes; the subject gets short |
| 6+ branches | The subject wraps — use `--col`-style narrowing: name fewer worktrees, or widen the terminal |

Cost in git calls is linear and cheap: one `git log` for the rows, plus one
`rev-list` per column.

> **No operators anywhere.** `--date` takes one exact day and nothing else;
> `>=`/`<=`/`=` are refused with a pointer to `--date-since`/`--date-until`,
> which say which end they mean. Nothing here needs quoting against the shell,
> which is the point: an unquoted `>` is a redirect, and the date never arrives.

`--date` and its bounds compare the date the table prints, the **author** date.
git's own `--since`/`--until` filter on *committer* dates and would quietly
disagree with the column, so git-wt does the comparison itself. Those two
spellings are not flags here — use `--date-since`/`--date-until`.

**Days and times.** The column shows a day, but the rows are ordered by the
full timestamp — commits from one day sort by time of day, so a busy afternoon
reads in the order it happened even though every row says `2026-07-17`. Add
`--time` when you need to see why:

```
commit    author         date                 uat  main  subject
4ddb114   kinlie         2026-07-17 21:00:00   ✓    ·    ...
c8eed92   jhoriz.aquino  2026-07-17 17:00:00   ·    ✓    ...
4bcb71a   kinlie         2026-07-17 13:00:00   ✓    ·    ...
241a891   nino           2026-07-17 09:00:00   ·    ✓    ...
```

Date filters, by contrast, are whole days: `--date 2026-07-17` takes every
commit of that day, 09:00 and 23:30 alike, and `--date-since 2026-07-17` starts
at that day's first moment. The day is the **author's own** — a commit written
at 23:30 +0800 belongs to the day it was there, not to whatever day it was
where you're standing — which is exactly why a bound can never contradict the
date printed next to it.

Rows are ordered by date across all branches at once, so a row's neighbors are
the commits written around the same time, not the rest of its branch. That
answers "what happened when". `--topo` answers the other question — "what did
each branch do" — by keeping each branch's line of history in one block:

```sh
git-wt 1,2,3 commits --topo
```

```
default (by date)                    --topo
feat-07  ·  ✓                        feat-07  ·  ✓
main-06  ✓  ·                        feat-05  ·  ✓
feat-05  ·  ✓                        feat-03  ·  ✓
main-04  ✓  ·                        main-06  ✓  ·
feat-03  ·  ✓                        main-04  ✓  ·
main-02  ✓  ·                        main-02  ✓  ·
```

Both keep ancestry intact — neither can show a parent above its child — so
`--topo` is a matter of which story you want to read, never of correctness.

A subject too long for your terminal is cut with an `…`, never wrapped — the
marks are the point of the table, and a wrapped row strands them on a line of
their own. Long author names cap at 16 characters for the same reason. Piped
output has no terminal to fit, so it keeps whole subjects and names, and
`git-wt 1,2 commits | grep oauth` still works.

One honest limitation: the author column *is* padded, so a name written in a
double-width script (CJK) can still shift the columns to its right by a
character. Latin names — the overwhelming case — are unaffected, and fixing it
properly would mean a Unicode width table this crate has no dependency for.

### What the marks mean

| Mark | Meaning |
|---|---|
| `✓` | The branch has this commit |
| `≈` | The branch has this **patch**, under a different sha |
| `·` | The branch has neither |

`≈` is a cherry-pick, or a rebase's copy. To git those are different commits,
so a bare `✓`/`·` calls them *missing* — which reads as work still to do, when
the work is already there. That distinction is the difference between "needs
merging" and "already shipped, twice":

```
commit   author  date        main  feat  subject
2526759  Nino    2026-07-17   ✓     ≈    the shared fix      ← main's copy
04ecfc1  Jhon    2026-07-17   ·     ✓    feat only           ← genuinely missing
2a506f6  Jhon    2026-07-17   ≈     ✓    the shared fix      ← feat's original
```

A picked commit shows up twice, once per sha, and each row's `≈` is true from
its own side — they're two commits carrying one patch. Work nobody picked keeps
its `·`, which is what makes `≈` worth reading.

The comparison is git's own `git cherry` — patch-ids, not history — run per
pair of branches. It costs one walk per ordered pair, bounded by that pair's
merge-base; measured at 0.13s for two columns and 0.43s for six on a 59-commit
repo. `--no-cherry` skips it if your branches have diverged by thousands of
commits and you'd rather have the cheap answer.

A **cherry-picked or rebased commit is a different commit** to git — same
patch, new sha — so identity alone would call it missing. It isn't: those rows
are marked `≈` rather than `·`, which is what [the marks](#what-the-marks-mean)
are about. When the question is the content of the whole branch rather than
per-commit, `1,2 diff` and `1,2 merged` still answer it directly.

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
git-wt 1,2 merge            # worktree 2's branch -> worktree 1's branch
git-wt 1 merge feat/x       # a branch name works too
git-wt 1,2 merge dry-run    # would it conflict? nothing is touched
git-wt 1,2 merge theirs     # let 2 win every collision
```

The merge runs **inside worktree N**, so N's branch is the one that moves — the
source is only read. `git-wt 1,2 merge` reads as "merge 2 into worktree 1",
and you never have to `cd` anywhere to do it.

The source can be a worktree number (via the list) or any branch/ref. A number
that names a worktree wins over a branch of the same name; to merge a branch
actually named `2`, spell it `heads/2`.

### Two ways to name the targets

`git-wt 1,2 merge` is the list form, the same shape `diff` and `meld` use — every
multi-worktree action names its targets one way. The single-target form is still
used for branch sources and for `continue`/`abort`:

```sh
git-wt 1,2 merge     # list-style (preferred for worktree sources)
git-wt 1 merge feat/x # branch source; no list equivalent
```

Options work the same either way — `git-wt 1,2 merge theirs dry-run`.

Two differences from `diff`/`meld`, both because a merge is directional rather
than symmetric:

- The list takes **exactly two** worktrees. `meld` diffs 2–3; a merge has one
  destination and one source, so `git-wt 1,2,3 merge` is an error.
- A worktree source must use the list form, like `diff`. A branch source still
  uses the single-target form (`git-wt 1 merge feat/x`) because a list can only
  name worktrees.

The list already names the source, so it can't be combined with
`continue`/`abort` — those take a single target (`git-wt 1 merge continue`), and
asking otherwise says so.

### Words and options

The verb-ish words take an **optional `--`**, and all but one a short form —
`abort`, `--abort` and `-a` are the same thing:

| Word | Short | Does |
|---|---|---|
| `continue` | `-c` | Conclude a conflicted merge |
| `abort` | `-a` | Undo a conflicted merge |
| `ours` | `-o` | On a conflicting hunk, keep worktree N's side |
| `theirs` | `-t` | On a conflicting hunk, take the source's side |
| `dry-run` | `-d` | Report whether it would merge; change nothing |
| `review` | — | Report the verdict **and** the commits it would bring; change nothing |

These words win over a branch of the same name, so to merge a branch actually
called `theirs`, spell it `heads/theirs`.

`review` is the one with no short form, deliberately: every letter it could
take is already a `commits` flag on the far side of the handoff, and `--review`
is where merge stops reading.

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
git-wt 1,2 merge theirs   # collisions resolve to 2's side
git-wt 1,2 merge ours     # collisions resolve to 1's side
```

A side is chosen while the merge is **computed**, so it cannot be bolted onto a
merge that has already stopped — git offers no way to do that. Ask for one
anyway and `git-wt` explains, then offers the only route that honors it:

```
$ git-wt 1,2 merge theirs
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
git-wt 1,2 merge dry-run
```

It exits 0 when clean and 1 when it would conflict, so it drives a script:

```sh
if git-wt 1,2 merge dry-run; then git-wt 1,2 merge; fi
```

`dry-run` takes none of the flags that need a real merge to run — `-m`,
`--no-ff`, `--squash`, `--ff-only`, `-f`. Some shape the resulting commit and
there is no commit; `--ff-only` and `-f` gate whether the merge may run at all,
and nothing runs. (In particular `merge-tree` resolves in memory and never
fast-forwards, so `--ff-only` could not be honored even in principle.) A side is
allowed, since it changes the answer.

### `--review`

`dry-run` answers *would this merge?* `--review` answers *what would it bring?*
— the same verdict as a header, then the commits about to land. It merges
nothing either.

```sh
git-wt 1,2 merge --review          # what would 2 bring into 1?
git-wt 1,2 merge --review -f       # + the files under each commit
git-wt 1,2 merge --review -n 5 --author nino
```

```
$ git-wt 1,2 merge --review
feat/login -> main   3 commits, merges cleanly

commit   author  date        main  subject
9c3237e  Nino    2026-07-19   ·    fix: six review findings
267a002  Nino    2026-07-19   ≈    refactor: --matched-files is --match-only
2c7b804  Nino    2026-07-19   ·    fix: --filename missed the merges
```

This is the question `commits` could already answer, in a different command and
a **reversed argument order** — `git-wt 2,1 commits` for a merge you'd spell
`1,2`. `merge` reads dest-first, `commits` reads column order. `--review` takes
*merge's* order and does the translation, so you never do it in your head.

Exit codes are `dry-run`'s: **0** clean, **1** conflict with the paths listed.

```sh
if git-wt 1,2 merge --review; then git-wt 1,2 merge; fi
```

#### The one column is the destination's

Every row is in the source by definition — the range is "what the source has
that the destination lacks" — so a source column would be a `✓` on every row,
repeating the range's own definition. The destination's column is the useful
one, and it has two answers:

| Mark | Meaning |
|---|---|
| `·` | Genuinely new to the destination |
| `≈` | Its **patch** is already there, under a different sha |

`≈` is what a cherry-picked hotfix leaves behind. Pick a fix straight from
`feat/login` onto `main` and `main` holds that patch under a new sha, while
`feat/login` keeps the original — which is still absent *by sha*, so the merge
still lists it. Without the mark that row reads as work about to land; in fact
it has landed, and the merge will resolve it to a no-op or conflict against the
copy. Telling those apart is what a review is for.

A merge commit carries no patch of its own, so it can never be `≈` and
`--review --pick-id` leaves its cell empty. That is the column declining to
speak about merges, not a missing answer.

#### It inherits the `commits` flags

`--review` **ends merge's own flags**. Everything after it is a `commits` flag,
passed through untouched — which is the only way both commands keep the letters
they share:

```sh
git-wt 1,2 merge --review -f     # --files (NOT --force: merge never sees it)
git-wt 1,2 merge --review -fn 5  # bundles, like commits
git-wt 1,2 merge --review --filename api.php
```

So put merge's own options *before* `--review` and they are an error, not a
silent claim — `merge -f --review` would otherwise have set force before
`--review` was ever read. And a merge option typed *after* it says which:

```
$ git-wt 1,2 merge --review --dry-run
error: '--dry-run' and '--review' answer the same question
hint: '--review' already reports the verdict '--dry-run' prints, plus the commits behind it
```

Two `commits` flags are refused as well, though: `--all` and `--union`. Both
name a row *source*, and a review's is already the range. (`-a` is `--all`, so
it goes under that name; `-fn 5` is the bundle that works here.)

**Merge commits are shown**, unlike in `commits` — a review range is bounded by
the merge about to happen, so a merge inside it is the cargo rather than the
noise it is on a long-lived branch. `--review --no-merges` drops them, and it
is a real flag here for exactly that reason.

A merge already in progress needs no special case: it has not committed, so the
destination is still where it was and the range still names what has yet to
land. `--review` reports before that check, like `dry-run`.

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
$ git-wt 1,2 merge
error: merge conflict in /Users/me/code/myapp
  src/auth.rs
hint: resolve them there, 'git add' each, then 'git-wt 1 merge continue'
hint: or undo the merge with 'git-wt 1 merge abort'
hint: or redo it letting one side win: 'git-wt 1 merge abort', then 'git-wt 1,2 merge theirs'
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

## Merged

Ask whether one branch is already contained in another. This is different from
`merge dry-run`: `dry-run` asks "would it merge cleanly?", while `merged` asks
"is it already in?".

```sh
git-wt 1 merged              # is worktree 1's branch already in the current branch?
git-wt 1,2 merged            # is worktree 2's branch already in worktree 1's branch?
git-wt 1 merged feat/x       # is branch feat/x already in worktree 1's branch?
```

It uses `git merge-base --is-ancestor`, so it exits `0` when already merged and
`1` when not — the same contract as `merge dry-run`. That makes it useful for
safe cleanup:

```sh
if git-wt 1 merged; then git-wt 1 remove -y; fi
```

Output mirrors the dry-run style:

```
Merged   feat/x is already in main
Ahead    feat/x is NOT in main (ahead 3)
```

The ahead count comes from `git rev-list --count main..feat/x`. Both
single-target and list forms read dest-first, exactly like `merge`.

Naming a pair of worktrees uses the list form, as `merge` and `diff` do, so
`git-wt 1 merged 2` is rejected in favour of `git-wt 1,2 merged`. The
single-target form stays for a branch source, which a list of numbers cannot
name.

Detached worktrees are named by their short commit SHA in `merged` and `diff`
output instead of a branch name; `merge` and `meld` still show `(detached)`.
The answer is still correct, just less readable. A detached worktree has no
branch to name, so the list form is the way to ask about one:
`git-wt 1,2 merged` answers by SHA.

## Sync — `fetch` / `pull` / `push`

Run a remote verb inside a worktree's own directory, so it syncs that
worktree's branch against that branch's upstream. Nothing here does anything
git does not — it is the `cd` you would type first.

```sh
git-wt 1 pull                # git -C <dir 1> pull
git-wt 1,3 fetch --prune     # both, one after the other
git-wt pull --all            # every worktree
git-wt 2 push -u             # push and set the upstream
```

`--all` is the point. A repo with six worktrees is six branches, and they go
stale one at a time. It sweeps every worktree in `list` order, and it names no
target — `git-wt pull --all` is the one verb-first form left in the grammar,
because there is nothing to put in front of it.

### A sweep never stops on a failure

One worktree with no upstream, or a pull that hits a conflict, would otherwise
leave the worktrees after it untouched and unmentioned — half-synced, with no
line saying which half. So every one runs, each failure prints where it
happened, and the last line counts them:

```
pull main
Updating cc9e437..a22b97f
Fast-forward
skip (detached) (detached HEAD, no branch to sync)
pull feat/x
Already up to date.
pull lonely
error: There is no tracking information for the current branch.

pull: 2 ok, 1 failed, 1 skipped
error: pull failed in 1: lonely
```

The exit code is that summary — nonzero when anything failed. A single target
is not a sweep: git's own error is the whole story, and it exits with it
unsummarized rather than with a one-line count of itself.

Skipped is what the verb cannot mean, and it is not a failure — there was
nothing to do. A bare worktree has nothing to pull into, and a detached HEAD
has no branch, so no upstream to push to. `fetch` only moves remote-tracking
refs, so it runs on both.

### `--all` counts worktrees, not remotes

`git fetch --all` means every *remote*. Here `--all` means every *worktree*,
always, for all three verbs: `git-wt` counts worktrees, that is what it is for.
For every remote, run git yourself.

### Options

Flags are a curated list, not a passthrough — the same rule `diff` follows.
Any other git flag is an error, and the error prints the command to run
instead.

| Verb | Flags |
|---|---|
| `fetch` | `-p, --prune` · `--tags` · `--no-tags` · `--force` |
| `pull` | `--rebase` · `--no-rebase` · `--ff-only` · `-p, --prune` · `--autostash` |
| `push` | `-u, --set-upstream` · `--force-with-lease` · `--tags` · `-n, --dry-run` |
| all three | `-a, --all` |

`push --force` is the one flag refused outright: it overwrites a remote branch
without checking what is on it, and `--all` would do that to every branch at
once. `--force-with-lease` is the one that checks.

`push -u` on a branch with no upstream names the remote and branch for you —
a bare `git push -u` has no upstream to read them off of, which is the exact
situation `-u` exists for, so git rejects it. The remote is `origin` when it
exists, otherwise the only remote there is; more than one and no `origin` is an
error, since that is not a choice git-wt can make for you.

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
| `git-wt list --files` | Add each worktree's uncommitted files under its row (`-f`) |
| `git-wt --files` | Same, without the `list` word |

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
| `git-wt 1,2 merge` | Merge worktree 2's branch into worktree 1's |
| `git-wt 1 merge feat/x` | Merge branch `feat/x` into worktree 1's branch |
| `git-wt 1,2 merge --no-ff -m "sync"` | Merge commit with a message |
| `git-wt 1,2 merge --squash` | Stage the merge, don't commit |
| `git-wt 1,2 merge -f` | Merge even though worktree 1 is dirty |
| `git-wt 1,2 merge theirs` | Merge, letting 2 win every collision (`-X theirs`) |
| `git-wt 1,2 merge ours` | Merge, letting 1 win every collision (`-X ours`) |
| `git-wt 1,2 merge dry-run` | Report whether it would merge; change nothing |
| `git-wt 1,2 merge --review` | The verdict **plus** the commits it would bring; change nothing |
| `git-wt 1,2 merge --review -f` | Same, with each commit's files (`-f` is `--files` past `--review`) |
| `git-wt 1,2 merge --review --no-merges` | Drop the merge rows a review keeps by default |
| `git-wt 1 merge feat/x --review` | Review a branch source; the positional still belongs to `merge` |
| `git-wt 1 merge continue` | Conclude a conflicted merge (`--continue`, `-c`) |
| `git-wt 1 merge abort` | Undo a conflicted merge (`--abort`, `-a`) |
| `git-wt 1 merge 2` | `merge takes a worktree list: 'git-wt 1,2 merge' (or use 'heads/2' for a branch of the same name)` |
| `git-wt 1 merged` | Is worktree 1's branch already in the current branch? |
| `git-wt 1 merged feat/x` | Is branch `feat/x` already in worktree 1's branch? |
| `git-wt 1,2 merged` | Is worktree 2's branch already in worktree 1's? |

### Sync — `fetch` / `pull` / `push`

| Command | Effect |
|---|---|
| `git-wt 1 fetch` | `git fetch` in worktree 1 |
| `git-wt 1 pull` | `git pull` in worktree 1 |
| `git-wt 1 push` | `git push` in worktree 1 |
| `git-wt 1,3 pull` | Each worktree listed, in that order |
| `git-wt pull --all` | Every worktree (`-a` too) |
| `git-wt 1 fetch --prune` | `-p` also |
| `git-wt 1 pull --rebase` | Also `--no-rebase`, `--ff-only`, `--prune`, `--autostash` |
| `git-wt 1 push -u` | Push and set the upstream; names `origin <branch>` when there is none |
| `git-wt 1 push --force-with-lease` | Force, but refuse when the remote moved |
| `git-wt 1 push --dry-run` | `-n` also |
| `git-wt pull --all` (one fails) | Sweep finishes, `pull: 2 ok, 1 failed, 1 skipped`, exit 1 |
| `git-wt 1 pull` (fails) | git's own error, unsummarized, exit 1 |
| `git-wt pull` | Error — `'pull' needs a worktree: 'git-wt <N> pull'` |
| `git-wt 1 pull --all` | Error — `--all` is every worktree, so a target adds nothing |
| `git-wt 1,1 fetch` | Error `worktree #1 listed twice` |
| `git-wt 1 push --force` | Error — use `--force-with-lease` |
| `git-wt 1 fetch --rebase` | Error — `--rebase` is `pull`'s flag |
| `git-wt 1 pull --depth=1` | Error — not a passthrough; the error prints the `git` command |
| `git-wt 1 pull --rebase --no-rebase` | Error — the two contradict each other |

### Diff — `git-wt <N>,<M> diff [flags]`

| Command | Effect |
|---|---|
| `git-wt 1,2 diff` | `git diff <branch 1>...<branch 2>` — only 2's own commits, i.e. what `1,2 merge` brings |
| `git-wt 1,2 diff ...` | Same, spelled out |
| `git-wt 1,2 diff ..` | `git diff <branch 1>..<branch 2>` — tip vs tip, includes 1's commits inverted |
| `git-wt 1,2 diff --name-only` | File names only (also `--name-status`, `--stat`) |
| `git-wt 1,2 diff -- src/` | Limit to `src/`; combines with the flags above |
| `git-wt 1,2 diff live` | Compare the files on disk, `.gitignore` honored |
| `git-wt 1,2 diff live hunks` | Same, plus each file's changed line numbers |
| `git-wt 1,2 diff hunks` | Line numbers on the committed diff |
| `git-wt 1,2 diff live ..` | Error — a range compares commits, `live` compares disk |
| `git-wt 1,1 diff` | Error `worktree #1 against itself is always empty` |
| `git-wt 1 diff` | Error — `diff` takes a worktree list |
| `git-wt 1,2 diff -w` | Error — unknown flag, with the `git diff` command to run instead |
| `git-wt 1,2,3 diff` | Error — `diff` takes exactly two worktrees; `meld` compares three |

### Commits — `git-wt <N>,<M>[,...] commits`

| Command | Effect |
|---|---|
| `git-wt 1,2 commits` | Worktree 1's commits down to its earliest one that 2 is missing, with a column saying whether 2 has each |
| `git-wt 1,2,3 commits` | Same, cut at the earliest commit *any* other branch is missing; one more check column |
| `git-wt 2 commits` | Worktree 2's own log — one branch, no check columns |
| `git-wt 1,2 commits -n 20` | Newest 20 rows only (also `--limit 20`, `--limit=20`) |
| `git-wt 1,2 commits --union` | Rows from both branches, whole logs, not just 1's range |
| `git-wt 1,2 commits --all` (`-a`) | Worktree 1's whole log, no cut at the divergence; other branches stay check columns |
| `git-wt 1,2,3 commits --topo` | Group each branch's commits instead of interleaving by date |
| `git-wt 1,2 commits --merges` | Keep merge rows; they're dropped by default |
| `git-wt 1,2 commits --time` | Add the time to the date column, 24-hour |
| `git-wt 1,2 commits --date-human` | `Jan. 31, 2026` instead of the default `2026-01-31` |
| `git-wt 1,2 commits --reverse` | Newest last (also `--oldest-first`) |
| `git-wt 1,2 commits --md` | Write `commits_<date>_<time>.md` in the current directory |
| `git-wt 1,2 commits --md report.md` | Write that path, overwriting it |
| `git-wt 1,2 commits --author nes` | Only commits whose author fuzzy-matches `nes` |
| `git-wt 1,2 commits -m oauth` | Only commits with `oauth` in the subject or body (also `--message`) |
| `git-wt 1,2 commits --filename api.php` | Only commits touching a matching path; implies `--files`, block cut to the matches |
| `git-wt 1,2 commits --filename api.php --all-files` | Same rows, but each block is the commit's whole file list |
| `git-wt 1,2 commits --date 2026-01-31` | Commits on exactly that day (also `-d`) |
| `git-wt 1,2 commits --date-since 2026-01-01 --date-until 2026-06-30` | A date range, inclusive, no quoting |
| `git-wt 1,2 commits --commit-since 5568a21` | The day that commit was authored, and after |
| `git-wt 1,2 commits --commits af48509,f9e2427` | Only those commits (also `-c`); implies `--all` |
| `git-wt 1,2 commits --date-until 2026-06-30` | Trims the top of the default rows; does **not** imply `--all` |
| `git-wt 1,2 commits --author nes --all` | `--author` never implies `--all` — say it yourself |
| `git-wt 1,2 commits -af` | Short flags bundle: `--all --files` |
| `git-wt 1,2 commits --commit-since 5568a21` | Commit `5568a21` and everything after it |
| `git-wt 1,2 commits --commit-until HEAD~3` | `HEAD~3` and everything it can reach |
| `git-wt 1,2 commits --date '>=2026-01-01'` | Error — no operators in `--date`; use `--date-since` |
| `git-wt 1,2 commits --from 5568a21` | Error — unknown argument; `--commit-since` takes a commit, `--date-since` a date |
| `git-wt 1,2 commits --since 2026-01-01` | Error — unknown argument; `--date-since` is the flag here |
| `git-wt 1,2 commits --commit-since zzz9` | Error `--commit-since: no commit 'zzz9'` |
| `git-wt 1,2 commits` (empty branch) | `no commits on <worktree 1>`, exit 0 |
| `git-wt 1,1 commits` | Error `worktree #1 listed twice` |
| `git-wt 1,2 commits -n 0` | Error `-n 0 would show nothing` |
| `git-wt 1,2 commits --stat` | Error — unknown argument; `commits` takes no git flags |
| `git-wt 1,2 commits --no-merges` | Error — unknown argument here; merges are dropped already, and `--merges` keeps them |
| `git-wt 1,2 commits --match-only` | Error — unknown argument; `--filename` already cuts the block, `--all-files` widens it |
| `git-wt 1,2 commits --all-files` | Error — needs a `--filename` to widen |
| `git-wt 1,2 commits --grep x` | Error — unknown argument; `--message` is the flag here |

### Multi-target — `git-wt <N>,<N>[,<N>] meld`

| Command | Effect |
|---|---|
| `git-wt 1,2 meld` | 2-way diff of worktrees 1 and 2, in that pane order |
| `git-wt 2,1,3 meld` | 3-way diff; worktree 2 on the left |
| `git-wt 1 meld` | Error `meld needs 2 or 3 worktrees` |
| `git-wt 1,2,3,4 meld` | Error `meld takes at most 3 worktrees, got 4` |
| `git-wt 1,1 meld` | Error `worktree #1 listed twice` |
| `git-wt 1,2 remove` | Error — only `diff`, `meld`, `merge` and `merged` take a list |

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
| `git-wt 1 bogus` | `unknown action 'bogus' (switch, path, remove, diff, commits, merge, meld, merged)` |
| `git-wt 1 switch path` | `too many arguments` |
| `git-wt 1 -n x` | `'-n' is an option, not an action; options follow the action, e.g. 'git-wt 1 remove -f' or 'git-wt 1,2 diff --stat'` |
| `git-wt 1 remove` on main/bare | `refusing to remove the main worktree` |
| `git-wt foo` (branch-like: has `/` or `-`) | `unknown command 'foo'; did you mean 'add foo'?` |
| `git-wt lsit` (not branch-like) | `unknown command 'lsit'` |
| `git-wt show 1` (legacy) | `unknown command 'show'; use 'git-wt 1 path'` |
| `git-wt remove 1` (legacy) | `unknown command 'remove'; use 'git-wt 1 remove'` |
| `git-wt merge 2` (target missing) | `unknown command 'merge'; use 'git-wt 1,2 merge'` |
| `git-wt 1,2,3 merge` | `merge takes exactly two worktrees, not 3` |
| `git-wt 1,2 merge continue` | `'continue' takes no source, so a worktree list has nothing to name` |
| `git-wt 1,x merge` | `bad worktree list '1,x'; want numbers, e.g. '1,2'` |
| `git-wt 1,2` (no action) | `a worktree list needs an action, e.g. 'git-wt 1,2 meld'` |
| `git-wt 1 merge` | `merge needs a source: 'git-wt <N>,<M> merge' (or 'git-wt <N> merge <BRANCH>', or continue/abort)` |
| `git-wt 1 merge zzz` | `no worktree or branch 'zzz' (see 'git-wt list')` |
| `git-wt 1,1 merge` | `'main' is already checked out in worktree 1` |
| `git-wt 1,2 merge` (worktree 1 dirty) | `worktree 1 has uncommitted changes` + `-f` hint |
| `git-wt 1,2 merge` (merge in progress) | `a merge is already in progress` + continue/abort hint |
| `git-wt 1,2 merge theirs` (merge in progress) | Explains, then prompts to abort and redo `[y/N]` |
| `git-wt 1 merge continue` (none started) | `no merge in progress in <path>` |
| `git-wt 1 merge continue 2` | `continue takes no argument (got '2')` |
| `git-wt 1,2 merge ours theirs` | `ours and theirs conflict` |
| `git-wt 1 merge theirs continue` | `continue takes no merge options` + why, and the abort/redo route |
| `git-wt 1,2 merge dry-run --no-ff` | `dry-run takes no merge options` |
| `git-wt 1,2 merge -f --review` | `review takes no merge options (got -f)` — it was claimed before `--review` was read |
| `git-wt 1,2 merge --review --dry-run` | `'--dry-run' and '--review' answer the same question` |
| `git-wt 1,2 merge --review --squash` | `'--squash' shapes a merge commit` + drop-one hint |
| `git-wt 1,2 merge --review --all` | `no '--all' under '--review'` — the rows are the range `dest..src` |
| `git-wt 1,2 merge --review -a` | Same; `-a` is `--all`, refused under its full name |
| `git-wt 1,2 merge dry-run` (git < 2.38) | `dry-run needs git 2.38 or newer (git merge-tree --write-tree)` |
| `git-wt merged` (bare word) | `unknown command 'merged'; use 'git-wt 1 merged' or 'git-wt 1,2 merged'` |
| `git-wt 1 merged 2` | `merged takes a worktree list: 'git-wt 1,2 merged' (or use 'heads/2' for a branch of the same name)` |
| `git-wt 1,2,3 merged` | `merged takes exactly two worktrees, not 3` |
| `git-wt 1 merged feat/x extra` | `too many arguments` |
| `git-wt 1 merged zzz` | `no worktree or branch 'zzz' (see 'git-wt list')` |
| `git-wt 1 merged 1` | `'main' is already checked out in worktree 1` |
| `git-wt 1,1 merged` | `worktree #1 listed twice` |
| `git-wt 1,2 merged` (2 not in 1) | `error: Ahead <branch> is NOT in <branch> (ahead N)`, exit 1 |
| `git-wt list --col 6` | Adds a `merged`/`ahead N`/`ahead` column relative to current branch |
| `git-wt list --col 7` | `no column 7 (use 1=id, 2=branch, 3=dir, 4=status, 5=last, 6=merged)` |
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
