# `live` — diff worktrees by file content, not by commit

Status: proposed. Depends on the `align-diff` branch (`git-wt <N>,<M> diff`).

## The problem

`git-wt <N>,<M> diff` compares the two worktrees' **committed state** — it runs
`git diff <branch N>..<branch M>`. Once you hand `git diff` two refs, the working
tree drops out of the comparison entirely. Uncommitted work is invisible.

The pathological case is two worktrees on the same commit:

```
$ git worktree list
/Users/me/code/git-wt        52c7aa4 [main]
/Users/me/code/git-wt-merge  52c7aa4 [merge]

$ git -C git-wt-merge status --short
 M README.md
 M src/main.rs
 M test.sh

$ git-wt 1,3 diff --stat
<nothing>
```

508 lines of real difference on disk, and the diff is provably empty — both refs
resolve to the same tree, so no flag can rescue it.

`cmd_diff` already warns when either side is dirty and points at `meld`
(`src/main.rs`), which is the honest answer today. `live` makes it answerable in
the terminal.

## Why `git diff` alone cannot do this

| Form | Compares |
|---|---|
| `git diff` | working tree vs index |
| `git diff HEAD` | working tree vs HEAD |
| `git diff <ref>` | working tree vs that ref |
| `git diff A B` / `A..B` | tree vs tree — **working tree ignored** |
| `git diff A...B` | merge-base(A,B) vs B — **working tree ignored** |

Dropping to one ref does not help either: "working tree" means the *current
process's* worktree. `git-wt` runs git from the repo root, so `git diff <branch
M>` there would compare the root's files, not worktree N's.

One git process has one working tree. **No single `git diff` invocation can ever
show both worktrees' uncommitted work.** That is the structural reason `live`
needs its own path-walking implementation.

## Why not just shell out to meld

meld is GUI-only. Its full option list is `--version -h -L/--label -n/--newtab
-a/--auto-compare -u/--unified -o/--output --auto-merge --comparison-file
--diff`. Every one of them opens a window.

Two tells that this is deliberate:

- `-u, --unified` is documented as **"Ignored for compatibility"** — an explicit
  no-op, so meld can be dropped into scripts expecting `diff -u` without ever
  printing unified output.
- `-o/--output` is the target file for saving a *merge result* the GUI writes
  after you resolve, not a listing.

meld stays the tool for eyeballing three directories. It is not a backend.

## Design

Two separate jobs, only the first of which needs git:

**1. Which paths to consider** — `.gitignore` must be honored, or `target/`
drowns the output. `diff -rq A B` cannot do this; meld cannot either (its filters
are hardcoded patterns like `*.o`, not `.gitignore`-aware). Only git knows:

```sh
git -C <worktree> ls-files --cached --others --exclude-standard
```

Tracked plus untracked-but-not-ignored. Run per worktree, union the results.

**2. Compare content** — literal bytes on disk. Byte-compare first as a cheap
filter, then `git diff --no-index` on the survivors for the actual hunks.
`--no-index` mode is git ignoring that it is git: a pure two-path content
compare, no refs, no index. It is the same role meld plays, minus the window.

On the real repo this reduced to 17 candidate paths (not thousands — `target/`
never entered), of which 3 spawned a git process.

## Grammar

Bare words, no dash required — `diff` already parses `..` and `...` as bare
positionals, so this matches. Pathspecs are only legal after `--`, so `live`
cannot collide with a filename.

```sh
git-wt 1,2 diff                    # committed state (unchanged default)
git-wt 1,2 diff live               # literal files on disk
git-wt 1,2 diff live hunks         # + line numbers
git-wt 1,2 diff --live --hunks     # dashes optional, same thing
```

Rules:

- `live` + `..`/`...` → error. Ranges are a ref concept; meaningless against
  working files.
- `live` suppresses the dirty warning — `live` *is* the answer to it.
- `--name-only` / `--name-status` / `--stat` keep working under `live`.
- `hunks` without `live` is allowed; line numbers work on a ref diff too.

## Output

Default is compact:

```
diff main ↔ merge   live — literal contents, .gitignore honored

M README.md     +90  −10
M src/main.rs   +345 −38
M test.sh       +73  −4

3 files changed, 508 insertions(+), 52 deletions(-)
```

With `hunks`, each file gains its changed line numbers:

```
M README.md     +90 −10
      119  modified 1
      290  added 2
      301  added 19
      328  added 50
      395  modified 3
```

Line numbers are the **`+` side** (worktree M), which is what you want for
jumping to the file.

The `M`/`A`/`D` column comes from the union: a file untracked-and-new in M is
genuinely an add.

## Hunk parsing

Line numbers come from `-U0` hunk headers, `@@ -oldStart,oldCount
+newStart,newCount @@`. Two traps, both hit in prototyping:

- **An omitted count means 1.** `@@ -119 +119 @@` is a one-line change, not a
  malformed header.
- **A zero count is not an edit.** `oldCount == 0` is a pure insertion;
  `newCount == 0` is a pure deletion. Labeling by the new-side number alone
  reports deletions as `+0` additions.

So the label needs both numbers: `old == 0` → added, `new == 0` → deleted,
otherwise modified.

Files present on only one side are handled by substituting `/dev/null` for the
missing path, which gives real hunks instead of a crash.

## Three-way

`live` cannot support three worktrees in content mode: `git diff --no-index`
takes exactly two paths, and there is no three-path form. `diff3` and `git
merge-file` exist but answer a different question ("how would these combine"),
not "what differs".

Listing mode *could* support three — union three `ls-files` sets, report which
sides differ as a matrix. That means one verb with two output shapes depending on
target count, which is a real wart.

Current decision: **`diff` takes exactly two worktrees**, and `git-wt 1,2,3 diff`
errors with a pointer to `meld`, which genuinely does three panes. Revisit once
`live` has some mileage.

## Open questions

- Should `hunks` print `README.md:290` (clickable) instead of the indented
  column? The column reads better; `file:line` is more useful.
- Should `live` eventually become the default, with the ref diff behind a flag?
- Ignored-but-differing files stay invisible to `live`, since `ls-files
  --exclude-standard` omits them. Same blind spot as plain git. Acceptable?

## Prototype

Verified against the real repo (`main` vs `merge`, both at `52c7aa4`, 508
insertions uncommitted). Output matched `git status` exactly, and the per-file
totals summed to the same 508/52 that `--stat` reported once the work was
committed.

```sh
A=/path/to/git-wt
B=/path/to/git-wt-merge

git -C "$A" ls-files --cached --others --exclude-standard | sort > a.txt
git -C "$B" ls-files --cached --others --exclude-standard | sort > b.txt

sort -u a.txt b.txt | while IFS= read -r f; do
  if   [ ! -e "$A/$f" ]; then echo "only-B  $f"
  elif [ ! -e "$B/$f" ]; then echo "only-A  $f"
  elif ! cmp -s "$A/$f" "$B/$f"; then echo "differ  $f"
  fi
done
```
