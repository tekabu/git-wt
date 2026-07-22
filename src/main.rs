//! git-wt — create and manage git worktrees in sibling directories named
//! `<repo-folder>-<sanitized-branch>`.
//!
//! Installed on PATH as `git-wt`, so it is also reachable as `git wt`.
//!
//! Grammar is target-first for existing worktrees (`git-wt <N> <action>`) with
//! an explicit `add` verb for creation.

mod cli;
mod cmd;
mod git;
mod ui;
mod worktree;

use crate::cli::{
    branch_targets, dispatch_target, dispatch_targets, extract_branch_flag, list_from_args,
    parse_target_list, resolve_target, resolve_target_list, unknown_command_msg,
};
use crate::cmd::add::cmd_add;
use crate::cmd::sync::{cmd_sync, parse_sync_args, SyncOp, ALL_HINT};
use crate::worktree::{current_worktree_index, repo_root, worktrees};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The short usage list -- one line per command, no inline prose. The
/// option tables below it are not duplicated by hand: `short_help` pulls
/// them straight out of `HELP`, so a flag added there never has to be
/// added here too.
const SHORT_USAGE: &str = "\
git-wt — worktrees in sibling directories named <repo>-<branch>

USAGE:
    git-wt                       List worktrees, numbered from 1
    git-wt list [SEARCH] [--col ...] [--long|--short] [--show-path] [--files]
    git-wt <N>                   == git-wt <N> switch
    git-wt <N> switch            cd into worktree N (alias: cd)
    git-wt <N> path              Print worktree N's path only (alias: show)
    git-wt <N> remove [-y] [-f]  Remove worktree N
    git-wt <N>,<M> merge         Merge M into N
    git-wt <N> merge <BRANCH>    Merge BRANCH into worktree N
    git-wt <N>,<M> merge review  What would that merge bring over?
    git-wt <N> merge continue|abort
    git-wt <N>,<M> merged        Is M's branch already in N's branch?
    git-wt <N> merged <BRANCH>   Is BRANCH already in worktree N's branch?
    git-wt <N>,<M> diff [flags]  Diff worktree N against worktree M
    git-wt <N>,<M>[,...] commits Table: which commit is on which branch
    git-wt <N>,<N>[,<N>] meld    Diff 2-3 worktrees side by side in meld
    git-wt -b/--branch LIST <action>
                                 LIST with the current worktree prepended
    git-wt <N> fetch|pull|push   Run it in worktree N
    git-wt fetch|pull|push --all Run it in every worktree
    git-wt add [BRANCH] [flags]  Create a worktree (picker when BRANCH omitted)
    git-wt version
    git-wt --help                This: options, no prose
    git-wt --help -f             Full manual: every flag, every section

    Aliases: ls = list, rm = remove, cd = switch, show = path.

    A worktree may be named by the branch it holds instead of its number:
    'git-wt main commits', 'git-wt main,2 diff'.
";

const SHORT_FOOTER: &str = "\
'git-wt --help -f' (or '-hf') for the full manual, prose and all -- the
same option tables above, plus every section explaining them.
";

/// Section headers (the text before ':') worth keeping in the short help:
/// pure option tables, no prose paragraphs mixed in. Order here doesn't
/// matter -- sections are emitted in the order `HELP` already has them.
const SHORT_HELP_SECTIONS: &[&str] = &[
    "ADD OPTIONS",
    "REMOVE OPTIONS",
    "DIFF OPTIONS",
    "COMMITS OPTIONS",
    "SYNC OPTIONS",
    "MERGE WORDS",
    "MERGE OPTIONS",
];

/// A section's header line has no leading indentation; every other line in
/// `HELP` (body text, blanks) does. That is the one signal used to tell
/// them apart -- no regex, no line-number bookkeeping to keep in sync by
/// hand as sections are added or reordered.
fn short_help() -> String {
    let mut out = String::from(SHORT_USAGE);
    out.push('\n');
    let mut keep = false;
    for line in HELP.lines() {
        let is_header = !line.is_empty() && !line.starts_with(' ');
        if is_header {
            let name = line.split(':').next().unwrap_or("").trim();
            keep = SHORT_HELP_SECTIONS.contains(&name);
        }
        if keep {
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push_str(SHORT_FOOTER);
    out
}

const HELP: &str = "\
git-wt — worktrees in sibling directories named <repo>-<branch>

USAGE:
    git-wt                       List worktrees, numbered from 1
    git-wt list [SEARCH] [--col ...] [--long|--short] [--show-path] [--files]
                                 List, optional fuzzy filter; --col picks/orders
                                 columns (1=id, 2=branch, 3=dir, 4=status,
                                 5=last-commit, 6=merged, 7=merged-ref, 8=merged-at,
                                 9=push, 10=pull). Push/pull are the commits ahead of
                                 and behind the branch's upstream, as of the last fetch.
                                 --show-path (-p) adds the dir column, which a terminal
                                 leaves out; --long shows id/branch/dir/status/last/push/pull; --short
                                 id+branch+status summary. --files (-f) lists each
                                 worktree's uncommitted files under its row.
    git-wt <N>                   == git-wt <N> switch
    git-wt <N> switch            cd into worktree N (alias: cd)
    git-wt <N> path              Print worktree N's path only (alias: show)
    git-wt <N> remove [-y] [-f]  Remove worktree N
    git-wt <N>,<M> merge         Merge M into N
    git-wt <N> merge <BRANCH>    Merge BRANCH into worktree N
    git-wt <N>,<M> merge review  What would that merge bring over?
    git-wt <N> merge continue|abort
    git-wt <N>,<M> merged        Is M's branch already in N's branch?
    git-wt <N> merged <BRANCH>   Is BRANCH already in worktree N's branch?
    git-wt <N> merged            Is N's branch already in the current branch?
    git-wt <N> merged --others, --ot
                                 List all worktrees; show which are merged into N
    git-wt <N>,<M> diff [flags]  Diff worktree N against worktree M
    git-wt <N>,<M>[,...] commits Table: which commit is on which branch
    git-wt <N> commits           Same, N against the worktree you are in
    git-wt <N>,<N>[,<N>] meld    Diff 2-3 worktrees side by side in meld
    git-wt -b/--branch LIST <action>
                                 LIST with the current worktree prepended:
                                 '-b 1,2 commits' == '<cur>,1,2 commits'
    git-wt <N> fetch|pull|push   Run it in worktree N
    git-wt <N>,<M> pull          Run it in each worktree listed
    git-wt fetch|pull|push --all Run it in every worktree
    git-wt add [BRANCH] [flags]  Create a worktree (picker when BRANCH omitted)
    git-wt version
    git-wt --help
    git-wt --help -f             Full manual, this (alias: --full, -hf)

    Aliases: ls = list, rm = remove, cd = switch, show = path.

    Anywhere <N> or a <N>,<M> list appears above, a worktree may be named by
    the branch it holds instead of its number, and the two spellings mix:
    'git-wt main commits', 'git-wt main,2 diff', 'git-wt main,feat/x merge'.
    A bare number is always the worktree number, and a verb always wins over
    a branch of the same name; 'heads/main' reaches the branch either way.

GRAMMAR:
    git-wt [TARGET] [VERB] [FLAGS...]

    TARGET is one of:
        <N>               A worktree number, 1-based, from 'git-wt list'
        <BRANCH>          A branch name -- must be checked out in some
                          worktree; 'heads/<name>' forces branch over number
        <N>,<M>[,...]     Comma list, no spaces; numbers and branches mix
                          freely: '1,main,3'
        (omitted)         Defaults to 'list', or 'fetch|pull|push --all'
    -b/--branch LIST is not a target itself; it prepends the current
    worktree to whatever list the command already has:
        git-wt -b 1,2 commits   == git-wt <cur>,1,2 commits
        git-wt --branch=main diff  == git-wt <cur>,main diff

    VERB count per command depends on the target list's length:
        one target   -> switch, path, remove, commits, fetch/pull/push,
                         merge <BRANCH>, merged [BRANCH]
        two targets   -> merge, merged, diff, meld (also 3 for meld)
        any length    -> commits, meld (2-3), pull (each in turn)
    Some verbs also take a bare word before their own flags, matched ahead
    of a branch of the same name ('merge continue', 'merged --others'):
    spell it 'heads/continue' on the rare branch actually called that.

    FLAGS combine freely after the verb, in any order, short or long:
        git-wt 1,2 commits --author nino --all-files --filename api.php -n 5
        git-wt 1,2 commits -af          # bundle: -a (--all) + -f (--files)
        git-wt 1,2 commits -fn 20       # bundle ending in a value-flag
    Bundling (see -af above) only applies to single-dash letters in
    'commits'; a bundle stops at the first flag that takes a value, which
    then must be last in the bundle. Double-dash long forms and their
    short aliases (--au, --cs, --nf, --rb, ...) are never bundled -- each
    is typed on its own, exactly like the long spelling it stands in for.
    '-h'/'--help' and '-f'/'--full' bundle as the one exception: '-hf'
    is 'git-wt --help -f' spelled as a single token.

ADD OPTIONS:
    -n, --name NAME       Suffix only -> leaf = <repo>-NAME
        --dirname DIR     Whole leaf, verbatim (sanitized); with '/' = a path
    -p, --parentdir DIR   Parent dir (default: primary worktree's parent)
        --from REF        Base ref for a NEW branch
                          (default: the branch of the worktree you run from)
        --stay            wrapper: do NOT cd into the new worktree

REMOVE OPTIONS:
    -y                    Skip the confirmation prompt
    -f, --force           Discard uncommitted/untracked changes

DIFF OPTIONS:
    live                  Compare the files on disk, not the commits
    hunks                 Print each file's changed line numbers
    ...                   Range: only what M added since it forked from N (default)
    ..                    Range: everything that differs between the two tips
        --name-only       File names only
        --name-status     File names with A/M/D
        --stat            File names with a churn summary
    -- PATH...            Limit to these paths

DIFF:
    Diffs the two worktrees' committed state (their branches), through git's
    own pager, so uncommitted work does not show up; diff warns when either
    side is dirty and points at 'live'.

        git-wt 1,2 diff              -> git diff <branch 1>...<branch 2>
        git-wt 1,2 diff ..           -> git diff <branch 1>..<branch 2>
        git-wt 1,2 diff --stat
        git-wt 1,2 diff -- src/

    The default range is '...', so '1,2 diff' shows exactly what '1,2 merge'
    would bring in: M's own commits since the fork, and nothing of N's. '..'
    compares the two tips instead, which also reports N's commits, inverted,
    as if M had removed them.

    Any other git flag is an error, not a passthrough: run git yourself,
    'git diff <A>...<B> <flag>'. The error prints that command for you.

DIFF LIVE:
    'live' compares the literal bytes in the two directories, so uncommitted
    work shows up -- including the case no ref diff can ever answer, two
    worktrees sitting on the same commit. Only paths git would list are
    considered, so .gitignore is honored and build output stays out.

        git-wt 1,2 diff live         # literal files on disk
        git-wt 1,2 diff live hunks   # + changed line numbers
        git-wt 1,2 diff --live       # dashes optional, same thing

    'live' takes no range: '..'/'...' compare commits, which is the opposite
    question. --name-only/--name-status/--stat/-- PATH... all still apply.
    'hunks' works without 'live' too; its line numbers are the '+' side (M).

COMMITS OPTIONS:
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
        --date-human, --dh 'Jan. 31, 2026' instead of '2026-01-31'
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
        --filename, --fn TERM
                          Only commits touching a path containing TERM;
                          implies --files, and cuts the block to the matches
        --all-files, --af With --filename, show every file each commit
                          touched, not only the matched paths

COMMITS:
    A merge-request-style view of the first worktree, counter-checked
    against the rest. The default rows are the slice of the first branch
    that the other branches are missing -- from the oldest missing commit
    up to the first branch's tip -- so the table reads like a set of MRs
    opened against worktree 1. Add --all to see the first branch's whole
    log, or --union to see every listed branch's commits.

        git-wt 1,2,3 commits         # branch 1's range the others miss
        git-wt 1,2,3 commits --all   # 1's full log, checked against 2 and 3
        git-wt 2 commits             # worktree 2's own log, no comparison
        git-wt 1,2 commits -n 20     # newest 20 rows of the range
        git-wt 1,2,3 commits --union # every branch's commits as rows
        git-wt 1,2 commits --merges  # add the merge commits back
        git-wt 1,2 commits -af       # short flags bundle: == --all --files
        git-wt 1,2 commits -fn 20    # a value-taking flag ends the bundle

    The first worktree is the target: 'git-wt 1,2,3 commits' asks what 1
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

    A single target is that worktree alone: 'git-wt 2 commits' is 2's own
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

COMMITS FILTERS:
    Filters narrow the rows; the columns stay whatever the worktree list
    named. They AND together, and -n counts what survives them.

        git-wt 1,2 commits --author nino
        git-wt 1,2 commits --date 2026-01-31        # exactly that day
        git-wt 1,2 commits --date-since 2026-01-01 --date-until 2026-06-30
        git-wt 1,2 commits --commit-since 5568a21 --commit-until HEAD
        git-wt 1,2 commits --commits af48509,f9e2427

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
    same match 'git-wt list SEARCH' uses: 'nes' finds 'Nino Escalera'.

    Date bounds are whole days: '--date 2026-07-17' takes every commit
    of that day, 09:00 and 23:30 alike. The day is the author's own --
    a commit written at 23:30 +0800 belongs to the day it was there, not
    to yours -- so a bound never contradicts the printed column. Rows are
    still ordered by the full timestamp: same-day commits sort by time of
    day, even though the column only shows the day. '--time' prints
    that time, 24-hour, which is what tells a busy day's rows apart.

COMMITS MD:
    '--md' writes the table to a markdown file rather than the terminal.
    The file records the command that made it, so a report pasted into an
    issue says how to reproduce itself.

        git-wt 1,2 commits --md              -> commits_<date>_<time>.md
        git-wt 1,2 commits --md report.md    -> that path, overwritten
        git-wt 1,2 commits --merges --md  # filters apply as usual

    The default name is stamped to the second, so a re-run never eats the
    last report; a name you pass is yours, and is overwritten. The path is
    optional, so a flag may follow '--md' -- it is read as a flag, never
    as a filename.

    Subjects are whole in a file: there is no right edge to run out of, so
    nothing is truncated. A '|' in a subject is escaped rather than left
    to end the cell and shift the columns after it.

COMMITS DATES:
    The date column is ISO, the same shape the filters take, so a date
    read off the table pastes straight back into --date-since. It also
    sorts, greps, and is one width on every row.

        git-wt 1,2 commits                     -> 2026-01-31
        git-wt 1,2 commits --time              -> 2026-01-31 14:30:05
        git-wt 1,2 commits --date-human        -> Jan. 31, 2026
        git-wt 1,2 commits --date-human --time
                                               -> Jan. 31, 2026 14:30:05

    --date-human is easier to read a date out of, at the cost of the
    round-trip: it is not what --date-since accepts. What --date compares
    never changes shape whatever the column is spelled as.

    Quote --date, always. '>' and '<' are redirects, so an unquoted
    --date >=2026-01-01 writes a file called '=2026-01-01' and git-wt
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

        git-wt 1,2,3 commits --topo

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

        git-wt 1,2 commits --wrap 2      # two lines of subject
        git-wt 1,2 commits -w full       # whole subject, however many
        git-wt 1,2 commits --wrap        # the same 'full'

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

        git-wt 1,2 commits --subject-width 100   # 100 columns, edge or no edge
        git-wt 1,2 commits --subjw full          # never cut, however long
        git-wt 1,2 commits --subjw 60 --wrap 3   # 3 lines of 60

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

        git-wt 1,2 commits --branch-width 40   # allow longer names
        git-wt 1,2 commits --branchw full      # never cut, however long

    Unlike '--subject-width', this has nothing to do with the terminal --
    the branch column is fixed-width, so the cut is the same piped or not.

    '--files' adds the files a commit touched, indented under the subject.
    Each file shows a status letter (A/M/D/R/C) and the added/removed line
    count. A blank line separates the commit from its file block, and another
    separates the block from the next commit. The work is scoped to the rows
    the table already shows, so pair it with '-n' or filters on large logs.
    Merge commits show the diff against their first parent.

        git-wt 1,2 commits -n 10 --files
        git-wt 1,2 commits --author regoso --files

MARKS:
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

SYNC OPTIONS:        (fetch/pull/push; any other git flag is an error, not a passthrough)
    -a, --all             Every worktree, not the ones a list named
    fetch: -p, --prune | --tags | --no-tags, --nt | --force
    pull:  --rebase, --rb | --no-rebase, --nr | --ff-only | -p, --prune |
           --autostash, --as
    push:  -u, --set-upstream | --force-with-lease, --fl | --tags |
           -n, --dry-run

SYNC:
    fetch/pull/push run git in a worktree's own directory, so each one syncs
    its own branch against its own upstream. Nothing here is a shortcut for
    something git does not do -- it is the cd you would type first.

        git-wt 1 pull                # git -C <dir 1> pull
        git-wt 1,3 fetch --prune     # both, one after the other
        git-wt pull --all            # every worktree
        git-wt 2 push -u             # push and set the upstream

    '--all' is the whole point: a repo with six worktrees is six branches, and
    they go stale one at a time. It sweeps every worktree in 'list' order, and
    it names no target -- 'git-wt pull --all' is the one verb-first form left,
    because there is nothing to put in front of it.

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

MELD:
    Opens meld on the worktree directories, in the order you list them, and
    waits until you close it. Requires meld on PATH.

        git-wt 1,3 meld      -> meld <dir 1> <dir 3>
        git-wt 2,1,3 meld    -> meld <dir 2> <dir 1> <dir 3>  (3-way)

    With --diff, meld sees only the files that differ between the two refs,
    extracted into sparse temp directories. Add --3way or --base <ref> to
    include the merge-base as a third pane.

        git-wt 1,2 meld --diff            # only files that differ
        git-wt 1,2 meld --diff ...        # only what branch 2 added since fork
        git-wt 1,2 meld --diff --3way     # + merge-base in the middle pane
        git-wt 1,2 meld --diff --base main # + explicit base in the middle pane

MERGE WORDS:            (each takes an optional '--': 'abort' == '--abort')
    -c, continue          Conclude a conflicted merge
    -a, abort             Undo a conflicted merge
    -o, ours              On a conflicting hunk, keep worktree N's side
    -t, theirs            On a conflicting hunk, take the source's side
    -d, dry-run           Report whether it would merge; change nothing
        review            Show the commits it would bring over; change nothing

MERGE OPTIONS:
    -m, --message MSG     Merge commit message
        --no-ff, --nf     Always create a merge commit
        --ff-only, --fo   Refuse anything but a fast-forward
        --squash          Stage the merge without committing
    -f, --force           Merge even when worktree N has uncommitted changes

MERGE:
    The merge runs inside worktree N, so N's branch is the one that moves:

        git-wt 1,2 merge            # worktree 2's branch -> worktree 1's branch
        git-wt 1 merge feat/x       # a branch name works too
        git-wt 1,2 merge dry-run    # would it conflict? nothing is touched
        git-wt 1,2 merge theirs     # let 2 win every collision

    The list reads dest-first, so '1,2 merge' merges 2 into 1. It takes
    exactly two worktrees -- unlike meld, which diffs 2-3 -- because a
    merge has one destination and one source. The list already names the
    source, so it cannot be combined with 'continue'/'abort'; those take a
    single target, 'git-wt 1 merge continue' (or 'git-wt 1 merge abort').

    A number that names a worktree wins over a branch of the same name, and
    the words above win over a branch of the same name: to merge a branch
    called 'theirs', spell it 'heads/theirs'.

    On conflict, git-wt exits nonzero and lists the conflicted files; fix
    them in worktree N, then run 'git-wt N merge continue' (or abort).
    Merge commits never open an editor: without -m, git's default message is
    taken as-is.

MERGE REVIEW:
    'dry-run' answers whether a merge conflicts. '--review' answers what it
    would bring: the same verdict as a header, then the commit table for
    'dest..src'. It merges nothing and keeps dry-run's exit codes, 0 clean
    and 1 on conflict.

        git-wt 1,2 merge --review        # what would 2 bring into 1?
        git-wt 1,2 merge --review -f     # + the files under each commit
        git-wt 1,2 merge --review -n 5 --author nino

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

MERGED:
    Ask whether one branch is already contained in another.

        git-wt 1,2 merged             # is 2's branch in 1's branch?
        git-wt 1 merged feat/x        # is feat/x in worktree 1's branch?
        git-wt 1 merged               # is worktree 1's branch in the current branch?
        git-wt 1 merged --others      # list every worktree against worktree 1
        git-wt 1 merged --ot -p       # short alias for --others; -p adds the path column

    The normal forms answer yes/no, exiting 0 for \"already merged\" and nonzero
    for \"ahead\". The `--others` form prints a table with a `merged` column and
    a `merged-at` column showing when the source branch was last merged into the
    selected branch. `merged-at` is '-' for fast-forward merges and for branches
    that are not yet merged.

ADD:
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

STDOUT:
    Only 'switch'/'path' (bare <N>), 'add', and 'remove' print a path, alone,
    on stdout, so a shell can cd into it or capture it. Status goes to stderr.

        cd \"$(git-wt 1 path)\"
        dir=\"$(git-wt add feature/login)\"

COLOR:
    Color and status/last-commit columns turn on only when stdout is a
    terminal, so 'git-wt list | cat' stays plain and parseable. Honors
    NO_COLOR (disable) and CLICOLOR_FORCE (force on).
";

fn main() {
    let code = match run() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Meta / no-args first — these don't all need a repo, and no-args = list.
    match args.first().map(String::as_str) {
        None => {
            let root = repo_root()?;
            return list_from_args(&root, &[]);
        }
        // A leading list flag with no `list` word: `git-wt --col 1,2`.
        Some("--col") | Some("-c") | Some("--files") | Some("-f") => {
            let root = repo_root()?;
            return list_from_args(&root, &args);
        }
        Some(s) if s.starts_with("--col=") => {
            let root = repo_root()?;
            return list_from_args(&root, &args);
        }
        Some("-hf") => {
            print!("{HELP}");
            return Ok(());
        }
        Some("-h") | Some("--help") | Some("help") => {
            let full = args[1..].iter().any(|a| a == "-f" || a == "--full");
            print!("{}", if full { HELP.to_string() } else { short_help() });
            return Ok(());
        }
        Some("-V") | Some("--version") | Some("version") => {
            println!("git-wt {VERSION}");
            return Ok(());
        }
        _ => {}
    }

    let first = &args[0];

    if first == "list" || first == "ls" {
        let root = repo_root()?;
        return list_from_args(&root, &args[1..]);
    }

    if first == "add" {
        let root = repo_root()?;
        return cmd_add(&root, &args[1..]);
    }

    // `git-wt fetch --all` sweeps every worktree; bare `git-wt fetch` (no
    // target, no `--all`) stands for the current worktree, the same way bare
    // `commits` and `remove` do below.
    if let Some(op) = SyncOp::from_word(first) {
        let parsed = parse_sync_args(op, &args[1..])?;
        let root = repo_root()?;
        let trees = worktrees(&root)?;
        if parsed.all {
            let idxs: Vec<usize> = (0..trees.len()).collect();
            return cmd_sync(&trees, &idxs, &parsed);
        }
        let idx = current_worktree_index(&trees).ok_or_else(|| {
            format!("not inside a worktree; use 'git-wt <N> {first}'\n{ALL_HINT}")
        })?;
        return dispatch_target(&root, idx + 1, &args);
    }

    // `remove`/`rm` with no target — the worktree standing in for itself, the
    // same as bare `commits` below.
    if first == "remove" || first == "rm" {
        let root = repo_root()?;
        let trees = worktrees(&root)?;
        let idx = current_worktree_index(&trees)
            .ok_or_else(|| format!("not inside a worktree; use 'git-wt <N> {first}'"))?;
        return dispatch_target(&root, idx + 1, &args);
    }

    // `diff`/`meld`/`merged` with no leading target — same reading as bare
    // `commits -b`, but `-b` isn't optional: diff and meld always need a
    // pair, and target-first `merged` already owns the no-source meaning
    // ("is my branch merged into what I'm standing in"), so the bare verb
    // needs `-b` to say what it's being compared against.
    if first == "diff" || first == "meld" || first == "merged" {
        let root = repo_root()?;
        let trees = worktrees(&root)?;
        let cur = current_worktree_index(&trees)
            .ok_or_else(|| format!("not inside a worktree; use 'git-wt <N>,<M> {first}'"))?;
        let (rest, val) = extract_branch_flag(&args[1..])?;
        let val = val.ok_or_else(|| {
            format!(
                "'{first}' needs another worktree: 'git-wt {first} -b <N>' or \
                 'git-wt <N>,<M> {first}'"
            )
        })?;
        let mut ns: Vec<usize> = branch_targets(&trees, &val)?.iter().map(|i| i + 1).collect();
        ns.insert(0, cur + 1);
        let mut full_rest = vec![first.clone()];
        full_rest.extend(rest);
        return dispatch_targets(&root, &ns, &full_rest);
    }

    // `--branch/-b LIST` — the multi-target grammar with the current worktree
    // prepended, so it can join a comparison without its own number being
    // looked up first: 'git-wt -b 1,branch1' is 'git-wt <cur>,1,branch1'.
    if first == "--branch" || first == "-b" || first.starts_with("--branch=") {
        let (rest, val) = extract_branch_flag(&args)?;
        let val = val.expect("matched above");
        let root = repo_root()?;
        let trees = worktrees(&root)?;
        let cur = current_worktree_index(&trees)
            .ok_or("not inside a worktree; can't resolve --branch's current worktree")?;
        let mut ns: Vec<usize> = branch_targets(&trees, &val)?.iter().map(|i| i + 1).collect();
        ns.insert(0, cur + 1);
        return dispatch_targets(&root, &ns, &rest);
    }

    // `commits` with no target — the worktree standing in for itself, so a
    // solo log doesn't require typing its own number back at it. `-b <N>`
    // still adds a comparison target, the same as the leading-`-b` form:
    // 'git-wt commits -b 2' is 'git-wt <cur>,2 commits'.
    if first == "commits" {
        let root = repo_root()?;
        let trees = worktrees(&root)?;
        let cur = current_worktree_index(&trees)
            .ok_or("not inside a worktree; use 'git-wt <N> commits'")?;
        let (rest, val) = extract_branch_flag(&args[1..])?;
        if let Some(val) = val {
            let mut ns: Vec<usize> = branch_targets(&trees, &val)?.iter().map(|i| i + 1).collect();
            ns.insert(0, cur + 1);
            let mut full_rest = vec![first.clone()];
            full_rest.extend(rest);
            return dispatch_targets(&root, &ns, &full_rest);
        }
        return dispatch_target(&root, cur + 1, &args);
    }

    // <N> <action> — the target-first grammar.
    if let Ok(n) = first.parse::<usize>() {
        let root = repo_root()?;
        return dispatch_target(&root, n, &args[1..]);
    }

    // <N>,<N>[,<N>] <action> — the multi-target grammar (meld). Parts may also
    // be branch names; they are resolved to numbers here so the grammar below
    // only ever sees numbers.
    if let Some(parts) = parse_target_list(first)? {
        let root = repo_root()?;
        let trees = worktrees(&root)?;
        let ns = resolve_target_list(&trees, &parts)?;
        return dispatch_targets(&root, &ns, &args[1..]);
    }

    if first.starts_with('-') {
        return Err(format!("unknown option '{first}'\nTry 'git-wt --help'"));
    }

    // <BRANCH> <action> — the same grammar, with the worktree named by the
    // branch it holds instead of its number. Last, so every verb outranks it: a
    // branch called `list` is reachable only as `heads/list`. A failure to find
    // the repo is swallowed rather than reported, because outside one the honest
    // answer is still "unknown command", which is what falls through below.
    if let Ok(root) = repo_root() {
        if let Ok(trees) = worktrees(&root) {
            if let Some(n) = resolve_target(&trees, first) {
                return dispatch_target(&root, n, &args[1..]);
            }
        }
    }

    Err(unknown_command_msg(first))
}

