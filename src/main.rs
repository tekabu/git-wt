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

use crate::cli::*;
use crate::cmd::*;
use crate::worktree::*;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
git-wt — worktrees in sibling directories named <repo>-<branch>

USAGE:
    git-wt                       List worktrees, numbered from 1
    git-wt list [SEARCH] [--col ...] [--long|--short] [--show-path]
                                 List, optional fuzzy filter; --col picks/orders
                                 columns (1=id, 2=branch, 3=dir, 4=status,
                                 5=last-commit, 6=merged, 7=merged-ref, 8=merged-at,
                                 9=push, 10=pull). Push/pull are the commits ahead of
                                 and behind the branch's upstream, as of the last fetch.
                                 --show-path (-p) adds the dir column, which a terminal
                                 leaves out; --long shows id/branch/dir/status/last/push/pull; --short
                                 id+branch+status summary.
    git-wt <N>                   == git-wt <N> switch
    git-wt <N> switch            cd into worktree N (alias: cd)
    git-wt <N> path              Print worktree N's path only (alias: show)
    git-wt <N> remove [-y] [-f]  Remove worktree N
    git-wt <N>,<M> merge         Merge M into N
    git-wt <N> merge <BRANCH>    Merge BRANCH into worktree N
    git-wt <N> merge continue|abort
    git-wt <N>,<M> merged        Is M's branch already in N's branch?
    git-wt <N> merged <BRANCH>   Is BRANCH already in worktree N's branch?
    git-wt <N> merged            Is N's branch already in the current branch?
    git-wt <N> merged --others   List all worktrees; show which are merged into N
    git-wt <N>,<M> diff [flags]  Diff worktree N against worktree M
    git-wt <N>,<M>[,...] commits Table: which commit is on which branch
    git-wt <N> commits           Same, N against the worktree you are in
    git-wt <N>,<N>[,<N>] meld    Diff 2-3 worktrees side by side in meld
    git-wt <N> fetch|pull|push   Run it in worktree N
    git-wt <N>,<M> pull          Run it in each worktree listed
    git-wt fetch|pull|push --all Run it in every worktree
    git-wt add [BRANCH] [flags]  Create a worktree (picker when BRANCH omitted)
    git-wt version
    git-wt --help

    Aliases: ls = list, rm = remove, cd = switch, show = path.

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
    -n, --limit N         Show at most N commits (newest first)
        --all             Full log of the first worktree (default is the
                          range the other worktrees are missing)
        --union           Rows from every worktree listed, not just the
                          first one's range (alias: --any)
        --no-merges       Drop merge commits; keep the work they joined
        --no-cherry       Skip the patch comparison behind '≈' (faster)
        --pick-id         Add a 'pick' column: the sha the '≈' copy of the
                          commit carries elsewhere
        --files           Add the changed files under each commit, with
                          status and +/- line counts
        --topo            Group each branch's commits, don't interleave
        --reverse         Newest last (alias: --oldest-first)
    -w, --wrap [N]        Let a long subject take N terminal lines, not
                          one; 'full' or a bare --wrap never cuts it
        --subject-width N Give the subject N columns rather than what the
                          terminal left it; 'full' never cuts (alias:
                          --subjw)
        --md [FILE]       Write a markdown table instead of printing one
                          (default: commits_<date>_<time>.md in the cwd)
        --show-time       Add the time to the date column, 24-hour
        --date-human      'Jan. 31, 2026' instead of '2026-01-31'
        --author NAME     Only NAME's commits (fuzzy, like list's SEARCH)
    -d, --date CMP        Only commits on a date: '=', '>=' or '<=' a
                          YYYY-MM-DD, e.g. --date '>=2026-01-01'. Repeat
                          for a range. QUOTE IT: '>' is a shell redirect
        --from-date DATE  Same as --date '>=DATE', no quoting needed
        --to-date DATE    Same as --date '<=DATE', no quoting needed
        --from-id COMMIT  Only COMMIT and what came after it
        --to-id COMMIT    Only COMMIT and what it can reach

COMMITS:
    A merge-request-style view of the first worktree, counter-checked
    against the rest. The default rows are the slice of the first branch
    that the other branches are missing -- from the oldest missing commit
    up to the first branch's tip -- so the table reads like a set of MRs
    opened against worktree 1. Add --all to see the first branch's whole
    log, or --union to see every listed branch's commits.

        git-wt 1,2,3 commits         # branch 1's range the others miss
        git-wt 1,2,3 commits --all   # 1's full log, checked against 2 and 3
        git-wt 2 commits             # worktree 2 vs the one you stand in
        git-wt 1,2 commits -n 20     # newest 20 rows of the range
        git-wt 1,2,3 commits --union # every branch's commits as rows
        git-wt 1,2 commits --no-merges   # only the commits someone wrote

    The first worktree is the target: 'git-wt 1,2,3 commits' asks what 1
    has that 2 and 3 do not. The range is computed from those missing
    commits, so rows can include shared history if another branch diverged
    earlier and branch 1 has kept committing since.

    '--union' asks the other question -- 'who is out of sync with who' --
    and every worktree listed contributes rows: the table becomes the union
    of their full logs, and a commit missing from the first one gets a row
    with a '·' under it.

    '-n' caps the rows after the range is chosen; filters apply the same
    way. '--no-merges' drops merge commits: they carry no work of their own,
    and on a branch that merges often they are most of the table. The
    commits a merge joined all stay -- only the merge's own row goes,
    and the marks are untouched either way.

    A single target reads the way 'merged' does: the worktree you are in
    is the other column, so 'git-wt 2 commits' == 'git-wt <here>,2
    commits'. Standing in the one you named is an error, not a column of
    guaranteed checks.

    Any number of worktrees can be columns -- there is no cap, unlike
    diff's two or meld's three. The terminal is the real limit: each
    column costs its branch name plus two, and once the row no longer
    fits, the subject wraps. The marks never do: they are left of it.

COMMITS FILTERS:
    Filters narrow the rows; the columns stay whatever the worktree list
    named. They AND together, and -n counts what survives them.

        git-wt 1,2 commits --author nino
        git-wt 1,2 commits --date '>=2026-01-01' --date '<=2026-06-30'
        git-wt 1,2 commits --from-date 2026-01-01 --to-date 2026-06-30
        git-wt 1,2 commits --from-id 5568a21 --to-id HEAD

    Two vocabularies, one shape: '-id' bounds take a commit -- a sha, a
    branch, a tag, 'HEAD~3' -- and '-date' bounds take a YYYY-MM-DD.
    Both ends include what they name: '--from-id X' lists X itself, and
    '--from-date 2026-01-01' takes that whole day. So there is no '>' or
    '<': the day either side of a bound is the inclusive one next door.

    --date compares the date the table prints, which is the AUTHOR date;
    git's own --since/--until read committer dates and would disagree
    with the column. --author is a fuzzy subsequence, case-folded, the
    same match 'git-wt list SEARCH' uses: 'nes' finds 'Nino Escalera'.

    Date bounds are whole days: '--date =2026-07-17' takes every commit
    of that day, 09:00 and 23:30 alike. The day is the author's own --
    a commit written at 23:30 +0800 belongs to the day it was there, not
    to yours -- so a bound never contradicts the printed column. Rows are
    still ordered by the full timestamp: same-day commits sort by time of
    day, even though the column only shows the day. '--show-time' prints
    that time, 24-hour, which is what tells a busy day's rows apart.

COMMITS MD:
    '--md' writes the table to a markdown file rather than the terminal.
    The file records the command that made it, so a report pasted into an
    issue says how to reproduce itself.

        git-wt 1,2 commits --md              -> commits_<date>_<time>.md
        git-wt 1,2 commits --md report.md    -> that path, overwritten
        git-wt 1,2 commits --no-merges --md  # filters apply as usual

    The default name is stamped to the second, so a re-run never eats the
    last report; a name you pass is yours, and is overwritten. The path is
    optional, so a flag may follow '--md' -- it is read as a flag, never
    as a filename.

    Subjects are whole in a file: there is no right edge to run out of, so
    nothing is truncated. A '|' in a subject is escaped rather than left
    to end the cell and shift the columns after it.

COMMITS DATES:
    The date column is ISO, the same shape the filters take, so a date
    read off the table pastes straight back into --from-date. It also
    sorts, greps, and is one width on every row.

        git-wt 1,2 commits                     -> 2026-01-31
        git-wt 1,2 commits --show-time         -> 2026-01-31 14:30:05
        git-wt 1,2 commits --date-human        -> Jan. 31, 2026
        git-wt 1,2 commits --date-human --show-time
                                               -> Jan. 31, 2026 14:30:05

    --date-human is easier to read a date out of, at the cost of the
    round-trip: it is not what --from-date accepts. What --date compares
    never changes shape whatever the column is spelled as.

    Quote --date, always. '>' and '<' are redirects, so an unquoted
    --date >=2026-01-01 writes a file called '=2026-01-01' and git-wt
    sees no date at all. --from-date/--to-date need no quoting.

    Rows are ancestry-first: no parent is ever listed above its child, so
    reading down the table is reading the real history. Dates only order
    commits that do not descend from each other -- which is why a commit
    authored before its own parent (a rebase, a cherry-pick, a bad clock)
    reads as out of order against the date column. The story is right; the
    clock is not.

    Within that, two readings. By default the rows are newest-first, so a
    row's neighbors are its contemporaries -- what happened when. '--topo'
    keeps each line of history in one block instead -- what each branch did,
    which is what --union tables are usually read for. Neither depends on --show-time:
    the order always reads the full timestamp, and --show-time only prints
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
    ·   the branch has neither

    '≈' is a cherry-pick or a rebase's copy. To git those are different
    commits, so a bare '✓/·' calls them missing -- which reads as work to
    do, when the work is done. The comparison is git's own 'git cherry':
    patch-ids, not history, per pair of branches. '--no-cherry' skips it
    and takes the old, cheaper answer, for a repo whose branches have
    diverged by thousands of commits.

    A picked commit shows twice, once per sha: the original row is '≈' in
    the branch that took it, the copy's row is '≈' in the branch it came
    from. Both are true -- they are two commits carrying one patch.

    '--pick-id' names the other sha: a 'pick' column after 'commit', holding
    the sha the same patch was committed under elsewhere. It is the row's
    other half -- the sha to hand 'git show', or to check a pick landed
    where you meant it to. Rows with no copy leave it blank, and a patch
    carried under three shas names the first of the others.

SYNC OPTIONS:        (fetch/pull/push; any other git flag is an error, not a passthrough)
    -a, --all             Every worktree, not the ones a list named
    fetch: -p, --prune | --tags | --no-tags | --force
    pull:  --rebase | --no-rebase | --ff-only | -p, --prune | --autostash
    push:  -u, --set-upstream | --force-with-lease | --tags | -n, --dry-run

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

MERGE OPTIONS:
    -m, --message MSG     Merge commit message
        --no-ff           Always create a merge commit
        --ff-only         Refuse anything but a fast-forward
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
        git-wt 1 merged --others -p   # include the path column

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
        Some("--col") | Some("-c") => {
            let root = repo_root()?;
            return list_from_args(&root, &args);
        }
        Some(s) if s.starts_with("--col=") => {
            let root = repo_root()?;
            return list_from_args(&root, &args);
        }
        Some("-h") | Some("--help") | Some("help") => {
            print!("{HELP}");
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

    // `git-wt fetch --all` — the one verb-first form, because the sweep names no
    // target at all. Without `--all` it is the target that is missing, not the
    // verb, so say that rather than "unknown command".
    if let Some(op) = SyncOp::from_word(first) {
        let args = parse_sync_args(op, &args[1..])?;
        if !args.all {
            return Err(format!(
                "'{first}' needs a worktree: 'git-wt <N> {first}'\n{ALL_HINT}"
            ));
        }
        let root = repo_root()?;
        let trees = worktrees(&root)?;
        let idxs: Vec<usize> = (0..trees.len()).collect();
        return cmd_sync(&trees, &idxs, &args);
    }

    // <N> <action> — the target-first grammar.
    if let Ok(n) = first.parse::<usize>() {
        let root = repo_root()?;
        return dispatch_target(&root, n, &args[1..]);
    }

    // <N>,<N>[,<N>] <action> — the multi-target grammar (meld).
    if let Some(ns) = parse_target_list(first)? {
        let root = repo_root()?;
        return dispatch_targets(&root, &ns, &args[1..]);
    }

    if first.starts_with('-') {
        return Err(format!("unknown option '{first}'\nTry 'git-wt --help'"));
    }

    Err(unknown_command_msg(first))
}

