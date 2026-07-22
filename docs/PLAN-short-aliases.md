# Short aliases for long-only flags

> **Status: proposed.** Not yet built.

## Context

`git-wt` has no arg-parsing library (see `src/cli.rs`, `src/cmd/*`): every
subcommand is a hand-rolled `match` over `&str` tokens. Several flags already
carry a short spelling this way — `-n`/`--limit`, `-f`/`--files`,
`-a`/`--all`, `-w`/`--wrap`, `-d`/`--date`, `-m`/`--message`,
`-c`/`--commits` in `commits/args.rs`; `-u`/`-p`/`-n` in `sync.rs`; `-b` for
`--branch` in `cli.rs`. There's also a precedent for a second *long* spelling
that's just shorter — `--subjw` for `--subject-width`, `--branchw` for
`--branch-width` (`commits/args.rs:400,413`).

A pass over every parser turned up hyphenated, multi-word long flags with no
short form at all — the ones that cost the most keystrokes and have none of
the single-letter alphabet (`-a`..`-z`) already claimed for them in that
command's own `match`. Since nothing here is clap, a short form can be
whatever string is useful: a single-dash letter, or a short double-dash
spelling (`--au`) the way `--subjw` already works. This plan proposes the
latter for every candidate below — single letters are scarcer and better
saved for the flags used constantly (`-f`, `-a`, `-n`, ...); a two/three-letter
long alias reads clearly on its own (`--cs 2026-01-01` vs cryptic `-cs`) and
needs no `expand_short_bundles` handling.

**Not proposed:** flags that are already short as a single word (`--topo`,
`--merges`, `--squash`, `--union`, `--time`, `--md`, `--from`, `--dirname`) —
aliasing a 4-8 letter single word saves little and adds a second name to
remember. The candidates below are all hyphenated (multi-word) or long
enough that a short form earns its keep, and `--author` is the specific
single-word exception (long, and among the most frequently typed filters).

## Candidates

### `commits` (`src/cmd/commits/args.rs`, match starts `:356`)

| Long | Alias | Note |
|---|---|---|
| `--author` | `--au` | frequent filter, no existing short |
| `--date-human` | `--dh` | |
| `--all-files` | `--af` | |
| `--filename` | `--fn` | |
| `--commit-since` | `--cs` | |
| `--commit-until` | `--cu` | |
| `--date-since` | `--ds` | |
| `--date-until` | `--du` | |
| `--no-cherry` | `--nc` | |
| `--pick-id` | `--pi` | |

Checked against every existing token in this match (`-n`, `--limit`, `--topo`,
`--topo-order`, `--merges`, `--no-merges`, `--reverse`, `--oldest-first`,
`-f`, `--files`, `--squash`, `--union`, `-a`, `--all`, `-w`, `--wrap`,
`--subjw`, `--branchw`, `--time`, `--md`, `-d`, `--date`, `-m`, `--message`,
`-c`, `--commits`) — no collisions.

### `merge` (`src/cmd/merge.rs`, match starts `:97`)

| Long | Alias |
|---|---|
| `--no-ff` | `--nf` |
| `--ff-only` | `--fo` |

Existing tokens in this match: `review`, `continue`/`-c`, `abort`/`-a`,
`ours`/`-o`, `theirs`/`-t`, `dry-run`/`-d`, `-m`/`--message`, `-f`/`--force`.
No collisions.

### `sync` (`src/cmd/sync.rs`, `flags()` at `:42` + canon match `:70`)

| Long | Op | Alias |
|---|---|---|
| `--rebase` | pull | `--rb` |
| `--no-rebase` | pull | `--nr` |
| `--autostash` | pull | `--as` |
| `--no-tags` | fetch | `--nt` |
| `--force-with-lease` | push | `--fl` |

`--set-upstream` already has `-u`; `--prune` already has `-p`; `--dry-run`
(push) already has `-n`; fetch `--force` is left alone — deliberately
unaliased already (no `-f`/`-u` short exists for it in `flags()`), and giving
it a two-letter alias doesn't change that it's the one flag on this list
worth *not* making easier to fat-finger.

### `meld` / `add` / `cli`

| Command | Long | Alias |
|---|---|---|
| `cli.rs` (`merged --others`) | `--others` | `--ot` |

Nothing else in `meld.rs` or `add.rs` clears the bar above (`--diff`,
`--base`, `--3way`, `--dirname`, `--from`, `--stay` are all already short or
low-frequency).

## Not doing

- **Single-dash multi-char aliases** (`-au`, `-nf`) — technically possible
  with this parser (plain string match, no clap grouping rules), but
  indistinguishable at a glance from a bundle like `-af` (`--all` +
  `--files`), which `commits` already supports via `expand_short_bundles`
  (`commits/args.rs:200`). Double-dash avoids that ambiguity entirely.
- **Aliasing single-word long flags** — see "Not proposed" above.
- **`--force` (fetch) alias** — see sync table note.

## Build order

Each command's aliases are an independent, mechanical addition (one new
`|` arm per flag, same pattern as `--subjw`/`--branchw`) and can land as
separate small commits:

1. `commits/args.rs` — 10 aliases, plus a test per alias asserting it sets
   the same field as the long form (mirrors the existing `--subjw` test at
   `:1257`).
2. `merge.rs` — 2 aliases.
3. `sync.rs` — 5 aliases, one per `(op, flag)` pair in the `canon` match.
4. `cli.rs` — `--others`/`--ot`.
5. Update `--help` text and `README`/`docs` flag tables wherever the long
   forms are currently listed, so the new aliases are discoverable and not
   just accepted.

No open questions; ready to build once the alias table above is confirmed.
