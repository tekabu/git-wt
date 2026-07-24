# PLAN: clap migration + verb-first grammar rework

## Why

`src/cli.rs` + `src/main.rs` hand-roll ~1200 lines of arg parsing: two
overlapping grammars (target-first `<N> <verb>` and verb-first bare
`<verb>`), manual bundling of short flags, manual `--help` slicing out
of a giant string constant (`HELP`/`SHORT_USAGE` kept in sync by hand).
Every new flag needs edits in 3+ places (parser, `HELP` text, `VERB_WORDS`/
`verb_sections`/`usage_snippet`). This plan:

1. Replaces the parser with `clap` (derive API).
2. Collapses the two grammars into one **verb-first** grammar.
3. Gives every command its own module folder, holding its clap `Args`
   struct, the command body, and (where relevant) rendering — the shape
   `src/cmd/commits/` and `src/cmd/log/` already use, generalized to
   every command.

No behavior beyond the grammar/parsing layer changes: git semantics,
render output, marks, filters, etc. all stay as documented in `HELP` —
only how a command line reaches them changes.

## New grammar

One shape for every command:

```
git-wt <VERB> [TARGET_LIST] [-b/--branch TARGET_LIST] [FLAGS...]
```

- `VERB` — full word or alias (`add`/`a`, `list`/`ls`, `commits`/`c`,
  `log`/`l`, `merge`, `merged`/`m`, `diff`, `meld`, `remove`/`rm`,
  `switch`/`cd`/`s`, `path`/`show`, `fetch`, `pull`/`p`, `push`,
  `doctor`, `version`).
- `TARGET_LIST` — a single positional: comma-separated, no spaces,
  each part a worktree number or branch name (`heads/<name>` forces
  branch reading over number). **First part is optional and defaults
  to the current worktree** for every command that accepts a target
  list at all (`commits`, `log`, `diff`, `meld`, `merge`, `merged`,
  `fetch`/`pull`/`push`). Bare number-first dispatch (`git-wt 1 log`)
  is retired — the verb always comes first.
- `-b`/`--branch TARGET_LIST` — an alternative/additive way to spell
  extra targets, same comma-list syntax, appended after whatever the
  positional already named. `git-wt commits -b 2` ==
  `git-wt commits <cur>,2`; `git-wt commits 1 -b 2,3` ==
  `git-wt commits 1,2,3`. This replaces today's "prepend current
  worktree" `-b` behavior — with a positional always available and
  defaulting to current, `-b` becomes purely "add more targets," no
  special prepend rule to explain.
- `FLAGS` — unchanged per-command option set (`--author`, `--stat`,
  `--rebase`, ...), parsed by clap instead of the hand-rolled bundler.
  Short-flag bundling (`-af`, `-fn 20`) is what clap gives natively via
  derive `#[arg(short, long)]` — no bespoke bundler code needed.

### Worked examples (old -> new)

| Old | New |
|---|---|
| `git-wt 1,2 commits` | `git-wt commits 1,2` |
| `git-wt 2 commits` | `git-wt commits 2` |
| `git-wt commits` (bare, current worktree) | `git-wt commits` (unchanged — target list optional) |
| `git-wt commits -b 2` (prepend current) | `git-wt commits -b 2` (unchanged meaning, simpler rule) |
| `git-wt 1,2 log src/ui.rs` | `git-wt log 1,2 src/ui.rs` |
| `git-wt 1 remove -y -f` | `git-wt remove 1 -y -f` |
| `git-wt 1 path` | `git-wt path 1` |
| `git-wt 1` (bare switch) | `git-wt switch 1` (bare number-first dispatch removed; see Open Questions) |
| `git-wt 1,2 merge` | `git-wt merge 1,2` |
| `git-wt merge -b 2` (merge into current) | `git-wt merge -b 2` (unchanged) |
| `git-wt add feature/login` | `git-wt add feature/login` (unchanged — already verb-first) |
| `git-wt list --col 1,2` | `git-wt list --col 1,2` (unchanged) |

Net effect: every command becomes `git-wt <verb> ...`, full stop. The
asymmetry where `add`/`list`/`remove`/`commits`/`log`/sync verbs already
worked bare but `merge`/`merged`/`diff`/`meld` needed target-first only
is gone — all of them now take an optional leading target list the
same way.

### Comma list as one positional, not `n` positionals

Clap can accept `Vec<String>` positionals, but git-wt's list is a
*single token* (`1,2,main`), matching branch names that may contain
`/` and `-` freely. Keep it a single `String` positional per command,
parsed by the existing `parse_target_list`/`resolve_target_list`
helpers in `cli.rs` (kept, not rewritten — they already do exactly
this job and are well-tested). clap's role is argv splitting and flag
parsing; target-list semantics stay hand-written since they're
domain logic, not generic parsing.

## Folder structure

Every command gets its own directory under `src/cmd/`, dropping the
current flat-file commands (`add.rs`, `diff.rs`, `list.rs`, `merge.rs`,
`merged.rs`, `meld.rs`, `remove.rs`, `sync.rs`, `doctor.rs`) down to the
`commits/`/`log/` shape:

```
src/cmd/
  mod.rs              # re-exports; Commands enum lives in src/cli/mod.rs
  add/
    mod.rs            # cmd_add body
    args.rs           # AddArgs (clap derive)
  list/
    mod.rs            # cmd_list, cmd_switch (picker) body
    args.rs           # ListArgs
  remove/
    mod.rs
    args.rs
  switch/
    mod.rs            # switch/cd/s + path/show (two thin verbs, one module: both just resolve+print/cd)
    args.rs
  commits/             # already this shape — args.rs/mod.rs/render.rs/rows.rs unchanged
    args.rs
    mod.rs
    render.rs
    rows.rs
  log/                 # already this shape — unchanged
    args.rs
    mod.rs
    paths.rs
    render.rs
  diff/
    mod.rs
    args.rs
  meld/
    mod.rs
    args.rs
  merge/
    mod.rs
    args.rs
  merged/
    mod.rs
    args.rs
  sync/                # fetch/pull/push share one SyncOp today; keep that
    mod.rs
    args.rs
  doctor/
    mod.rs
    args.rs
```

Rule going forward: **any command whose flag count exceeds ~5, or
whose parsing has its own edge cases, gets `args.rs` split out; a
command with no flags (e.g. `version`) stays a bare function in
`src/cmd/mod.rs`.** `commits`/`log` additionally split `render.rs`
(table rendering) and `rows.rs`/`paths.rs` (data gathering) because
those are large; other commands don't need that split yet and
shouldn't preemptively add it.

## clap architecture

```rust
// src/cli/mod.rs
#[derive(Parser)]
#[command(name = "git-wt", version, disable_help_subcommand = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>, // None => bare `git-wt` == interactive switch
}

#[derive(Subcommand)]
enum Commands {
    #[command(alias = "ls")]
    List(list::ListArgs),
    #[command(alias = "a")]
    Add(add::AddArgs),
    #[command(alias = "rm")]
    Remove(remove::RemoveArgs),
    #[command(alias = "cd", alias = "s")]
    Switch(switch::SwitchArgs),
    #[command(alias = "show")]
    Path(switch::PathArgs),
    #[command(alias = "c")]
    Commits(commits::CommitsArgs),
    #[command(alias = "l")]
    Log(log::LogArgs),
    Diff(diff::DiffArgs),
    Merge(merge::MergeArgs),
    #[command(alias = "m")]
    Merged(merged::MergedArgs),
    Meld(meld::MeldArgs),
    Fetch(sync::SyncArgs),
    #[command(alias = "p")]
    Pull(sync::SyncArgs),
    Push(sync::SyncArgs),
    Doctor(doctor::DoctorArgs),
    Version,
}
```

Shared target-list type used by every `*Args` struct that takes one:

```rust
// src/cli/target.rs
#[derive(Clone, Debug, Default)]
pub struct TargetList(pub Option<String>); // raw comma string, or absent -> current worktree

impl clap::builder::ValueParserFactory for TargetList { ... } // thin wrapper, defers to existing parse_target_list
```

Each `*Args` struct looks like:

```rust
#[derive(clap::Args)]
pub struct CommitsArgs {
    targets: Option<String>,       // "1,2,main" — positional, optional
    #[arg(short = 'b', long = "branch")]
    branch: Option<String>,        // additive, same comma syntax
    #[arg(short, long)]
    limit: Option<usize>,
    #[arg(short, long)]
    all: bool,
    // ... existing COMMITS OPTIONS, one field each
}
```

`--help` becomes clap-generated per-subcommand (`git-wt commits --help`)
instead of the hand-sliced `command_help`/`usage_snippet`/`sections_text`
machinery in `main.rs` — that ~150 lines goes away entirely. The long
prose sections in today's `HELP` (COMMITS, LOG, MARKS, MERGE, MELD,
SYNC, ADD, DOCTOR, STDOUT, COLOR) don't fit clap's `--help` well as
flag docs; keep them as a `git-wt <verb> --guide` (or a `docs/` manual)
rather than fighting clap's help renderer for prose paragraphs. Decide
this in the Open Questions section below before implementing.

## Migration phases

1. **Add clap dependency**, define `Cli`/`Commands` skeleton alongside
   the existing hand-rolled dispatcher (`main.rs` keeps calling old
   code paths; clap only replaces the top-level `match args.first()`
   routing). Get every subcommand routing to today's `cmd_*` functions
   unchanged, just via clap-parsed args instead of raw `Vec<String>`.
2. **Per-command folder moves**, one command at a time (`add` first —
   smallest surface, already verb-first, lowest risk). Move file into
   `src/cmd/<verb>/mod.rs` + `args.rs`, port its existing hand-rolled
   flag parsing (e.g. `add.rs`'s own arg loop) to a clap derive struct.
   Existing unit tests for that command's argument parsing move
   with it and get rewritten against the derive struct.
3. **Retire target-first dispatch** (`dispatch_target`/`dispatch_targets`
   /`resolve_target`/`check_index` machinery in `cli.rs`) once every
   verb is reachable verb-first. `resolve_target_list`/`parse_target_list`
   /`worktree_on_branch` survive — they're target-string resolution,
   not dispatch, and every new `*Args` struct still calls them.
4. **Drop the hand-written `HELP`/`SHORT_USAGE` constants** once clap's
   generated help covers the option tables; migrate the prose sections
   per the Open Questions decision.
5. **Update `README.md`, `_alias.sh`, install scripts** for the new
   invocation shape, and re-run `docs/baseline-tests.txt` /
   `test-mac.sh` / `test-linux.sh` against the new grammar.

Each phase should land as its own PR/commit — this is a big enough
change that squashing it into one commit makes review and bisection
useless if something regresses.

## Open questions (resolve before/while implementing)

- **Bare number dispatch (`git-wt 1`, `git-wt 1,2 diff`)**: fully
  retired, or kept as a deprecated compat shim that prints a migration
  hint and forwards to the verb-first form? Given how much of `cli.rs`'s
  complexity exists purely to support target-first, retiring it
  outright is what actually simplifies things — but it's the most
  visible breaking change for existing muscle memory / scripts using
  `git-wt <N> <verb>`.
- **Prose help sections** (COMMITS/LOG/MARKS/MERGE/MELD/SYNC/ADD/DOCTOR
  explanations): move into each command's `long_about`/`after_help` in
  the derive struct (stays in `--help`, but clap's wrapping/formatting
  differs from the hand-tuned current layout), or split out to
  `docs/MANUAL.md` and have `--help` point at it? Recommend: short
  clap-generated `--help` per verb (flags only, like today's
  `SHORT_HELP_SECTIONS`), full prose moved to a manual doc, since that's
  already the split `git-wt --help` vs `git-wt --help -f` draws today.
- **`-b`/`--branch` semantics change**: today it *prepends* current
  worktree unconditionally, even to commands invoked target-first
  (`git-wt -b 1,2 commits`, no target before `-b`). New grammar makes
  the positional always-present-and-defaulting, so `-b` becomes pure
  "append these too." Confirm this reading matches intent for `merge`
  specifically, since `git-wt merge -b 2` today means "merge 2 into
  current" (2 is the *source*, current is the *dest*, order matters)
  — the append reading must preserve that dest-first ordering
  (`[current, 2]` interpreted as `<cur>,2 merge`, unchanged).
- **`--all` for fetch/pull/push**: stays a flag on `sync::SyncArgs`
  (`git-wt pull --all`), independent of the target-list positional;
  confirm clap rejects positional+`--all` together with today's
  message ("'--all' is every worktree, so a worktree list has nothing
  to add") rather than a generic clap conflict error — likely needs a
  manual post-parse check, not a clap `conflicts_with`, to keep the
  worded hint.
- **`heads/<name>` escape hatch, alias-shadows-branch warnings,
  `resume_word` (`continue`/`abort` for merge)**: all remain hand
  logic post-parse (clap doesn't know about worktrees or branches);
  confirm they're re-hung off the new `*Args` structs in the same
  places they check today.

## Testing plan

- Every existing `cli.rs`/per-command `#[cfg(test)]` module ports over
  1:1 against the new `*Args` structs — these tests check target-list
  resolution and flag semantics, not argv shape, so most assertions
  don't change, only what constructs the `Args` value under test.
- `docs/baseline-tests.txt` and `test-mac.sh`/`test-linux.sh` (already
  in the repo) need every example invocation rewritten to verb-first
  before they'll pass — treat that rewrite as part of phase 5, not a
  follow-up.
- Add a small integration test invoking the built binary
  (`assert_cmd`-style, or plain `std::process::Command`) for the
  `-b`/positional-append interaction and the retired-bare-number-dispatch
  error message, since those are exactly the two behavior seams this
  migration touches.
