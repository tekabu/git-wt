# `git-wt commits --files`

Proposed flag for the `commits` command that adds, under each commit, the list of files the commit touched with a short change summary.

---

## Motivation

`git-wt 12,3 commits` already answers "who has what commit". Often the next question is "and what files did that commit touch?"  Today you have to run `git show --stat <sha>` per commit.  `--files` answers that inline, scoped to the rows the table is already showing.

---

## Proposed syntax

```sh
git-wt 12,3 commits --files
git-wt 12,3 commits --author regoso -n 10 --files
git-wt 12,3 commits --files --no-merges
```

Opt-in only.  It adds work proportional to the number of displayed commits, so it pairs naturally with `-n` and filters.

---

## Output format

Each file appears on its own line under its commit, tab-indented.  A blank line
separates the commit row from its file block.

```text
commit    pick      author             date  uat  deploy-uat  subject
8b0e6d41            john.regoso  2026-07-17   ✓       ·       Merge branch 'main-feat-issue-36-rewards-api' into uat

	M  src/rewards/api.rs       +12  -3
	A  README.md                +0  -1
	M  tests/rewards.rs          +45  -0
f32ab908  b1cb85a1  john.regoso  2026-07-16   ✓       ≈       [fix] Response issue

	M  src/rewards/response.rs  +21  -4
068d3661            john.regoso  2025-07-16   ✓       ✓       Rewards-v106 : points per objective

	M  src/rewards/points.rs   +112  -9
	M  src/rewards/db.rs        +33 -12
	A  migrations/20250716.sql    +1  -0
210bf55c            john.regoso  2025-01-15   ✓       ✓       fix(Merge): Conflict

	M  src/rewards/merge.rs      +8  -8
d3a784b1            john.regoso  2024-12-19   ✓       ✓       [rewardsAppTotalSpent] Update schedule

	M  src/schedule.rb          +15  -3
ca613432            john.regoso  2024-12-06   ✓       ✓       Total time spent computation - Task scheduled

	M  src/time.rs              +67 -13
```

### Columns inside the file block

| field | meaning |
|---|---|
| status letter | `A` added, `M` modified, `D` deleted, `R` renamed, `C` copied, `T` type-changed, `U` updated-but-unmerged |
| path | full relative path, left-aligned; renames shown as `old => new` |
| `+N` | lines added; `-` for binary files |
| `-M` | lines removed; `-` for binary files |

Right-align the `+N` and `-M` columns within the block so the numbers line up vertically.

---

## Implementation approach

### 1. Add the option

In `CommitsArgs` (`src/main.rs`):

```rust
files: bool,
```

Parse `--files` in `parse_commits_args`.  Reject unknown companions if necessary; `--files` is standalone.

### 2. Gather file stats for displayed commits only

After filtering and `-n` truncation in `cmd_commits`, for each surviving `CommitRow`:

```bash
git rev-list --parents -n 1 <commit>          # first parent (or none for root)
git diff-tree -r --name-status -M -C <base> <commit>
git diff-tree -r --numstat -M -C <base> <commit>
```

- `<base>` is the first parent, or the empty tree for root commits.
- Merge commits diff against the first parent only (not the combined merge).
- `--numstat` gives `added<tab>removed<tab>path`; binary files show `-` for counts.
- `-M`/`-C` detect renames and copies; renames display as `old => new`.

Store per-row:

```rust
struct FileStat {
    status: char,
    path: String,
    added: Option<usize>,
    removed: Option<usize>,
}
```

### 3. Render

Extend `render_commits` with a `&[Vec<FileStat>]` aligned to `rows`.  After the commit row and any wrapped-subject continuations, print a blank line then each file line tab-indented:

```
\tM  <path-padded>  +<added>  -<removed>
```

Padding should be calculated once per block so `+` / `-` columns line up.

### 4. Markdown output

`write_md` should render the same file block under each commit row, using markdown indentation (four spaces or a tab).  Keep the table valid markdown.

---

## Performance note

Cost is one `git diff-tree` / `git show` per **displayed** commit.  With `-n 20` this is negligible.  Without `-n` on a large shared history it can be slow, but the user explicitly asked for it and the rows already limit what is queried.  No extra work is done for commits filtered out before rendering.

---

## Open questions

1. Should `--files` accept a path limit like `commits --files -- src/rewards/`?  Possibly useful; can be deferred.
2. Should renames show the old path too?  `git diff-tree -M -r --numstat` shows `old -> new`; matching that is fine.
3. Should merge commits default to first-parent only, or list all parents' combined changes?  First-parent is less noisy and matches normal log reading.
