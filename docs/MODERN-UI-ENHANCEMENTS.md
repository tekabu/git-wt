# git-wt — Modern UI Enhancement Scan

A survey of opportunities to modernize the user-interface layer of `git-wt` without changing its core mental model: target-first grammar for existing worktrees, explicit `add` for creation, and stdout kept scriptable.

Status: design notes / backlog. No implementation commitments yet.

---

## 1. Current UI baseline

The tool is intentionally plain-terminal:

- Output is columnar plain text (`src/main.rs:244-307`).
- Branch picker uses `fzf` when available, otherwise a numbered stdin prompt (`src/main.rs:618-718`).
- Errors and status go to `stderr`; only paths go to `stdout` (`src/main.rs:562-563`).
- No ANSI colors, no spinners, no icons, no persistent config.
- Help is a static string (`src/main.rs:15-65`).

That minimalism is a feature for scripting, but leaves room for a richer *interactive* experience when a TTY is detected.

---

## 2. Richer list view

### 2.1 What to show

`git worktree list --porcelain` gives path and branch. We could cheaply augment each row with:

| Column | Source | Value |
|---|---|---|
| Status | `git status --short` in worktree | clean / modified / untracked |
| Last commit | `git log -1 --format=%ar` | relative time |
| Ahead/Behind | `git rev-list --left-right HEAD...@{u}` | e.g. `+2 -1` |
| Detached sha | `git rev-parse --short HEAD` | short hash when detached |

These are read-only, so they can be fetched concurrently without changing the worktree state.

### 2.2 Visual formatting

- Color branch names by status: green = clean, yellow = modified, red = untracked + modified.
- Dim or italicize the main worktree row to de-emphasize it.
- Right-align numbers; pad columns consistently (the current code already computes per-column widths at `src/main.rs:283-287`).
- Add a subtle header row when output is a terminal.

### 2.3 Compact vs. verbose modes

```text
$ git-wt
  #  branch           path                         status   last
  1  main             ~/code/myapp                 clean    2m ago
  2  feature/login    ~/code/myapp-feature-login   dirty    1h ago
  3  (detached a1b2)  ~/code/myapp-a1b2            clean    3d ago
```

Keep `--col` for scripting; add `--long` for metadata and `--short` for a single-line summary.

---

## 3. Interactive TUI mode

A full-screen terminal UI could be an *opt-in* mode (`git-wt tui` or auto-detected when stdout is a TTY and `--interactive` is passed).

### 3.1 Layout ideas

```text
┌─────────────────────────────────────────────────────────────┐
│  git-wt — 3 worktrees                                       │
│                                                             │
│  > 2  feature/login    ~/code/myapp-feature-login   dirty   │
│    1  main             ~/code/myapp                 clean     │
│    3  (detached)       ~/code/myapp-a1b2            clean     │
│                                                             │
│  [Enter] switch  [a] add  [r] remove  [p] path  [q] quit   │
└─────────────────────────────────────────────────────────────┘
```

Keyboard-driven actions preserve the existing grammar while reducing typing for interactive users.

### 3.2 Implementation approaches

| Approach | Dependency | Trade-off |
|---|---|---|
| Minimal: raw terminal + ANSI escapes | none | Small, but easy to get redraw/resize wrong |
| `ratatui` + `crossterm` | 2 crates | Battle-tested widgets, key handling, layouts |
| Keep CLI-only, improve picker | none | Lowest risk, least visual impact |

Given the crate currently has **zero dependencies**, adding `ratatui` is a meaningful policy change. A possible compromise: gate the TUI behind a non-default Cargo feature (`--features tui`) so the default build stays tiny.

---

## 4. Enhanced branch picker

### 4.1 Current picker

- `fzf` path: prompt text, 40% height, reverse order (`src/main.rs:667-668`).
- Fallback: numbered list read from stdin (`src/main.rs:695-718`).

### 4.2 Improvements

- **Preview pane**: show last commit message and date for the highlighted branch.
- **Group branches**: local / remote / already-checked-out, each with a header.
- **Search both name and description**: branch names plus recent commit subjects.
- **Recent-branches section**: sort branches touched recently to the top.
- **Better empty state**: when all branches are checked out, offer to create a new branch by name.

For the built-in fallback picker, a line editor (arrow keys, incremental search) would feel closer to `fzf`. Crates like `dialoguer` can provide that without building a full TUI.

---

## 5. Color, icons, and theming

### 5.1 Smart color detection

- Use `NO_COLOR` / `CLICOLOR_FORCE` conventions.
- Disable colors when stdout is piped (so `git-wt list | cat` stays parseable).
- Detect terminal truecolor vs. 256-color vs. 16-color and adjust palette.

### 5.2 Iconography

Optional if a Nerd Font or emoji is available:

| State | Suggested glyph |
|---|---|
| clean | `✓` or `✅` |
| modified | `~` or `⚠` |
| untracked | `+` or `?` |
| detached | `⎇` or `🔀` |
| main worktree | `🏠` |

Default to ASCII-safe symbols; enable richer icons via `--icons` or config.

### 5.3 Theming

A small config file (e.g. `~/.config/git-wt/config.toml`) could define:

```toml
[ui]
color = "auto"      # auto | always | never
icons = "auto"      # auto | ascii | emoji | none
header = true

[ui.colors]
clean = "green"
dirty = "yellow"
error = "red"
accent = "cyan"
```

Keep the default build theme-free; read the file only when it exists.

---

## 6. Progress and feedback

### 6.1 Long-running operations

`git worktree add` on large repos can pause. Provide a small spinner + message on `stderr`:

```text
Creating worktree for feature/login at ~/code/myapp-feature-login...
✓ Done
```

Since scripts capture `stdout`, progress must stay on `stderr`, preserving the current contract.

### 6.2 Operation summary

After `add` / `remove`, show a one-line summary on `stderr`:

```text
Created myapp-feature-login  (branch feature/login from main)
Removed myapp-feature-login  (branch feature/login kept)
```

This gives interactive users context without polluting the path on `stdout`.

---

## 7. Contextual help and discovery

### 7.1 Better error UX

Current errors are terse (`src/main.rs:134-138`). We can add:

- **Did-you-mean suggestions** for typos in actions and branch names.
- **Command examples** after an error in a TTY.
- **Hint levels**: concise in pipes, expanded with examples in an interactive terminal.

### 7.2 Inline cheat sheet

A `git-wt help --examples` page with real command combinations, mirroring the README command reference but compact.

### 7.3 Shell completions

Generate `bash`/`zsh`/`fish`/`powershell` completion scripts:

```sh
git-wt completion bash > /path/to/completions/git-wt.bash
```

Completions could offer branch names after `git-wt add`, worktree indices after `git-wt`, and flag names everywhere. The `clap` crate automates this, but a hand-rolled generator keeps the dependency count at zero.

---

## 8. Shell wrapper (`wt`) enhancements

The `install.sh --alias` wrapper (`install.sh:72-99`) is where most daily UX lives.

### 8.1 Ideas

- Print a styled one-liner after switching: `→ ~/code/myapp-feature-login (feature/login, dirty)`.
- Remember last-used worktrees and allow `wt -` to jump back (like `cd -`).
- Alias `wt .` to switch to the worktree matching the current branch, if one exists.
- Optional transient notification on macOS (`osascript`) / Linux (`notify-send`) after long `add` operations.

### 8.2 Cross-shell prompt integration

Offer a shell prompt helper that shows the current worktree index/branch, e.g. a Powerline segment or starship module.

---

## 9. New interaction paradigms

### 9.1 Favorites / recents

Track recently switched-to worktrees in a small state file (`~/.local/share/git-wt/recents.json`). Provide:

```sh
git-wt recent          # list last 5 worktrees across repos
git-wt switch <name>   # jump to a recent one by branch or folder name
```

No git state changes; purely a UX convenience.

### 9.2 Bulk operations

Allow ranges and comma-separated indices:

```sh
git-wt 2-4 remove -y      # remove worktrees 2 through 4
git-wt 2,5,7 remove -y    # remove selected set
git-wt 1-3 status         # show status for all three
```

Requires careful confirmation UX and clear stdout/stderr separation.

### 9.3 Rename / move

A `rename` verb could move a worktree directory and update git’s worktree metadata:

```sh
git-wt 2 rename myapp-review
```

Today this requires manual `git worktree move` + directory handling; surfacing it would fill a common gap.

---

## 10. Accessibility and compatibility

Any visual enhancements must not regress the current behavior:

| Requirement | How |
|---|---|
| Scriptable stdout | Colors/progress/icons on `stderr` only |
| Pipes stay clean | Auto-disable ANSI when not a TTY |
| `NO_COLOR` | Respect immediately |
| Windows Terminal | Use portable `std::io::IsTerminal` / `is-terminal` crate |
| Legacy consoles | Fallback to plain ASCII |
| Minimal binary | Gate heavy deps behind a Cargo feature |

---

## 11. Suggested roadmap

A conservative, incremental order that keeps the tool small by default:

1. **No-dependency polish**
   - Smart ANSI on/off.
   - Colorized list with status metadata.
   - Improved error hints and examples.

2. **Optional richer picker**
   - Add `--preview` to show branch commit info in the picker.
   - Built-in numbered picker with arrow-key support (e.g. `dialoguer`, behind feature).

3. **Shell experience**
   - Completion scripts.
   - Wrapper post-switch summary.
   - `wt -` / recent-jump support.

4. **Full TUI (optional feature)**
   - `git-wt tui` or auto TUI with `--interactive`.
   - Keyboard-driven switch / add / remove / status.

5. **Config and themes**
   - `~/.config/git-wt/config.toml`.
   - Custom color palettes and icon sets.

---

## 12. Related documents

- [`CROSS-PLATFORM-ISSUES.md`](CROSS-PLATFORM-ISSUES.md) — platform gaps that any UI work must also account for, especially Windows console handling and path separators.
- [`README.md`](../README.md) — current command grammar and stdout contract.
