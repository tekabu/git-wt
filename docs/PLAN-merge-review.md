# `merge --review`: see what is about to come over

> **Status: planned** on `feat/merge-review`. Nothing implemented yet. Branched
> from `fix/review-round-findings` (`9c3237e`) — the stable line — so the
> review-round fixes are in the base.

## Context

`merge` can already answer one question about a merge it has not run yet:

```
$ wt 1,5 merge --dry-run
Clean fix/review-round-findings merges into main cleanly
```

That is the *mechanical* question — will it conflict — and `merge_dry_run`
(`merge.rs:392`) answers it with `git merge-tree --write-tree`, touching
nothing. What it does not answer is the question a reviewer asks first: **what
is coming over?** Which commits, whose, touching which files.

Today that answer exists, in a different command and a reversed argument order:

```
$ wt 5,1 commits -f          # note: 5,1, not 1,5
```

`commits` takes the pair as column order; `merge` takes it dest-first
(`merge.rs:241` — `trees[idx]` is the destination, the second target resolves to
the source). Same two digits, same tool, reversed meaning. That flip is the
friction this flag removes: `--review` takes *merge's* order and does the
translation.

## The flag

`--review` is a reporting mode of `merge`, parallel to `--dry-run`. It merges
nothing.

```
$ wt 1,5 merge --review
fix/review-round-findings -> main   3 commits, merges cleanly

commit   author  date        subject
9c3237e  Nino    2026-07-19  fix: six review findings
267a002  Nino    2026-07-19  refactor(commits): --matched-files is --match-only
2c7b804  Nino    2026-07-19  fix(commits): --filename missed the merges
```

### It inherits the `commits` flag set

```
wt 1,5 merge --review -f                  # file blocks under each commit
wt 1,5 merge --review -n 5 --files
wt 1,5 merge --review --author Nino -f
wt 1,5 merge --review -af                 # bundles work, see step 2
```

Not `--stat` — that is a `diff` flag (`diff.rs:70`), and `commits` rejects it
(`commits/args.rs:671`; confirmed: `error: unexpected argument '--stat' for
commits`).

## The parsing problem, and the rule that solves it

**`merge` claims the short flags first.** `parse_merge_args` is one flat match
(`merge.rs:87-101`), so every short form is consumed before any handoff could
happen. Verified against the built binary:

| Typed | What `merge` does today | Wanted under `--review` |
|---|---|---|
| `-a` | `set_merge_op(Abort)` (`merge.rs:88`) | `--all` |
| `-c` | `set_merge_op(Continue)` (`merge.rs:87`) | `--commits` |
| `-d` | `dry_run = true` (`merge.rs:91`) | `--date` |
| `-m X` | eats `X` as the merge message (`merge.rs:92-94`) | `--message` (body filter) |
| `-f` | `force = true` (`merge.rs:100`) — **and merges** | `--files` |
| `-n 5` | `error: unknown option '-n 5' for merge` | `--limit 5` |
| `--author` | `error: unknown option '--author' for merge` | `--author` |

`start_only_flags` does **not** free these up. It runs *after* the claim and
only inside the `dry_run` branch (`merge.rs:158-165`), so it rejects rather than
releases. Outside that branch nothing intervenes: `wt 1,5 merge -f` is a real
forced merge, which it performed during this investigation.

So the ambiguity is genuine at parse time — it simply resolves in merge's favour
today.

### The rule: `--review` ends merge's parsing

`--review` must **stop merge flag parsing and pass the remainder verbatim** to
`parse_commits_args`. That makes it positional, and the plan commits to it:

- **`--review` comes first**, before any `commits` flag.
- **Merge *flags* before it are an error**, not a silent claim. `merge -a
  --review` has already set `Abort` by the time `--review` is seen; that must
  error rather than produce an aborted review.
- **The positional source is exempt.** It is not a flag and `merge` still needs
  it: `wt 1 merge feat/x --review` must consume `feat/x` (the catch-all at
  `merge.rs:105-110`) before the handoff. Only one positional is allowed, as
  today — a second is still `too many arguments`. The pair form
  `wt 1,5 merge --review` carries no positional at all, since the source comes
  from the target list.
- Everything after `--review` is never inspected by `merge`, so `-f`, `-m`,
  `-n`, `-d` land on their `commits` meanings intact.

Handing off the tail verbatim also gets `expand_short_bundles`
(`commits/args.rs:200`) for free, so `--review -af` works — another thing
merge's parser cannot do, and a further reason not to re-parse in `merge`.

The long forms were never in conflict: the full-name intersection between the
two flag sets is exactly one member, `--message`, and the handoff settles it.

### Range

One-directional: `dest..src`, what the merge would bring *over*. Not the
two-column presence table `wt 5,1 commits` prints — that view answers "how do
these two branches compare", and a review asks the narrower question.

### Exit code

Keeps `dry-run`'s contract: **0** clean, **1** conflict with the paths listed,
so `if wt 1,5 merge --review; then` stays meaningful.

## Implementation approach

### 1. `parse_merge_args` gains `--review`

Sets the flag, stops consuming, and returns the untouched tail. Runs
`start_only_flags` over what came *before* it and errors on any merge option.
`--review` with `--dry-run` is an error — review already reports what dry-run
reports.

### 2. Hand the tail to `parse_commits_args`

Unchanged and uninspected, per the rule above. Unknown flags keep producing the
`commits` error, which already names the right help.

### 3. Render through the existing `commits` path

Reuse `commit_rows` + the render module over the `dest..src` range. **No new
table code** — `commits/render.rs` is 289 lines and `rows.rs` 1518; a third copy
of the row/file-block rendering is not on the table.

This is the house rule, not a preference. Commit `9c3237e` closed *two*
duplications, each with a named bug caused by the copy:

- `cmd_list_with_ref` — lived in **`merged.rs`**, a ~110-line near-verbatim copy
  of `cmd_list`. "Every table fix had to be made twice." Now
  `cmd_list(.., merged_ref: Option<&str>)` (`list.rs:204-211`), `merged.rs`
  −119 lines.
- `git_run` / `git_run_no_editor` — collapsed into `run_and_relay` (`git.rs`,
  +41/−41) after a stderr-on-success fix landed in only one of the twins.

### 4. Split the verdict from the reporting

Section 4 cannot just call `merge_dry_run`: it returns `Err(...)` on conflict
(`merge.rs:416-421`) and the caller prints that error afterwards, so there is no
way to emit header → table → exit 1 in that order.

**Named refactor:** extract the `merge-tree --write-tree` probe into a function
returning the verdict (clean, or the conflicting paths). `merge_dry_run` becomes
a thin caller that formats the verdict as today's `Ok`/`Err`; `--review` uses
the same verdict for its header and controls its own exit code.

## `--filename` lands on its hardest case here

`--review` shows `dest..src` — precisely a range that can carry merge commits —
so an inherited `--filename` hits the exact bug commit `2c7b804` was written
for: `git log -- <path>` prunes merges, so "a merge whose block listed thirty
matching files matched nothing at all". The fix walks `--full-history` for
candidates, then re-checks with `--name-only --diff-merges=first-parent`.

`--review --filename` inherits that machinery unchanged and is covered by it.
This must be tested in the review range specifically, not assumed from the
`commits` tests — and if the range is ever narrowed to exclude merges, that
silently undoes the fix in this view.

## Decided: `--review` shows merge commits by default

`commits` drops merge commits and rejects `--no-merges` outright
(`NO_MERGES_MSG`, `commits/args.rs:520`, raised at `:267`). Inherited as-is,
`wt 1,5 merge --review` would not show a merge commit that is about to come
over — the one thing the flag exists to report. **`--review` flips the default:
merges are kept, as if `--merges` had been passed.**

The justification is the `commits` default's own reasoning, not an exception to
it. Merges are dropped there because on a long-lived branch they are noise —
"on a branch that merges often they are most of the table"
(`commits/args.rs:147`). A review range is the opposite case: it is bounded by
the merge about to happen, and a merge commit inside it is *cargo*, not noise.
Same principle, different range, different answer.

### This makes `--no-merges` valid under `--review`

It has to. Today's hard error reads "merge commits are dropped already" — true
in `commits`, **false** under `--review` once the default flips, and an error
message that lies is worse than the flag it refuses. So under `--review`,
`--no-merges` is accepted and means what it says; `--merges` becomes the no-op
in that direction.

That is not an inconsistency in the shared vocabulary: the guard is conditional
on the actual default, so the message stays true in both modes. It does mean
`NO_MERGES_MSG` can no longer be raised unconditionally at `:267`.

### Implementation note

`parse_commits_args` takes only `args` (`commits/args.rs:232`), so the default
has to become a parameter — `parse_commits_args_with(args, merges_default:
bool)`, with today's `parse_commits_args` as a thin caller passing `false`.
Same shape as the `merge_dry_run` split in step 4: the behaviour is lifted into
a parameterised inner function and the existing entry point stays a one-liner
over it, so no call site changes and there is no second parser. Six call sites
in total (`commits/mod.rs:41` plus five in tests), all unaffected.

### Watch the polarity

`CommitsArgs.merges` is the **positive** ("keep them"), but `commit_rows` takes
**`no_merges: bool`** (`rows.rs:262`) and pushes `--no-merges` from it
(`rows.rs:280`). So `merges_default` crosses an inversion between parse and
rows.

This is the failure mode where a refactor flips the sense once, and the test
asserts against the same wrong constant and passes. Two guards, both cheap:

- Name the polarity in the signature — `merges_default` stays positive
  throughout the parse layer, and the single inversion happens at the one call
  into `commit_rows`, not scattered.
- **Assert both directions** in step 5, not just the new one. The existing
  `no_merges_drops_only_the_merge_commits` (`rows.rs:1427`) already
  parameterises on `no_merges` (`:1456-1457`) — reuse that shape rather than
  inventing one.

### A merge row cannot carry the `≈` mark

`rows.rs:532` hardcodes `--no-merges` in the patch-id walk, and correctly:
"Merges carry no patch of their own, and `git cherry` skips them too." That
stays unconditional.

Consequence of the Q4 flip: `--review` puts merge rows into a table whose
`≈` / `--no-cherry` column structurally cannot speak about them. This is right
behaviour, but it is **new** — `commits` never showed a merge row by default, so
the gap has never been visible. It needs a line in the help (and should not be
read as a bug on first sighting), and `--review --pick-id` on a merge row wants
a documented answer rather than a surprising one.

### The flip makes one empty-output path *less* likely

`window_to_divergent` (`rows.rs:461-465`) names a real empty-table case:
"`--no-merges` dropped the only commits the others were missing". Under the old
default a review range whose only cargo was a merge would print nothing.
Flipping the default removes that path for `--review` — the decision helps here
rather than costing.

## Rejected alternatives

### A separate `review` command

`merge --review` keeps the argument order and destination semantics `merge`
already owns. A standalone command would redefine both and sit next to
`commits` doing most of what `commits` does.

### Hint-only: print the equivalent `commits` line

```
hint: 'git-wt 5,1 commits -f' shows the commits and files coming over
```

Cheap, zero new grammar, teaches the existing tool. Cut because it makes the
user run a second command with a reversed argument order to get an answer
`merge` was already positioned to give.

### Dropping the colliding short aliases

An earlier draft had `--review` reject `-f`, `-a`, `-c`, `-d` as ambiguous and
require long spellings. Rejected — but not for the reason that draft gave. The
draft claimed `start_only_flags` had already removed merge's claim on those
shorts, which is false: it rejects after the claim, and only under `dry_run`.
The ambiguity is real. The verbatim handoff resolves it; alias-dropping was
solving it in the wrong place.

## Build order

Each step is independently testable and lands before the one that needs it.

1. **`parse_commits_args_with(args, merges_default)`** — thin-caller refactor.
   `parse_commits_args` passes `false`, so no call site changes and no
   behaviour change *on that path*. **Includes making the `NO_MERGES_MSG` guard
   (`args.rs:267`) conditional on `merges_default`** — without it the `true`
   path rejects `--no-merges` with a message that is false under `--review`,
   and the failure surfaces two steps from its cause (step 4, when `true` is
   first passed).
2. **Split the merge-tree probe** out of `merge_dry_run` so the verdict is
   reusable. `merge_dry_run` becomes a thin caller; its output and exit codes
   are unchanged, which the existing E2E cases already pin.
3. **`--review` in `parse_merge_args`** — stop parsing, return the tail, exempt
   the positional source, error on merge flags seen before it.
4. **Wire it up** — render over `dest..src`, header from the verdict, exit 0/1.
   Includes the two pieces of user-facing text the `≈` gap owes (help line: the
   cherry column cannot speak about merge rows; a documented answer for
   `--review --pick-id` on one). These ship with **this** step, not after — the
   blank `≈` has no precedent for a user to pattern-match against, so it reads
   as a bug unless the text is already there.
5. **Tests.** Three that matter beyond the obvious:
   - **`--review --filename` in a merge-carrying range.** The one that
     regresses silently — `2c7b804` exists because `git log -- <path>` prunes
     merges, and `dest..src` is exactly where that bites. Must be its own
     real-repo test, not inferred from the `commits` suite.
   - **Both polarities of the merges default**, per the inversion above:
     `--review` keeps merge rows, `--review --no-merges` drops them, plain
     `commits` is untouched.
   - **The handoff boundary** — `--review -f` means files (not force),
     `--review -af` bundles, `merge -f --review` errors, and
     `wt 1 merge feat/x --review` still consumes its positional.

## Settled

1. **`--dry-run` is unchanged.** It stays the quick confirmation — one line, its
   own exit contract, no new output. `--review` is the verbose sibling, not a
   replacement, and nothing about `dry-run` moves. Callers of
   `if wt 1,5 merge dry-run; then` see no change.
2. **Bare `--review` does not imply `--files`.** Parity with `commits`, and the
   header already answers the first question (how many commits, does it merge
   cleanly). File blocks are long enough that they should be asked for; `-f` is
   one keystroke, and it works because of the verbatim handoff.
3. **A merge in progress needs no special case — `dest..src` is already
   correct.** A stopped merge has not committed, so `HEAD` has not moved: the
   destination is still exactly where it was when the merge began, and
   `dest..src` still names precisely the commits that have yet to land. There is
   nothing for a `MERGE_HEAD`-relative range to fix. `--review` therefore follows
   `dry_run` and reports before the `in_progress` guard
   (`merge.rs:285-286`), the same way and for the same reason.
4. **Merges are shown by default under `--review`** — the default flips. See the
   section above; it is the only one of these that changes what the command
   prints.

No open questions remain. The design is ready to build.
