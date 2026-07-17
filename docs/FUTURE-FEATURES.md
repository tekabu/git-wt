# git-wt — Future Features

Proposed features that extend the target-first grammar in `UX-REDESIGN.md`.

Each entry is grouped by scope and includes a priority suggestion. The goal is to keep `git-wt` small and shell-native while covering the full worktree lifecycle.

---

## Quick wins (existing grammar, high daily value)

### 1. `git-wt <N> run <command>` — run a command inside a worktree

Execute a command in the worktree directory without switching your current shell.

```sh
git-wt 2 run cargo test
git-wt 2 run -- make        # passes "make" as the command
```

- The `wt` wrapper should execute the command inside the worktree and stay in the current shell afterwards.
- The binary prints an error: a child process cannot run interactively for the parent shell.

### 2. ~~`git-wt <N> diff [REF]` — diff a worktree without switching to it~~ — SHIPPED

Shipped as `git-wt <N> diff <M>`: the second side is a worktree number, not a
ref, which keeps the whole grammar target-first. `..`/`...` pick the range;
`--name-only`, `--name-status`, `--stat` and `-- PATH...` are the only flags —
git diff itself remains the escape hatch for the rest.

### 3. `git-wt <N> fetch` / `git-wt <N> pull` — remote operations in a worktree

Avoid `cd`-ing just to update a branch.

```sh
git-wt 2 pull --rebase
git-wt 2 fetch origin
```

Flags pass through to `git fetch` / `git pull`.

Shipped, with `push` alongside them and a `--all` sweep over every worktree,
which is what makes it more than a saved `cd`. Flags do not pass through: each
verb takes a curated list and any other git flag is an error, the same rule
`diff` follows. `--all` counts worktrees, never remotes.

### 4. `git-wt <N> open` — open the worktree in an external tool

```sh
git-wt 2 open            # $EDITOR or configured opener
git-wt 2 open --finder  # open in Finder (macOS)
git-wt 2 open --code    # code <path>
```

A future config file could set the default opener.

---

## Moderate additions (new subcommand or flag)

### 5. `git-wt <N> rename [NEW-NAME]` — move a worktree directory

Uses `git worktree move`. The index number may change after the move; print the new number.

```sh
git-wt 2 rename hotfix                  # leaf becomes myapp-hotfix
git-wt 2 rename --path ../elsewhere/myapp-hotfix
```

If the index changes, output something like:

```
moved to worktree #3
```

### 6. `git-wt <N> sync [REF]` — rebase or merge onto a base branch

Common worktree workflow: keep a feature branch rebased on `main`.

```sh
git-wt 2 sync          # rebase onto upstream/main or the default --from ref
git-wt 2 sync main     # rebase onto main
git-wt 2 sync --merge  # merge instead of rebase
```

- Refuse if the worktree is dirty, or require `--force`.
- Reuse `--from` semantics from `git-wt add`.

### 7. `git-wt prune` — remove stale worktrees

Clean up worktrees whose branch has been merged or deleted from the remote.

```sh
git-wt prune          # dry-run preview by default
git-wt prune --apply  # actually remove
git-wt prune --merged # only merged into default branch
git-wt prune --gone   # only branches deleted from origin
```

- Never remove the main worktree.
- Never remove dirty worktrees unless `--force` is given.

### 8. Machine-readable `git-wt list`

```sh
git-wt list --porcelain
git-wt list --json
```

Example `--porcelain` output:

```
1\tmain\t~/code/myapp\t*\n
2\tfeat/x\t~/code/myapp-feat-x\n
```

Useful for scripts that need to consume all worktrees at once.

### 9. `git-wt switch -` — go to the previous worktree

Like `cd -`. Only meaningful inside the `wt` wrapper.

```sh
wt 2
wt 3
wt -          # back to worktree 2
```

- The wrapper needs a small piece of state (environment variable or a tiny state file).
- The binary can print a hint that this only works through the wrapper.

---

## Deeper features

### 10. Config file (`~/.git-wt.toml` or `.git-wt.toml`)

Store defaults and aliases.

```toml
default_parent = "~/worktrees"
opener = "code"
editor = "vim"

[aliases]
p = "pull"
t = "run cargo test"
```

Then `git-wt 2 t` could resolve to `git-wt 2 run cargo test`.

- `--parentdir` default should come from config.
- Aliases should only expand for subcommands, not for worktree numbers.

### 11. Worktree labels / notes

Attach short notes to a worktree without affecting git metadata.

```sh
git-wt 2 note "WIP: refactor auth"
git-wt 2 note --clear
git-wt list              # shows notes column
```

Storage options:

- `.git/git-wt/notes.json` in the main worktree, or
- `.git/worktrees/<id>/git-wt.note` next to each worktree's metadata.

### 12. `git-wt add --copy-from <N>` — seed a worktree from another

Copy untracked files from an existing worktree when setting up a new one. Useful when build artifacts are expensive to regenerate.

```sh
git-wt add feat/x --copy-from 1 --exclude target
```

- This is a sharp tool; require confirmation and maybe an explicit allow-list.
- Should refuse if source and target are the same worktree.

### 13. `git-wt add --remote <url>` — add a remote branch as a worktree

Open a branch from a different remote as a local worktree.

```sh
git-wt add --remote git@github.com:me/myapp.git --branch feat/x
```

- May be out of scope; overlaps with `git clone` and `git remote add`.

---

## Polish and integrations

### 14. Shell completions

Bash, Zsh, and Fish completions for:

- valid worktree numbers,
- local and remote branch names,
- subcommands and aliases.

Number completion is the biggest UX win: `wt 1<TAB>` should only suggest existing indices.

### 15. `git-wt doctor`

Diagnose common worktree problems and offer repairs.

```sh
git-wt doctor
# worktree #3: path missing on disk
# worktree #5: branch gone from origin
```

Possible actions:

- Prune missing paths.
- Re-link orphaned worktrees if the directory still exists.

### 16. Global `--no-prompt` / `GIT_WT_NO_PROMPT`

Convert all `[y/N]` prompts into errors. Useful for scripts and CI.

```sh
GIT_WT_NO_PROMPT=1 git-wt 2 remove -y
# success if safe, error if it would have prompted
```

---

## Suggested ordering

| Phase | Features |
|---|---|
| First ship | `run`, ~~`diff`~~ (shipped), ~~`fetch`, `pull`~~ (shipped, with `push`), `open`, shell completions |
| Next | `rename`, `prune`, `list --porcelain` / `--json` |
| Later | `sync`, `switch -`, config file, worktree notes |
| Maybe | `--copy-from`, `--remote`, `doctor` |

The guiding principle: every feature should feel like a natural extension of the target-first grammar, and the binary must remain safe to use from scripts.
