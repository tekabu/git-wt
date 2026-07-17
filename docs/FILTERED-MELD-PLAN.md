# Plan: filtered meld for git-wt

Add a `--diff` mode to the existing `git-wt <N>,<M> meld` command that opens
[meld](https://meldmerge.org/) on sparse temp directories containing only the
files that differ between the selected refs, instead of the whole worktree
directories.

Renames are intentionally treated as `D` (deleted) + `A` (added). There is no
cross-directory rename detection; meld has none, and git's rename detection across
diverged branches is unreliable anyway.

## CLI grammar

```sh
# current behavior, unchanged
$ git-wt 1,2 meld                     # full worktree directories, 2-way
$ git-wt 2,1,3 meld                   # full worktree directories, 3-way

# filtered meld: only files that differ
$ git-wt 1,2 meld --diff              # 2-way, all files that differ
$ git-wt 1,2 meld --diff ...          # 2-way, only what branch 2 added since fork

# filtered 3-way meld
$ git-wt 1,2 meld --diff --3way       # 2 panes + auto base (merge-base of A and B)
$ git-wt 1,2 meld --diff --base main  # 2 panes + explicit base ref
$ git-wt 1,2 meld --diff --base 3    # 2 panes + worktree 3 as base
```

In 3-way mode the panes are ordered `LOCAL  BASE  REMOTE` (left/middle/right),
which matches meld's conventional merge-tool layout. The middle pane is read-only
in our implementation: we do not add `--output` support.

## Ref resolution

Worktree indices are resolved to branch names with the existing `ref_of` helper.
Detached worktrees resolve to the short `HEAD` sha. A bare worktree is an error.

For `--base` we reuse `resolve_merge_source`, so a number is interpreted as a
worktree index and a bare word is interpreted as a branch/ref.

## File set

Use `git diff --name-status` so we know which side a file belongs to.

```sh
git diff --name-status <A> <B>        # default (--diff)
git diff --name-status <A>...<B>      # ... range (--diff ...)
git diff --name-status <base> <A>     # 3-way left half
git diff --name-status <base> <B>     # 3-way right half
```

The union of all paths from the relevant diff(s) becomes the candidate set.
`--name-only` is not enough because it loses the add/delete/modify direction.

Parse each line:

| Line | Meaning | Left dir | Right dir | Base dir |
|---|---|---|---|---|
| `A path` | added | absent | extract from B | absent from base vs A/B diff |
| `M path` | modified | extract from A | extract from B | extract from base |
| `D path` | deleted | extract from A | absent | absent from base vs A/B diff |

Renames are not parsed specially: a rename appears as `D old` + `A new` and is
treated exactly as two independent changes.

## Temp directories

Create a single parent temp directory per invocation, then one subdirectory per
ref:

```
/tmp/git-wt-meld-<pid>-<random>/
  a/        # left / worktree 1
  b/        # right / worktree 2
  base/     # only in 3-way mode
```

Use `std::env::temp_dir()` so it works cross-platform without dependencies.
Clean up the whole parent directory when meld exits, regardless of success.

## Extraction

For each `(ref, dir, path)`:

1. Create parent directories in the temp dir.
2. Try `git show <ref>:<path>` and write the bytes to the temp path.
3. If `git show` fails because the path does not exist in that ref, skip it.
   That naturally leaves the file absent for adds/deletes.

Use a new `git_bytes` helper returning `Vec<u8>` so binary files are preserved.

## Meld launch

```sh
meld /tmp/.../a /tmp/.../b            # 2-way
meld /tmp/.../base /tmp/.../a /tmp/.../b   # 3-way (BASE in middle)
```

Wait for meld to close, then remove the temp directory.

## Error cases

- `--diff` with 3 worktree numbers: error, `--diff` only supports exactly 2
  worktree targets.
- `--diff` without 2 worktree targets: error.
- `--3way` and `--base` together: error, they are alternatives.
- `--base` with no value: error.
- Bare worktree as target or base: error, no HEAD to compare.
- `meld` not on PATH: existing install hint.
- No changed files: print a short notice and exit 0 instead of opening meld on
  empty directories.

## Help text updates

Add to the `MELD` section of `src/main.rs`:

```
git-wt <N>,<M> meld --diff            # only files that differ
    --diff ...                        # only what M added since fork
    --3way                            # add merge-base as middle pane
    --base <REF>                      # use explicit base (or worktree number)
```

## Tests

Add cases to `test.sh` using the existing fake `meld` stub that echoes its
arguments.

1. `--diff` 2-way extracts only changed files and passes two temp dirs.
2. `--diff ...` uses the merge-base range and yields a smaller file set.
3. `--diff --3way` passes three temp dirs in `base a b` order.
4. `--diff --base <ref>` uses the explicit base.
5. Add/delete/modify paths are all present and missing where expected.
6. Empty diff prints a notice and exits 0 without calling meld.
7. `--3way --base` is rejected.
8. `--diff` with three worktree numbers is rejected.

## Implementation order

1. Add `git_bytes` helper in `src/main.rs`.
2. Implement temp-directory extraction and `cmd_meld_filtered`.
3. Add argument parsing for `--diff`, `...`, `--3way`, `--base` in `cmd_meld`.
4. Update help text.
5. Add tests to `test.sh` and run them.

## Non-goals

- Rename detection (`-M` / `--find-renames`): out of scope. Renames look like
  delete + add.
- Writing merge results back to the worktree: out of scope. The middle pane is
  read-only.
- Live working-tree comparison: out of scope. `--diff` compares refs only.
  Existing `git-wt <N>,<M> diff live` already covers literal disk contents.
