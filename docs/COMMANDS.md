# git-wt commands

Quick reference. Happy path only.

## switch / cd / s

    git-wt                      list worktrees
    git-wt switch 2              cd worktree 2
    git-wt cd main                cd branch main
    git-wt switch -t 2            same, via -t

Sample (`git-wt switch 2`, via wrapper):

    clap
    /home/nino/dev/git-wt-clap

Options:

    -t, --target TARGET_LIST     alternative spelling of the positional target

## path / show

    git-wt path                  current worktree's path
    git-wt path 2                worktree 2's path

Sample (`git-wt path 2`):

    clap
    /home/nino/dev/git-wt-clap

Options:

    -t, --target TARGET_LIST     alternative spelling of the positional target

## list / ls

    git-wt list
    git-wt list foo               fuzzy filter
    git-wt list --long
    git-wt list --short
    git-wt list -p                 with path column
    git-wt list -f                  with uncommitted files
    git-wt list --col 1,2,6
    git-wt list --less             page through less instead of the screen

Sample (`git-wt list -p`):

    1  main            /home/nino/dev/git-wt
    2  clap            /home/nino/dev/git-wt-clap
    3  feature/review  /home/nino/dev/git-wt-feature-review

Options:

    -c, --col COLS       pick and order columns: 1=id,2=branch,3=dir,4=status,
                          5=last-commit,6=merged,7=merged-ref,8=merged-at,9=push,10=pull
    -l, --long            long output (id, branch, dir, status, last-commit, merged, push, pull)
    -s, --short           short output (id, branch, status)
    -p, --path             include the directory/path column
    -f, --files           list uncommitted files under each worktree
        --less             page through less/$PAGER instead of printing straight to the screen

## add / a

    git-wt add feature/login
    git-wt add feature/login -n review
    git-wt add feature/login --from develop

Sample (`git-wt add feature/review --from main`):

    Branch 'feature/review' does not exist. Create it from 'main'? [y/N] y
    Creating new branch 'feature/review' from 'main'
    Preparing worktree (new branch 'feature/review')
    Created repo-feature-review
    /home/nino/dev/repo-feature-review

Options:

    -n, --name NAME           suffix only: leaf becomes <repo>-NAME
        --dirname DIRNAME     whole leaf, verbatim (sanitized); with '/' it's a path
    -p, --parentdir DIR       parent directory (default: primary worktree's parent)
        --from REF            base ref for a new branch
    -s, --stay                shell-wrapper hint: don't cd into the new worktree

## remove / rm

    git-wt remove 2 -y
    git-wt remove 2 -y -f
    git-wt remove 2 -y -f -D

Sample (`git-wt remove 2 -y`):

    Removed repo-feature-review  (branch feature/review kept)

Options:

    -t, --target TARGET_LIST     alternative spelling of the positional target
    -y, --yes                    skip the confirmation prompt
    -f, --force                  discard uncommitted/untracked changes; with -D, force-delete the branch
    -D, --delete-branch          delete the worktree's branch too

## commits / c

    git-wt commits
    git-wt commits 2
    git-wt commits 1,2
    git-wt commits -b 2
    git-wt commits -t 3 -b 2
    git-wt commits 1,2 --author alex -n 5
    git-wt commits 1,2 -af
    git-wt commits main,2          mix branch names and numbers

No target and no -b: current worktree only. A target list replaces that
default outright -- it is not current-plus-list; -b is what's additive
(`-b 2` alone means current worktree + 2).

Sample (`git-wt commits 3 -n 2`):

    commit   author  date        subject
    b9d8c1f  nino    2026-07-19  Simplify alias function and fzf hint in install script
    c93b8e0  nino    2026-07-19  Rename linux-test.sh to test-linux.sh

Options (target list, then raw `git log` options/filters):

    -b, --branch TARGET_LIST     extra worktrees to include, alongside the target (global flag)
    -t, --target TARGET          target worktree, alternative to leading positional
        --author NAME            filter by author
    -n COUNT                     limit number of commits
    -a                           all branches
    -f                           include file stats

## log / l

    git-wt log
    git-wt log 1,2
    git-wt log 1,2 src/ui.rs
    git-wt log main,2               mix branch names and numbers

Sample (`git-wt log 2 -n 1`):

    .   clap   1 commit, +200 -72, 1 author
    commit   author  date        ±         path         subject
    510045b  nino    2026-07-23  +200 -72  README.md     docs: add doctor, picker, -b flag

Options (optional target list, then path and raw `git log` options; first
token is consumed as target only if it resolves as a worktree list):

    -b, --branch TARGET_LIST     extra worktrees to include, alongside the target (global flag)

## diff

    git-wt diff 1,2
    git-wt diff 1 -b 2
    git-wt diff 1,2 --stat
    git-wt diff 1,2 live
    git-wt diff 1,2 -- src/
    git-wt diff main,2             mix branch names and numbers

Sample (`git-wt diff 1,2 --stat`):

     new.txt | 1 +
     1 file changed, 1 insertion(+)

Options:

    -t, --target TARGET_LIST     alternative spelling of the positional target
    -b, --branch TARGET_LIST     extra worktrees to include, alongside the target (global flag)
    ..                            tip-vs-tip range word
    ...                           fork-point range word
    live / --live                 diff against working tree
    hunks / --hunks                hunk-level diff
    --name-only / --name-status / --stat   git diff pass-through flags
    -- PATHSPEC                   restrict to paths

## meld

    git-wt meld 1,2
    git-wt meld 1,2,3
    git-wt meld 1,2 --diff
    git-wt meld main,2,3            mix branch names and numbers

Sample (`git-wt meld 1,2 --diff`, nothing differs):

    no files differ between main and feature/review

Options:

    -t, --target TARGET_LIST     alternative spelling of the positional target
    -d, --diff                   filter to files that differ, extracted into temp dirs
        --3way                   diff only: three-way with auto base
        --base REF                diff only: explicit base ref (branch, commit, or worktree number)
    RANGE                         diff only: `..` (tip-vs-tip, default under --diff) or `...` (fork)

## merge

    git-wt merge 1,2
    git-wt merge 1 feat/x
    git-wt merge 1 -b 2
    git-wt merge -b 2
    git-wt merge 1,2 dry-run
    git-wt merge 1,2 theirs
    git-wt merge 1,2 ours
    git-wt merge 1,2 --review
    git-wt merge 1 continue
    git-wt merge 1 abort
    git-wt merge main,2             mix branch names and numbers

Sample (`git-wt merge 1 feature/review dry-run`):

    Clean feature/review merges into main cleanly

Options (target list, then merge options/words — raw catch-all):

    -b, --branch TARGET_LIST     extra worktrees to include, alongside the target (global flag)
    -t, --theirs                  take theirs on conflict
    dry-run                       preview without merging
    theirs / ours                 conflict-resolution strategy words
    --review                      hand off to review flow
    continue / abort              resume or abort an in-progress merge

## merged / m

    git-wt merged 1
    git-wt merged 1,2
    git-wt merged 1 feat/x
    git-wt merged 1 --others
    git-wt merged main,2            mix branch names and numbers

Sample (`git-wt merged 1 --others`):

    1  main            self     -
    2  feature/review  ahead 1  -

Options:

    -t, --target TARGET_LIST     alternative spelling of the positional target
    SOURCE                        with one target, compare against this branch/worktree instead of listing every other worktree
    -o, --others                  list every worktree and whether it's merged into the target
    -s, --show-path                include the worktree path in the --others table

## fetch / pull / push

    git-wt fetch
    git-wt fetch 1
    git-wt fetch 1,2
    git-wt fetch --all
    git-wt pull 1
    git-wt push 1 -u
    git-wt fetch main,2,3           mix branch names and numbers

Sample (`git-wt fetch --all`):

    fetch main
    fetch feature/review

    fetch: 2 ok, 0 failed, 0 skipped

Options:

    -t, --target TARGET_LIST     alternative spelling of the positional target
    -a, --all                     run in every worktree (omit target list to use this)
    FLAGS                         git flags for the verb (curated list; see --help), e.g. push -u

## doctor

    git-wt doctor
    git-wt doctor --repair

Sample (`git-wt doctor`):

    all worktrees healthy

Options:

    -r, --repair                  attempt to fix what is found

## misc

    git-wt version
    git-wt -h
    git-wt -f
    git-wt <verb> --help
