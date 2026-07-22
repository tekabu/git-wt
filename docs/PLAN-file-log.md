# `log`: one file's history, across branches

> **Status: implemented** on `feat/file-log`, branched from `main` (`ae67e0b`).
> The `±` cell on rows before a followed rename is computed against the
> post-rename pathspec, so it can under-report churn on those older rows --
> not yet fixed.

## Context

`commits` answers "which branch has this commit". The question it cannot answer
is the one asked while staring at a file: **what happened to *this* file, and on
which branch did it happen.** Today that is `git log --follow -- src/ui.rs` in
each worktree in turn, with no way to see the branches side by side.

`log` is that table. Same rows, same columns, same filters, same renderer as
`commits` — the only thing that changes is what selects the rows: a pathspec
instead of a branch range.

```
$ wt 1,2 log src/cmd/commits/render.rs
$ wt log -b 2,3 /Users/nino/Scripts/src/git-wt-feat-x/src/ui.rs
$ wt 1,2 log src/ui.rs --author nino --date-since 2026-01-01 -w
```

## Grammar

```
git-wt <N>[,<M>...] log [PATH...] [commits options]
git-wt log [PATH...] [commits options]          # current worktree only
git-wt -b <LIST> log [PATH...]                  # current worktree + LIST
```

Targets are worktrees or branch names, exactly as `commits` takes them, and mean
the same thing: **the first is the row source, the rest are mark columns.**
`-b`/`--branch` is the existing global flag and rides anywhere in the line
(`extract_branch_flag`, `cli.rs`), so `wt log -b 2,3` is `wt <cur>,2,3 log`.

`PATH` is an argument of the verb, never of the target slot. A path in the
target slot would collide with branch names (`feature/foo` vs `src/foo`) and
force an existence check to decide meaning; putting it after the verb costs one
word and makes multiple paths fall out for free.

**`PATH` omitted** = the current directory, repo-relative. Standing in
`src/cmd/commits/` and typing `wt 1,2 log` is that directory's history, which is
the useful reading and never an accident: an empty pathspec would just be
`commits`, which already has a name.

### Resolving `PATH`

Every form has to become one repo-relative pathspec, because the same pathspec
is applied to every listed branch:

1. Absolute (`/Users/nino/Scripts/src/git-wt-feat-x/src/ui.rs`) — strip the
   *worktree* root it sits under, not the primary root. Any worktree's copy of
   `src/ui.rs` names the same file in history; a path under a worktree that is
   not in the target list is fine and is not an error.
2. Relative (`../src/ui.rs`, `src/ui.rs`) — canonicalize against the cwd, then
   as above.
3. Repo-relative already (`src/ui.rs` typed from anywhere in the repo) — used
   verbatim when it resolves under no worktree root but does exist in the tree.

A path under no worktree at all is an error naming both readings:

```
'/etc/hosts' is outside the repository
hint: paths are resolved against the worktree they sit in
```

A path that resolves but exists in no listed branch is **not** an error — a file
deleted on `main` and alive on `feat/x` is exactly the case this table is for.
The empty result says so:

```
no commits touched 'src/old.rs' on main
hint: it may live under another name; --no-follow shows the literal path only
```

## What it inherits from `commits`

`parse_commits_args_with(args, review: bool)` becomes
`parse_commits_args_with(args, mode: Mode)` with `Mode { Commits, Review, Log }`.
The `review` bool already gates four separate behaviours through one flag
(see its doc comment); a third mode is the same shape, not a new one.

Free, unchanged, no new code:

| flag | behaviour in `log` |
| --- | --- |
| `-n`/`--limit` | same; default 10 |
| `--date`, `--date-since`, `--date-until` | same |
| `--commit-since`, `--commit-until`, `--commits`/`-c` | same |
| `--author` | same |
| `--message`/`-m` | same, and still implies `--wrap full` |
| `--time`, `--date-human` | same |
| `--reverse`/`--oldest-first`, `--topo` | same |
| `--merges` | same; dropped by default |
| `--no-cherry`, `--pick-id` | same |
| `-w`/`--wrap`, `--subject-width`, `--branch-width` | same |
| `--md` | same, with a `git-wt log` heading naming the path |

### The flags that do not exist here

`--filename`, `--all`, and `--all-files` are simply not options of `log`. No
bespoke refusal, no per-flag hint: an unrecognized flag gets the parser's
existing unknown-option error, the same one every other typo gets. Tailored
messages would be three strings to keep true as the flags evolve, in exchange
for telling the user what `--help` already tells them.

Why each is absent:

- **`--filename`** — the path *is* the target.
- **`--all`** — a file's history is its whole log; there is no divergence floor
  to lift. The branch-range view is `commits --all`.
- **`--all-files`** — in `commits` it needs a `--filename` to widen. Here it is
  the same request against the target path, which is `-f`; one spelling only.

### The flags that change meaning

- **`--union`** — kept, and load-bearing. Default rows are the *first* branch's
  history of the path; `--union` takes every listed branch's, unioned. This is
  how you see a commit that touched the file on `feat/x` and never reached
  `main`.

- **`--files`/`-f`** — in `commits` it adds the changed-file block. In `log` the
  row already carries the path's own `+/-`, so `-f` means "the *other* files
  each of these commits touched" — the blast radius of every change to this
  file. Off by default.

- **`--squash`** — the consolidated block becomes the path's lifetime totals:
  commits, authors, `+/-`, first and last touch.

## Rows

Default source: `git log --follow -- <pathspec> <first-ref>`.

`commit_rows` already takes `refs` and a format string; it gains a
`paths: &[String]` parameter appended as `-- <paths>`. Under `--union` it is one
walk per ref, deduplicated by sha — the same shape `--union` already has.

Two constraints worth stating up front, because they decide the implementation:

1. **`--follow` takes exactly one pathspec.** So renames are followed only when
   a single path was given. With several, `--no-follow` behaviour is what git
   gives and the header says so.
2. **`git log -- <path>` prunes merges** — the same trap `path_shas`
   (`rows.rs:430`) documents. A merge that brought the whole file over lists
   nothing and vanishes. `--merges` in `log` mode therefore implies
   `--full-history --diff-merges=first-parent`, matching what `--filename`
   already does, so the merge shown is one whose first-parent diff actually
   touched the path.

## Columns

```
sha | author | date [time] | b1 b2 b3 | ± | path | subject
```

- **Mark grid** (`b1 b2 b3`) — unchanged. `✓` present, `≈` patch-equivalent
  under another sha, `·` absent. `MarkKind` and `equivalents` are reused whole.
  Solo target = no mark columns, same rule `commits` has.
- **`±`** — new cell: `+12 -3` for **this path only**, from `--numstat --
  <path>` on the same walk. Not the commit-wide count `--files` prints.
- **`path`** — printed only when it varies: a rename under `--follow`, or more
  than one path given. Otherwise it is the header's job, not a column's.

## Header

```
src/ui.rs   main, feat/x, feat/y   34 commits, +1204 -318, 3 authors
```

Under a rename, the header names the span:

```
src/ui.rs (was src/render.rs before 4a2379d)
```

## Later, not now

Deliberately out of this plan, in rough value order:

1. **`-L A:B` / `--lines A:B`** — `git log -L` over the path: history of one
   function. Highest-value follow-up and the reason not to build a blame view;
   blame is a different table shape.
2. **`--content TERM`** — pickaxe (`-S`/`-G`) restricted to the path. Composes
   with `--message`, which searches the log text instead.
3. **`--who`** — collapse the table to ownership: author, commits, `+/-`,
   share. `--squash` is the precedent for "collapse to a summary block".
4. **`--birth` / `--death`** — only the commit that created (or deleted) the
   path.
5. **`--dirty`** — a synthetic top row for uncommitted local changes, so the
   table is never silently stale.

## Phases

1. `Mode` enum in `args.rs`; `Log` arm; the redefined flags (`--union`, `-f`,
   `--squash`) and the three that are absent; parser tests for each.
2. Path resolution (worktree-root stripping, absolute/relative/repo-relative,
   the outside-repo error) as a standalone function with unit tests.
3. `commit_rows` takes `paths`; `--follow` and the merge-pruning rule.
4. The `±` cell from per-path `--numstat`.
5. Render: `path` column when it varies, header line.
6. `--md` heading and `--squash` totals.
7. `--help` text and README.

## Tests

- Parser: `--filename`, `--all`, `--all-files` under `log` all produce the
  generic unknown-option error, no special-cased text; `-b` merges into the
  target list; `PATH` after the verb never parses as a flag value.
- Path resolution: absolute under a *non-target* worktree; relative through
  `..`; repo-relative from a subdirectory; outside-repo error.
- Rows: a file deleted on one branch and alive on another shows on both under
  `--union` and only on the first without it.
- Merges: a merge that brought the file over is a row under `--merges` and
  absent without it — the `path_shas` trap, asserted rather than assumed.
- Renames: `--follow` spans the rename; `--no-follow` stops at it.
