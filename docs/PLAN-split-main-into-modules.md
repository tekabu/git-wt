# Plan: split `src/main.rs` into modules

Branch: `refactor/split-main-into-modules`

## Current state

- `src/main.rs` — 6964 lines, the entire crate.
- Zero external dependencies (`Cargo.toml` has an empty `[dependencies]`).
- One test module: `#[cfg(test)] mod tests` at line 5461, ~1500 lines, 69 `#[test]` fns, single `use super::*;`.
- Everything is private (no `pub`), so a split needs visibility added at every module boundary.

## Goal

Same binary, same behavior, same tests passing. Only file layout and visibility change. No logic edits, no renames, no signature changes in the same commits as the moves.

## Target layout

```
src/
  main.rs          entry point only: `mod` decls, main(), run(), HELP, VERSION
  cli.rs           arg dispatch
  worktree.rs      Worktree type + discovery
  git.rs           git process wrappers
  ui.rs            color, width, table rendering primitives
  cmd/
    mod.rs
    list.rs
    add.rs
    remove.rs
    merge.rs
    merged.rs
    diff.rs
    meld.rs
    sync.rs
    commits.rs
```

## Module contents

### `main.rs` (keep)
`VERSION` (14), `HELP` (16-482), `main` (483), `run` (503).

`run` **stays in `main.rs`** — only the dispatch *helpers* move to `cli.rs`. After the split `run` is ~75 lines of top-level routing that calls into `crate::cli::*` and `crate::cmd::*`. Moving it to a `cli::run()` and reducing `main.rs` to a one-line call is a defensible follow-up, but it is not this refactor.

### `cmd/mod.rs`
Module declarations only, no code:

```rust
pub(crate) mod add;
pub(crate) mod commits;
pub(crate) mod diff;
pub(crate) mod list;
pub(crate) mod meld;
pub(crate) mod merge;
pub(crate) mod merged;
pub(crate) mod remove;
pub(crate) mod sync;
```

Add each line in the step that creates its file, not all at once in step 1 — a `mod` decl for a file that does not exist yet will not compile.

### `cli.rs`
`unknown_command_msg` (582), `branch_like` (595), `dispatch_target` (603), `parse_target_list` (761), `dispatch_targets` (775), `resume_word` (859), `check_index` (868), `show_path_from_rest` (885), `list_from_args` (891).

### `worktree.rs`
`struct Worktree` (495), `worktrees` (5349), `label` (5295), `ref_of` (2118), `here_index` (4056), `repo_root` (5311), `current_ref` (5328), `canon` (5283), `leaf_of` (5289), `sanitize` (5257), `sh_quote` (5277).

Plus the status cluster, which three command modules share: `enum Status` (1216), `classify_status` (1226), `worktree_status` (1238), `status_text` (1245), `status_color` (1255), `is_dirty` (2723). See shared-helper ownership.

### `git.rs`
`git_cmd` (5381), `git_run` (5388), `git_run_no_editor` (5409), `git_stdout` (5428), `git_bytes` (5440), `git_quiet` (5452), `on_path` (4811), `is_executable` (4822/4828, both cfg arms).

### `ui.rs`
`RESET`/`GREEN`/`YELLOW`/`RED`/`DIM` (1183-1187), `color_enabled` (1192), `paint` (1206), `term_width` (4346), `width_bound` (4372), `ellipsize` (4385), `ellipsize_wide` (4394), `split_at_width` (4546), `wrap_wide` (4570), `abbrev` (4286), glyph consts `CHECK`/`MISS`/`EQUIV`/`ELLIPSIS`/`PICK_HEAD` (4273-4280), `MIN_TEXTW`/`AUTHOR_MAX`/`BRANCH_MIN`.

### `cmd/list.rs`
`enum ListMode` (924), `cmd_list` (930), `col_header` (1105), `render_row` (1124), `COL_HELP` (1156), `parse_cols` (1158), `last_commit` (1266), `push_pull_text` (1276), `fuzzy_match` (1370), `is_subseq` (1376).

`merged_text` (1306) moves to `cmd/merged.rs`, and the status cluster (1216-1255) to `worktree.rs` — see shared-helper ownership below.

### `cmd/add.rs`
`cmd_add` (4836), `find_remote_branch` (4999), `resolve_add_path` (5021), `pick_branch` (5074), `new_branch_prompt` (5136), `fzf_pick` (5156), `number_pick` (5204), `confirm` (5238).

### `cmd/remove.rs`
`cmd_remove` (1389).

### `cmd/merge.rs`
`enum MergeOp` (1466), `enum Side` + impl (1477/1482), `struct MergeArgs` (1498), `parse_merge_args` (1526), `set_side` (1626), `set_merge_op` (1640), `start_only_flags` (1659), `cmd_merge` (1685), `merge_dry_run` (1842), `resolve_merge_source` (2056), `has_tracked_changes` (2082), `conflicted_files` (2089), `conflict_msg` (2097).

### `cmd/merged.rs`
`cmd_merged_others` (1891), `cmd_list_with_ref` (1903), `cmd_merged` (2017), `merged_text` (1306), `merged_status_text` (1314), `merged_text_at` (1333), `last_merge_date` (1351), `ahead_count` (2047).

## Shared-helper ownership

Verified by grepping call sites, not guessed. Settled here so it is not re-argued mid-refactor.

| Helper | Callers | Owner |
|---|---|---|
| `worktree_status` (1238) | `cmd_list:1006`, `cmd_list_with_ref:1932`, `is_dirty:2724` | `worktree.rs` |
| `status_text` (1245) | `cmd_list:1041`, `cmd_list_with_ref:1955` | `worktree.rs` |
| `classify_status`/`Status` (1216/1226) | `worktree_status:1240` + tests | `worktree.rs` |
| `status_color` (1255) | `render_row:1142` only | `worktree.rs` (travels with cluster) |
| `is_dirty` (2723) | `cmd_diff:2229` only | `worktree.rs` (travels with cluster) |
| `ahead_count` (2047) | `merged_status_text:1320`, `cmd_merged:2033` | `cmd/merged.rs` |
| `merged_text` (1306) | `cmd_list:1012` | `cmd/merged.rs` |
| `merged_text_at` (1333) | `cmd_list:1017`, `cmd_list_with_ref:1938` | `cmd/merged.rs` |
| `merged_status_text` (1314) | `merged_text:1310`, `merged_text_at:1340` | `cmd/merged.rs` |
| `last_merge_date` (1351) | `merged_text_at:1342` | `cmd/merged.rs` |
| `resolve_merge_source` (2056) | `run:712`, `cmd_merge:1726` | `cmd/merge.rs` |

Two clusters, both kept whole:

**Status → `worktree.rs`.** `is_dirty` does not call `classify_status` directly; it goes through `worktree_status`, whose three callers live in three different command modules (list, merged, diff). Owning this in `cmd/list.rs` would make `diff.rs` and `merged.rs` import from `list.rs` — command modules reaching into each other for status plumbing. `worktree.rs` is the shared layer they all already depend on, so the cluster goes there whole. `status_color` and `is_dirty` each have one caller and could technically live with it, but splitting a six-item chain across three files to save one import is the worse trade.

**Merge-status → `cmd/merged.rs`.** The module name is slightly overloaded: it holds both the `merged` command entry points (`cmd_merged`, `cmd_merged_others`) and the merge-status helper cluster that `list` also uses. Read the cluster as "merge-status", not "part of the merged command". Not worth a separate module for six fns. The one command-to-command edge that remains: `cmd_list` calls `merged_text` and `merged_text_at` for columns 6/7/8. `list.rs` imports those two entry points. Do not split this cluster either — `merged_text` is a one-line wrapper over `merged_status_text`, and separating them would put half a chain in each file.

`cmd/merge.rs` and `cmd/merged.rs` have **no** shared helpers in either direction, so their relative order does not matter.

### `cmd/diff.rs`
`cmd_diff` (2132), `struct Hunk` (2298), `struct FileDiff` (2306), `live_files` (2318), `same_bytes` (2335), `live_diff` (2350), `no_index_diff` (2412), `ref_diff` (2429), `split_patch` (2445), `parse_patch_into` (2495), `eat_patch_line` (2511), `parse_hunk_header` (2533), `parse_range` (2551), `status_paint` (2563), `render` (2571), `render_stat` (2653), `summary` (2703).

`is_dirty` (2723) is **not** here — it moved to `worktree.rs` with the status cluster. `cmd_diff:2229` calls it across that boundary.

### `cmd/meld.rs`
`struct MeldArgs` (2735), `parse_meld_args` (2746), `cmd_meld` (2777), `cmd_meld_filtered` (2836), `merge_base` (2917), `changed_paths_status` (2923), `changed_paths` (2936), `parse_name_status` (2944), `temp_meld_dir` (2963), `extract_files` (2977).

### `cmd/sync.rs`
`enum SyncOp` + impl (3000/3006), `struct SyncArgs` (3039), `ALL_HINT` (3047), `parse_sync_args` (3049), `sync_skip` (3109), `default_remote` (3122), `sync_argv` (3148), `cmd_sync` (3176).

### `cmd/commits.rs`
Everything from 3242 to 4810 minus the `ui.rs` extractions: `DateFmt`, `CommitRow`, `FileStat`, `DateOp`, `DateFilter`, `SubjectWidth`, `Wrap`, `CommitsArgs`, `parse_commits_args`, `parse_subjectw`, `parse_wrap`, all the `*_MSG`/`*_MISSING` consts (3584-3599, 3656), `parse_date_filter`, `strict_msg`, `iso_date`, `parse_limit`, `cmd_commits`, `commit_files`, `commit_rows`, `commit_of`, `older_than`, `reachable_from`, `divergent_set`, `window_to_divergent`, `equivalents`, `pick_ids`, `patch_ids`, `ref_shas`, `enum Mark` + impl, `md_filename`, `md_cell`, `write_md`, `render_commits`.

Biggest single unit (~1500 lines). Split further only if it stays awkward after the move — a follow-up `commits/{args,rows,render,md}.rs` is a separate commit.

## Tests

`mod tests` uses `use super::*;` and reaches many private fns. Two options:

1. **Move each test next to its subject.** Each module grows its own `#[cfg(test)] mod tests { use super::*; }`. Keeps `pub(crate)` surface minimal. Preferred.
2. Keep one `tests.rs` and mark everything it touches `pub(crate)`. Faster but leaks visibility for test-only reasons.

Go with 1. Map each test by the fn it names (e.g. `sanitize_collapses_separators` → `worktree.rs`, `add_path_default_is_sibling` → `cmd/add.rs`).

Not every name points at one fn. Tiebreak, in order:

1. The fn the assertions actually call — not the one the test name suggests. `tracked_changes_ignore_untracked_only` (5733) reads like a status test, but seven of its eight asserts are on `has_tracked_changes`, so it goes to `cmd/merge.rs`. Its last line asserts `classify_status` purely to contrast the two, so the test imports `Status`/`classify_status` from `worktree.rs`. A single contrasting assert does not move a test.
2. If assertions span two fns, the module owning the behavior under test, not the one supplying setup. A test that builds a repo with `git_run` to check `commit_rows` is a commits test.
3. Still ambiguous: leave it in whichever module it currently compiles against and note it in the commit message. Do not invent a `tests/common` module mid-refactor.

No test is cross-cutting — checked. The four module-scoped test helpers are each domain-local and travel with their tests: `merge_args` (5613) → `cmd/merge.rs`, `sync_args` (5618) → `cmd/sync.rs`, `hunk` (5887) → `cmd/diff.rs`, `sha_by_subject` (6617) → `cmd/commits.rs`. The repeated `git()` fixtures are nested *inside* individual test fns (5990, 6343, 6470, ...), so they move with the test that owns them. Deduping those into one shared fixture is a separate idea, not this refactor.

## Visibility

Default to `pub(crate)` on every moved item. No `pub` — this is a binary crate, nothing is an API. Add `pub(crate)` on struct fields the moment a cross-module read fails to compile; don't pre-emptively open them.

## Execution order

One commit per step. `cargo build && cargo test` green after each.

0. Record a baseline: `cargo test 2>&1 | tee docs/baseline-tests.txt`. The pass count at the end must match it exactly — that is the proof of "no behavior change".
1. Create empty `git.rs`, `ui.rs`, `worktree.rs`, `cli.rs` and an empty `cmd/mod.rs`; add `mod git; mod ui; mod worktree; mod cli; mod cmd;` to `main.rs`. Steps 5-10 then add their `pub(crate) mod ...;` lines to a `cmd/mod.rs` that already exists. Compiles, does nothing.
2. `git.rs` — leaf, no inbound deps. Move, add `pub(crate)`, fix call sites with `use crate::git::*`.
3. `ui.rs` — also a leaf.
4. `worktree.rs` — depends on `git.rs` only.
5. `cmd/sync.rs`, `cmd/remove.rs`, `cmd/meld.rs` — self-contained commands, smallest blast radius.
6. `cmd/merge.rs`, `cmd/merged.rs` — no shared helpers in either direction; order does not matter. Move whichever is convenient.
7. `cmd/diff.rs`.
8. `cmd/list.rs`.
9. `cmd/add.rs`.
10. `cmd/commits.rs` — largest, do it last when the shared layers are settled.
11. `cli.rs` — move the dispatch *helper* fns (`unknown_command_msg`, `branch_like`, `dispatch_target`, `parse_target_list`, `dispatch_targets`, `resume_word`, `check_index`, `show_path_from_rest`, `list_from_args`). **`run` stays in `main.rs`**, updated to call them via `crate::cli::*`. Mark each `cmd_*` `pub(crate)` during *its own* move (steps 5-10), not in a single visibility sweep here; assuming that was done, this step adds no visibility changes of its own.
12. Move tests into their modules. Prefer interleaving this into steps 2-11 — each module arrives with its own tests, and every commit stays green on its own. Doing so makes this step a no-op; delete it rather than leaving an empty commit. Keep it as a real step only if the moves run ahead and tests are left behind in `main.rs`.

## Every move is a three-part edit

Each step is not just a cut-and-paste. All three parts land in the *same* commit, or the build breaks:

1. Move the items into the new module, adding `pub(crate)`.
2. Add the `mod` decl (and, for `cmd/`, the line in `cmd/mod.rs`).
3. **Add the `use` in every file that still calls the moved items — including `main.rs`.** Easy to forget: after step 4, `run` is still in `main.rs` calling `repo_root()` and `worktrees()`, so that commit must add `use crate::worktree::{repo_root, worktrees};` to `main.rs`. Same shape for every later step.

Prefer explicit `use crate::foo::{a, b};` over glob `use crate::foo::*;` — the glob hides which module actually owns what, which is the thing this refactor exists to make visible.

## Verification per step

```
cargo build
cargo test
```

Clippy is **not** available on this toolchain — `cargo clippy` reports `'cargo-clippy' is not installed for the toolchain 'stable-aarch64-apple-darwin'`. It is not a gate for this refactor. Installing it (`rustup component add clippy`) and fixing whatever it finds on the pre-split file is worth doing, but as separate work: a lint sweep tangled into the moves would destroy the "pure relocation" property the diffs depend on.

Plus a manual smoke pass at the end: `git-wt`, `git-wt list --col 1,2,9,10`, `git-wt 1 commits`, `git-wt 1 diff 2`, `git-wt <N> merged`.

## Rules

- No behavior changes. If a move surfaces a bug, note it and fix it in a separate commit off this branch.
- No reordering fns within a move — keep diffs reviewable as pure relocations where possible.
- Doc comments travel with their items untouched.
- Keep `HELP` in `main.rs` as one string; splitting it per-command is a separate idea, not this refactor.
