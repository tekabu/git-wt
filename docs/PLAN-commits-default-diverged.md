# Plan: `git-wt <N>,<M>[,...] commits` — merge-request-style range of the first branch

## Goal

Change the default `git-wt 1,2,3,4 commits` so it no longer dumps the whole log of worktree 1. Instead, show a **slice of branch 1’s commits** defined by where the other branches diverged, while still checking every column against its full history.

## New default behavior

For `git-wt 1,2,3,4 commits`:

1. For each source branch `i` in `{2, 3, 4}`, compute the commits that are in branch 1 but **not** in branch `i`:
   ```
   git rev-list i..1
   ```
   This is exactly the set branch 1 would bring if merged into branch `i`.

2. Union those sets and find the **earliest** (oldest) commit in the union — the furthest-back commit on branch 1 that some other branch is missing.

3. Rows = all commits in branch 1 **from that earliest commit (inclusive) up to branch 1’s HEAD**.

4. Columns still use each branch’s full `rev-list`, so a commit already merged into branch 2 or 3 still shows `✓` if it falls inside the range.

### Git equivalent

```bash
# oldest commit in (2..1 ∪ 3..1 ∪ 4..1)
oldest=$(git rev-list --reverse --ancestry-path 2..1 3..1 4..1 | head -1)

# branch 1 from that commit (inclusive) to HEAD
git log --author-date-order 1 --not ${oldest}^@
```

`${oldest}^@` means "oldest’s parents"; excluding them includes `oldest` itself. For a root commit, there is no parent, so we fall back to the full branch-1 log.

### Example

```
1 (main):  A --- B --- C --- D --- E
                ↑           ↑
2 (feat):       F --------- G
3 (fix):        H
```

- `2..1` = C, D, E  (commits in main not in feat, because feat forked at B and merged back through G)
- `3..1` = B, C, D, E  (fix forked at A, so it is missing B onward)
- Union = B, C, D, E
- Earliest = B
- Rows = B, C, D, E (B is included)

Table:

```
commit   1  2  3  subject
B        ✓  ✓  ✓  shared base (already in 2 and 3)
C        ✓  ✓  ·  main work
D        ✓  ·  ✓  more main work
E        ✓  ·  ·  latest main work
```

## `--all`

`git-wt 1,2,3,4 commits --all` keeps the **current** behavior:

- Rows = full log of branch 1.
- This answers "show all commit of 1st branch".

## `--union`

`git-wt 1,2,3,4 commits --union` stays unchanged:

- Rows = union of the full logs of every listed branch.
- Still answers "who is out of sync with who".

`--union` and `--all` cannot be combined because they are two different row-source modes.

## Edge cases

- **No source branch is missing anything from branch 1** — the union is empty. Print `no commits ahead of <branch 1>` and exit cleanly.
- **Single target** (`git-wt 2 commits` while standing in worktree 1) — the internal list is `[here, 2]`. Apply the same rule: rows = branch 1’s commits from the oldest commit in `2..here` up to `here`.
- **Root commit as the earliest** — fall back to full branch-1 log, which is correct because there is nothing older to exclude.
- **Already merged commits** — included as rows if they fall inside the computed range, and their marks reflect each branch's full history.

## Implementation

### 1. `CommitsArgs` struct

Add a field:

```rust
all: bool,
```

### 2. `parse_commits_args`

- Accept `--all`.
- Remove the current `ALL_MSG` error for `--all`.
- Reject `--all` together with `--union`.

### 3. New helper

```rust
/// The oldest commit on `target` that any source branch is missing.
fn divergence_base(root: &Path, target: &str, sources: &[String]) -> Result<Option<String>, String> {
    let mut args = vec!["rev-list", "--reverse", "--ancestry-path"];
    for src in sources {
        args.push(&format!("{src}..{target}"));
    }
    let out = git_stdout(root, &args)?;
    Ok(out.lines().next().map(|s| s.to_string()))
}

/// A base expression that, when excluded, leaves `sha` and its descendants
/// in `target` visible. For a root commit we return None (exclude nothing).
fn inclusive_base(root: &Path, target: &str, sha: &str) -> Result<Option<String>, String> {
    let parents = git_stdout(root, &["rev-list", "--parents", "-n", "1", sha])?;
    let has_parent = parents
        .lines()
        .next()
        .map(|line| line.split_whitespace().count() > 1)
        .unwrap_or(false);
    if has_parent {
        Ok(Some(format!("{}^@", sha)))
    } else {
        Ok(None)
    }
}
```

### 4. `cmd_commits` row-source logic

Replace the current simple selection with:

```rust
let (row_refs, base_str): (&[String], Option<String>) = if args.union {
    (&refs, None)
} else if args.all {
    (&refs[..1], None)
} else {
    match divergence_base(root, &refs[0], &refs[1..])? {
        Some(oldest) => (&refs[..1], inclusive_base(root, &refs[0], &oldest)?),
        None => {
            eprintln!("no commits ahead of {}", label(&trees[idxs[0]]));
            return Ok(());
        }
    }
};
let base = base_str.as_deref();

let all_rows = commit_rows(
    root,
    row_refs,
    base,
    git_limit,
    order,
    args.fmt,
    args.no_merges,
)?;
```

The column sets (`ref_shas`) still use `None` as base so marks reflect each branch’s full history.

### 5. Empty-result message

When `--all` returns no rows, keep the current message:
```
no commits on <branch 1>
```

When `--union` returns no rows, keep:
```
no commits
```

When the new default returns no rows (handled above):
```
no commits ahead of <branch 1>
```

### 6. Help text updates

Change the examples in `HELP`:

```
git-wt 1,2,3 commits         # branch 1 commits the others are missing
git-wt 1,2,3 commits --all   # 1's full log, checked against 2 and 3
git-wt 2 commits             # worktree 2 vs the one you are in
git-wt 1,2 commits -n 20     # newest 20 rows of the slice
git-wt 1,2,3 commits --union # every branch's full log as rows
```

Update the prose to say the default rows are a slice of branch 1 defined by the other branches’ divergence, while `--all` gives the full branch-1 log and `--union` gives the full union.

### 7. Tests

#### Unit test update

In `commits_args_take_a_limit_and_all`:

- `--all` now parses and sets `all = true`.
- `--all --union` errors.

#### New integration test

Extend `commit_rows_stop_at_the_common_ancestor` or add a new test that verifies:

1. Default slice: with the example repo, `git-wt 1,2,3 commits` shows the correct slice of branch 1.
2. Already-merged commits inside the slice still appear with `✓` in the relevant columns.
3. `--all` returns the full branch-1 log.
4. `--union` returns the full union.

## Files to edit

- `src/main.rs` only.
