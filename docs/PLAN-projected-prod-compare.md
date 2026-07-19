# `--overlay`: compare a branch against a *projected* merge result

> **Status: proposed.** Design only — no code yet.

## Context

A team runs a fixed promotion flow:

```
main ──► stable ──► devs branch off stable, MR back to stable
stable ──► uat   (code review) ──► CICD deploys UAT
stable ──► main  (code review) ──► CICD deploys PROD
```

They want a **code-to-code** guarantee that what is tested in UAT is what will
reach PROD. The obvious check — `git-wt <uat>,<main> diff` — is misleading here:
`main` always *lags* `uat` by the `stable → main` promotion, so the diff is
dominated by the normal backlog and drowns the signal.

The question they actually want answered is:

> If we promote `stable` to `main` **right now**, will PROD equal UAT?

That means comparing `uat` not against `main`'s current tip but against
**`merge(main, stable)`** — the tree `main` *will become*. Concretely they
described: "checkout a detached HEAD from `main`, merge `stable` into it,
compare `uat` against that." The comparison catches two things:

- **left behind in UAT** — a change on `uat` (e.g. a fix made directly during
  UAT code review) that is *absent* from projected-PROD, so PROD will never get
  it though UAT tested it.
- **extra** — a change in projected-PROD absent from `uat`: commits merged to
  `stable` after UAT was cut, **or a hotfix committed straight to `main`**. PROD
  will ship it though UAT never saw it.

Note: when `main` only ever advances through `stable → main` (no direct commits
to `main`), `main` is an ancestor of `stable` and `merge(main, stable) == stable`
— so `uat` vs `stable` would already answer it. The projection earns its keep
**only when someone commits directly to `main`** (a prod hotfix that bypassed
`stable`); then only `merge(main, stable)` reflects what PROD will truly hold.
The command must be correct in that messy case, which is its whole reason to
exist.

## Goal

Let any of the three existing content/commit comparisons —
`commits`, `diff`, `meld` — treat one side as *projected*: merge a named ref
into that side in memory before comparing. No detached checkout, no scratch
worktree, no cleanup, no working-tree race window from a checkout.

## Grammar

A shared modifier, `--overlay <ref>`, accepted by `commits`, `diff`, and `meld`.
It merges `<ref>` into the **destination** side (the last-listed worktree) before
the comparison runs:

```sh
# 1 = uat worktree, 2 = main worktree, stable = the overlay
git-wt 1,2 commits --overlay stable            # commit matrix: uat vs (main+stable)
git-wt 1,2 diff    --overlay stable            # textual patch
git-wt 1,2 meld --diff --overlay stable        # visual, only differing files
```

`<ref>` is resolved the way merge sources already are (`resolve_merge_source`,
`src/cmd/merge.rs:601`): a number naming a worktree wins over a same-named
branch; otherwise it is a branch/ref.

One overlay, three renderers — the tool's existing "small verbs, composable
modifiers" shape. No new verb.

### The verdict is `commits` + `diff`, never `meld`

`meld` is a human eyeball layer and **must not** be the pass/fail signal. It can
show *nothing* for a file `git diff` flags, because the meld path extracts only
file **content** (`extract_files`, `git show <ref>:<path>`, `src/cmd/meld.rs:303`)
and drops everything that is not content:

- **mode / permission changes** (`old mode 100644 / new mode 100755`) — content
  identical, meld blank;
- **EOL / whitespace normalization** via `.gitattributes` or clean/smudge
  filters — `git show` yields identical bytes on both sides;
- **type changes** (symlink↔file, submodule pointer) — invisible as plain files;
- meld configured to ignore whitespace/blank lines.

So the authoritative "are UAT and projected-PROD the same code" answer comes from
the machine layers: `commits` (patch-id: *is this change present*) and `diff`'s
`--name-status` / exit code (*does anything differ at all*, mode and type changes
included). `meld --overlay` is offered only to *read* the difference the other two
already found. The runbook and any docs must state this — a blank meld is not a
green light.

### Small companion fix: `meld --diff` should flag content-identical files

Independently of overlay, `meld --diff` today can open a file that `git diff`
listed yet show it as identical (the mode/normalization cases above), which reads
as a false "no difference." `cmd_meld_filtered` (`src/cmd/meld.rs:136`) already
extracts both sides; when a listed file's two extracted blobs are byte-equal, it
should note that to stderr (e.g. `differs but not in content (mode/eol/type):
<path>`) rather than presenting a silent identical pane. This directly closes the
"git diff has a result but meld shows nothing" trap.

### Auditability (the snapshot gap)

Any such comparison is a point-in-time photograph: a dev landing a commit on
`uat` after you fetch is invisible until the next fetch. The command cannot
remove that, but it makes the snapshot **auditable** by printing, to stderr, the
three oids it compared:

```
projected  main (a1b2c3d) + stable (e4f5a6b) = tree 90fdea1
against     uat (7c8d9e0)
```

Workflow guidance (fetch all three immediately before; re-run after a fetch)
belongs in the runbook, not the tool.

## Implementation

The primitive already exists. `merge_probe` (`src/cmd/merge.rs:565`) runs
`git merge-tree --write-tree --name-only HEAD <src>`; **its stdout's first line
is the resulting tree oid** (and it exits 1, listing paths, on conflict). So:

1. Resolve the destination worktree's ref (`ref_of`) and the overlay ref.
2. Run a `merge-tree --write-tree` of overlay into the destination ref. Reuse
   `merge_probe`'s parsing; on `Conflict(paths)`, stop and report the conflicting
   paths (projected-PROD cannot be formed cleanly — that is itself a finding).
3. On `Clean`, take the tree oid and wrap it in a throwaway commit:
   `git commit-tree <tree> -p <main> -p <stable> -m projected`. The resulting
   commit has real commit identity (so `rev-list`/patch-id work → `commits`
   matrix is meaningful) and real tree content (so `diff`/`meld` work). It is
   unreferenced and gets gc'd; nothing to clean up.
4. Substitute that commit oid as the destination ref, then call the existing
   `cmd_commits` / `cmd_diff` / `cmd_meld` path unchanged.
5. Print the audit line from step above.

A thin resolver — "given `idxs` + `--overlay <ref>`, return the refs to compare,
with the destination possibly replaced by a projected commit oid" — is the only
new logic. The renderers are untouched.

### Where it wires in

- `src/cli.rs` — parse `--overlay <ref>` in the tails handed to the `commits`,
  `diff`, and `meld` dispatch arms (around lines 204–206 for the comma-target
  form). Reject `--overlay` on verbs that don't support it.
- New helper (e.g. `src/cmd/overlay.rs` or a fn in `src/cmd/merge.rs`, reusing
  `merge_probe`) that returns the projected commit oid + the audit strings.
- `commits` / `diff` / `meld` arg parsers accept and thread through the resolved
  ref; no change to their comparison logic.

### Edge cases

- **Conflict** forming projected-PROD → report paths, exit non-zero; do not fall
  back to a plain compare (that would silently answer a different question — the
  same rule `meld`'s `--diff`-only flags follow, `src/cmd/meld.rs:57`).
- **Dirty destination worktree** → irrelevant: the projection is built from
  committed refs via `merge-tree`, so warn as the ref-based commands already do.
- **git < 2.38** → `merge_probe` already emits the right error; inherit it.
- `--overlay` with `meld` **without** `--diff` is meaningless (full-directory
  meld compares checked-out dirs, not refs) → reject, mirroring the existing
  "flag only applies to `meld --diff`" rejections.

## Verification

- Unit tests beside the new helper: a fixture repo where `main` has a
  direct hotfix `stable` lacks; assert the projected oid's tree contains both the
  hotfix and stable's content, and that a uat-only commit shows as "left behind"
  in the `commits` matrix while a stable-after-cut commit shows as "extra".
- A conflict fixture (main and stable edit the same line) asserts the command
  stops with the conflicting path, not a misleading clean compare.
- A **mode-only** fixture (same content, `100644` → `100755`) asserts `diff`
  name-status still reports the file **and** that `meld --diff`'s new check emits
  the "differs but not in content" note instead of a silent identical pane — the
  regression guard for the false-green this whole section is about.
- Manual: `git-wt 1,2 commits --overlay stable` on a scratch repo modelling the
  flow; confirm the audit line's three oids match `git rev-parse`.
- `cargo test` green; `cargo build --release` clean.
