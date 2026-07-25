# git-wt commands

Quick reference. Happy path only.

Aliases: `ls`=list, `rm`=remove, `cd`=switch, `a`=add, `c`=commits,
`l`=log, `m`=merged, `p`=pull, `r`=review, `s`=switch.

`git-wt` (binary) always prints a path; `wt` (the `--alias` shell function)
`cd`'s into it for `switch`/`cd`/`add`/`remove`. Everything else behaves
identically either way.

### Branch names instead of numbers

Anywhere a target or target list takes `<N>`, a worktree may be named by its
branch instead, mixed freely with numbers: `git-wt diff main,2`. The branch
must be checked out in a worktree. A bare number always means a worktree
number, even if a branch shares that name — write `heads/2` for a branch
literally called `2`. A command word likewise wins over a same-named branch —
reach a branch called `list` as `heads/list`.

### `-b`/`--branch` — append extra targets

`-b`/`--branch LIST` appends to whatever target list the positional already
named (or to the current worktree if the positional is omitted):

```sh
git-wt commits -b 1,2       # == git-wt commits <current>,1,2
git-wt review -s 2          # review: has its own -s/--source instead, no -b at all
git-wt merge -s 2           # merge: same, its own -s/--source instead, no -b at all
```

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

## path

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
    git-wt diff 1,2 --live
    git-wt diff 1,2 -p src/       limit to paths (comma list: -p src/,docs/)
    git-wt diff main,2             mix branch names and numbers
    git-wt diff 1,2 -m             open changed files in meld, wait for exit
    git-wt diff 1,2 --live -A      files worktree 1 has and 2 does not
    git-wt diff 1,2 --live -B      files worktree 2 has and 1 does not

Sample (`git-wt diff 1,2 --stat`):

     new.txt | 1 +
     1 file changed, 1 insertion(+)

Options:

    -t, --target TARGET_LIST     alternative spelling of the positional target
    -b, --branch TARGET_LIST     extra worktrees to include, alongside the target (global flag)
    ..                            tip-vs-tip range word
    ...                           fork-point range word
    --live                        diff against working tree
    --hunks                       hunk-level diff
    -A, --a-only                  only files the first worktree has and the second lacks
    -B, --b-only                  only files the second worktree has and the first lacks
    -m, --meld                    copy changed files to a tmp dir and open meld, waiting for exit
    --name-only / --name-status / --stat   git diff pass-through flags
    -p, --path PATH_LIST          restrict to paths, comma-separated (globs ok; errors if one matches nothing)

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

## compare

Compares files in the current worktree against a ref — not two worktrees, just
files vs. one rev, so it takes its own `-r/--ref` and refuses the global
`-b/--branch`.

    git-wt compare -f src/main.rs -r main
    git-wt compare -f src/main.rs,Cargo.toml -r HEAD~3
    git-wt compare -f src/main.rs -r origin/main -m   open in meld, wait for exit

Sample (`git-wt compare -f Cargo.toml -r main`, plain diff to stdout):

    diff --git a/Cargo.toml b/Cargo.toml
    index 465958a..66c8ce7 100644
    --- a/Cargo.toml
    +++ b/Cargo.toml
    @@ -13,3 +13,4 @@ lto = true
     strip = true
     codegen-units = 1
     panic = "abort"
    +test

Options:

    -f, --file FILE_LIST         required; comma-separated relative paths to compare
    -r, --ref REF                required; anything git resolves to a commit — branch, origin/branch, tag, sha, HEAD~3
    -m, --meld                   extract ref's versions to a temp dir and open meld, waiting for exit

## merge

    git-wt merge 2                  merge 2 into the current worktree
    git-wt merge feat/x             a branch works where a number does
    git-wt merge 2 -d 1             merge 2 into worktree 1
    git-wt merge -s 2 [-d 1]        name the source with -s instead
    git-wt merge 2 --dry-run
    git-wt merge 2 --theirs
    git-wt merge 2 --ours
    git-wt merge --continue [-d 1]
    git-wt merge --abort [-d 1]

The bare word is always the source, read the way `git merge <thing>` reads it;
the destination is `-d/--destination` (alias `--dest`) and defaults to the
current worktree. Two sources (`merge 2 -s feat/x`) is not accepted — use one
or the other. Before an actual merge runs (not
`--dry-run`/`--continue`/`--abort`), it asks `Merge <src> into <dest>? [y/N]`.

Sample (`git-wt merge -s feature/review -d 1 --dry-run`):

    Clean feature/review merges into main cleanly

Options (source, then merge options/words):

    -s, --source BRANCH           one source branch (merge's own; '-b' is a plain worktree/branch elsewhere)
    -d, --destination, --dest TARGET  destination worktree (default: current)
    --theirs                      take theirs on conflict
    --ours                        take ours on conflict
    --dry-run                     preview without merging
    -c, --continue                 resume an in-progress merge
    -a, --abort                    abort an in-progress merge

## review / r

    git-wt review 2                 what would 2 bring into the current worktree?
    git-wt review 2 -d 1            ...into worktree 1 instead
    git-wt review feat/x            a branch works where a number does
    git-wt review -s 2 [-d 1]       name the source with -s instead
    git-wt review 2 -f              + the files under each commit
    git-wt review 2 -n 5 --author alex
    git-wt review 2 --squash        one consolidated file block
    git-wt review 2 --meld          open the touched files in meld instead

`merge --dry-run` says whether a merge conflicts; `review` says what it would
bring: the same verdict as a header, then the commit table for `dest..src`. It
merges nothing, and its exit code is the verdict — 0 clean, 1 on conflict.

Source and destination read exactly as `merge`'s do: the lone positional is the
source, `-d/--destination` (alias `--dest`) is the destination and defaults to
the current worktree.

Being its own verb is what lets `review` own the whole `commits` flag
vocabulary without arbitrating letters against merge's: `-f` is `--files` here
and `--force` under `merge`, `-a` is `--all` here and `--abort` there. Its
own `-d/--destination` (rather than `commits`' shared `-t/--target`) is the
same reasoning: `commits`' own `--date` already sits on `-d`.

`--all` and `--union` are refused: both name a row source, and a review's is
already the range `dest..src`.

Sample (`git-wt review 2`):

    feat -> main   2 commits, merges cleanly
    commit   author        date  main  subject
    9c21184  t       2026-07-25   ·    touch shared
    2aa82e4  t       2026-07-25   ·    add a.txt

Options: `-s, --source BRANCH` (second spelling of the source),
`-d, --destination, --dest TARGET`, plus the `commits` table's and `--meld`.
See `commits` for the full list.

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
