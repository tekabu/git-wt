GIT-WT(1)                       git-wt Manual                       GIT-WT(1)

# NAME

git-wt — worktrees in sibling directories named <repo>-<branch>

# SYNOPSIS

    git-wt <VERB> [TARGET_LIST] [-t/--target TARGET_LIST] [-b/--branch TARGET_LIST] [FLAGS...]

# DESCRIPTION

git-wt manages git worktrees as sibling directories named `<repo>-<branch>`,
with `/`, ` `, `:` and `\` collapsed to `-`.  Commands are verb-first: name the
action, then the worktrees or branches it acts on.

# USAGE

    git-wt                       List worktrees (same as 'git-wt list')
    git-wt switch <N>            cd into worktree N (alias: cd)
    git-wt path <N>              Print worktree N's path only (alias: show)
    git-wt remove <N> [-y] [-f] [-D]
                                 Remove worktree N (-D: delete branch too)
    git-wt merge <N>,<M>         Merge M into N
    git-wt merge <N> <BRANCH>    Merge BRANCH into worktree N
    git-wt merge <N> -b <M>      Merge M into worktree N (-b is the one
                                 source to merge, not an "other target" the
                                 way it is elsewhere; -t <N> -b <M> also works)
    git-wt merge <N>,<M> review  What would that merge bring over?
    git-wt merge <N> continue|abort
    git-wt merged <N>,<M>        Is M's branch already in N's branch?
    git-wt merged <N> <BRANCH>   Is BRANCH already in worktree N's branch?
    git-wt merged <N>            Is N's branch already in the current branch?
    git-wt merged <N> --others, --ot, -o
                                 List all worktrees; show which are merged into N
    git-wt diff <N>,<M> [flags]  Diff worktree N against worktree M
    git-wt commits <N>,<M>[,...] Table: which commit is on which branch
    git-wt commits <N>           Same, just N's own log (not paired with the
                                 worktree you are in -- name it too for that:
                                 'git-wt commits -b <N>')
    git-wt log <N>[,<M>...] [PATH...] [flags]
                                 Same table, narrowed to one file's history
    git-wt meld <N>,<M>[,<N>]   Diff 2-3 worktrees side by side in meld
    git-wt <VERB> [TARGET_LIST] -b/--branch LIST
                                 Append LIST to the command's target list:
                                 'git-wt commits 1 -b 2,3' == 'git-wt commits 1,2,3'
                                 (merge is the exception -- see MERGE above)
    git-wt <VERB> -t/--target <N>
                                 Alternative spelling of the leading TARGET_LIST,
                                 for scripts that would rather always use a flag:
                                 'git-wt commits -t 1' == 'git-wt commits 1'
                                 (not on 'merge' -- '-t' there is 'theirs')
    git-wt fetch|pull|push <N>   Run it in worktree N
    git-wt pull <N>,<M>          Run it in each worktree listed
    git-wt fetch|pull|push --all Run it in every worktree
    git-wt list [SEARCH] [--col ...] [--long|--short] [--path] [--files] [--less]
                                 List, optional fuzzy filter; --col picks/orders
                                 columns (1=id, 2=branch, 3=dir, 4=status,
                                 5=last-commit, 6=merged, 7=merged-ref, 8=merged-at,
                                 9=push, 10=pull). Push/pull are the commits ahead of
                                 and behind the branch's upstream, as of the last fetch.
                                 --path (-p) adds the dir column, which a terminal
                                 leaves out; --long shows id/branch/dir/status/last/push/pull; --short
                                 id+branch+status summary. --files (-f) lists each
                                 worktree's uncommitted files under its row. Prints
                                 straight to the screen; --less pages through
                                 less/$PAGER instead.
    git-wt add [BRANCH] [flags]  Create a worktree (picker when BRANCH omitted)
    git-wt doctor [--repair]     Report worktree issues; --repair attempts fixes
    git-wt version
    git-wt -h, --help            Flag summary (clap-generated)
    git-wt -f, --full            This manual
    git-wt -hf                   Same as -f; -h is redundant once -f is given
    git-wt <VERB> --help         Options for a single command

    Aliases: ls = list, rm = remove, cd = switch, show = path,
    a = add, c = commits, l = log, m = merged, p = pull, s = switch.

    Anywhere a TARGET_LIST appears above, a worktree may be named by the branch
    it holds instead of its number, and the two spellings mix:
    'git-wt commits main', 'git-wt diff main,2', 'git-wt merge main,feat/x'.
    A bare number is always the worktree number, and a verb always wins over
    a branch of the same name; 'heads/main' reaches the branch either way.

# GRAMMAR

    git-wt <VERB> [TARGET_LIST] [-t/--target TARGET_LIST] [-b/--branch TARGET_LIST] [FLAGS...]

    VERB is one of the commands listed above.  If it is omitted and a
    TARGET_LIST is given, 'switch' is assumed; with no TARGET_LIST either,
    the worktree list is shown ('git-wt list').

    TARGET_LIST is one of:
        <N>               A worktree number, 1-based, from 'git-wt list'
        <BRANCH>          A branch name -- must be checked out in some
                          worktree; 'heads/<name>' forces branch over number
        <N>,<M>[,...]     Comma list, no spaces; numbers and branches mix
                          freely: '1,main,3'
        (omitted)         Defaults to the current worktree; use '--all'
                          with fetch/pull/push to act on every worktree
    -t/--target TARGET_LIST is an alternative spelling of the leading
    positional -- the two are interchangeable and giving both is an error:
        git-wt commits -t 1         == git-wt commits 1
        git-wt commits 1 -t 2       -> error: target given twice
    (merge does not take '-t' -- see the note under MERGE OPTIONS.)

    -b/--branch LIST is not a target itself; it appends its list to the
    target list the command already has:
        git-wt commits 1 -b 2,3     == git-wt commits 1,2,3
    merge reads it differently: there '-b' names the one source branch to
    merge, and the target list (positional or '-t') is the destination:
        git-wt merge 1 -b 2         == git-wt merge 1,2   (2 into 1)
        git-wt merge -b 2           == git-wt merge 2     (2 into current)

    VERB count per command depends on the target list's length:
        one target   -> switch, path, remove, commits, fetch/pull/push,
                         merge <BRANCH>, merged [BRANCH]
        two targets   -> merge, merged, diff, meld (also 3 for meld)
        any length    -> commits, meld (2-3), pull (each in turn)
    Some verbs also take a bare word before their own flags, matched ahead
    of a branch of the same name ('merge continue', 'merged --others'):
    spell it 'heads/continue' on the rare branch actually called that.

    FLAGS combine freely after the target list, in any order, short or long:
        git-wt commits 1,2 --author alex --all-files --filename api.php -n 5
        git-wt commits 1,2 -af          # bundle: -a (--all) + -f (--files)
        git-wt commits 1,2 -fn 20       # bundle ending in a value-flag
    Bundling (see -af above) is what clap gives natively for single-dash
    letters in 'commits'; a bundle stops at the first flag that takes a
    value, which then must be last in the bundle. Double-dash long forms
    and their short aliases (--au, --cs, --nf, --rb, ...) are never
    bundled -- each is typed on its own, exactly like the long spelling
    it stands in for.

# ADD OPTIONS

    -n, --name NAME       Suffix only -> leaf = <repo>-NAME
        --dirname DIR     Whole leaf, verbatim (sanitized); with '/' = a path
    -p, --parentdir DIR   Parent dir (default: primary worktree's parent)
        --from REF        Base ref for a NEW branch
                          (default: the branch of the worktree you run from)
        --stay            wrapper: do NOT cd into the new worktree

# REMOVE OPTIONS

    -y                    Skip the confirmation prompt
    -f, --force           Discard uncommitted/untracked changes; alongside
                          -D, also force-deletes the branch (git branch -D)
    -D, --delete-branch   Delete the worktree's branch too (git branch -d;
                          -D above forces it). Errors on a detached worktree.

# DIFF OPTIONS

    live                  Compare the files on disk, not the commits
    hunks                 Print each file's changed line numbers
    ...                   Range: only what M added since it forked from N (default)
    ..                    Range: everything that differs between the two tips
        --name-only       File names only
        --name-status     File names with A/M/D
        --stat            File names with a churn summary
    -- PATH...            Limit to these paths

# DIFF

Diffs the two worktrees' committed state (their branches), through git's
own pager, so uncommitted work does not show up; diff warns when either
side is dirty and points at 'live'.

    git-wt diff 1,2              -> git diff <branch 1>...<branch 2>
    git-wt diff 1,2 ..           -> git diff <branch 1>..<branch 2>
    git-wt diff 1,2 --stat
    git-wt diff 1,2 -- src/

The default range is '...', so 'diff 1,2' shows exactly what 'merge 1,2'
would bring in: M's own commits since the fork, and nothing of N's. '..'
compares the two tips instead, which also reports N's commits, inverted,
as if M had removed them.

Any other git flag is an error, not a passthrough: run git yourself,
'git diff <A>...<B> <flag>'. The error prints that command for you.

# DIFF LIVE

'live' compares the literal bytes in the two directories, so uncommitted
work shows up -- including the case no ref diff can ever answer, two
worktrees sitting on the same commit. Only paths git would list are
considered, so .gitignore is honored and build output stays out.

    git-wt diff 1,2 live         # literal files on disk
    git-wt diff 1,2 live hunks   # + changed line numbers
    git-wt diff 1,2 --live       # dashes optional, same thing

'live' takes no range: '..'/'...' compare commits, which is the opposite
question. --name-only/--name-status/--stat/-- PATH... all still apply.
'hunks' works without 'live' too; its line numbers are the '+' side (M).

# COMMITS OPTIONS

    -n, --limit N         Show at most N commits (newest first; default 10,
                          lifted by --all or --union)
    -a, --all             Full log of the first worktree (default is the
                          range the other worktrees are missing)
        --union           Rows from every worktree listed, not just the
                          first one's range
        --merges          Keep merge commits; they are dropped by default
        --no-cherry, --nc Skip the patch comparison behind '≈' (faster)
        --pick-id, --pi   Add a 'pick' column: the sha the '≈' copy of the
                          commit carries elsewhere
    -f, --files           Add the changed files under each commit, with
                          status and +/- line counts
        --topo            Group each branch's commits, don't interleave
        --reverse         Newest last (alias: --oldest-first)
    -w, --wrap [N]        Let a long subject take N terminal lines, not
                          one; 'full' or a bare --wrap never cuts it
        --subject-width, --subjw N
                          Give the subject N columns rather than what the
                          terminal left it; 'full' never cuts
        --branch-width, --branchw N
                          Cut a mark column's branch name to N columns
                          (default 24); 'full' never cuts
        --md [FILE]       Write a markdown table instead of printing one
                          (default: commits_<date>_<time>.md in the cwd)
        --time            Add the time to the date column, 24-hour
        --date-human, --dh 'Jan. 31, 2028' instead of the default '2028-01-31'
        --author, --au NAME
                          Only NAME's commits (fuzzy, like list's SEARCH)
    -d, --date DATE       Only commits on exactly this YYYY-MM-DD day
        --date-since, --ds DATE  That day and after
        --date-until, --du DATE  That day and before
        --commit-since, --cs C   Same bound, dated by commit C: C's day,
                          and after
        --commit-until, --cu C   Same bound, dated by commit C: C's day,
                          and before
    -c, --commits IDS     Only these commits, comma-separated shas
    -m, --message TERM    Only commits whose subject or body contains TERM
        --search TERM      Highlight every match of TERM; never drops a row.
                          'a|b' highlights both, each its own color -- quote
                          it, or the shell reads '|' as a pipe
        --filename, --fn TERM
                          Only commits touching a path containing TERM;
                          implies --files, and cuts the block to the matches
        --all-files, --af With --filename, show every file each commit
                          touched, not only the matched paths

# COMMITS

A merge-request-style view of the first worktree, counter-checked
against the rest. The default rows are the slice of the first branch
that the other branches are missing -- from the oldest missing commit
up to the first branch's tip -- so the table reads like a set of MRs
opened against worktree 1. Add --all to see the first branch's whole
log, or --union to see every listed branch's commits.

    git-wt commits 1,2,3         # branch 1's range the others miss
    git-wt commits 1,2,3 --all   # 1's full log, checked against 2 and 3
    git-wt commits 2             # worktree 2's own log, no comparison
    git-wt commits 1,2 -n 20     # newest 20 rows of the range
    git-wt commits 1,2,3 --union # every branch's commits as rows
    git-wt commits 1,2 --merges  # add the merge commits back
    git-wt commits 1,2 -af       # short flags bundle: == --all --files
    git-wt commits 1,2 -fn 20    # a value-taking flag ends the bundle

The first worktree is the target: 'git-wt commits 1,2,3' asks what 1
has that 2 and 3 do not. The range is computed from those missing
commits, so rows can include shared history if another branch diverged
earlier and branch 1 has kept committing since.

'--union' asks the other question -- 'who is out of sync with who' --
and every worktree listed contributes rows: the table becomes the union
of their full logs, and a commit missing from the first one gets a row
with a '·' under it.

'-n' caps the rows after the range is chosen; filters apply the same
way. Merge commits are dropped: they carry no work of their own, and on
a branch that merges often they are most of the table. The commits a
merge joined all stay either way -- only the merge's own row goes, and
the marks are untouched. '--merges' puts those rows back.

A single target is that worktree alone: 'git-wt commits 2' is 2's own
log, with no mark columns and nothing to be ahead of, for when the
question is about one branch's history rather than two branches'
difference. Name a second worktree to get the comparison back.

Any number of worktrees can be columns -- there is no cap, unlike
diff's two or meld's three. Each column costs its branch name plus
two; the name itself is cut at 24 columns by default so one
issue-shaped branch cannot push the subject off the edge on every
row. '--branch-width'/'--branchw' moves that cut, and 'full' lifts
it -- same shape as '--subject-width' below. The marks never wrap:
they are left of the subject.

# COMMITS FILTERS

Filters narrow the rows; the columns stay whatever the worktree list
named. They AND together, and -n counts what survives them.

    git-wt commits 1,2 --author alex
    git-wt commits 1,2 --date 2028-01-31        # exactly that day
    git-wt commits 1,2 --date-since 2028-01-01 --date-until 2028-06-30
    git-wt commits 1,2 --commit-since 5568a21 --commit-until HEAD
    git-wt commits 1,2 --commits af48509,f9e2427

Two vocabularies, one shape: '--commit-' bounds take a commit -- a
sha, a branch, a tag, 'HEAD~3' -- and '--date-' bounds take a
YYYY-MM-DD. A commit bound is read for its DAY and nothing else.
Both ends include what they name, and '--date' is one exact day: no
operators anywhere, so nothing here needs quoting against the shell.

The default rows are cut at the BOTTOM, at the earliest divergent
commit, so only a filter that names a floor has to widen them:
--commits, --date, --date-since and --commit-since imply --all, since
what they name can sit below that cut. --date-until/--commit-until do
not -- an upper bound only trims the top, which the rows already end
at, so it stays a post-filter. A range widens via its lower bound.
--author never widens: it matches many commits and named none of them.
When a filter keeps nothing, the message says which flag reaches back.

A filter also highlights what it read, so a long table can be
skimmed. The highlight follows the flag you typed, not the filter it
became: --author lights the author column, --date/--date-since/
--date-until light the date column, and --commits/--commit-since/
--commit-until light only the sha of the commit they name -- a
commit bound is a date bound underneath, but you named a commit.

--date compares the date the table prints, which is the AUTHOR date;
git's own --since/--until read committer dates and would disagree
with the column, so they are not flags here. --author is a fuzzy subsequence, case-folded, the
same match 'git-wt list SEARCH' uses: 'ach' finds 'Alex Chen'.

Date bounds are whole days: '--date 2028-07-17' takes every commit
of that day, 09:00 and 23:30 alike. The day is the author's own --
a commit written at 23:30 +0800 belongs to the day it was there, not
to yours -- so a bound never contradicts the printed column. Rows are
still ordered by the full timestamp: same-day commits sort by time of
day, even though the column only shows the day. '--time' prints
that time, 24-hour, which is what tells a busy day's rows apart.

# COMMITS MD

'--md' writes the table to a markdown file rather than the terminal.
The file records the command that made it, so a report pasted into an
issue says how to reproduce itself.

    git-wt commits 1,2 --md              -> commits_<date>_<time>.md
    git-wt commits 1,2 --md report.md    -> that path, overwritten
    git-wt commits 1,2 --merges --md     # filters apply as usual

The default name is stamped to the second, so a re-run never eats the
last report; a name you pass is yours, and is overwritten. The path is
optional, so a flag may follow '--md' -- it is read as a flag, never
as a filename.

Subjects are whole in a file: there is no right edge to run out of, so
nothing is truncated. A '|' in a subject is escaped rather than left
to end the cell and shift the columns after it.

# COMMITS DATES

The date column is ISO, the same shape the filters take, so a date
read off the table pastes straight back into --date-since. It also
sorts, greps, and is one width on every row.

    git-wt commits 1,2                     -> 2028-01-31
    git-wt commits 1,2 --time              -> 2028-01-31 14:30:05
    git-wt commits 1,2 --date-human        -> Jan. 31, 2028
    git-wt commits 1,2 --date-human --time
                                           -> Jan. 31, 2028 14:30:05

--date-human is easier to read a date out of, at the cost of the
round-trip: it is not what --date-since accepts. What --date compares
never changes shape whatever the column is spelled as.

Quote --date, always. '>' and '<' are redirects, so an unquoted
--date >=2028-01-01 writes a file called '=2028-01-01' and git-wt
sees no date at all. --date-since/--date-until need no quoting.

Rows are ancestry-first: no parent is ever listed above its child, so
reading down the table is reading the real history. Dates only order
commits that do not descend from each other -- which is why a commit
authored before its own parent (a rebase, a cherry-pick, a bad clock)
reads as out of order against the date column. The story is right; the
clock is not.

Within that, two readings. By default the rows are newest-first, so a
row's neighbors are its contemporaries -- what happened when. '--topo'
keeps each line of history in one block instead -- what each branch did,
which is what --union tables are usually read for. Neither depends on --time:
the order always reads the full timestamp, and --time only prints
what it read. '--reverse' puts the newest last, after the -n cap, so
the rows are the same ones read bottom-up.

    git-wt commits 1,2,3 --topo

The subject comes last because it is the only free-form cell: an emoji is
two terminal columns wide but one character, so a padded subject column
would shift every column after it. Nothing is padded after it, so the marks
line up whatever the subject holds. Too long for the terminal, it is cut
rather than wrapped; piped output is never cut, so 'commits | grep' still
sees whole subjects. Dates are author dates, and the author is
.mailmap-aware, so one contributor is one name.

'--wrap N' buys the cut subject more room: N lines instead of one, each
the width the subject column already had, so what a conventional-commit
prefix pushes past the edge lands on the next line rather than in an
ellipsis. The extra lines are indented to the subject column, so the
table still reads as one row per commit -- and one row per commit is
why the default stays 1.

    git-wt commits 1,2 --wrap 2      # two lines of subject
    git-wt commits 1,2 -w full       # whole subject, however many
    git-wt commits 1,2 --wrap        # the same 'full'

Only the last line an N allows is ellipsized, and only when the subject
outruns it. 'full' wraps until the subject is spent, so nothing is ever
lost; off a terminal there is nothing to wrap to and --wrap does nothing,
the whole subject being on the line already.

'--subject-width N' moves the cut itself. The subject's width is normally
whatever the columns left of it did not take, which is the right answer
until the subject is what you came to read -- and then those columns are
the ones in the way. N is that width instead, however wide the terminal
is, so a subject may run past the edge. That is the point: the terminal
soft-wraps it, or 'less -S' scrolls it, and either beats an ellipsis.

    git-wt commits 1,2 --subject-width 100   # 100 columns, edge or no edge
    git-wt commits 1,2 --subjw full          # never cut, however long
    git-wt commits 1,2 --subjw 60 --wrap 3   # 3 lines of 60

The two compose: --subject-width is how wide a line is, --wrap is how many
of them. An asked-for width is the width, so unlike --wrap it also applies
off a terminal -- '--subjw 60 | grep' cuts at 60, where a bare 'commits |
grep' still sees whole subjects. N is at least 24: below that a cut subject
says nothing, which is what 'full' is for.

'--branch-width N' does the same for a mark column's header: branch
names have no natural bound the way author names do, and an
issue-shaped one is cut to 24 columns by default, on a terminal or
off, so it cannot drag every row's marks and subject rightward.
'full' never cuts it; N is at least 12, the shortest a header can be
and still tell two branches apart.

    git-wt commits 1,2 --branch-width 40   # allow longer names
    git-wt commits 1,2 --branchw full      # never cut, however long

Unlike '--subject-width', this has nothing to do with the terminal --
the branch column is fixed-width, so the cut is the same piped or not.

'--files' adds the files a commit touched, indented under the subject.
Each file shows a status letter (A/M/D/R/C) and the added/removed line
count. A blank line separates the commit from its file block, and another
separates the block from the next commit. The work is scoped to the rows
the table already shows, so pair it with '-n' or filters on large logs.
Merge commits show the diff against their first parent.

    git-wt commits 1,2 -n 10 --files
    git-wt commits 1,2 --author chen --files

# LOG

'log' is the same table as 'commits' -- same rows, columns, filters,
renderer -- with a pathspec selecting the rows instead of a branch range.

    git-wt log 1,2 src/cmd/commits/render.rs
    git-wt log 1 src/ui.rs --author alex --date-since 2028-01-01 -w

Targets are worktrees or branches, exactly like 'commits': the first is
the row source, the rest are mark columns. PATH always comes after the
target list, never in the target slot -- that would collide with a branch
name like 'feature/foo'. It resolves against the worktree it sits under
(absolute, relative, or repo-relative, from anywhere), so any worktree's
copy of the path names the same file in history. PATH omitted is the
current directory, repo-relative.

Every 'commits' flag still works the same way, with four exceptions:
'--filename', '--all', and '--all-files' are not words 'log' knows at
all -- the path already is the target, so typing one gets the same
unknown-argument error any other typo would. '--union' is load-bearing:
the default rows are the first branch's history of the path; '--union'
unions in every listed branch's. '-f'/'--files' means the *other* files
each shown commit touched, since the row already carries the path's own
'±'. '--squash' becomes the path's lifetime totals -- commits, authors,
'±', first and last touch -- folded into the header line.

    git-wt log 1,2,3 src/ui.rs --union     # every branch's touches
    git-wt log 2 src/ui.rs -f               # + the other files touched
    git-wt log 1 src/ui.rs --squash         # lifetime totals

'--no-follow' is new: with exactly one PATH, 'log' follows a rename
automatically; '--no-follow' stops at it instead. A path deleted on this
branch and alive on another is not an error -- that is exactly what
'log' is for -- and the empty result names the rename escape hatch:

    no commits touched 'src/old.rs' on main
    hint: it may live under another name; --no-follow shows the literal path only

A path outside every worktree is an error naming both readings:

    error: '/etc/hosts' is outside the repository
    hint: paths are resolved against the worktree they sit in

The table adds one column, '±': the added/removed line count scoped to
the path alone, not the commit-wide count '-f' prints. A 'path' column
appears only when it varies -- a rename crossed under '--follow', or more
than one PATH given -- otherwise the header already names it.

# MARKS

    ✓   the branch has this commit
    ≈   the branch has this patch under a different sha
    ←   the branch has a commit whose `-x` trailer names this commit
    ~   the branch has a commit with the same author/date/subject
    ·   the branch has none of the above

Precedence: ✓ > ≈ > ← > ~ > ·.

'≈' is a cherry-pick or a rebase's copy detected by patch-id. To git
those are different commits, so a bare '✓/·' calls them missing -- which
reads as work to do, when the work is done. The comparison is git's own
'git cherry': patch-ids, not history, per pair of branches. '--no-cherry'
skips it and takes the old, cheaper answer, for a repo whose branches have
diverged by thousands of commits.

'←' is a stronger signal: a `git cherry-pick -x` on the other branch left
a trailer that names this exact commit. '~' is the fallback for picks that
changed enough in conflict resolution to defeat patch-id, or for picks made
without `-x`: it matches the author email, author date (with timezone), and
subject that cherry-pick preserves exactly.

A picked commit shows twice, once per sha: the original row is '≈'/←/~ in
the branch that took it, the copy's row is '≈'/←/~ in the branch it came
from. Both are true -- they are two commits carrying one patch.

'--pick-id' names the other sha for '≈': a 'pick' column after 'commit',
holding the sha the same patch was committed under elsewhere. It is the
row's other half -- the sha to hand 'git show', or to check a pick landed
where you meant it to. Rows with no copy leave it blank, and a patch
carried under three shas names the first of the others.

# SYNC OPTIONS        (fetch/pull/push; any other git flag is an error, not a passthrough)

    -a, --all             Every worktree, not the ones a list named
    fetch: -p, --prune | --tags | --no-tags, --nt | --force
    pull:  --rebase, --rb | --no-rebase, --nr | --ff-only | -p, --prune |
           --autostash, --as
    push:  -u, --set-upstream | --force-with-lease, --fl | --tags |
           -n, --dry-run

# SYNC

fetch/pull/push run git in a worktree's own directory, so each one syncs
its own branch against its own upstream. Nothing here is a shortcut for
something git does not do -- it is the cd you would type first.

    git-wt pull 1                # git -C <dir 1> pull
    git-wt fetch 1,3 --prune     # both, one after the other
    git-wt pull --all            # every worktree
    git-wt push 2 -u             # push and set the upstream

'--all' is the whole point: a repo with six worktrees is six branches, and
they go stale one at a time. It sweeps every worktree in 'list' order. Use
'git-wt pull --all' when you mean every worktree; 'git-wt pull' with no
target syncs the worktree you are standing in.

A sweep never stops on a failure. One worktree with no upstream, or a pull
that hits a conflict, would otherwise leave the worktrees after it untouched
and unmentioned -- half-synced, and no line saying which half. So every one
runs, each failure prints where it happened, and the last line counts them:

    pull: 4 ok, 1 failed, 1 skipped

The exit code is that summary, nonzero when anything failed. A single
target is not a sweep: git's own error is the whole story, and it exits
with it unsummarized.

Skipped is what the verb cannot mean. A bare worktree has nothing to pull
into, and a detached HEAD has no branch, so no upstream to push to; fetch
only moves remote-tracking refs, so it runs on both. A skip is not a
failure -- there was nothing to do.

'git fetch --all' means every REMOTE. Here '--all' means every worktree,
always, for all three verbs: 'git-wt' counts worktrees, that is what it is
for. For every remote, run git yourself.

Flags are the curated list above, not a passthrough, the same rule diff
follows. 'push --force' is the one refused outright: it overwrites a remote
branch without checking what is on it, and '--all' would do that to every
branch at once. '--force-with-lease' is the one that checks.

# MELD

Opens meld on the worktree directories, in the order you list them, and
waits until you close it. Requires meld on PATH.

    git-wt meld 1,3      -> meld <dir 1> <dir 3>
    git-wt meld 2,1,3    -> meld <dir 2> <dir 1> <dir 3>  (3-way)

With --diff, meld sees only the files that differ between the two refs,
extracted into sparse temp directories. Add --3way or --base <ref> to
include the merge-base as a third pane.

    git-wt meld 1,2 --diff            # only files that differ
    git-wt meld 1,2 --diff ...        # only what branch 2 added since fork
    git-wt meld 1,2 --diff --3way     # + merge-base in the middle pane
    git-wt meld 1,2 --diff --base main # + explicit base in the middle pane

# MERGE WORDS            (each takes an optional '--': 'abort' == '--abort')

    -c, continue          Conclude a conflicted merge
    -a, abort             Undo a conflicted merge
    -o, ours              On a conflicting hunk, keep worktree N's side
    -t, theirs            On a conflicting hunk, take the source's side
    -d, dry-run           Report whether it would merge; change nothing
        review            Show the commits it would bring over; change nothing

# MERGE OPTIONS

    -m, --message MSG     Merge commit message
        --no-ff, --nf     Always create a merge commit
        --ff-only, --fo   Refuse anything but a fast-forward
        --squash          Stage the merge without committing
    -f, --force           Merge even when worktree N has uncommitted changes

'-t/--target' is not a merge flag: '-t' is already 'theirs' above, so
merge's target (worktree N) is positional-only, never '-t <N>'. '-b' means
something different here too -- see MERGE below.

# MERGE

The merge runs inside worktree N, so N's branch is the one that moves:

    git-wt merge 1,2            # worktree 2's branch -> worktree 1's branch
    git-wt merge 1 feat/x       # a branch name works too
    git-wt merge 1 -b 2         # worktree 2's branch -> worktree 1's branch
    git-wt merge -b 2           # same, N defaults to the current worktree
    git-wt merge 1,2 dry-run    # would it conflict? nothing is touched
    git-wt merge 1,2 theirs     # let 2 win every collision

'-b/--branch' on merge is the one source branch to merge in, not an
"other target" the way it is on every other verb -- it takes exactly one
branch, and errors if it names the same worktree as the target.

The list reads dest-first, so 'merge 1,2' merges 2 into 1. It takes
exactly two worktrees -- unlike meld, which diffs 2-3 -- because a
merge has one destination and one source. The list already names the
source, so it cannot be combined with 'continue'/'abort'; those take a
single target, 'git-wt merge 1 continue' (or 'git-wt merge 1 abort').

A number that names a worktree wins over a branch of the same name, and
the words above win over a branch of the same name: to merge a branch
called 'theirs', spell it 'heads/theirs'.

On conflict, git-wt exits nonzero and lists the conflicted files; fix
them in worktree N, then run 'git-wt merge N continue' (or abort).
Merge commits never open an editor: without -m, git's default message is
taken as-is.

# MERGE REVIEW

'dry-run' answers whether a merge conflicts. '--review' answers what it
would bring: the same verdict as a header, then the commit table for
'dest..src'. It merges nothing and keeps dry-run's exit codes, 0 clean
and 1 on conflict.

    git-wt merge 1,2 --review        # what would 2 bring into 1?
    git-wt merge 1,2 --review -f     # + the files under each commit
    git-wt merge 1,2 --review -n 5 --author alex

'--review' ends merge's own flags. Everything after it is a 'commits'
flag and is passed through untouched, which is the only way both can keep
the letters they share: '-f' after '--review' is --files, not --force.
Merge options before it are an error rather than a silent claim, so put
them after -- or drop them, since nothing is being merged. After it they
are an error too, and one that says which: '--review --dry-run' is told
the two answer the same question, not that '--dry-run' is unexpected.

'--all' and '--union' are refused as well, though they are commits flags:
both name a row source, and a review's is already the range 'dest..src'.
('-a' is '--all', so it is refused under that name -- '-fn 5' is the
bundle that still works here.)

The single mark column is the DESTINATION's, and it has four answers:

    ·  the commit is new to the destination
    ≈  its patch is already there under a different sha
    ←  a `-x` trailer on the destination names this commit as its source
    ~  the destination has a commit with the same author/date/subject

The non-'·' marks are what a cherry-picked hotfix leaves behind: the row is
genuinely absent by sha, so the merge still lists it, but the work has
landed. There is no check column -- every row is in the source by
definition, so it would say nothing.

Merge commits are shown, unlike in 'commits', where they are dropped: a
review range is bounded by the merge about to happen, so a merge inside
it is the cargo rather than the noise. '--review --no-merges' drops them.
A merge carries no patch of its own, so it can never be marked '≈' and
'--review --pick-id' leaves its cell empty; that is the mark saying
nothing about merges, not a missing answer.

'ours'/'theirs' are git's -X strategy options, so they settle only the
hunks that actually collide -- the rest of both sides still merges. They
are applied while the merge is computed, so they cannot join a merge that
has already stopped: git-wt offers to abort and redo it instead.

# MERGED

Ask whether one branch is already contained in another.

    git-wt merged 1,2             # is 2's branch in 1's branch?
    git-wt merged 1 feat/x        # is feat/x in worktree 1's branch?
    git-wt merged 1               # is worktree 1's branch in the current branch?
    git-wt merged 1 --others      # list every worktree against worktree 1
    git-wt merged 1 --ot -p       # short alias for --others; -p adds the path column
    git-wt merged 1 -o            # -o is the same short alias

The normal forms answer yes/no, exiting 0 for "already merged" and nonzero
for "ahead". The `--others` form prints a table with a `merged` column and
a `merged-at` column showing when the source branch was last merged into
the selected branch. `merged-at` is '-' for fast-forward merges and for
branches that are not yet merged.

# ADD

The worktree directory is a sibling of the repo root, named
<repo-folder>-<branch>, with '/', ' ', ':' and '\\' collapsed to '-'.

    ~/code/myapp  +  feature/login  ->  ~/code/myapp-feature-login

Branch resolution, in order:
  1. Local branch exists      -> check it out
  2. <remote>/<branch> exists -> create a tracking branch from it
                                 (prefers origin, else first remote match)
  3. Neither                  -> prompt, then create from --from (HEAD)

With no BRANCH, a picker lists local branches: fzf when installed,
otherwise a numbered prompt.

# DOCTOR

Reports what is wrong with a worktree's registration -- not its working
tree contents, which 'list' already covers -- read straight off git's own
'worktree list --porcelain' and the filesystem, so it costs no extra
history walk:

    prunable        git's own verdict: the directory is gone or its
                      administrative files no longer point at a live one
                      (the same signal 'git worktree prune' acts on) --
                      this is what a moved or deleted worktree looks like
    directory not found on disk   the filesystem check, for a git that
                      has not rescanned yet
    '.git' points to a missing admin dir   the directory is still there,
                      but its back-pointer names an admin dir that no
                      longer exists -- what a linked worktree looks like
                      after the *main* worktree gets moved or renamed,
                      since git never updates that pointer on its own
    HEAD unreadable   The directory exists but git can't read its HEAD
    locked            not broken -- why 'remove'/'prune' refuse to
                      touch it -- reported alongside, never suppressing
                      the checks above it

    git-wt doctor              # report only, nothing changed
    git-wt doctor --repair     # attempt to fix what it found

'--repair' runs 'git worktree repair' over every candidate this repo
might mean: every worktree's own recorded path (the fix when the *main*
worktree moved -- each linked worktree's manual '.git' file is rewritten
using the path 'worktree list' already had), plus every sibling of the
repo root whose '.git' is a plain file (the fix when a *linked* worktree
moved -- 'add' puts every worktree it you create there, so one moved by
hand usually still turns up in the list even though its old recorded
path does not name it anymore). 'repair' only relinks a candidate whose
'.git' file already agrees with one of this repo's admin dirs, so
handing it every candidate is safe: an unrelated directory is left
untouched. Whatever neither fixes -- a directory truly deleted, not
moved -- is swept by 'git worktree prune' afterward, which only removes
entries git already marked prunable. The report then re-runs, so the
output says what is actually still wrong, not what was true before the
repair.

# STDOUT

Only 'switch'/'path' (with a target), 'add', and 'remove' print a path,
alone, on stdout, so a shell can cd into it or capture it. Status goes
to stderr. 'switch'/'cd' need a target -- see 'git-wt list' to pick one.

    cd "$(git-wt path 1)"
    cd "$(git-wt switch 1)"
    dir="$(git-wt add feature/login)"

# COLOR

Color and status/last-commit columns turn on only when stdout is a
terminal, so 'git-wt list | cat' stays plain and parseable. Honors
NO_COLOR (disable) and CLICOLOR_FORCE (force on).
