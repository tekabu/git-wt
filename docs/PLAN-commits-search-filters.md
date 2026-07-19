# `commits`: text and path filters — `--message`, `--filename`

> **Status: implemented** on `feat/commits-search`. Code and tests done; README
> and `docs/COMMITS-FILES.md` still to update.

## Context

`git-wt N,M commits` can already narrow rows by date, author, and named commits,
but there is no way to ask *"which of these commits mentions X"* — where X is a
word in the message or a path that was touched. Today you pipe to `grep`, which
kills the color, the mark columns, and the file blocks.

The ask began as a single `--search` that matched anything on screen. Three
findings during exploration reshaped it:

1. **Matching against the *cut* subject is unsound.** The subject column's width
   is derived from the rows (`render.rs:47-100`), so filtering on the cut text
   would make the result set depend on terminal width — resize the window and
   rows appear or vanish, and `| grep` / `--md` (which cut nothing) return a
   different set again. Instead: match the **full** text and *guarantee it is
   displayed*.
2. **One flag cannot serve every half well.** Subject text is already in hand
   (`%s`, `rows.rs:232`). Bodies need `%b` and a parsing change. Filenames are
   best asked of git as a **pathspec** — one `rev-list`, no per-commit diff.
3. **The rule that ties them together:** *a filter must show what it matched.*
   Each flag below either highlights the matched text or prints the line it
   lived on. No exceptions -- see the rejected `--grep` below.

Notably `--subject` is currently a *hard error* (`args.rs:282`, `SUBJECT_MSG`)
reading "it would read as a filter, and it is a width". This change gives users
the filter that error has been apologizing for.

## The two flags

| Flag | Matches | Syntax | Runs in | Match visible? |
|---|---|---|---|---|
| `--message TERM` (`-m`) | subject **+ body** | substring, case-folded | Rust, on `%s` + `%b` | **yes** — implies `--wrap full`; matching body lines printed under the row |
| `--filename TERM` | paths the commit touched | substring, case-folded | git pathspec, one `rev-list` | **yes** — implies `--files`, matching path highlighted |

### Rejected: `--subject`

A subject-only twin of `--message` was built and then cut: `--message` already
searches the subject, so the narrower flag was a second spelling of a subset.
`--subject` is now an error naming `--message`, and `--subject-width` keeps its
own longer name.

They AND together, and with every existing filter.

### Rejected: `--grep`

A `--grep PAT` passthrough to `git log --grep` was planned and cut. Testing in
this repo showed `^` anchors to the start of **any line of the message**, not to
the subject: `--grep '^A range'` matches a commit whose *body* has that line, and
`--grep '^Co-Authored'` matches nearly every commit in the repo via its trailer.
Git offers no subject-scoped grep, and with no regex engine in-crate there is no
way to re-check the match in Rust. A flag whose anchor means something other
than it reads is worse than no flag.

If patterns are ever wanted, the honest path is to pipe the subjects through one
`grep -E` process and keep the matching row indices — no dependency, and `^`
would then mean what people expect.

### Confirmed decisions

- **Substring, not `is_subseq`.** A subsequence over a file path is noise.
  `--author`'s fuzziness stays `--author`'s.
- **`--subject` implies `--wrap full`** so a match past the right edge still
  shows. An explicit `--wrap`/`-w` wins — it is an answer already given.
- **`--message` prints matching body lines**, dim and indented, in the same
  shape as the file block: a row kept for body text shows the body text it
  was kept for.
- **`--filename` implies `--files`**, and the block lists **every file the
  commit touched**, not only the matched one — `--filename` picks the *row*, it
  does not trim the block. The counts still sum to the whole commit, and you
  still see the shape of the change the match led you to. Matching paths are
  highlighted amber among the rest.
- **None of them widen the default slice to `--all`.** Like `--author`: they
  match many commits and name none, so "what in this branch comparison" stays
  the question (README:536). The existing "no commits match those filters" hint
  already points at `--all`.

## Changes

### `src/cmd/commits/rows.rs`

**Body capture.** `commit_rows` gains a `want_body: bool` parameter. When set,
the format string appends `%x09%b` as the **last** field.

The record separator has to change: a body contains newlines, and the parse is
currently one record per line (`rows.rs:252-266`). Terminate each record with
`%x00` **inside the format string** — not the `-z` flag, whose `git log`
semantics are tangled up with the diff options — and split the output on `'\0'`,
trimming each record. Because the body is last, `splitn(8, '\t')` sweeps any
tabs inside it into that field harmlessly.

`CommitRow` gains `body: String`, empty when not asked for. Ordinary runs pay
nothing.

**Pathspec walk**, a sibling of `ref_shas`:

```rust
/// The shas, among `refs`, of commits that touched a path containing `term`.
///
/// A pathspec, so git does the walk: one rev-list, where matching in Rust would
/// cost a diff per commit. `:(icase)` case-folds, and the bare `*term*` glob is
/// the default (non-`:(glob)`) pathspec whose `*` crosses directory separators
/// -- which is what a substring over a path has to do.
///
/// Path limiting brings git's history simplification with it: a merge whose
/// result matched no differently from its first parent is not listed, the same
/// commits `git log -- <path>` shows and for the same reason.
pub(crate) fn path_shas(root: &Path, refs: &[String], term: &str)
    -> Result<HashSet<String>, String>
```

Escape `\`, `*`, `?`, `[` in `term` before wrapping it in `*…*`, so a substring
stays a substring and a user's `[` is not read as a glob class. Build
`[":(icase)*", escaped, "*"].concat()` as one argument after a literal `--`.
Called with `row_refs`.

**Matching body lines**, for the render block:

```rust
/// The body lines containing `term`, case-folded. Empty when the match was in
/// the subject, which the table already prints.
pub(crate) fn body_hits(body: &str, term: &str) -> Vec<String>
```

Trim each line; drop blanks; cap at a handful (say 3) with the rest implied,
so a commit whose body repeats a word does not swamp the table.

### `src/cmd/commits/args.rs`

Add to `CommitsArgs`:

```rust
/// Only commits whose subject contains this, case-folded. Substring, not the
/// subsequence --author uses: a subsequence over prose is noise.
pub(crate) subject: Option<String>,
/// Only commits whose subject or body contains this, case-folded.
pub(crate) message: Option<String>,
/// Only commits touching a path containing this, case-folded.
pub(crate) filename: Option<String>,
```

Parsing — each in both spellings (` ` and `=`), each rejecting an empty value in
the shape of `AUTHOR_MISSING`:

- `--subject`. **Delete** the `"--subject"` error arm and `SUBJECT_MSG`;
  `--subject-width` / `--subjw` are distinct match arms and are untouched.
- `--message`, plus `-m` (add `m` to `VALUE_SHORTS`, currently `"ndwc"`).
- `--filename`.
- `--grep` → error pointing at `--subject` and `--message`, the git habit users
  will bring (the treatment `--since`/`--until` already get at `args.rs:353`).
- `--file` → error naming both, mirroring the `SUBJECT_MSG` pattern being
  removed:
  `"no '--file' for commits: '--filename TERM' filters rows, '--files' shows the file block"`
  — one letter from `--files`, opposite meaning.

Implications must only fire when the user did not say otherwise, so track wrap
as `Option<Wrap>` internally and resolve at the end:

```rust
// The match has to be on screen or the filter is lying about what it found.
// A subject is cut at the terminal's edge, so searching one means showing all
// of it -- unless a --wrap was typed, which is an answer already given.
let wrap = wrap.unwrap_or(if subject.is_some() { Wrap::Full } else { Wrap::Lines(1) });
// Likewise: a row kept for a path it touched should say which path.
let files = files || filename.is_some();
```

`names_a_floor` is **unchanged** — none of the three widens.

Tests in the existing `mod tests`:
- both spellings reach each field; missing-value errors for all three
- `--subject foo` ⇒ `wrap == Wrap::Full`; `--subject foo -w 1` ⇒ `Lines(1)`
- `--filename x` ⇒ `files == true`
- all three ⇒ `!all` (extend
  `a_lower_bound_widens_the_source_and_an_upper_one_does_not`)
- `--file x` names `--filename` and `--files`; `--grep x` names `--subject`
  and `--message`
- `--subject-width 80` still parses as a width beside a `--subject` filter
- `-m` bundles per the existing value-short rules (`-fm term` ok, `-mf term`
  errors) — covered by `a_value_taking_short_ends_the_bundle`'s pattern

### `src/cmd/commits/mod.rs`

- Add all three to the `filtered` flag (`mod.rs:82-86`), so an empty result
  reports "no commits match those filters" rather than "no commits ahead of".
- `want_body = args.message.is_some()`, passed to `commit_rows`.
- Resolve the pathspec set once, before the row filter, only when asked:

```rust
let paths = args.filename.as_ref()
    .map(|t| path_shas(root, row_refs, t))
    .transpose()?;
```

- Two more links in the existing filter chain (`mod.rs:145-155`), beside the
  `--author` one:

```rust
.filter(|r| subj_needle.as_ref()
    .is_none_or(|n| r.text.to_lowercase().contains(n)))
.filter(|r| msg_needle.as_ref().is_none_or(|n| {
    r.text.to_lowercase().contains(n) || r.body.to_lowercase().contains(n)
}))
.filter(|r| paths.as_ref().is_none_or(|p| p.contains(&r.sha)))
```

- Build `row_bodies: Vec<Vec<String>>` from `body_hits` over the surviving rows,
  scoped to displayed rows exactly as `row_files` is (`mod.rs:168-174`).
- Pass the lowercased terms into `Highlight`, next to `needle` at `mod.rs:143`.

### `src/ui.rs`

One shared helper — the subject line, the body block, and the file block all
need it:

```rust
/// Paint every case-folded occurrence of `needle` in `s` with `code`, leaving
/// the rest under `base` (or unpainted when `base` is empty).
///
/// Widths are measured on plain text and color applied after -- the rule the
/// table already follows -- so this never shifts a column. `base` exists
/// because a file block is already dim: the RESET ending a highlight would
/// otherwise drop the rest of the line out of dim.
pub(crate) fn paint_matches(s: &str, needle: &str, code: &str, base: &str, on: bool) -> String
```

Search over a lowercased copy; **map** the byte offsets back rather than
assuming they line up — lowercasing can change byte length. Tests: no match and
`on == false` both return the input unchanged; multiple occurrences; a match at
either end; non-ASCII without a panic.

### `src/cmd/commits/render.rs`

- `Highlight` gains `subject`, `message`, `file: Option<String>` (lowercased).
  Extend its doc: these mark the text actually matched, not merely the column a
  filter read — the same thing `shas` does.
- `render_commits` gains `row_bodies: &[Vec<String>]`.
- Subject: `text[0]` and each continuation line through `paint_matches(...,
  MATCH, "", color)`, applied **after** wrapping so the width budget is
  untouched. One-line comment on the known limit: a term split across a wrap
  boundary highlights on neither half — the row is still correct, just unmarked.
- **Body block**, printed between the subject and the file block, dim and
  indented to `fixed` like a wrapped subject, with the term amber. Reuses the
  `grouped` blank-line fencing already at `render.rs:166-171` — extend that
  condition to cover non-empty bodies.
- File block: `paint(&file_line, DIM, color)` becomes
  `paint_matches(&file_line, term, MATCH, DIM, color)` when `hl.file` is set.
  `row_files` is **not** filtered: `--filename` chose the row, and the block
  answers "what did that commit do", which a trimmed block could not.

### `src/cmd/commits/md.rs`

`write_md` takes `row_bodies` and appends matching body lines to the subject
cell with `<br>`, exactly as the file block already does (`md.rs:119-133`).
No color to carry, and nothing is cut in markdown, so the rows are identical to
the terminal's.

## Docs

- **README.md** — the three-flag table above goes in the filter section (~523),
  examples into the `sh` block (~508), and prose covering: substring vs
  `--author`'s subsequence; the "a filter shows what it matched" rule and how
  each flag honors it; none of them widening (extend the paragraph at 536);
  all three in the highlight paragraph at 541.
  Rows into the recipe table (~1081).
- **docs/COMMITS-FILES.md** — `--filename` beside `--files`.
- **docs/PLAN-commits-search-filters.md** — this plan, refreshed on approval.

## Verification

```sh
./build.sh
./test.sh                        # existing suite must stay green
```

Then in this repo, which has the fixtures to hand:

```sh
# subject: substring, case-folded, wraps full so the match is visible
git-wt 1,2 commits --subject highlight
git-wt 1,2 commits --subject HIGHLIGHT       # same rows
git-wt 1,2 commits --subject highlight -w 1  # explicit wrap beats the implication

# message: reaches the body, and prints the body line it matched
git-wt 1,2 commits --message worktree --all
git-wt 1,2 commits -m worktree --all         # short form

# filename: implies --files, ALL files listed, matching path amber
git-wt 1,2 commits --filename render.rs --all
git-wt 1,2 commits --filename RENDER --all   # case-folded
git-wt 1,2 commits --filename 'cmd/commits' --all   # '*' crosses the separator

# ANDs with what already exists; -n still counts survivors
git-wt 1,2 commits --subject fix --author nino --all -n 3

# errors
git-wt 1,2 commits --file x                  # names --filename and --files
git-wt 1,2 commits --grep x                  # names --subject and --message
git-wt 1,2 commits --subject-width 80        # still a width, not a filter
git-wt 1,2 commits --subject                 # needs a term

# no match inside the default slice keeps the existing hint
git-wt 1,2 commits --subject zzzznope
```

Check by eye:
- a `--filename render.rs` row lists **all** of that commit's files, `render.rs`
  amber among them
- a `--message` row that matched only in the body shows that body line beneath it
- matched text is amber in subject, body block, and file block
- the mark columns still line up under their headers with highlights present
  (widths are measured pre-color)
- `--md` writes the same rows the terminal listed, body hits included
- `| cat` is uncolored and uncut, and a body with newlines does not break the
  NUL-separated parse

New `test.sh` cases, following the neighbours at 769-774 and the `hlcheck`
assertions at 602-604:
- exact / case-folded / no-match for each of the three
- missing-value errors; the `--file` redirect
- `--message` finds a commit whose term is body-only, and the body line prints
- a body containing a newline **and** a tab survives the record parse
- none of the three leaks pre-slice rows (mirror the `--author` slice test at
  570-578)
- `hlcheck` for an amber subject match, body match, and file-block match
