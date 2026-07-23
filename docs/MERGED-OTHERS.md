# `git-wt <N> merged --others`

## What it does

Select one worktree and list every worktree, showing whether its branch is already fully merged into the selected branch and, if so, when it was last merged there.

```text
git-wt <N> merged --others     # list all worktrees relative to worktree N
git-wt <N> merged --others -p  # include the path column
```

## Columns

| column | id | meaning |
|--------|----|---------|
| `#` | 1 | worktree number |
| `branch` | 2 | branch name |
| `status` | 4 | clean / dirty / untracked |
| `last-commit` | 5 | relative time of the branch's own latest commit |
| `merged` | 7 | `merged`, `ahead N`, `self`, or `-` relative to the selected tree |
| `merged-at` | 8 | when the branch was last merged into the selected tree |

`last-commit` was previously rendered as `last`; the header now matches the `--col` documentation.

## Example

```text
$ git-wt 1 merged --others
#  branch              status  last-commit         merged  merged-at
1  main                clean   10 minutes ago      self    -
2  align-diff          clean   25 hours ago        merged  24 hours ago
3  feat/commits-table  clean   12 hours ago        merged  -
4  live-diff           clean   24 hours ago        merged  24 hours ago
5  merge               clean   24 hours ago        merged  24 hours ago
6  merged              clean   23 hours ago        merged  23 hours ago
7  show-files          clean   10 minutes ago      merged  -
8  worktree-status     clean   10 minutes ago      merged  -
```

A `-` in `merged-at` means either:

- the row is the selected tree itself (`self`),
- the branch is not yet merged into the selected tree (`ahead N`), or
- the branch was brought in by fast-forward (no merge commit on the ancestry path).

## How `merged-at` is computed

1. `git merge-base --is-ancestor <branch> <selected-tree-branch>` decides if it is merged.
2. If merged, `git log -1 --ancestry-path --merges --format=%ar <branch>..<selected-tree-branch>` finds the most recent merge commit that first made the branch reachable.
3. If no merge commit exists (fast-forward), the cell is `-`.

## New `--col` ids

The normal `git-wt list` also accepts the new columns, using the branch you are currently standing in as the reference:

```text
git-wt list --col 1,2,7,8
```

`git-wt list` now shows `merged` (col 6, relative to the branch you are
standing in) by default on a terminal — no `--col` needed. The `merged`
command's grammar and `--others` flag are unchanged; this only changes what
`list` shows without asking.

## Implementation notes

- Added in `src/main.rs`:
  - `--others` handling in the single-target `merged` dispatch.
  - New `cmd_merged_others` and `cmd_list_with_ref` functions.
  - New helpers `merged_status_text`, `merged_text_at`, and `last_merge_date`.
  - Column ids 7 and 8; header `last` renamed to `last-commit`.
- Tests: `parse_cols` accepts `7` and `8`; `col_header` returns `last-commit`, `merged`, `merged-at`.
