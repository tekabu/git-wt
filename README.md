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
| See which tasks have which commits | `wt 1,2,3 commits` | A table: one row per commit, a check under every task that has it |
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
git-wt <N>,<M> merge         Merge M into N
git-wt <N> merge <BRANCH>    Merge BRANCH into worktree N
git-wt <N> merge continue|abort
git-wt <N>,<M> merged        Is M's branch already in N's branch?
git-wt <N> merged <BRANCH>   Is BRANCH already in worktree N's branch?
git-wt <N> merged            Is N's branch already in the current branch?
git-wt <N>,<M> diff [flags]  Diff worktree N against worktree M
git-wt <N>,<M>[,...] commits Table: which commit is on which branch
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

Which branch has which commit, for any number of worktrees at once:

```sh
git-wt 1,2,3 commits
```

```
commit   author  date            main  feat/login  bugfix-123  subject
a1b2c3d  Nino    Sep. 15, 2026    ·        ✓           ✓       fix token expiry
9f8e7d6  Jhon     Apr. 2, 2026    ·        ✓           ·       🚀 add oauth scopes
4c5d6e7  Nino     Jan. 1, 2026    ✓        ✓           ✓       bump serde
```

One row per commit — sha, author, date — then one column per worktree, checked
where that branch contains it, and the subject last. This is the question
`diff` cannot answer: `diff` compares exactly two branches, and compares their
*content*. `commits` compares any number, by *commit*, so "who already has the
oauth fix?" is one glance instead of three `merged` calls.

Authors come from `%aN`, so a `.mailmap` is honored and a contributor who has
committed under two names or addresses shows up as one. Dates are *author*
dates — when the work was written, not when it landed here — and the rows sort
by the same clock, so a rebased commit never appears out of order against its
own printed date.

The subject sits last for a reason worth knowing: it is the only free-form cell,
and an emoji like 🚀 occupies two terminal columns while being a single
character. Any column padded to a "length" measured in characters would shift
by one on exactly those rows. Nothing is padded after the subject, so the marks
line up whatever anyone puts in a commit message — no Unicode width tables, and
no dependencies.

Rows stop at the branches' common ancestor, so only the diverged commits are
listed — the shared history would be a check in every column, saying nothing,
and skipping it is what keeps the table fast on a repo with real history. Ask
for it with `--all`, and cap the rows with `-n`:

```sh
git-wt 1,2 commits -n 20    # newest 20 rows
git-wt 1,2,3 commits --all  # shared history too (slow on a big repo)
```

### Filtering the rows

Filters narrow which commits are listed; the columns stay whatever your
worktree list named. They AND together, and `-n` counts what survives them.

```sh
git-wt 1,2 commits --author nino                    # fuzzy, like list's SEARCH
git-wt 1,2 commits --from-date 2026-01-01           # that day and after
git-wt 1,2 commits --from-date 2026-01-01 --to-date 2026-06-30
git-wt 1,2 commits --date '>=2026-01-01'            # same bound, operator form
git-wt 1,2 commits --from-id 5568a21 --to-id HEAD   # a span of commits
```

Two vocabularies, one shape. The `-id` bounds take a **commit** — a sha, a
branch, a tag, `HEAD~3` — and the `-date` bounds take a **YYYY-MM-DD**:

| Flag | Means |
|---|---|
| `--author NAME` | Only NAME's commits; fuzzy subsequence, case-folded (`nes` → `Nino Escalera`) |
| `--date '>=D'` | Commits on D or after; also `<=` and `=`. Repeat for a range |
| `--from-date D` | Same as `--date '>=D'`, no quoting needed |
| `--to-date D` | Same as `--date '<=D'`, no quoting needed |
| `--from-id C` | Commit C and everything after it |
| `--to-id C` | Commit C and everything it can reach |

**Both ends include what they name.** `--from-id 5568a21` lists `5568a21`
itself, and `--from-date 2026-01-01` takes that whole day. That's why there's
no `>` or `<`: the day either side of a bound is just the inclusive bound next
door, and a strict comparison would be a second spelling of the same thing —
costing a character the shell wants for itself.

> **Quote `--date`.** `>` and `<` are shell redirects, so an unquoted
> `--date >=2026-01-01` writes a file named `=2026-01-01` and git-wt never sees
> the date. `--from-date`/`--to-date` need no quoting, which is why they exist.

`--date` compares the date the table prints, which is the **author** date.
git's own `--since`/`--until` filter on *committer* dates and would quietly
disagree with the column, so git-wt does the comparison itself and rejects
those two spellings with a pointer to `--from-date`/`--to-date`.

Rows are ordered by date across all branches at once, so a row's neighbors are
the commits written around the same time, not the rest of its branch.

A subject too long for your terminal is cut with an `…`, never wrapped — the
marks are the point of the table, and a wrapped row strands them on a line of
their own. Long author names cap at 16 characters for the same reason. Piped
output has no terminal to fit, so it keeps whole subjects and names, and
`git-wt 1,2 commits | grep oauth` still works.

One honest limitation: the author column *is* padded, so a name written in a
double-width script (CJK) can still shift the columns to its right by a
character. Latin names — the overwhelming case — are unaffected, and fixing it
properly would mean a Unicode width table this crate has no dependency for.

One caveat, inherited from git: a **cherry-picked or rebased commit is a
different commit**, so it shows unchecked in the branch it was copied from —
same patch, new sha. When the question is content rather than identity, that's
what `1,2 diff` and `1,2 merged` are for.

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
| `git-wt 1,2 merge` | Merge worktree 2's branch into worktree 1's |
| `git-wt 1 merge feat/x` | Merge branch `feat/x` into worktree 1's branch |
| `git-wt 1,2 merge --no-ff -m "sync"` | Merge commit with a message |
| `git-wt 1,2 merge --squash` | Stage the merge, don't commit |
| `git-wt 1,2 merge -f` | Merge even though worktree 1 is dirty |
| `git-wt 1,2 merge theirs` | Merge, letting 2 win every collision (`-X theirs`) |
| `git-wt 1,2 merge ours` | Merge, letting 1 win every collision (`-X ours`) |
| `git-wt 1,2 merge dry-run` | Report whether it would merge; change nothing |
| `git-wt 1 merge continue` | Conclude a conflicted merge (`--continue`, `-c`) |
| `git-wt 1 merge abort` | Undo a conflicted merge (`--abort`, `-a`) |
| `git-wt 1 merge 2` | `merge takes a worktree list: 'git-wt 1,2 merge' (or use 'heads/2' for a branch of the same name)` |
| `git-wt 1 merged` | Is worktree 1's branch already in the current branch? |
| `git-wt 1 merged feat/x` | Is branch `feat/x` already in worktree 1's branch? |
| `git-wt 1,2 merged` | Is worktree 2's branch already in worktree 1's? |

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
| `git-wt 1,2 commits` | Table of the commits 1 and 2 do not share |
| `git-wt 1,2,3 commits` | Same for three worktrees; any number of columns |
| `git-wt 1,2 commits -n 20` | Newest 20 rows only (also `--limit 20`, `--limit=20`) |
| `git-wt 1,2 commits --all` | Include the shared history, not just what diverged |
| `git-wt 1,2 commits --author nes` | Only commits whose author fuzzy-matches `nes` |
| `git-wt 1,2 commits --date '>=2026-01-01'` | Commits on that day or after; also `<=`, `=` |
| `git-wt 1,2 commits --from-date 2026-01-01 --to-date 2026-06-30` | A date range, inclusive, no quoting |
| `git-wt 1,2 commits --from-id 5568a21` | Commit `5568a21` and everything after it |
| `git-wt 1,2 commits --to-id HEAD~3` | `HEAD~3` and everything it can reach |
| `git-wt 1,2 commits --date '>2026-01-01'` | Error — bounds are inclusive; use `>=` or `--from-date` |
| `git-wt 1,2 commits --from 5568a21` | Error — `--from-id` takes a commit, `--from-date` takes a date |
| `git-wt 1,2 commits --since 2026-01-01` | Error — git's word; use `--from-date` |
| `git-wt 1,2 commits --from-id zzz9` | Error `--from-id: no commit 'zzz9'` |
| `git-wt 1,2 commits` (same history) | `no diverged commits: every branch listed has the same history`, exit 0 |
| `git-wt 1 commits` | Error — `commits` takes a worktree list |
| `git-wt 1,1 commits` | Error `worktree #1 listed twice` |
| `git-wt 1,2 commits -n 0` | Error `-n 0 would show nothing` |
| `git-wt 1,2 commits --stat` | Error — unknown argument; `commits` takes no git flags |

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
