//! git-wt — create and manage git worktrees in sibling directories named
//! `<repo-folder>-<sanitized-branch>`.
//!
//! Installed on PATH as `git-wt`, so it is also reachable as `git wt`.
//!
//! Grammar is target-first for existing worktrees (`git-wt <N> <action>`) with
//! an explicit `add` verb for creation.

use std::collections::{HashMap, HashSet};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
git-wt — worktrees in sibling directories named <repo>-<branch>

USAGE:
    git-wt                       List worktrees, numbered from 1
    git-wt list [SEARCH] [--col ...] [--long|--short] [--show-path]
                                 List, optional fuzzy filter; --col picks/orders
                                 columns (1=id, 2=branch, 3=dir, 4=status,
                                 5=last-commit, 6=merged, 7=merged-ref, 8=merged-at).
                                 --show-path (-p) adds the dir column, which a terminal
                                 leaves out; --long shows id/branch/dir/status/last; --short
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

/// A worktree as reported by `git worktree list --porcelain`.
struct Worktree {
    path: PathBuf,
    /// Short branch name, or None when detached/bare.
    branch: Option<String>,
    detached: bool,
    bare: bool,
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

/// Message for a leading word that is neither a number nor a known verb.
/// Legacy verb-first forms get a migration hint; branch-like words get an
/// `add` suggestion.
fn unknown_command_msg(tok: &str) -> String {
    match tok {
        "show" => "unknown command 'show'; use 'git-wt 1 path'".into(),
        "remove" | "rm" => format!("unknown command '{tok}'; use 'git-wt 1 remove'"),
        "merge" => "unknown command 'merge'; use 'git-wt 1,2 merge'".into(),
        "merged" => "unknown command 'merged'; use 'git-wt 1 merged' or 'git-wt 1,2 merged'".into(),
        "commits" => "unknown command 'commits'; use 'git-wt 1,2 commits'".into(),
        _ if branch_like(tok) => format!("unknown command '{tok}'; did you mean 'add {tok}'?"),
        _ => format!("unknown command '{tok}'"),
    }
}

/// A word looks like a branch when it has a `/` or `-` and no whitespace.
fn branch_like(s: &str) -> bool {
    !s.chars().any(char::is_whitespace) && (s.contains('/') || s.contains('-'))
}

// ---------------------------------------------------------------------------
// Target dispatch: git-wt <N> [action]
// ---------------------------------------------------------------------------

fn dispatch_target(root: &Path, n: usize, rest: &[String]) -> Result<(), String> {
    let trees = worktrees(root)?;
    let idx = check_index(n, trees.len())?;

    let action = rest.first().map(String::as_str).unwrap_or("switch");
    match action {
        "switch" | "cd" | "path" | "show" => {
            if rest.len() > 1 {
                return Err("too many arguments\nTry 'git-wt --help'".into());
            }
            // The branch is status, so it goes to stderr; the path is the
            // stdout contract (`cd "$(git-wt 1 path)"` stays clean).
            eprintln!("{}", label(&trees[idx]));
            println!("{}", trees[idx].path.display());
            Ok(())
        }
        "remove" | "rm" => {
            let mut yes = false;
            let mut force = false;
            for a in &rest[1..] {
                match a.as_str() {
                    "-y" => yes = true,
                    "-f" | "--force" => force = true,
                    other => {
                        return Err(format!("unexpected argument '{other}' for remove"));
                    }
                }
            }
            cmd_remove(root, &trees, idx, yes, force)
        }
        // `1 diff 2` was the old grammar; point at the list form meld already
        // uses. `merge` keeps the single-target form for branch sources and for
        // `continue`/`abort`; only a worktree-number source now uses the list.
        // A source equal to the destination is left to `cmd_merge`, which gives
        // the clearer "already checked out" error and preserves the documented
        // worktree-wins rule for digit branch names.
        "diff" => Err(format!(
            "diff takes a worktree list: 'git-wt {n},<M> diff'"
        )),
        // The same reading 'merged' gives a single target: N against the
        // worktree you are standing in. A one-column table would just be
        // 'git log', so the second column is the one you are already in.
        "commits" => {
            let Some(here) = here_index(&trees) else {
                return Err(format!(
                    "not inside a worktree, so there is no second branch to compare \
                     against\nhint: 'git-wt {n},<M> commits' names both"
                ));
            };
            if here == idx {
                return Err(format!(
                    "worktree #{n} is the one you are standing in, so the table would \
                     compare it with itself\nhint: 'git-wt {n},<M> commits' names both"
                ));
            }
            cmd_commits(root, &trees, &[here, idx], &rest[1..])
        }
        "merge" => {
            let args = parse_merge_args(&rest[1..])?;
            if let MergeOp::Start(src) = &args.op {
                if let Ok(m) = src.parse::<usize>() {
                    if m != n && (1..=trees.len()).contains(&m) {
                        return Err(format!(
                            "merge takes a worktree list: 'git-wt {n},{m} merge' \
                             (or use 'heads/{m}' for a branch of the same name)"
                        ));
                    }
                }
            }
            cmd_merge(root, &trees, idx, &args)
        }
        "merged" => {
            let args = &rest[1..];
            // `--others` asks for a table, not a yes/no answer.
            if args.iter().any(|a| a == "--others") {
                let show_path = show_path_from_rest(args);
                let extra: Vec<&str> = args
                    .iter()
                    .map(String::as_str)
                    .filter(|a| *a != "--others" && *a != "-p" && *a != "--show-path")
                    .collect();
                if !extra.is_empty() {
                    return Err(format!(
                        "--others takes no arguments (got '{}')\nTry 'git-wt --help'",
                        extra.join("', '")
                    ));
                }
                return cmd_merged_others(root, &trees, idx, show_path);
            }
            if args.len() > 1 {
                return Err("too many arguments\nTry 'git-wt --help'".into());
            }
            // A worktree-number source uses the list form, as merge and diff do;
            // the single form stays for a branch source, which a list of numbers
            // cannot name. A source equal to the destination falls through to the
            // self-check below for its clearer "already checked out" error.
            if let Some(src) = args.first() {
                if let Ok(m) = src.parse::<usize>() {
                    if m != n && (1..=trees.len()).contains(&m) {
                        return Err(format!(
                            "merged takes a worktree list: 'git-wt {n},{m} merged' \
                             (or use 'heads/{m}' for a branch of the same name)"
                        ));
                    }
                }
            }
            let has_explicit_source = !args.is_empty();
            let src = if has_explicit_source {
                // "git-wt N merged BRANCH" reads dest-first, like merge.
                resolve_merge_source(root, &trees, &args[0])?
            } else {
                // "git-wt N merged" asks whether N's branch is already in the
                // branch we are standing in now.
                ref_of(&trees[idx])?
            };
            let dest = if has_explicit_source {
                ref_of(&trees[idx])?
            } else {
                current_ref()
            };
            // Reject the explicit self-check (1 merged 1) the same way merge
            // does; the bare form (1 merged) intentionally asks about itself.
            if has_explicit_source && src == dest {
                return Err(format!("'{src}' is already checked out in worktree {}", idx + 1));
            }
            cmd_merged(root, &src, &dest)
        }
        "fetch" | "pull" | "push" => {
            let op = SyncOp::from_word(action).expect("matched above");
            let args = parse_sync_args(op, &rest[1..])?;
            // `--all` is every worktree, so a target contradicts it; the target
            // is the more specific thing said, so name the form that keeps it.
            if args.all {
                return Err(format!(
                    "'--all' is every worktree, so worktree #{n} has nothing to add\n\
                     hint: 'git-wt {action} --all', or 'git-wt {n} {action}' for just this one"
                ));
            }
            cmd_sync(&trees, &[idx], &args)
        }
        // A single target can't be melded, but say so in meld's own terms.
        "meld" => cmd_meld(&trees, &[idx]),
        // An option in the action slot is never right, whatever the option is:
        // each action carries its own, after its own verb.
        other if other.starts_with('-') => Err(format!(
            "'{other}' is an option, not an action; options follow the action, \
             e.g. 'git-wt {n} remove -f' or 'git-wt {n},2 diff --stat'"
        )),
        other => Err(format!(
            "unknown action '{other}' (switch, path, remove, diff, commits, merge, meld, \
             merged, fetch, pull, push)"
        )),
    }
}

/// Recognize a comma-separated target list like `1,2,3`. Returns Ok(None) when
/// the token is not one at all (so the caller keeps looking), and an error when
/// it clearly meant to be one but is malformed (`1,,2`, `1,x`).
fn parse_target_list(tok: &str) -> Result<Option<Vec<usize>>, String> {
    if !tok.contains(',') {
        return Ok(None);
    }
    let mut out = Vec::new();
    for part in tok.split(',') {
        let n: usize = part
            .parse()
            .map_err(|_| format!("bad worktree list '{tok}'; want numbers, e.g. '1,2'"))?;
        out.push(n);
    }
    Ok(Some(out))
}

fn dispatch_targets(root: &Path, ns: &[usize], rest: &[String]) -> Result<(), String> {
    let trees = worktrees(root)?;
    let mut idxs = Vec::new();
    for &n in ns {
        idxs.push(check_index(n, trees.len())?);
    }

    match rest.first().map(String::as_str) {
        Some("meld") => {
            if rest.len() > 1 {
                return Err("meld takes no options\nTry 'git-wt --help'".into());
            }
            cmd_meld(&trees, &idxs)
        }
        Some("diff") => cmd_diff(root, &trees, &idxs, &rest[1..]),
        Some("commits") => cmd_commits(root, &trees, &idxs, &rest[1..]),
        // `1,2 merge`: the list reads dest-first, so 2 merges into 1.
        Some("merge") => {
            // The list already names the source, so a resume word contradicts
            // it: there is nothing for `continue` to take a source from.
            // Check this before the count so an over-long list with `continue`
            // gets the more useful resume-word message.
            if let Some(word) = rest[1..].iter().find_map(|a| resume_word(a)) {
                return Err(format!(
                    "'{word}' takes no source, so a worktree list has nothing to name\n\
                     hint: 'git-wt {n} merge {word}'",
                    n = ns[0]
                ));
            }
            if idxs.len() != 2 {
                return Err(format!(
                    "merge takes exactly two worktrees, not {}: 'git-wt <N>,<M> merge' \
                     merges M into N",
                    idxs.len()
                ));
            }
            // Hand the source to the single-target parser as the positional it
            // already understands, so both spellings share one code path.
            let mut argv = vec![ns[1].to_string()];
            argv.extend_from_slice(&rest[1..]);
            let args = parse_merge_args(&argv)?;
            cmd_merge(root, &trees, idxs[0], &args)
        }
        // `1,2 merged` == "is 2 already in 1?" — same dest-first reading as merge.
        Some("merged") => {
            if idxs.len() != 2 {
                return Err(format!(
                    "merged takes exactly two worktrees, not {}: 'git-wt <N>,<M> merged' \
                     asks whether M is already in N",
                    idxs.len()
                ));
            }
            if rest.len() > 1 {
                return Err("merged takes no arguments\nTry 'git-wt --help'".into());
            }
            if idxs[0] == idxs[1] {
                return Err(format!("worktree #{} listed twice", idxs[0] + 1));
            }
            let dest = ref_of(&trees[idxs[0]])?;
            let src = ref_of(&trees[idxs[1]])?;
            cmd_merged(root, &src, &dest)
        }
        // `1,3 pull`: the sweep, narrowed to the worktrees you named.
        Some(w) if SyncOp::from_word(w).is_some() => {
            let op = SyncOp::from_word(w).expect("matched above");
            let args = parse_sync_args(op, &rest[1..])?;
            if args.all {
                return Err(format!(
                    "'--all' is every worktree, so a worktree list has nothing to add\n\
                     hint: 'git-wt {w} --all', or drop it to sweep just the ones you named"
                ));
            }
            cmd_sync(&trees, &idxs, &args)
        }
        // A list only makes sense for actions that take more than one worktree.
        Some(other) => Err(format!(
            "'{other}' takes a single worktree; only 'commits', 'diff', 'meld', 'merge', \
             'merged', 'fetch', 'pull' and 'push' take a list"
        )),
        None => Err("a worktree list needs an action, e.g. 'git-wt 1,2 diff'".into()),
    }
}

/// The resume word a token spells, in any of its accepted forms, or None.
///
/// Only `continue`/`abort` qualify: they act on a merge that already exists, so
/// they name no source and a worktree list has nothing to hand them. The other
/// keywords — `ours`, `theirs`, `dry-run` — all describe a merge that is about
/// to start, so they combine with a list perfectly well.
fn resume_word(tok: &str) -> Option<&'static str> {
    match tok {
        "continue" | "--continue" | "-c" => Some("continue"),
        "abort" | "--abort" | "-a" => Some("abort"),
        _ => None,
    }
}

/// Map a 1-based index to a 0-based one, or an error.
fn check_index(n: usize, len: usize) -> Result<usize, String> {
    if n == 0 {
        return Err("no worktree #0".into());
    }
    if n > len {
        return Err(format!(
            "no worktree #{n}; there are {len} (see 'git-wt list')"
        ));
    }
    Ok(n - 1)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// True when a slice of arguments contains `-p` or `--show-path`.
fn show_path_from_rest(args: &[String]) -> bool {
    args.iter().any(|a| a == "-p" || a == "--show-path")
}

/// Parse `list` arguments (an optional SEARCH plus `--col`) then list. Shared
/// by `list`/`ls`, the no-args default, and a bare leading `--col`.
fn list_from_args(root: &Path, args: &[String]) -> Result<(), String> {
    let mut search: Option<String> = None;
    let mut cols: Option<Vec<usize>> = None;
    let mut mode = ListMode::Normal;
    let mut show_path = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--col" | "-c" => {
                let v = it.next().ok_or("--col needs columns, e.g. 1,2,3")?;
                cols = Some(parse_cols(v)?);
            }
            s if s.starts_with("--col=") => cols = Some(parse_cols(&s["--col=".len()..])?),
            "--long" | "-l" => mode = ListMode::Long,
            "--short" | "-s" => mode = ListMode::Short,
            "--show-path" | "-p" => show_path = true,
            s if s.starts_with('-') && s != "-" => {
                return Err(format!("unknown option '{s}'\nTry 'git-wt --help'"));
            }
            s => {
                if search.is_some() {
                    return Err("too many arguments\nTry 'git-wt --help'".into());
                }
                search = Some(s.to_string());
            }
        }
    }
    cmd_list(root, search.as_deref(), cols, mode, show_path)
}

/// Verbosity for `list`. Normal enriches to status + last-commit only on a
/// terminal; on a pipe it stays the plain id/branch/dir contract.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ListMode {
    Short,
    Normal,
    Long,
}

fn cmd_list(
    root: &Path,
    search: Option<&str>,
    cols: Option<Vec<usize>>,
    mode: ListMode,
    show_path: bool,
) -> Result<(), String> {
    let trees = worktrees(root)?;

    // Keep the original 1-based index so `git-wt <N> ...` means the same tree
    // no matter what filter was applied.
    let rows: Vec<(usize, &Worktree)> = trees
        .iter()
        .enumerate()
        .filter(|(_, w)| match search {
            Some(s) => fuzzy_match(w, s),
            None => true,
        })
        .collect();

    if let Some(s) = search {
        if rows.is_empty() {
            return Err(format!("no worktree matches '{s}'"));
        }
    }

    let stdout_tty = std::io::stdout().is_terminal();
    let color = color_enabled(stdout_tty);
    let explicit = cols.is_some();

    // Columns to show, in order: 1=id, 2=branch, 3=dir, 4=status, 5=last-commit,
    // 6=merged-into-current, 7=merged-into-ref, 8=merged-at.
    // Without --col: Short is a compact summary, Long shows everything, and
    // Normal enriches only on a TTY so a piped `git-wt list` keeps the plain
    // id/branch/dir contract.
    //
    // A path is the widest cell and the least read -- it is the branch name
    // with a prefix, and `git-wt <N> path` is how a script gets one anyway --
    // so on a terminal it waits for --show-path. A pipe keeps it unasked: the
    // id/branch/dir contract is what the flagless form has always emitted.
    let cols = match (cols, mode) {
        (Some(c), _) => c,
        (None, ListMode::Short) => vec![1, 2, 4],
        (None, ListMode::Long) => vec![1, 2, 3, 4, 5],
        (None, ListMode::Normal) if stdout_tty && show_path => vec![1, 2, 3, 4, 5],
        (None, ListMode::Normal) if stdout_tty => vec![1, 2, 4, 5],
        (None, ListMode::Normal) => vec![1, 2, 3],
    };

    // Branch color needs status too, so fetch it whenever we color or show it.
    let need_status = color || cols.contains(&4);
    let need_last = cols.contains(&5);
    let need_merged = cols.contains(&6);
    let need_merged_ref = cols.contains(&7);
    let need_merged_at = cols.contains(&8);
    let header = !explicit && stdout_tty && mode != ListMode::Short;

    // Right-align the index to the widest possible so filtered output lines up.
    let numw = trees.len().to_string().len();

    // The branch we are standing in; column 6 asks whether each row's branch is
    // already contained in it. Columns 7/8 use the same reference in normal list
    // mode; the `--others` command overrides the reference explicitly.
    let here = if need_merged || need_merged_ref || need_merged_at {
        current_ref()
    } else {
        String::new()
    };
    let merged_ref = here.clone();

    // Per-row metadata, fetched once (read-only git calls).
    let meta: Vec<(Status, String, String, String, String)> = rows
        .iter()
        .map(|(_, w)| {
            let st = if need_status && !w.bare {
                worktree_status(&w.path)
            } else {
                Status::Unknown
            };
            let last = if need_last { last_commit(&w.path) } else { String::new() };
            let merged = if need_merged {
                merged_text(root, w, &here)
            } else {
                String::new()
            };
            let (merged_r, merged_a) = if need_merged_ref || need_merged_at {
                merged_text_at(root, w, &merged_ref)
            } else {
                (String::new(), String::new())
            };
            (st, last, merged, merged_r, merged_a)
        })
        .collect();

    // Plain (uncolored) cells drive column widths; color is applied at print
    // time so the ANSI escapes never skew alignment.
    let cells: Vec<Vec<String>> = rows
        .iter()
        .zip(&meta)
        .map(|((i, w), (st, last, merged, merged_r, merged_a))| {
            cols.iter()
                .map(|c| match c {
                    1 => format!("{:>numw$}", i + 1, numw = numw),
                    2 => label(w),
                    3 => w.path.display().to_string(),
                    4 => status_text(*st).to_string(),
                    5 => last.clone(),
                    6 => merged.clone(),
                    7 => merged_r.clone(),
                    _ => merged_a.clone(),
                })
                .collect()
        })
        .collect();

    let header_cells: Vec<String> = cols.iter().map(|c| col_header(*c).to_string()).collect();
    let mut cells = cells;

    // Per-column width over the header and every data row.
    let mut widths = vec![0usize; cols.len()];
    for row in cells.iter().chain(header.then_some(&header_cells)) {
        for (k, cell) in row.iter().enumerate() {
            widths[k] = widths[k].max(cell.chars().count());
        }
    }

    // A branch name is the one cell with no bound, and an issue-shaped one is
    // long enough to wrap the row on its own. So it gets whatever width the
    // terminal has left once every other column is paid for, and the overflow
    // is cut off each cell. `term_width` is None off a terminal, so a pipe
    // still sees every name whole.
    if let (Some(term), Some(k)) = (term_width(stdout_tty), cols.iter().position(|c| *c == 2)) {
        let gaps = 2 * (cols.len() - 1);
        let others: usize = widths
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != k)
            .map(|(_, w)| w)
            .sum();
        let budget = term.saturating_sub(gaps + others);
        if budget < widths[k] {
            // Below the floor the row wraps anyway, and a name cut to nothing
            // names nothing: keep enough to tell two branches apart.
            let cap = budget.max(BRANCH_MIN);
            for row in cells.iter_mut() {
                row[k] = ellipsize(&row[k], cap);
            }
            widths[k] = cells.iter().map(|r| r[k].chars().count()).max().unwrap_or(0);
            if header {
                widths[k] = widths[k].max(header_cells[k].chars().count());
            }
        }
    }

    if header {
        let line = render_row(&header_cells, &cols, &widths, Status::Unknown, false);
        println!("{}", paint(&line, DIM, color));
    }

    for (row, (st, _, _, _, _)) in cells.iter().zip(&meta) {
        let line = render_row(row, &cols, &widths, *st, color);
        println!("{line}");
    }
    Ok(())
}

/// Header label for a column id.
fn col_header(c: usize) -> &'static str {
    match c {
        1 => "#",
        2 => "branch",
        3 => "path",
        4 => "status",
        5 => "last-commit",
        6 => "merged",
        7 => "merged",
        _ => "merged-at",
    }
}

/// Join one row's cells with two-space gaps, padding all but the last column.
/// When `color`, the branch (col 2) and status (col 4) cells are tinted by
/// `st`. Padding is computed on the plain text, then color wraps it, so ANSI
/// never affects alignment.
fn render_row(
    row: &[String],
    cols: &[usize],
    widths: &[usize],
    st: Status,
    color: bool,
) -> String {
    let mut line = String::new();
    let last = row.len() - 1;
    for (k, cell) in row.iter().enumerate() {
        if k > 0 {
            line.push_str("  ");
        }
        let padded = if k == last {
            cell.clone()
        } else {
            format!("{:<w$}", cell, w = widths[k])
        };
        let code = status_color(st);
        let tinted = matches!(cols[k], 2 | 4) && color && !code.is_empty();
        if tinted {
            line.push_str(&paint(&padded, code, true));
        } else {
            line.push_str(&padded);
        }
    }
    line
}

/// Parse `--col` value like "1,2,4" into column ids.
/// 1=id, 2=branch, 3=dir, 4=status, 5=last-commit, 6=merged-into-current,
/// 7=merged-into-ref, 8=merged-at.
const COL_HELP: &str = "1=id, 2=branch, 3=dir, 4=status, 5=last-commit, 6=merged, 7=merged-ref, 8=merged-at";

fn parse_cols(s: &str) -> Result<Vec<usize>, String> {
    let mut v = Vec::new();
    for part in s.split(',') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let n: usize = p
            .parse()
            .map_err(|_| format!("bad column '{p}' (use {COL_HELP})"))?;
        if n < 1 || n > 8 {
            return Err(format!("no column {n} (use {COL_HELP})"));
        }
        v.push(n);
    }
    if v.is_empty() {
        return Err("--col needs columns, e.g. 1,2,3".into());
    }
    Ok(v)
}

// ---------------------------------------------------------------------------
// Color, status, and metadata (no dependencies; ANSI on a TTY only)
// ---------------------------------------------------------------------------

const RESET: &str = "\x1b[0m";
const GREEN: &str = "32";
const YELLOW: &str = "33";
const RED: &str = "31";
const DIM: &str = "2";

/// Whether to emit ANSI for a stream that is (or isn't) a terminal. Honors the
/// `NO_COLOR` (any value disables) and `CLICOLOR_FORCE` (nonzero forces on)
/// conventions; otherwise follows the stream's TTY-ness.
fn color_enabled(is_tty: bool) -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if let Some(v) = std::env::var_os("CLICOLOR_FORCE") {
        if !v.is_empty() && v != "0" {
            return true;
        }
    }
    is_tty
}

/// Wrap `s` in an ANSI SGR code when `on`, else return it unchanged. The code
/// is a bare parameter string like "32" or "2".
fn paint(s: &str, code: &str, on: bool) -> String {
    if on {
        format!("\x1b[{code}m{s}{RESET}")
    } else {
        s.to_string()
    }
}

/// Working-tree cleanliness of a worktree.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Status {
    Clean,
    Dirty,
    Untracked,
    /// Bare worktree, or git couldn't report (shown blank).
    Unknown,
}

/// Classify `git status --porcelain` output. Any `??` line means untracked;
/// other entries mean dirty; empty means clean.
fn classify_status(porcelain: &str) -> Status {
    if porcelain.trim().is_empty() {
        return Status::Clean;
    }
    if porcelain.lines().any(|l| l.starts_with("??")) {
        Status::Untracked
    } else {
        Status::Dirty
    }
}

/// Run `git status --porcelain` in the worktree and classify it.
fn worktree_status(path: &Path) -> Status {
    match git_stdout(path, &["status", "--porcelain"]) {
        Ok(s) => classify_status(&s),
        Err(_) => Status::Unknown,
    }
}

fn status_text(s: Status) -> &'static str {
    match s {
        Status::Clean => "clean",
        Status::Dirty => "dirty",
        Status::Untracked => "untracked",
        Status::Unknown => "",
    }
}

/// ANSI color for a status, or "" (no color) for Unknown.
fn status_color(s: Status) -> &'static str {
    match s {
        Status::Clean => GREEN,
        Status::Dirty => YELLOW,
        Status::Untracked => RED,
        Status::Unknown => "",
    }
}

/// Relative time of the worktree's last commit (e.g. "2 minutes ago"), or ""
/// when unavailable (bare / no commits).
fn last_commit(path: &Path) -> String {
    git_stdout(path, &["log", "-1", "--format=%ar"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Short text for the "merged" column: whether `w`'s branch is already in the
/// branch we are standing in (`here`). `-` for bare worktrees or failures.
fn merged_text(root: &Path, w: &Worktree, here: &str) -> String {
    let Some(src) = w.branch.as_deref() else {
        return "-".into();
    };
    merged_status_text(root, src, here)
}

/// Merge status text for a source branch relative to a destination branch.
fn merged_status_text(root: &Path, src: &str, dest: &str) -> String {
    match git_cmd(root, &["merge-base", "--is-ancestor", src, dest])
        .output()
    {
        Ok(out) => match out.status.code() {
            Some(0) => "merged".into(),
            Some(1) => match ahead_count(root, src, dest) {
                Ok(n) => format!("ahead {n}"),
                Err(_) => "ahead".into(),
            },
            _ => "-".into(),
        },
        Err(_) => "-".into(),
    }
}

/// Merge status text and, if merged, the relative time of the most recent merge
/// commit on `dest` that made `src` reachable. `-` for not-merged, bare,
/// fast-forward, or failures.
fn merged_text_at(root: &Path, w: &Worktree, dest: &str) -> (String, String) {
    let Some(src) = w.branch.as_deref() else {
        return ("-".into(), "-".into());
    };
    if src == dest {
        return ("self".into(), "-".into());
    }
    let status = merged_status_text(root, src, dest);
    let at = if status == "merged" {
        last_merge_date(root, src, dest)
    } else {
        "-".into()
    };
    (status, at)
}

/// Relative time of the most recent merge commit on `dest` after `src`.
/// Returns "-" when no merge commit is found (e.g. fast-forward).
fn last_merge_date(root: &Path, src: &str, dest: &str) -> String {
    git_stdout(
        root,
        &[
            "log",
            "-1",
            "--ancestry-path",
            "--merges",
            "--format=%ar",
            &format!("{src}..{dest}"),
        ],
    )
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "-".into())
}

/// Case-insensitive subsequence match over "<label> <path>".
fn fuzzy_match(w: &Worktree, needle: &str) -> bool {
    let hay = format!("{} {}", label(w), w.path.display()).to_lowercase();
    is_subseq(&hay, &needle.to_lowercase())
}

/// True when every char of `needle` appears in `hay`, in order.
fn is_subseq(hay: &str, needle: &str) -> bool {
    let mut chars = hay.chars();
    'outer: for nc in needle.chars() {
        for hc in chars.by_ref() {
            if hc == nc {
                continue 'outer;
            }
        }
        return false;
    }
    true
}

fn cmd_remove(
    root: &Path,
    trees: &[Worktree],
    idx: usize,
    yes: bool,
    force: bool,
) -> Result<(), String> {
    let wanted = &trees[idx];

    // The main worktree is the first entry git reports; a bare one is never
    // a checkout to remove.
    if idx == 0 || wanted.bare {
        return Err("refusing to remove the main worktree".into());
    }

    // Was the shell standing inside the tree we're about to remove? Capture it
    // before removal (canonicalize needs the dir to still exist). Only then does
    // a wrapper need to cd back to main; otherwise it should stay put.
    let inside = match std::env::current_dir() {
        Ok(cwd) => canon(&cwd).starts_with(canon(&wanted.path)),
        Err(_) => false,
    };

    let path_s = wanted.path.to_string_lossy().to_string();
    if !yes
        && !confirm(&format!(
            "Remove worktree '{}' at {path_s}? [y/N] ",
            label(wanted)
        ))?
    {
        eprintln!("Aborted.");
        return Ok(());
    }

    let mut argv = vec!["worktree", "remove"];
    if force {
        argv.push("--force");
    }
    argv.push(&path_s);

    git_run(root, &argv).map_err(|e| {
        if !force && e.contains("contains modified or untracked files") {
            format!("{e}\nhint: re-run with -f to discard them")
        } else {
            e
        }
    })?;

    git_run(root, &["worktree", "prune"])?;

    // The `remove` verb only detaches the worktree; the branch itself stays.
    let leaf = leaf_of(&wanted.path);
    let branch_note = match &wanted.branch {
        Some(b) => format!("branch {b} kept"),
        None => "detached".into(),
    };
    let on = color_enabled(std::io::stderr().is_terminal());
    eprintln!("{} {leaf}  ({branch_note})", paint("Removed", GREEN, on));

    // Only when the shell was inside the removed tree does its cwd now dangle,
    // so print the main path for a wrapper to cd back. Removing some other tree
    // leaves you where you are — print nothing, so the wrapper stays put.
    if inside {
        if let Some(main) = trees.first() {
            println!("{}", main.path.display());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Merge: git-wt <N>,<M> merge | --continue | --abort
// ---------------------------------------------------------------------------

/// What a `merge` invocation asks for. `continue`/`abort` resume or undo a
/// merge that is already in progress, so they carry no source and no options.
#[derive(Debug, PartialEq, Eq)]
enum MergeOp {
    Start(String),
    Continue,
    Abort,
}

/// Which side wins a hunk that both branches touched. Maps to git's
/// `-X ours` / `-X theirs`, never `-s ours`: the strategy *option* picks a side
/// only where hunks actually collide, while the whole-tree *strategy* would drop
/// the source's changes entirely and still record a merge — silent data loss.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Ours,
    Theirs,
}

impl Side {
    /// The word the user typed, for echoing back in messages.
    fn word(self) -> &'static str {
        match self {
            Side::Ours => "ours",
            Side::Theirs => "theirs",
        }
    }

    /// git's `-X` argument.
    fn strategy_option(self) -> &'static str {
        self.word()
    }
}

#[derive(Debug, PartialEq, Eq)]
struct MergeArgs {
    op: MergeOp,
    message: Option<String>,
    no_ff: bool,
    ff_only: bool,
    squash: bool,
    force: bool,
    side: Option<Side>,
    dry_run: bool,
}

/// Parse the words after a `merge` verb, in either target form.
///
/// The list form hands its source in as a positional, so both spellings land
/// here with the same shape. A worktree-number source is only legal via the
/// list — `dispatch_target` rejects `git-wt <N> merge <M>` before this parser's
/// result is used — but a branch source and the resume words (`continue`,
/// `abort`, whose source is implicit in the in-progress merge) still arrive
/// from the single-target form.
///
/// The verb-ish words — `continue`, `abort`, `ours`, `theirs`, `dry-run` — each
/// take an optional `--` plus a short form, so `abort`, `--abort` and `-a` are
/// one thing: they read as words in this grammar, and the dashes are noise. The
/// flags that mirror git's own spelling (`--no-ff`, `--squash`, ...) keep their
/// dashes so muscle memory carries over.
///
/// Keywords are matched ahead of the positional source, so a branch actually
/// named `ours` or `abort` must be spelled `heads/ours` to be merged.
fn parse_merge_args(args: &[String]) -> Result<MergeArgs, String> {
    let mut source: Option<String> = None;
    let mut op: Option<MergeOp> = None;
    let mut message = None;
    let mut side: Option<Side> = None;
    let (mut no_ff, mut ff_only, mut squash, mut force, mut dry_run) =
        (false, false, false, false, false);

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "continue" | "--continue" | "-c" => set_merge_op(&mut op, MergeOp::Continue)?,
            "abort" | "--abort" | "-a" => set_merge_op(&mut op, MergeOp::Abort)?,
            "ours" | "--ours" | "-o" => set_side(&mut side, Side::Ours)?,
            "theirs" | "--theirs" | "-t" => set_side(&mut side, Side::Theirs)?,
            "dry-run" | "--dry-run" | "-d" => dry_run = true,
            "-m" | "--message" => {
                message = Some(it.next().ok_or("--message needs a message")?.clone());
            }
            s if s.starts_with("--message=") => {
                message = Some(s["--message=".len()..].to_string())
            }
            "--no-ff" => no_ff = true,
            "--ff-only" => ff_only = true,
            "--squash" => squash = true,
            "-f" | "--force" => force = true,
            s if s.starts_with('-') && s != "-" => {
                return Err(format!("unknown option '{s}' for merge\nTry 'git-wt --help'"));
            }
            s => {
                if source.is_some() {
                    return Err("too many arguments\nTry 'git-wt --help'".into());
                }
                source = Some(s.to_string());
            }
        }
    }

    if no_ff && ff_only {
        return Err("--no-ff and --ff-only conflict".into());
    }
    if squash && no_ff {
        return Err("--squash and --no-ff conflict".into());
    }

    // A resume takes no source and no merge options: those were settled when
    // the merge started, so silently accepting them would be a lie.
    if let Some(op) = op {
        let word = if op == MergeOp::Continue { "continue" } else { "abort" };
        if let Some(s) = source {
            return Err(format!("{word} takes no argument (got '{s}')"));
        }
        // The tempting one: `merge theirs continue` reads as "finish this
        // conflict by taking theirs", but a side is chosen when the merge is
        // computed. Name the real path instead of ignoring the word.
        if let Some(sd) = side {
            return Err(format!(
                "{word} takes no merge options\n\
                 hint: '{w}' is applied when a merge starts, so it cannot join one already stopped\n\
                 hint: 'git-wt <N> merge abort', then re-run the merge with '{w}'",
                w = sd.word()
            ));
        }
        let mut bad = start_only_flags(message.as_ref(), no_ff, ff_only, squash, force);
        if dry_run {
            bad.push("dry-run");
        }
        if !bad.is_empty() {
            return Err(format!("{word} takes no merge options (got {})", bad.join(", ")));
        }
        return Ok(MergeArgs {
            op,
            message: None,
            no_ff: false,
            ff_only: false,
            squash: false,
            force: false,
            side: None,
            dry_run: false,
        });
    }

    // A dry run computes the merge in memory and writes nothing, so the flags
    // that shape a real merge commit have nothing to act on.
    if dry_run {
        let bad = start_only_flags(message.as_ref(), no_ff, ff_only, squash, force);
        if !bad.is_empty() {
            return Err(format!("dry-run takes no merge options (got {})", bad.join(", ")));
        }
    }

    let source = source.ok_or(
        "merge needs a source: 'git-wt <N>,<M> merge' \
         (or 'git-wt <N> merge <BRANCH>', or continue/abort)",
    )?;
    Ok(MergeArgs { op: MergeOp::Start(source), message, no_ff, ff_only, squash, force, side, dry_run })
}

/// Record `ours`/`theirs`. They are opposite answers to one question, so asking
/// for both is a mistake worth naming; asking twice for the same side is not.
fn set_side(slot: &mut Option<Side>, side: Side) -> Result<(), String> {
    match slot {
        Some(s) if *s == side => Ok(()),
        Some(_) => Err("ours and theirs conflict".into()),
        None => {
            *slot = Some(side);
            Ok(())
        }
    }
}

/// Record `continue`/`abort`. Like `set_side`, they are opposite answers to one
/// question, so asking for both is a mistake; asking twice for the same one is
/// only redundant.
fn set_merge_op(slot: &mut Option<MergeOp>, op: MergeOp) -> Result<(), String> {
    match slot {
        Some(cur) if *cur == op => Ok(()),
        Some(_) => Err("continue and abort conflict".into()),
        None => {
            *slot = Some(op);
            Ok(())
        }
    }
}

/// The flags that only mean something when a real merge runs, as the user would
/// type them. Some shape the resulting commit (`-m`, `--no-ff`, `--squash`),
/// others gate whether it may run at all (`--ff-only`, `-f`) — either way they
/// need a merge that is about to start, which is exactly what `continue`/
/// `abort` (one already stopped) and `dry-run` (none at all) do not have.
///
/// Shared so all three can name what they are rejecting rather than just
/// saying "no merge options".
fn start_only_flags(
    message: Option<&String>,
    no_ff: bool,
    ff_only: bool,
    squash: bool,
    force: bool,
) -> Vec<&'static str> {
    let mut v = Vec::new();
    if message.is_some() {
        v.push("-m");
    }
    if no_ff {
        v.push("--no-ff");
    }
    if ff_only {
        v.push("--ff-only");
    }
    if squash {
        v.push("--squash");
    }
    if force {
        v.push("-f");
    }
    v
}

fn cmd_merge(
    root: &Path,
    trees: &[Worktree],
    idx: usize,
    args: &MergeArgs,
) -> Result<(), String> {
    let dest = &trees[idx];
    if dest.bare {
        return Err("cannot merge into a bare worktree".into());
    }
    let dir = dest.path.as_path();
    let in_progress = git_quiet(dir, &["rev-parse", "--verify", "-q", "MERGE_HEAD"]);
    let color = std::io::stderr().is_terminal() && color_enabled(true);

    match &args.op {
        MergeOp::Abort => {
            if !in_progress {
                return Err(format!("no merge in progress in {}", dir.display()));
            }
            git_run(dir, &["merge", "--abort"])?;
            eprintln!("{} merge in {}", paint("Aborted", GREEN, color), leaf_of(dir));
            return Ok(());
        }
        MergeOp::Continue => {
            if !in_progress {
                return Err(format!("no merge in progress in {}", dir.display()));
            }
            // Unresolved paths make `git merge --continue` fail with a terse
            // message; naming the files is what the user actually needs.
            let stuck = conflicted_files(dir);
            if !stuck.is_empty() {
                return Err(conflict_msg(dir, &stuck, idx));
            }
            git_run_no_editor(dir, &["merge", "--continue"])?;
            eprintln!("{} merge in {}", paint("Completed", GREEN, color), leaf_of(dir));
            return Ok(());
        }
        MergeOp::Start(_) => {}
    }

    let MergeOp::Start(source) = &args.op else { unreachable!() };
    let src_branch = resolve_merge_source(root, trees, source)?;

    if dest.branch.as_deref() == Some(src_branch.as_str()) {
        return Err(format!("'{src_branch}' is already checked out in worktree {}", idx + 1));
    }

    // A dry run only reads, so it answers before any of the guards below: it
    // never prompts, never writes, and is happy to report on a merge that is
    // currently stopped.
    if args.dry_run {
        return merge_dry_run(dir, &src_branch, &label(dest), color);
    }

    if in_progress {
        // `ours`/`theirs` are applied while the merge is computed, so they can't
        // join one that has already stopped. Redoing it from a clean state is
        // the only way to honor the word — and it costs whatever resolution has
        // been done in that tree, so it is the user's call, not ours.
        let Some(sd) = args.side else {
            return Err(format!(
                "a merge is already in progress in {}\n\
                 hint: 'git-wt {n} merge continue' or 'git-wt {n} merge abort'",
                dir.display(),
                n = idx + 1
            ));
        };
        eprintln!(
            "A merge is already in progress in {}, and '{}' only applies when a merge starts.",
            dir.display(),
            sd.word()
        );
        // Mid-merge, tracked changes are the half-resolved conflict itself —
        // but a merge started with -f over a dirty tree buried the user's own
        // edits in there too, and `merge --abort` unwinds the lot. Say so only
        // when there is actually something to lose.
        let at_risk = git_stdout(dir, &["status", "--porcelain"])
            .map(|p| has_tracked_changes(&p))
            .unwrap_or(true);
        let cost = if at_risk {
            "Uncommitted changes in that tree, including any conflict resolution \
             already done, are discarded"
        } else {
            "Any conflict resolution already done there is discarded"
        };
        if !confirm(&format!(
            "Abort it and re-merge '{src_branch}' with '{}'? {cost}. [y/N] ",
            sd.word()
        ))? {
            eprintln!("Aborted.");
            return Ok(());
        }
        git_run(dir, &["merge", "--abort"])?;
        eprintln!("{} the previous merge", paint("Abandoned", GREEN, color));
    }

    // A merge into uncommitted work can end with the user's own edits tangled in
    // conflict markers, so it takes an explicit --force. A status git can't read
    // is not a green light, so the error propagates rather than merging blind.
    if !args.force {
        let porcelain = git_stdout(dir, &["status", "--porcelain"])?;
        if has_tracked_changes(&porcelain) {
            return Err(format!(
                "worktree {} has uncommitted changes\nhint: commit or stash them, or re-run with -f",
                idx + 1
            ));
        }
    }

    let mut argv: Vec<String> = vec!["merge".into()];
    if args.no_ff {
        argv.push("--no-ff".into());
    }
    if args.ff_only {
        argv.push("--ff-only".into());
    }
    if args.squash {
        argv.push("--squash".into());
    }
    if let Some(sd) = args.side {
        argv.extend(["-X".into(), sd.strategy_option().into()]);
    }
    if let Some(m) = &args.message {
        argv.extend(["-m".into(), m.clone()]);
    }
    argv.push(src_branch.clone());

    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    if let Err(e) = git_run_no_editor(dir, &refs) {
        let stuck = conflicted_files(dir);
        if stuck.is_empty() {
            return Err(e);
        }
        return Err(conflict_msg(dir, &stuck, idx));
    }

    let into = label(dest);
    let what = if args.squash { "Squashed" } else { "Merged" };
    // Name the side when one was forced: it silently decided every collision,
    // so it belongs in the record of what just happened.
    let how = match args.side {
        Some(sd) => format!("{}, {} won conflicts", leaf_of(dir), sd.word()),
        None => leaf_of(dir),
    };
    eprintln!("{} {src_branch} into {into}  ({how})", paint(what, GREEN, color));
    if args.squash {
        eprintln!("hint: the merge is staged but not committed");
    }
    Ok(())
}

/// Report whether `src` would merge into the worktree's HEAD, touching nothing.
///
/// `git merge-tree --write-tree` does the whole job: it resolves the merge into
/// a tree object and exits 1 when a path conflicts, with no index, no checkout
/// and nothing to clean up afterwards. It needs git 2.38+, which is checked by
/// running it rather than by parsing `git --version`.
fn merge_dry_run(dir: &Path, src: &str, into: &str, color: bool) -> Result<(), String> {
    let out = git_cmd(dir, &["merge-tree", "--write-tree", "--name-only", "HEAD", src])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    match out.status.code() {
        Some(0) => {
            eprintln!(
                "{} {src} merges into {into} cleanly",
                paint("Clean", GREEN, color)
            );
            Ok(())
        }
        // Conflicts. stdout is the resulting tree's oid, then the paths that
        // collided. Exiting nonzero keeps `if git-wt 1,2 merge dry-run; then`
        // meaningful, and mirrors what a real merge does on a conflict.
        Some(1) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let files: Vec<String> = stdout
                .lines()
                .skip(1)
                .filter(|l| !l.trim().is_empty())
                .map(str::to_string)
                .collect();
            let mut m = format!("{src} does NOT merge into {into} cleanly\n");
            for f in &files {
                m.push_str(&format!("  {f}\n"));
            }
            m.push_str("hint: nothing was changed — this was a dry run\n");
            m.push_str("hint: 'ours' or 'theirs' would settle these automatically");
            Err(m)
        }
        _ => {
            let err = String::from_utf8_lossy(&out.stderr);
            // Older git has merge-tree, but not --write-tree.
            if err.contains("unknown option") || err.contains("usage:") {
                return Err("dry-run needs git 2.38 or newer (git merge-tree --write-tree)".into());
            }
            Err(err.trim().to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// Merged: git-wt <N> merged [<M|BRANCH>] | git-wt <N>,<M> merged
// ---------------------------------------------------------------------------

/// List every worktree and whether its branch is already merged into the
/// selected worktree's branch, plus when it was merged there.
fn cmd_merged_others(
    root: &Path,
    trees: &[Worktree],
    idx: usize,
    show_path: bool,
) -> Result<(), String> {
    let dest = ref_of(&trees[idx])?;
    cmd_list_with_ref(root, &dest, show_path)
}

/// Shared implementation for the "list relative to a reference branch" view.
/// Used by `cmd_merged_others`; columns 7/8 are the merged-into-ref data.
fn cmd_list_with_ref(root: &Path, merged_ref: &str, show_path: bool) -> Result<(), String> {
    let trees = worktrees(root)?;
    let rows: Vec<(usize, &Worktree)> = trees.iter().enumerate().collect();

    let stdout_tty = std::io::stdout().is_terminal();
    let color = color_enabled(stdout_tty);

    let cols: Vec<usize> = if stdout_tty {
        if show_path {
            vec![1, 2, 3, 4, 5, 7, 8]
        } else {
            vec![1, 2, 4, 5, 7, 8]
        }
    } else {
        vec![1, 2, 3, 7, 8]
    };

    let need_status = color || cols.contains(&4);
    let need_last = cols.contains(&5);
    let need_merged_ref = cols.contains(&7);
    let need_merged_at = cols.contains(&8);
    let header = stdout_tty;

    let numw = trees.len().to_string().len();

    let meta: Vec<(Status, String, String, String)> = rows
        .iter()
        .map(|(_, w)| {
            let st = if need_status && !w.bare {
                worktree_status(&w.path)
            } else {
                Status::Unknown
            };
            let last = if need_last { last_commit(&w.path) } else { String::new() };
            let (merged_r, merged_a) = if need_merged_ref || need_merged_at {
                merged_text_at(root, w, merged_ref)
            } else {
                (String::new(), String::new())
            };
            (st, last, merged_r, merged_a)
        })
        .collect();

    let cells: Vec<Vec<String>> = rows
        .iter()
        .zip(&meta)
        .map(|((i, w), (st, last, merged_r, merged_a))| {
            cols.iter()
                .map(|c| match c {
                    1 => format!("{:>numw$}", i + 1, numw = numw),
                    2 => label(w),
                    3 => w.path.display().to_string(),
                    4 => status_text(*st).to_string(),
                    5 => last.clone(),
                    7 => merged_r.clone(),
                    _ => merged_a.clone(),
                })
                .collect()
        })
        .collect();

    let header_cells: Vec<String> = cols.iter().map(|c| col_header(*c).to_string()).collect();
    let mut cells = cells;

    let mut widths = vec![0usize; cols.len()];
    for row in cells.iter().chain(header.then_some(&header_cells)) {
        for (k, cell) in row.iter().enumerate() {
            widths[k] = widths[k].max(cell.chars().count());
        }
    }

    if let (Some(term), Some(k)) = (term_width(stdout_tty), cols.iter().position(|c| *c == 2)) {
        let gaps = 2 * (cols.len() - 1);
        let others: usize = widths
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != k)
            .map(|(_, w)| w)
            .sum();
        let budget = term.saturating_sub(gaps + others);
        if budget < widths[k] {
            let cap = budget.max(BRANCH_MIN);
            for row in cells.iter_mut() {
                row[k] = ellipsize(&row[k], cap);
            }
            widths[k] = cells.iter().map(|r| r[k].chars().count()).max().unwrap_or(0);
            if header {
                widths[k] = widths[k].max(header_cells[k].chars().count());
            }
        }
    }

    if header {
        let line = render_row(&header_cells,
            &cols,
            &widths,
            Status::Unknown,
            false,
        );
        println!("{}", paint(&line, DIM, color));
    }

    for (row, (st, _, _, _)) in cells.iter().zip(&meta) {
        let line = render_row(row, &cols, &widths, *st, color);
        println!("{line}");
    }
    Ok(())
}

/// Report whether `src` is already an ancestor of `dest`.
///
/// `git merge-base --is-ancestor` exits 0 when src is contained in dest, 1 when
/// it is not, and anything else is a real error. This is the same exit-code
/// contract `merge_dry_run` already uses, so `if git-wt 1 merged; then ...` works.
fn cmd_merged(dir: &Path, src: &str, dest: &str) -> Result<(), String> {
    let out = git_cmd(dir, &["merge-base", "--is-ancestor", src, dest])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    let color = std::io::stderr().is_terminal() && color_enabled(true);

    match out.status.code() {
        Some(0) => {
            eprintln!(
                "{} {src} is already in {dest}",
                paint("Merged", GREEN, color)
            );
            Ok(())
        }
        Some(1) => {
            let count_msg = match ahead_count(dir, src, dest) {
                Ok(n) => format!("ahead {n}"),
                Err(_) => "ahead".to_string(),
            };
            Err(format!("Ahead {src} is NOT in {dest} ({count_msg})"))
        }
        _ => {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(err.trim().to_string())
        }
    }
}

/// Number of commits in `src` that are not in `dest` (`dest..src`).
fn ahead_count(dir: &Path, src: &str, dest: &str) -> Result<usize, String> {
    let s = git_stdout(dir, &["rev-list", "--count", &format!("{dest}..{src}")])?;
    s.trim()
        .parse()
        .map_err(|e| format!("could not parse ahead count: {e}"))
}

/// Resolve a merge source word to a branch name. A number that names a
/// worktree wins over a same-named branch: numbers are this tool's grammar.
fn resolve_merge_source(
    root: &Path,
    trees: &[Worktree],
    source: &str,
) -> Result<String, String> {
    if let Ok(n) = source.parse::<usize>() {
        if n >= 1 && n <= trees.len() {
            let w = &trees[n - 1];
            return w.branch.clone().ok_or_else(|| {
                format!("worktree {n} is {} — no branch to merge", label(w))
            });
        }
    }
    if git_quiet(root, &["rev-parse", "--verify", "-q", &format!("{source}^{{commit}}")]) {
        return Ok(source.to_string());
    }
    Err(format!(
        "no worktree or branch '{source}' (see 'git-wt list')"
    ))
}

/// Whether `git status --porcelain` reports changes to *tracked* files, staged
/// or not. Untracked files don't count: a merge that would overwrite one makes
/// git refuse on its own, so they are no reason to demand `-f`. Deliberately
/// not `classify_status`, which collapses a tree holding both kinds to
/// `Untracked` and would wave those tracked edits through.
fn has_tracked_changes(porcelain: &str) -> bool {
    porcelain
        .lines()
        .any(|l| !l.trim().is_empty() && !l.starts_with("??"))
}

/// Paths with unresolved conflicts in a worktree, one per line.
fn conflicted_files(dir: &Path) -> Vec<String> {
    git_stdout(dir, &["diff", "--name-only", "--diff-filter=U"])
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// The message shown when a merge stops on conflicts: where it stopped, what
/// is conflicted, and the two ways out.
fn conflict_msg(dir: &Path, files: &[String], idx: usize) -> String {
    let mut m = format!("merge conflict in {}\n", dir.display());
    for f in files {
        m.push_str(&format!("  {f}\n"));
    }
    let n = idx + 1;
    m.push_str(&format!(
        "hint: resolve them there, 'git add' each, then 'git-wt {n} merge continue'\n\
         hint: or undo the merge with 'git-wt {n} merge abort'\n\
         hint: or redo it letting one side win: 'git-wt {n} merge abort', then \
         'git-wt {n},<M> merge theirs'"
    ));
    m
}

// ---------------------------------------------------------------------------
// Diff: git-wt <N>,<M> diff [..|...] [flags] [-- PATH...]
// ---------------------------------------------------------------------------

/// The committed state a worktree points at. A branch name reads better in
/// diff headers than a sha, so prefer it; detached/bare use the short sha.
fn ref_of(w: &Worktree) -> Result<String, String> {
    if let Some(b) = &w.branch {
        return Ok(b.clone());
    }
    let sha = git_stdout(&w.path, &["rev-parse", "--short", "HEAD"])
        .map_err(|e| format!("worktree {} has no HEAD: {e}", w.path.display()))?;
    Ok(sha.trim().to_string())
}

/// Diff two worktrees, as `git diff <ref1><dots><ref2>`.
///
/// Refs, not directories: a directory diff would drag in build output and
/// everything else .gitignore exists to hide. That also means uncommitted work
/// is invisible here, so warn when either side is dirty and point at meld.
fn cmd_diff(root: &Path, trees: &[Worktree], idxs: &[usize], rest: &[String]) -> Result<(), String> {
    let (idx, other) = match idxs {
        [a, b] => (*a, *b),
        _ => {
            return Err(format!(
                "diff takes exactly two worktrees, got {}\nhint: 'git-wt 1,2,3 meld' compares three",
                idxs.len()
            ));
        }
    };
    if other == idx {
        return Err(format!(
            "worktree #{} against itself is always empty",
            idx + 1
        ));
    }

    let a = ref_of(&trees[idx])?;
    let b = ref_of(&trees[other])?;

    // rather than becoming a flag with a new name to learn. `live`/`hunks` are
    // bare words for the same reason `..` is: they read as part of the sentence.
    // A pathspec can never be mistaken for one, since pathspecs follow `--`.
    // Settled before the main pass, so the unknown-argument hint below is right
    // whatever the word order: '1,2 diff -w live' must not be told to go run a
    // ref diff. Stops at `--`, where a *pathspec* named 'live' could begin.
    let live = rest
        .iter()
        .take_while(|a| a.as_str() != "--")
        .any(|a| a == "live" || a == "--live");

    let mut dots: Option<&str> = None;
    let mut hunks = false;
    let mut listing: Option<String> = None;
    let mut paths: Vec<String> = Vec::new();
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            ".." => dots = Some(".."),
            "..." => dots = Some("..."),
            // Already counted by the pre-scan.
            "live" | "--live" => {}
            "hunks" | "--hunks" => hunks = true,
            // Everything past `--` is a pathspec; git validates it, not us.
            "--" => {
                paths.extend(it.cloned());
                break;
            }
            "--name-only" | "--name-status" | "--stat" => listing = Some(arg.clone()),
            unknown => {
                // Under `live` there is no single git command to hand off to --
                // that is the whole reason `live` exists -- so pointing at one
                // would contradict the mode the user is already in.
                let hint = if live {
                    "hint: live has no git equivalent to defer to; \
                     'git diff --no-index <dir A>/<file> <dir B>/<file>' is the \
                     closest, one file at a time"
                        .to_string()
                } else {
                    let d = dots.unwrap_or("...");
                    format!(
                        "hint: for any other git flag, run git itself: \
                         git diff {a}{d}{b} {unknown}"
                    )
                };
                return Err(format!(
                    "unexpected argument '{unknown}' for diff\n\
                     diff takes live, hunks, .., ..., --name-only, --name-status, \
                     --stat, -- PATH...\n\
                     {hint}"
                ));
            }
        }
    }

    // A range is a statement about refs. `live` never looks at a ref, so the
    // two cannot both be honored -- silently dropping one would be worse.
    if live {
        if let Some(d) = dots {
            return Err(format!(
                "'live' and '{d}' cannot combine: a range compares commits, \
                 live compares the files on disk\n\
                 hint: drop '{d}' for live contents, or drop 'live' for the range"
            ));
        }
    }
    if let (true, Some(l)) = (hunks, listing.as_deref()) {
        return Err(format!(
            "'hunks' and '{l}' cannot combine: hunks prints line numbers per file, \
             {l} prints a listing"
        ));
    }

    let on_err = color_enabled(std::io::stderr().is_terminal());
    // `live` is the answer to the dirty warning, so it does not get warned at.
    if !live {
        for &i in &[idx, other] {
            if is_dirty(&trees[i].path) {
                eprintln!(
                    "{} #{} {} has uncommitted changes; this diff is committed state only \
                     (try 'git-wt {},{} diff live')",
                    paint("warning:", YELLOW, on_err),
                    i + 1,
                    label(&trees[i]),
                    idx + 1,
                    other + 1
                );
            }
        }
    }

    // '...' by default so a bare '1,2 diff' previews '1,2 merge': the range
    // holds M's commits since the fork and nothing of N's, which is what the
    // merge brings in. '..' answers a different question -- tip vs tip -- and
    // reports N's own commits as deletions, which reads as a huge phantom diff
    // on branches that have diverged at all.
    let dots = dots.unwrap_or("...");
    if live {
        let files = live_diff(
            root,
            &trees[idx].path,
            &trees[other].path,
            &paths,
            // --name-only/--name-status answer "which files", which the byte
            // compare already knows -- no per-file git process needed. Every
            // other view prints counts, which only the patch can supply.
            !matches!(listing.as_deref(), Some("--name-only") | Some("--name-status")),
        )?;
        let head = format!("diff {a} ↔ {b}   live — literal contents, .gitignore honored");
        return render(&files, &head, listing.as_deref(), hunks);
    }
    if hunks {
        let files = ref_diff(root, &format!("{a}{dots}{b}"), &paths)?;
        let head = format!("diff {a} ↔ {b}   {a}{dots}{b} — committed state");
        return render(&files, &head, None, true);
    }

    let mut argv: Vec<String> = Vec::new();
    if let Some(l) = &listing {
        argv.push(l.clone());
    }
    if !paths.is_empty() {
        argv.push("--".into());
        argv.extend(paths);
    }

    // Inherit stdio so git's own pager and color logic apply, exactly as a
    // hand-typed `git diff` would.
    let status = git_cmd(root, &[])
        .arg("diff")
        .arg(format!("{a}{dots}{b}"))
        .args(&argv)
        .status()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !status.success() {
        return Err("git diff exited with an error".into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// live: compare worktrees by file content instead of by commit
// ---------------------------------------------------------------------------

/// One hunk, reduced to what the `hunks` view prints: where it lands on the
/// `+` side, and what kind of change it is.
struct Hunk {
    line: usize,
    kind: &'static str,
    count: usize,
}

/// One differing path. `status` is A/M/D from the union of both sides, so a
/// file that is untracked-and-new on the `+` side is genuinely an add.
struct FileDiff {
    path: String,
    status: char,
    plus: usize,
    minus: usize,
    binary: bool,
    hunks: Vec<Hunk>,
}

/// Paths worth considering in a worktree: tracked, plus untracked that
/// `.gitignore` does not hide. Only git knows this set -- `diff -rq` would
/// drown in `target/`. `-z` because a path may contain anything but NUL.
fn live_files(dir: &Path, paths: &[String]) -> Result<Vec<String>, String> {
    let mut args: Vec<&str> = vec!["ls-files", "-z", "--cached", "--others", "--exclude-standard"];
    if !paths.is_empty() {
        args.push("--");
        args.extend(paths.iter().map(String::as_str));
    }
    let out = git_stdout(dir, &args)?;
    Ok(out
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}

/// Byte-for-byte equality. Length first, so unequal files usually cost one
/// `stat` each. An unreadable file counts as differing: better to show it and
/// let the diff report why than to silently call it unchanged.
fn same_bytes(a: &Path, b: &Path) -> bool {
    match (a.metadata(), b.metadata()) {
        (Ok(ma), Ok(mb)) if ma.len() != mb.len() => return false,
        (Ok(_), Ok(_)) => {}
        _ => return false,
    }
    match (std::fs::read(a), std::fs::read(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// The union of both worktrees' candidate paths, filtered to those that
/// actually differ on disk. With `content`, each survivor also gets a
/// `git diff --no-index` run for its counts and hunks.
fn live_diff(
    root: &Path,
    a_dir: &Path,
    b_dir: &Path,
    paths: &[String],
    content: bool,
) -> Result<Vec<FileDiff>, String> {
    let mut union: Vec<String> = live_files(a_dir, paths)?;
    union.extend(live_files(b_dir, paths)?);
    union.sort();
    union.dedup();

    let mut out = Vec::new();
    for p in union {
        let pa = a_dir.join(&p);
        let pb = b_dir.join(&p);
        // `--cached` lists index entries, so a path can be listed on a side
        // where the file is gone. Absent from both is nothing to report.
        let (ea, eb) = (pa.is_file(), pb.is_file());
        let status = match (ea, eb) {
            (false, false) => continue,
            (false, true) => 'A',
            (true, false) => 'D',
            (true, true) => {
                if same_bytes(&pa, &pb) {
                    continue;
                }
                'M'
            }
        };
        let mut fd = FileDiff {
            path: p,
            status,
            plus: 0,
            minus: 0,
            binary: false,
            hunks: Vec::new(),
        };
        if content {
            // Substituting /dev/null for the missing side turns a one-sided
            // file into real hunks instead of an error.
            let null = PathBuf::from("/dev/null");
            let text = no_index_diff(
                root,
                if ea { &pa } else { &null },
                if eb { &pb } else { &null },
                &fd.path,
            )?;
            parse_patch_into(&text, &mut fd);
        }
        out.push(fd);
    }
    Ok(out)
}

/// `git diff --no-index` on two literal paths: git ignoring that it is git.
/// It exits 1 to mean "they differ", which is the expected case here, so only
/// a code above 1 is a real failure. `show` names the path for errors, since
/// `a`/`b` may be absolute, or /dev/null for a one-sided file.
///
/// `root` is only the process's cwd; `--no-index` resolves the two paths
/// itself and never consults a repo, so any existing directory would do.
fn no_index_diff(root: &Path, a: &Path, b: &Path, show: &str) -> Result<String, String> {
    let out = git_cmd(root, &["diff", "--no-index", "-U0", "--no-color"])
        .arg(a)
        .arg(b)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    match out.status.code() {
        Some(0) | Some(1) => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
        _ => Err(format!(
            "git diff --no-index failed on '{show}': {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
    }
}

/// The `hunks` view over a ref diff. Line numbers are just as useful against
/// commits, so `hunks` does not require `live`.
fn ref_diff(root: &Path, range: &str, paths: &[String]) -> Result<Vec<FileDiff>, String> {
    // A rename reported as one entry would have no single `+` side to number;
    // --no-renames splits it back into the add and the delete `live` would
    // have seen anyway, so the two views agree.
    let mut args: Vec<&str> = vec!["diff", "-U0", "--no-color", "--no-renames", range];
    if !paths.is_empty() {
        args.push("--");
        args.extend(paths.iter().map(String::as_str));
    }
    let text = git_stdout(root, &args)?;
    Ok(split_patch(&text))
}

/// Split a multi-file patch on its `diff --git` headers. The path comes from
/// the `+++ b/` line, falling back to `--- a/` for a deletion, where the `+`
/// side is /dev/null.
fn split_patch(text: &str) -> Vec<FileDiff> {
    let mut out: Vec<FileDiff> = Vec::new();
    let mut cur: Option<FileDiff> = None;
    let mut in_hunks = false;
    for line in text.lines() {
        if line.starts_with("diff --git ") {
            if let Some(f) = cur.take() {
                out.push(f);
            }
            cur = Some(FileDiff {
                path: String::new(),
                status: 'M',
                plus: 0,
                minus: 0,
                binary: false,
                hunks: Vec::new(),
            });
            in_hunks = false;
            continue;
        }
        let Some(f) = cur.as_mut() else { continue };
        if !in_hunks {
            if let Some(p) = line.strip_prefix("--- ") {
                if p == "/dev/null" {
                    f.status = 'A';
                } else if f.path.is_empty() {
                    f.path = p.strip_prefix("a/").unwrap_or(p).to_string();
                }
                continue;
            }
            if let Some(p) = line.strip_prefix("+++ ") {
                if p == "/dev/null" {
                    f.status = 'D';
                } else {
                    f.path = p.strip_prefix("b/").unwrap_or(p).to_string();
                }
                continue;
            }
        }
        if line.starts_with("@@") {
            in_hunks = true;
        }
        eat_patch_line(line, f);
    }
    if let Some(f) = cur.take() {
        out.push(f);
    }
    out
}

/// Fold one file's `-U0` patch into `fd`'s counts and hunks.
fn parse_patch_into(text: &str, fd: &mut FileDiff) {
    let mut in_hunks = false;
    for line in text.lines() {
        if line.starts_with("@@") {
            in_hunks = true;
        }
        // The `---`/`+++` headers are +/- lines to a naive counter; skipping
        // everything before the first `@@` keeps them out of the totals.
        if !in_hunks && !line.starts_with("Binary files ") {
            continue;
        }
        eat_patch_line(line, fd);
    }
}

fn eat_patch_line(line: &str, fd: &mut FileDiff) {
    if line.starts_with("Binary files ") {
        fd.binary = true;
        return;
    }
    if line.starts_with("@@") {
        if let Some(h) = parse_hunk_header(line) {
            fd.hunks.push(h);
        }
        return;
    }
    if line.starts_with('+') {
        fd.plus += 1;
    } else if line.starts_with('-') {
        fd.minus += 1;
    }
}

/// `@@ -oldStart,oldCount +newStart,newCount @@`. Two traps live here: an
/// omitted count means 1, and a zero count is not an edit -- `old == 0` is a
/// pure insertion, `new == 0` a pure deletion. Labeling off the new-side
/// number alone would report every deletion as `+0`.
fn parse_hunk_header(line: &str) -> Option<Hunk> {
    let mut it = line.split_whitespace();
    it.next()?; // @@
    let (_, old_count) = parse_range(it.next()?)?;
    let (new_start, new_count) = parse_range(it.next()?)?;
    let (kind, count) = match (old_count, new_count) {
        (0, n) => ("added", n),
        (o, 0) => ("deleted", o),
        (_, n) => ("modified", n),
    };
    Some(Hunk {
        line: new_start,
        kind,
        count,
    })
}

/// `-119,3` / `+119` -> (start, count). No comma means a count of 1.
fn parse_range(tok: &str) -> Option<(usize, usize)> {
    let body = tok.strip_prefix('-').or_else(|| tok.strip_prefix('+'))?;
    match body.split_once(',') {
        Some((s, c)) => Some((s.parse().ok()?, c.parse().ok()?)),
        None => Some((body.parse().ok()?, 1)),
    }
}

// ---------------------------------------------------------------------------
// live: output
// ---------------------------------------------------------------------------

fn status_paint(s: char) -> &'static str {
    match s {
        'A' => GREEN,
        'D' => RED,
        _ => YELLOW,
    }
}

fn render(
    files: &[FileDiff],
    head: &str,
    listing: Option<&str>,
    hunks: bool,
) -> Result<(), String> {
    let on = color_enabled(std::io::stdout().is_terminal());

    // Silence is the right answer for "nothing differs", but on stdout it is
    // indistinguishable from the empty ref diff `live` exists to fix. Say so
    // on stderr, where it cannot corrupt a pipe -- from every view, so that
    // "no output" never means two different things depending on the flags.
    if files.is_empty() {
        eprintln!("no differences");
        return Ok(());
    }

    match listing {
        Some("--name-only") => {
            for f in files {
                println!("{}", f.path);
            }
            return Ok(());
        }
        Some("--name-status") => {
            for f in files {
                println!("{}\t{}", f.status, f.path);
            }
            return Ok(());
        }
        Some("--stat") => return render_stat(files, on),
        _ => {}
    }

    println!("{}\n", paint(head, DIM, on));
    let w = files.iter().map(|f| f.path.len()).max().unwrap_or(0);
    let pw = files
        .iter()
        .map(|f| format!("+{}", f.plus).len())
        .max()
        .unwrap_or(1);
    for f in files {
        let counts = if f.binary {
            "binary".to_string()
        } else {
            format!(
                "{:<pw$} {}",
                paint(&format!("+{}", f.plus), GREEN, on),
                paint(&format!("−{}", f.minus), RED, on),
                // `{:<n}` pads to a byte count, and paint() added bytes that
                // occupy no columns: "\x1b[" + GREEN + "m" ... RESET. Hence
                // +3 -- the two bytes of "\x1b[" plus the "m".
                pw = pw + if on { GREEN.len() + RESET.len() + 3 } else { 0 }
            )
        };
        println!(
            "{} {:<w$}  {}",
            paint(&f.status.to_string(), status_paint(f.status), on),
            f.path,
            counts,
            w = w
        );
        if hunks {
            // Right-align to this file's widest line number so the numbers
            // form a column, without padding every file out to a fixed width.
            let lw = f
                .hunks
                .iter()
                .map(|h| h.line.to_string().len())
                .max()
                .unwrap_or(1);
            for h in &f.hunks {
                println!("      {:>lw$}  {} {}", h.line, h.kind, h.count, lw = lw);
            }
        }
    }
    println!("\n{}", paint(&summary(files), DIM, on));
    Ok(())
}

/// `git diff --stat`'s shape: a churn bar per file, scaled so the widest row
/// fits, then the same summary line.
fn render_stat(files: &[FileDiff], on: bool) -> Result<(), String> {
    const BAR: usize = 40;
    let w = files.iter().map(|f| f.path.len()).max().unwrap_or(0);
    let max = files
        .iter()
        .map(|f| f.plus + f.minus)
        .max()
        .unwrap_or(0)
        .max(1);
    let nw = files
        .iter()
        .map(|f| (f.plus + f.minus).to_string().len())
        .max()
        .unwrap_or(1);
    for f in files {
        if f.binary {
            println!(" {:<w$} | Bin", f.path, w = w);
            continue;
        }
        let total = f.plus + f.minus;
        // Scale only when the widest row would overflow, so small diffs show
        // their exact churn one character per line, as git does.
        let cell = |n: usize| -> usize {
            if max <= BAR {
                n
            } else if n == 0 {
                0
            } else {
                (n * BAR / max).max(1)
            }
        };
        // An empty run must stay empty: painting "" would emit a colour code
        // wrapping nothing, which is invisible but real bytes on the pipe.
        let run = |n: usize, ch: &str, col: &str| match n {
            0 => String::new(),
            _ => paint(&ch.repeat(n), col, on),
        };
        let bar = format!(
            "{}{}",
            run(cell(f.plus), "+", GREEN),
            run(cell(f.minus), "-", RED)
        );
        println!(" {:<w$} | {:>nw$} {}", f.path, total, bar, w = w, nw = nw);
    }
    println!("{}", paint(&summary(files), DIM, on));
    Ok(())
}

/// git's own phrasing, singulars and all, so the line reads the same as the
/// `--stat` a user would get once the work is committed.
fn summary(files: &[FileDiff]) -> String {
    let p: usize = files.iter().map(|f| f.plus).sum();
    let m: usize = files.iter().map(|f| f.minus).sum();
    let mut s = format!(
        "{} file{} changed",
        files.len(),
        if files.len() == 1 { "" } else { "s" }
    );
    if p > 0 {
        s += &format!(", {p} insertion{}(+)", if p == 1 { "" } else { "s" });
    }
    if m > 0 {
        s += &format!(", {m} deletion{}(-)", if m == 1 { "" } else { "s" });
    }
    s
}

/// Does the worktree have uncommitted tracked changes or untracked files?
/// Unknown (bare, or git failed) counts as not dirty: no warning beats a wrong
/// one. Porcelain stays interpreted in exactly one place, `classify_status`.
fn is_dirty(dir: &Path) -> bool {
    matches!(worktree_status(dir), Status::Dirty | Status::Untracked)
}

// ---------------------------------------------------------------------------
// Meld: git-wt <N>,<N>[,<N>] meld
// ---------------------------------------------------------------------------

/// Open meld on 2-3 worktree directories, in the order given, and wait for it.
/// meld itself is the arbiter of 2-way vs 3-way, so we only pass the paths.
fn cmd_meld(trees: &[Worktree], idxs: &[usize]) -> Result<(), String> {
    match idxs.len() {
        2 | 3 => {}
        1 => return Err("meld needs 2 or 3 worktrees, e.g. 'git-wt 1,2 meld'".into()),
        n => return Err(format!("meld takes at most 3 worktrees, got {n}")),
    }

    // meld would silently show a directory against itself; that is never meant.
    for (i, a) in idxs.iter().enumerate() {
        if idxs[i + 1..].contains(a) {
            return Err(format!("worktree #{} listed twice", a + 1));
        }
    }

    if !on_path("meld") {
        return Err(
            "meld is not installed (or not on PATH)\n\
             hint: macOS 'brew install --cask meld', Debian/Ubuntu 'apt install meld', \
             Fedora 'dnf install meld'"
                .into(),
        );
    }

    let paths: Vec<&Path> = idxs.iter().map(|&i| trees[i].path.as_path()).collect();
    let on = color_enabled(std::io::stderr().is_terminal());
    let names: Vec<String> = idxs.iter().map(|&i| label(&trees[i])).collect();
    eprintln!("{} {}", paint("meld", GREEN, on), names.join("  ↔  "));

    let status = Command::new("meld")
        .args(&paths)
        .status()
        .map_err(|e| format!("failed to run meld: {e}"))?;
    if !status.success() {
        return Err("meld exited with an error".into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Sync: git-wt <N> fetch|pull|push, git-wt fetch|pull|push --all
// ---------------------------------------------------------------------------

/// The three remote verbs. They share a shape — run git in one worktree's
/// directory, over and over — and differ only in the word and the flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncOp {
    Fetch,
    Pull,
    Push,
}

impl SyncOp {
    fn word(self) -> &'static str {
        match self {
            SyncOp::Fetch => "fetch",
            SyncOp::Pull => "pull",
            SyncOp::Push => "push",
        }
    }

    /// The verb a token spells, or None. Only the exact word: these are actions,
    /// and an abbreviation would collide with a branch name.
    fn from_word(tok: &str) -> Option<SyncOp> {
        match tok {
            "fetch" => Some(SyncOp::Fetch),
            "pull" => Some(SyncOp::Pull),
            "push" => Some(SyncOp::Push),
            _ => None,
        }
    }

    /// The flags this verb accepts, in help order. Anything else is an error
    /// rather than a passthrough, the same rule `diff` follows.
    fn flags(self) -> &'static [&'static str] {
        match self {
            SyncOp::Fetch => &["--prune", "--tags", "--no-tags", "--force"],
            SyncOp::Pull => &["--rebase", "--no-rebase", "--ff-only", "--prune", "--autostash"],
            SyncOp::Push => &["--set-upstream", "--force-with-lease", "--tags", "--dry-run"],
        }
    }
}

/// What a `fetch`/`pull`/`push` invocation asked for.
#[derive(Debug, PartialEq, Eq)]
struct SyncArgs {
    op: SyncOp,
    /// Every worktree, rather than the ones a target list named.
    all: bool,
    /// Curated git flags, already canonical (`-u` has become `--set-upstream`).
    flags: Vec<String>,
}

const ALL_HINT: &str = "hint: 'git-wt <N> fetch' for one worktree, 'git-wt fetch --all' for every one";

fn parse_sync_args(op: SyncOp, args: &[String]) -> Result<SyncArgs, String> {
    let mut all = false;
    let mut flags: Vec<String> = Vec::new();
    let word = op.word();

    for a in args {
        let canon = match a.as_str() {
            "--all" | "-a" => {
                all = true;
                continue;
            }
            // Short forms, only where git has one and it is unambiguous here.
            "-u" if op == SyncOp::Push => "--set-upstream",
            "-p" if op != SyncOp::Push => "--prune",
            "-n" if op == SyncOp::Push => "--dry-run",
            // `git push --force` is the one flag we refuse outright: it is
            // `--force-with-lease` minus the check that makes it safe, and a
            // sweep would apply it to every branch at once.
            "-f" | "--force" if op == SyncOp::Push => {
                return Err(format!(
                    "no '--force' for push: it overwrites a remote branch without checking \
                     what is on it\nhint: '--force-with-lease' refuses when the remote moved \
                     since you last saw it"
                ));
            }
            s => s,
        };
        if !op.flags().contains(&canon) {
            return Err(format!(
                "unknown option '{a}' for {word}\n\
                 hint: {word} takes {}\n\
                 any other git flag is yours to run: 'git -C <dir> {word} {a}'",
                op.flags().join(", ")
            ));
        }
        let canon = canon.to_string();
        if !flags.contains(&canon) {
            flags.push(canon);
        }
    }

    // git would take both and let the last one win, silently. Two spellings of
    // opposite intent in one command line is a typo, not a preference.
    for (a, b) in [("--rebase", "--no-rebase"), ("--tags", "--no-tags")] {
        if flags.iter().any(|f| f == a) && flags.iter().any(|f| f == b) {
            return Err(format!("'{a}' and '{b}' contradict each other"));
        }
    }
    if op == SyncOp::Pull && flags.iter().any(|f| f == "--rebase") && flags.iter().any(|f| f == "--ff-only") {
        return Err("'--rebase' and '--ff-only' contradict each other".into());
    }

    Ok(SyncArgs { op, all, flags })
}

/// Why a worktree cannot take the verb at all, or None when it can.
///
/// A bare worktree has no working tree to pull into and no branch to push; a
/// detached HEAD has a commit but no branch, so there is no upstream to name.
/// `fetch` only touches remote-tracking refs, so it works on both.
fn sync_skip(w: &Worktree, op: SyncOp) -> Option<&'static str> {
    if w.bare {
        return Some("bare");
    }
    if op != SyncOp::Fetch && w.detached {
        return Some("detached HEAD, no branch to sync");
    }
    None
}

/// The remote to push a branch that has no upstream to: `origin` when it
/// exists, else the only remote there is. More than one and no origin is a
/// choice we cannot make for the user.
fn default_remote(dir: &Path) -> Result<String, String> {
    let remotes: Vec<String> = git_stdout(dir, &["remote"])?
        .lines()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(String::from)
        .collect();
    match remotes.len() {
        0 => Err("no remote to push to; add one with 'git remote add origin <url>'".into()),
        1 => Ok(remotes.into_iter().next().expect("len 1")),
        _ if remotes.iter().any(|r| r == "origin") => Ok("origin".into()),
        _ => Err(format!(
            "which remote? this repo has {}, and none is called 'origin'\n\
             hint: 'git -C <dir> push -u <remote> <branch>' names it",
            remotes.join(", ")
        )),
    }
}

/// The git command line for one worktree.
///
/// Only `push --set-upstream` needs the worktree to build it: a bare
/// `git push -u` has no upstream to read the remote and branch off of -- that
/// is the situation `-u` exists for -- so git rejects it. Naming them is what
/// the user would have typed, and the branch is per-worktree, which is why this
/// is built per-worktree rather than once.
fn sync_argv(w: &Worktree, args: &SyncArgs) -> Result<Vec<String>, String> {
    let mut argv: Vec<String> = vec![args.op.word().to_string()];
    argv.extend(args.flags.iter().cloned());

    if args.op == SyncOp::Push && args.flags.iter().any(|f| f == "--set-upstream") {
        let branch = w
            .branch
            .as_deref()
            .ok_or("no branch to set an upstream for")?;
        // An upstream already set is the one meant; -u then just re-points it
        // there, which is what a bare `git push -u` does.
        if !git_quiet(
            &w.path,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"],
        ) {
            argv.push(default_remote(&w.path)?);
            argv.push(branch.to_string());
        }
    }
    Ok(argv)
}

/// Run one remote verb across the given worktrees.
///
/// Every worktree runs, whatever the ones before it did: a sweep that stops
/// halfway leaves half the worktrees synced and does not say which half. The
/// failures are collected and reported at the end, and the exit code is the
/// summary.
fn cmd_sync(trees: &[Worktree], idxs: &[usize], args: &SyncArgs) -> Result<(), String> {
    for (i, a) in idxs.iter().enumerate() {
        if idxs[i + 1..].contains(a) {
            return Err(format!("worktree #{} listed twice", a + 1));
        }
    }

    let word = args.op.word();
    let on = color_enabled(std::io::stderr().is_terminal());
    // A sweep is many commands, so it says where each failure happened as it
    // goes. A single target is one command: main prints its error, and printing
    // it here too would say it twice.
    let sweep = idxs.len() > 1;

    let mut failed: Vec<(String, String)> = Vec::new();
    let mut skipped: Vec<(String, &'static str)> = Vec::new();
    let mut ok = 0usize;

    for &i in idxs {
        let w = &trees[i];
        let name = label(w);
        if let Some(why) = sync_skip(w, args.op) {
            eprintln!("{} {name} ({why})", paint("skip", DIM, on));
            skipped.push((name, why));
            continue;
        }
        eprintln!("{} {name}", paint(word, GREEN, on));
        // Building the command line can fail on its own (a worktree with no
        // remote to push to). That is this worktree's failure like any other:
        // a sweep owes the rest of the list their turn.
        let res = sync_argv(w, args).and_then(|argv| {
            let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
            git_run(&w.path, &argv)
        });
        match res {
            Ok(()) => ok += 1,
            Err(e) => {
                if sweep {
                    eprintln!("{} {e}", paint("error:", RED, on));
                }
                failed.push((name, e));
            }
        }
    }

    // One worktree is not a sweep: git's own error is the whole story, and a
    // summary of a single line repeats it.
    if !sweep {
        return match failed.pop() {
            Some((_, e)) => Err(e),
            None => Ok(()),
        };
    }

    eprintln!(
        "\n{word}: {ok} ok, {} failed, {} skipped",
        failed.len(),
        skipped.len()
    );
    if failed.is_empty() {
        return Ok(());
    }
    let names: Vec<&str> = failed.iter().map(|(n, _)| n.as_str()).collect();
    Err(format!("{word} failed in {}: {}", failed.len(), names.join(", ")))
}

/// Which of the two readings of "the story" the rows are in.
///
/// Both keep ancestry: git shows no parent before its children either way, so
/// neither can misreport what came from what. They differ in what fills the
/// gaps between unrelated commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Order {
    /// By author date, so a row's neighbors are its contemporaries and the
    /// branches interleave: "what happened when".
    Date,
    /// By topology, so each branch's line of history stays in one block:
    /// "what did each branch do".
    Topo,
}

impl Order {
    fn flag(self) -> &'static str {
        match self {
            Order::Date => "--author-date-order",
            Order::Topo => "--topo-order",
        }
    }
}

/// How the date column is spelled.
///
/// ISO by default: it is the shape the filters take, so what you read is what
/// you can paste back into `--from-date`. It also sorts and greps, and is the
/// same width on every row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DateFmt {
    /// `Jan. 31, 2026` instead of `2026-01-31`.
    human: bool,
    /// Append the time, 24-hour.
    time: bool,
}

impl DateFmt {
    /// The strftime git is asked for. `%-d` drops the day's leading zero, which
    /// only the human spelling wants; ISO is padded by definition.
    fn spec(self) -> &'static str {
        match (self.human, self.time) {
            (false, false) => "%Y-%m-%d",
            (false, true) => "%Y-%m-%d %H:%M:%S",
            (true, false) => "%b. %-d, %Y",
            (true, true) => "%b. %-d, %Y %H:%M:%S",
        }
    }
}

/// One table row: a commit, its short name, who wrote it when, and its subject.
struct CommitRow {
    /// Full sha, for the set lookups; never printed.
    sha: String,
    short: String,
    text: String,
    author: String,
    /// Author date as printed: `2026-01-31`, or whatever `DateFmt` asked for.
    date: String,
    /// The same date as `YYYY-MM-DD`, which `--date` compares against.
    key: String,
}

/// One file touched by a commit, with status and line-count summary.
#[derive(Debug, Clone)]
struct FileStat {
    status: char,
    path: String,
    /// Added lines. `None` means the file is binary.
    added: Option<usize>,
    /// Removed lines. `None` means the file is binary.
    removed: Option<usize>,
}

/// How a `--date` bound compares.
///
/// Inclusive bounds only: `--from-date`/`--to-date` already say "this day and
/// after/before", so a strict `>` would be a second way to spell a bound the
/// tool has, at the cost of a character the shell steals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DateOp {
    Eq,
    Ge,
    Le,
}

/// One `--date` bound. Several are an AND: `--date '>=A' --date '<B'`.
#[derive(Debug, PartialEq, Eq)]
struct DateFilter {
    op: DateOp,
    date: String,
}

impl DateFilter {
    /// ISO dates sort lexicographically, so a string compare *is* a date
    /// compare -- no timezone arithmetic, no calendar library.
    fn admits(&self, key: &str) -> bool {
        match self.op {
            DateOp::Eq => key == self.date,
            DateOp::Ge => key >= self.date.as_str(),
            DateOp::Le => key <= self.date.as_str(),
        }
    }
}

/// How wide the subject column is, when the terminal is not the one to say.
///
/// The terminal's answer is what is left of the line, which is the right answer
/// right up until the subject is what you came to read. Then the columns left
/// of it are the ones in the way, and the line running past the edge -- where
/// the terminal soft-wraps it, or 'less -S' scrolls it -- is the lesser evil.
#[derive(Debug, Clone, Copy, PartialEq)]
enum SubjectWidth {
    /// Exactly this many columns, terminal or no terminal.
    Cols(usize),
    /// However many the subject is. Nothing is cut.
    Full,
}

/// How many terminal lines a subject may take before it is cut.
///
/// One line is the table's shape -- a row is a commit -- so more of it is
/// asked for, never inferred: a subject that wraps by itself is the table
/// coming apart, which is what the budget exists to prevent.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Wrap {
    /// At most this many lines; the last one ellipsized if the subject runs on.
    Lines(usize),
    /// However many the subject needs. Nothing is cut.
    Full,
}

impl Wrap {
    fn lines(self) -> usize {
        match self {
            Wrap::Lines(n) => n,
            Wrap::Full => usize::MAX,
        }
    }
}

/// Options for `commits`.
#[derive(Debug)]
struct CommitsArgs {
    limit: Option<usize>,
    dates: Vec<DateFilter>,
    from: Option<String>,
    to: Option<String>,
    author: Option<String>,
    topo: bool,
    no_merges: bool,
    fmt: DateFmt,
    /// `Some(None)` is `--md` with no path: a timestamped name in the cwd.
    md: Option<Option<String>>,
    reverse: bool,
    no_cherry: bool,
    /// Print the sha the '≈' copy of each row carries elsewhere.
    pick: bool,
    /// Rows come from every worktree at once, not the first one's log alone.
    union: bool,
    /// Full first-branch log instead of the merge-request-style range.
    all: bool,
    /// Add the changed files under each displayed commit.
    files: bool,
    /// Terminal lines a subject may take. Moot off a terminal: nothing is cut.
    wrap: Wrap,
    /// Columns the subject gets. None lets the terminal decide, as it always has.
    subjectw: Option<SubjectWidth>,
}

fn parse_commits_args(args: &[String]) -> Result<CommitsArgs, String> {
    let mut limit = None;
    let mut dates = Vec::new();
    let mut from = None;
    let mut to = None;
    let mut author = None;
    let mut topo = false;
    let mut no_merges = false;
    let mut fmt = DateFmt { human: false, time: false };
    let mut md = None;
    let mut reverse = false;
    let mut no_cherry = false;
    let mut pick = false;
    let mut union = false;
    let mut all = false;
    let mut files = false;
    let mut wrap = Wrap::Lines(1);
    let mut subjectw = None;
    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-n" | "--limit" => {
                let v = it.next().ok_or("-n needs a count, e.g. '-n 20'")?;
                limit = Some(parse_limit(v)?);
            }
            s if s.starts_with("--limit=") => limit = Some(parse_limit(&s["--limit=".len()..])?),
            "--topo" | "--topo-order" => topo = true,
            "--no-merges" => no_merges = true,
            "--reverse" | "--oldest-first" => reverse = true,
            "--no-cherry" => no_cherry = true,
            "--pick-id" => pick = true,
            "--files" => files = true,
            "--union" | "--any" => union = true,
            "--all" => all = true,
            // The count is optional, and only a count or 'full' is read as
            // one: '--wrap --topo' asks for the whole subject, not for a
            // worktree named '--topo' to be parsed as a number.
            "--wrap" | "-w" => {
                wrap = match it.peek().and_then(|v| parse_wrap(v).ok()) {
                    Some(w) => {
                        it.next();
                        w
                    }
                    None => Wrap::Full,
                };
            }
            s if s.starts_with("--wrap=") => wrap = parse_wrap(&s["--wrap=".len()..])?,
            // Unlike --wrap, the count is required: a bare '--subject-width'
            // names no width, and 'full' is the word for wanting all of it.
            "--subject-width" | "--subjw" => {
                let v = it.next().ok_or(SUBJW_MISSING)?;
                subjectw = Some(parse_subjectw(v)?);
            }
            s if s.starts_with("--subject-width=") => {
                subjectw = Some(parse_subjectw(&s["--subject-width=".len()..])?);
            }
            s if s.starts_with("--subjw=") => {
                subjectw = Some(parse_subjectw(&s["--subjw=".len()..])?);
            }
            // A '--subject' would read as the filter --author is: same table,
            // same shape, and one of them cuts rows. Say which was meant.
            "--subject" => return Err(SUBJECT_MSG.into()),
            "--show-time" => fmt.time = true,
            "--date-human" => fmt.human = true,
            // The path is optional, so the next word is only it when it is not
            // another flag: 'commits --md --topo' asks for the default name.
            "--md" => {
                let path = match it.peek() {
                    Some(v) if !v.starts_with('-') => Some((*it.next().unwrap()).clone()),
                    _ => None,
                };
                md = Some(path);
            }
            s if s.starts_with("--md=") => md = Some(Some(s["--md=".len()..].to_string())),
            "--date" | "-d" => {
                let v = it.next().ok_or(DATE_MISSING)?;
                dates.push(parse_date_filter(v)?);
            }
            s if s.starts_with("--date=") => dates.push(parse_date_filter(&s["--date=".len()..])?),
            // The same two bounds --date spells with '>=' and '<=', named to
            // mirror --from-id/--to-id -- and needing no quoting, where '>' is
            // a redirect the shell eats before git-wt ever sees it.
            "--from-date" => {
                let v = it.next().ok_or(FROM_DATE_MISSING)?;
                dates.push(DateFilter { op: DateOp::Ge, date: iso_date(v)? });
            }
            s if s.starts_with("--from-date=") => {
                dates.push(DateFilter { op: DateOp::Ge, date: iso_date(&s["--from-date=".len()..])? });
            }
            "--to-date" => {
                let v = it.next().ok_or(TO_DATE_MISSING)?;
                dates.push(DateFilter { op: DateOp::Le, date: iso_date(v)? });
            }
            s if s.starts_with("--to-date=") => {
                dates.push(DateFilter { op: DateOp::Le, date: iso_date(&s["--to-date=".len()..])? });
            }
            "--author" => author = Some(it.next().ok_or(AUTHOR_MISSING)?.clone()),
            s if s.starts_with("--author=") => author = Some(s["--author=".len()..].to_string()),
            "--from-id" => from = Some(it.next().ok_or(FROM_MISSING)?.clone()),
            s if s.starts_with("--from-id=") => from = Some(s["--from-id=".len()..].to_string()),
            "--to-id" => to = Some(it.next().ok_or(TO_MISSING)?.clone()),
            s if s.starts_with("--to-id=") => to = Some(s["--to-id=".len()..].to_string()),
            // A bare --from names neither of the two things it could bound, and
            // guessing which was meant would be worse than saying so.
            "--from" | "--to" => {
                return Err(format!(
                    "no '{a}' for commits; '{a}-id' takes a commit, '{a}-date' takes a date"
                ));
            }
            // git's words for the same bounds: point at ours rather than let a
            // habit from 'git log' read as a typo.
            "--since" => return Err(SINCE_MSG.into()),
            "--until" => return Err(UNTIL_MSG.into()),
            other => {
                return Err(format!(
                    "unexpected argument '{other}' for commits\nTry 'git-wt --help'"
                ));
            }
        }
    }
    // The one asks for exactly what the other switches off: rather than let a
    // '--pick-id' quietly print nothing, say which flag to drop.
    if pick && no_cherry {
        return Err(
            "--pick-id needs the patch comparison that --no-cherry skips: drop one of them"
                .to_string(),
        );
    }
    if all && union {
        return Err("--all and --union are two different row sources: use one of them".into());
    }
    Ok(CommitsArgs {
        limit, dates, from, to, author, topo, no_merges, fmt, md, reverse, no_cherry, pick, union,
        all, files, wrap, subjectw,
    })
}

/// Read `--subject-width`'s value: a column count, or 'full' for no cut at all.
fn parse_subjectw(v: &str) -> Result<SubjectWidth, String> {
    if v.eq_ignore_ascii_case("full") || v.eq_ignore_ascii_case("all") {
        return Ok(SubjectWidth::Full);
    }
    match v.parse::<usize>() {
        // One column holds an ellipsis and nothing else: a column that says
        // only "there was a subject" is not a subject column.
        Ok(n) if n >= MIN_TEXTW => Ok(SubjectWidth::Cols(n)),
        Ok(n) if n > 0 => Err(format!(
            "--subject-width needs {MIN_TEXTW} columns or more: below that, a cut subject says nothing\n\
             hint: 'commits | grep' and '--md' never cut, however narrow the terminal\n  got: '{n}'"
        )),
        _ => Err(format!("{SUBJW_BAD}\n  got: '{v}'")),
    }
}

/// Read `--wrap`'s value: a line count, or 'full' for as many as it takes.
fn parse_wrap(v: &str) -> Result<Wrap, String> {
    if v.eq_ignore_ascii_case("full") || v.eq_ignore_ascii_case("all") {
        return Ok(Wrap::Full);
    }
    match v.parse::<usize>() {
        // Zero lines is no subject column, which no one means by 'wrap'.
        Ok(0) | Err(_) => Err(format!("{WRAP_BAD}\n  got: '{v}'")),
        Ok(n) => Ok(Wrap::Lines(n)),
    }
}

const WRAP_BAD: &str = "--wrap needs a line count of 1 or more, or 'full', e.g. '--wrap 2'\n\
     hint: a bare '--wrap' is 'full'";
const SUBJW_MISSING: &str = "--subject-width needs a column count, or 'full', e.g. '--subject-width 80'";
const SUBJW_BAD: &str = "--subject-width needs a column count, or 'full', e.g. '--subject-width 80'\n\
     hint: 'full' never cuts the subject, however wide it is";
const SUBJECT_MSG: &str = "no '--subject' for commits: it would read as a filter, and it is a width\n\
     hint: '--subject-width 80' widens the column; '--author NAME' filters rows";
const DATE_MISSING: &str = "--date needs a comparison, e.g. --date '>=2026-01-01'\n\
     hint: quote it, or the shell reads '>' as a redirect";
const FROM_DATE_MISSING: &str = "--from-date needs a date, e.g. '--from-date 2026-01-01'";
const TO_DATE_MISSING: &str = "--to-date needs a date, e.g. '--to-date 2026-06-30'";
const FROM_MISSING: &str = "--from-id needs a commit, e.g. '--from-id 5568a21'";
const TO_MISSING: &str = "--to-id needs a commit, e.g. '--to-id HEAD~3'";
const AUTHOR_MISSING: &str = "--author needs a name, e.g. '--author nino'";
const SINCE_MSG: &str = "no '--since' for commits; use '--from-date 2026-01-01'";
const UNTIL_MSG: &str = "no '--until' for commits; use '--to-date 2026-06-30'";

/// Parse `>=2026-01-01`, `<=2026-06-30`, `=2026-01-01`, or a bare date (`=`).
fn parse_date_filter(s: &str) -> Result<DateFilter, String> {
    // Two-character operators first, or the bare-'>' arm below would claim
    // '>=' and reject it as strict.
    let (op, rest) = if let Some(r) = s.strip_prefix(">=") {
        (DateOp::Ge, r)
    } else if let Some(r) = s.strip_prefix("<=") {
        (DateOp::Le, r)
    } else if let Some(r) = s.strip_prefix('=') {
        (DateOp::Eq, r)
    } else if s.starts_with('>') {
        return Err(strict_msg('>', ">=", "--from-date"));
    } else if s.starts_with('<') {
        return Err(strict_msg('<', "<=", "--to-date"));
    } else {
        (DateOp::Eq, s)
    };
    Ok(DateFilter { op, date: iso_date(rest.trim())? })
}

/// A strict bound names a day the inclusive bounds already reach, one day over.
fn strict_msg(op: char, incl: &str, flag: &str) -> String {
    format!(
        "no '{op}' comparison; bounds are inclusive: use '{incl}' (or {flag})\n\
         hint: a day either side is '{incl}' on the next day"
    )
}

/// Validate a `YYYY-MM-DD` date, which is the only shape the compare is sound
/// for: shorter spellings would compare as prefixes and quietly mean something
/// else.
fn iso_date(s: &str) -> Result<String, String> {
    let bad = || {
        // An empty value usually means the shell ate an unquoted '>'.
        if s.is_empty() {
            format!("--date needs a date after the comparison\nhint: {QUOTE_HINT}")
        } else {
            format!("bad date '{s}'; want YYYY-MM-DD, e.g. '>=2026-01-01'")
        }
    };
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return Err(bad());
    }
    if !b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit()) {
        return Err(bad());
    }
    let num = |r: std::ops::Range<usize>| s[r].parse::<u32>().unwrap_or(0);
    let (m, d) = (num(5..7), num(8..10));
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return Err(format!("no such date '{s}'"));
    }
    Ok(s.to_string())
}

const QUOTE_HINT: &str =
    "quote the comparison -- --date '>=2026-01-01' -- or the shell reads '>' as a redirect";

fn parse_limit(s: &str) -> Result<usize, String> {
    match s.parse::<usize>() {
        Ok(0) => Err("-n 0 would show nothing".into()),
        Ok(n) => Ok(n),
        Err(_) => Err(format!("bad count '{s}'; want a number, e.g. '-n 20'")),
    }
}

/// Print a commit-by-branch table for the listed worktrees.
///
/// Refs, not directories, and commits rather than content: this is the question
/// `diff` cannot answer once there are three branches in play -- not "how do
/// these differ" but "which of them has this commit". Rows come from one `git
/// log` over every ref at once, so they are interleaved by date; columns come
/// from one `rev-list` per ref, as sha sets to test each row against.
fn cmd_commits(
    root: &Path,
    trees: &[Worktree],
    idxs: &[usize],
    rest: &[String],
) -> Result<(), String> {
    if idxs.len() < 2 {
        return Err("commits needs 2 or more worktrees, e.g. 'git-wt 1,2,3 commits'".into());
    }
    for (i, a) in idxs.iter().enumerate() {
        if idxs[i + 1..].contains(a) {
            return Err(format!("worktree #{} listed twice", a + 1));
        }
    }
    let args = parse_commits_args(rest)?;

    let refs: Vec<String> = idxs
        .iter()
        .map(|&i| ref_of(&trees[i]))
        .collect::<Result<_, _>>()?;

    // Three row-source modes:
    //   --union: every branch contributes rows (full logs, unioned).
    //   --all:   only the first branch contributes rows (its full log).
    //   default: the first branch's log, truncated at its earliest divergent
    //            commit -- a merge-request view of what it has that the others
    //            do not, from the furthest divergence up to its tip. Shared
    //            commits above that floor stay in; the floor is found by
    //            position in the display order, not by ancestry, so a merge
    //            DAG's side branches cannot leak past it and --topo cuts at its
    //            own lowest divergent row.
    //
    // The column marks are always computed against each branch's full history,
    // so a shared commit inside the range still shows as present in the other
    // columns.
    let row_refs: &[String] = if args.union { &refs } else { &refs[..1] };
    // The set whose earliest member is the default view's floor: commits the
    // first branch has that at least one other is missing. `None` under --union
    // or --all, where the whole log is the rows and nothing is trimmed.
    let divergent = if args.union || args.all {
        None
    } else {
        let d = divergent_set(root, &refs[0], &refs[1..])?;
        if d.is_empty() {
            eprintln!("no commits ahead of {}", label(&trees[idxs[0]]));
            return Ok(());
        }
        Some(d)
    };

    // A filter runs here rather than in git, so `-n` has to as well: git's -n
    // caps the walk, and capping before the filter would leave rows the filter
    // was going to drop, i.e. fewer than asked for. Unfiltered, git can cap it
    // and skip the walk it saves. The default view walks whole too: its floor
    // can sit past any -n, and letting git cap first would hide it.
    let filtered = !args.dates.is_empty()
        || args.from.is_some()
        || args.to.is_some()
        || args.author.is_some();
    let git_limit = if filtered || divergent.is_some() { None } else { args.limit };
    let order = if args.topo { Order::Topo } else { Order::Date };
    let all_rows = commit_rows(
        root,
        row_refs,
        None,
        git_limit,
        order,
        args.fmt,
        args.no_merges,
    )?;
    // Default view: cut the ordered log at its lowest divergent row, keeping the
    // shared commits above it. Positional, so it stops at the same commit in
    // date or --topo order alike.
    let all_rows = match &divergent {
        Some(d) => window_to_divergent(all_rows, d),
        None => all_rows,
    };
    let unfiltered = all_rows.len();

    // Ancestry, not dates: '--from X' means "X and everything after it", so
    // the rows to drop are the ones strictly older than X. Both bounds resolve
    // first, so a typo'd ref is an error rather than an empty table.
    let older = match &args.from {
        Some(r) => Some(older_than(root, &commit_of(root, r, "--from-id")?)?),
        None => None,
    };
    let within = match &args.to {
        Some(r) => Some(reachable_from(root, &commit_of(root, r, "--to-id")?)?),
        None => None,
    };

    // Fuzzy, and the same fuzzy `list` uses: a subsequence, case-folded, so
    // '--author nes' finds 'Nino Escalera' and nobody types a full name twice.
    let needle = args.author.as_ref().map(|a| a.to_lowercase());

    let mut rows: Vec<CommitRow> = all_rows
        .into_iter()
        .filter(|r| args.dates.iter().all(|f| f.admits(&r.key)))
        .filter(|r| older.as_ref().is_none_or(|o| !o.contains(&r.sha)))
        .filter(|r| within.as_ref().is_none_or(|w| w.contains(&r.sha)))
        .filter(|r| {
            needle
                .as_ref()
                .is_none_or(|n| is_subseq(&r.author.to_lowercase(), n))
        })
        .collect();
    if let Some(n) = args.limit {
        rows.truncate(n);
    }
    // After the cap, not before: '-n 10 --reverse' is the same ten commits as
    // '-n 10', read bottom-up. Reversing first would cap the oldest ten
    // instead, which is a different question nobody asked.
    if args.reverse {
        rows.reverse();
    }

    // File stats are scoped to the displayed rows, so a large log only pays for
    // what the user is looking at. Merge commits diff against their first parent.
    let row_files: Vec<Vec<FileStat>> = if args.files {
        rows.iter()
            .map(|r| commit_files(root, &r.sha))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };

    if rows.is_empty() {
        // A filter that matched nothing is a different story from a history
        // with nothing in it: say which one happened.
        let msg = if filtered && unfiltered > 0 {
            format!("no commits match those filters: {unfiltered} commits, none kept")
        } else if args.union {
            "no commits".to_string()
        } else if args.all {
            format!("no commits on {}", label(&trees[idxs[0]]))
        } else {
            format!("no commits ahead of {}", label(&trees[idxs[0]]))
        };
        eprintln!("{msg}");
        return Ok(());
    }

    // A row is checked when the ref's own walk contains it. The walks are whole,
    // like the rows: the marks answer for a branch's entire history, so a row is
    // checked wherever that commit really is.
    let sets: Vec<HashSet<String>> = refs
        .iter()
        .map(|r| ref_shas(root, r, None))
        .collect::<Result<_, _>>()?;

    // Patch equivalence is what tells "not merged yet" from "already there,
    // under a different sha" -- the difference between work to do and work
    // done, which a bare '·' reports as the same thing. It costs a patch-id
    // walk per ordered pair, so --no-cherry buys the old, cheaper answer back
    // on a repo whose branches have diverged enormously.
    let equiv = if args.no_cherry {
        vec![HashSet::new(); refs.len()]
    } else {
        equivalents(root, &refs)
    };

    // Which sha the '≈' is pointing at, asked only when the column will print
    // it: it is a second patch-id walk over the same divergence.
    let picks = args.pick.then(|| pick_ids(root, &refs));

    let names: Vec<String> = idxs.iter().map(|&i| label(&trees[i])).collect();

    if let Some(path) = &args.md {
        let file = path.clone().unwrap_or_else(md_filename);
        let cmd = format!(
            "git-wt {} commits{}{}",
            idxs.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(","),
            if rest.is_empty() { "" } else { " " },
            rest.join(" ")
        );
        return write_md(
            Path::new(&file),
            &rows,
            &row_files,
            &names,
            &sets,
            &equiv,
            picks.as_ref(),
            &cmd,
        );
    }

    let tty = std::io::stdout().is_terminal();
    render_commits(
        &rows,
        &row_files,
        &names,
        &sets,
        &equiv,
        picks.as_ref(),
        color_enabled(tty),
        term_width(tty),
        args.wrap,
        args.subjectw,
    );
    Ok(())
}

/// Rows for the table: every commit reachable from any ref, newest first.
///
/// `%H` drives the set lookups and `%h %s` is what the row prints -- the same
/// text `git log --oneline` shows, which is the format the rows are meant to
/// read as. `%aN` respects .mailmap, so a contributor who has committed under
/// two names is one name here.
///
/// Author dates throughout, and `--author-date-order` to match the column the
/// table prints; commit dates answer "when did this land here", which is not
/// what a table about who-wrote-what is asking.
///
/// The order is ancestry first: git shows no parent before its children
/// whatever the timestamps claim, and the date only sequences commits that do
/// not descend from each other. So a commit authored before its own parent --
/// rebased, cherry-picked, or written on a machine with a bad clock -- reads as
/// out of order against its date column while the history stays true. That is
/// the right trade: a table whose rows contradicted the history would be
/// lying, where one whose dates jump is merely reporting a wrong clock.

/// The files a commit touched, with status and line counts.
///
/// Diffed against the first parent (or the empty tree for root commits), which
/// matches what a reader expects from a one-line log entry. Merge commits show
/// the first-parent diff only, not the combined merge.
fn commit_files(root: &Path, sha: &str) -> Result<Vec<FileStat>, String> {
    // First parent, or the empty tree for a root commit. The empty tree hash is
    // stable across git versions, so we use it directly rather than spawning a
    // command to compute it.
    const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
    let parents = git_stdout(root, &["rev-list", "--parents", "-n", "1", sha])?
        .lines()
        .next()
        .map(|line| {
            line.split_whitespace()
                .skip(1)
                .map(String::from)
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();
    let base = parents.first().map(String::as_str).unwrap_or(EMPTY_TREE);

    let status_out = git_stdout(
        root,
        &["diff-tree", "-r", "--name-status", "-M", "-C", base, sha],
    )?;
    let numstat_out = git_stdout(
        root,
        &["diff-tree", "-r", "--numstat", "-M", "-C", base, sha],
    )?;

    // Map path -> status. Renames/copies keep the new path.
    let mut status_by_path: HashMap<String, char> = HashMap::new();
    for line in status_out.lines() {
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let Some(status_field) = parts.next() else {
            continue;
        };
        let Some(status) = status_field.chars().next() else {
            continue;
        };
        match status {
            'R' | 'C' => {
                // R100<tab>old<tab>new
                let Some(old) = parts.next() else {
                    continue;
                };
                let Some(new) = parts.next() else {
                    continue;
                };
                status_by_path.insert(new.to_string(), status);
                // `--numstat` reports the rename as `old => new`, so keep that
                // lookup key too.
                status_by_path.insert(format!("{} => {}", old, new), status);
            }
            _ => {
                let Some(path) = parts.next() else {
                    continue;
                };
                status_by_path.insert(path.to_string(), status);
            }
        }
    }

    let mut stats: Vec<FileStat> = Vec::new();
    for line in numstat_out.lines() {
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(3, '\t');
        let Some(added_field) = parts.next() else {
            continue;
        };
        let Some(removed_field) = parts.next() else {
            continue;
        };
        let Some(path) = parts.next() else {
            continue;
        };
        let added = if added_field == "-" {
            None
        } else {
            added_field.parse::<usize>().ok()
        };
        let removed = if removed_field == "-" {
            None
        } else {
            removed_field.parse::<usize>().ok()
        };
        let status = status_by_path.get(path).copied().unwrap_or('M');
        stats.push(FileStat {
            status,
            path: path.to_string(),
            added,
            removed,
        });
    }

    stats.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(stats)
}

fn commit_rows(
    root: &Path,
    refs: &[String],
    base: Option<&str>,
    limit: Option<usize>,
    order: Order,
    fmt: DateFmt,
    no_merges: bool,
) -> Result<Vec<CommitRow>, String> {
    let count;
    let date_arg = format!("--date=format:{}", fmt.spec());
    let mut args = vec![
        "log",
        order.flag(),
        &date_arg,
        "--format=%H%x09%aN%x09%ad%x09%as%x09%h%x09%s",
    ];
    // Merge commits carry no work of their own; dropping them leaves the
    // commits someone actually wrote. The mark columns are unaffected: a
    // merge that is not a row is still in every rev-list that reaches it.
    if no_merges {
        args.push("--no-merges");
    }
    if let Some(n) = limit {
        count = format!("-n{n}");
        args.push(&count);
    }
    args.extend(refs.iter().map(String::as_str));
    if let Some(b) = base {
        args.push("--not");
        args.push(b);
    }

    let out = git_stdout(root, &args)?;
    Ok(out
        .lines()
        .filter_map(|line| {
            let mut f = line.splitn(6, '\t');
            Some(CommitRow {
                sha: f.next()?.to_string(),
                author: f.next()?.to_string(),
                date: f.next()?.to_string(),
                key: f.next()?.to_string(),
                short: f.next()?.to_string(),
                text: f.next()?.to_string(),
            })
        })
        .collect())
}

/// The worktree the shell is standing in, if any.
///
/// The deepest match wins: `add --dirname` can put one worktree inside
/// another's tree, and the innermost is the one you are actually in.
fn here_index(trees: &[Worktree]) -> Option<usize> {
    let cwd = canon(&std::env::current_dir().ok()?);
    trees
        .iter()
        .enumerate()
        .filter(|(_, w)| cwd.starts_with(canon(&w.path)))
        .max_by_key(|(_, w)| canon(&w.path).components().count())
        .map(|(i, _)| i)
}

/// Resolve `r` to a commit, or say which flag could not find it.
fn commit_of(root: &Path, r: &str, flag: &str) -> Result<String, String> {
    git_stdout(root, &["rev-parse", "--verify", "--quiet", &format!("{r}^{{commit}}")])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{flag}: no commit '{r}'"))
}

/// Everything strictly older than `c`: its parents and all their ancestors.
///
/// `c^@` is every parent at once, so `c` itself is never in the set -- which is
/// what makes `--from <c>` include `<c>`. A root commit has no parents and the
/// set is empty, as it should be: nothing is older than the beginning.
fn older_than(root: &Path, c: &str) -> Result<HashSet<String>, String> {
    Ok(git_stdout(root, &["rev-list", &format!("{c}^@")])?
        .lines()
        .map(str::to_string)
        .collect())
}

/// `c` and everything it can reach, so `--to <c>` includes `<c>`.
fn reachable_from(root: &Path, c: &str) -> Result<HashSet<String>, String> {
    Ok(git_stdout(root, &["rev-list", c])?
        .lines()
        .map(str::to_string)
        .collect())
}

/// The oldest commit on `target` that any source branch is missing.
///
/// For a merge request from target into each source, the missing commits are
/// `source..target` -- what target would bring. The oldest of all those sets
/// is where the relevant range of target begins.
fn divergent_set(root: &Path, target: &str, sources: &[String]) -> Result<HashSet<String>, String> {
    let mut out = HashSet::new();
    for src in sources {
        let range = format!("{src}..{target}");
        for sha in git_stdout(root, &["rev-list", &range])?.lines() {
            out.insert(sha.to_string());
        }
    }
    Ok(out)
}

/// Cut an ordered log at its lowest divergent row: keep every row from the tip
/// down to and including the last one in `divergent`, drop the rest.
///
/// The floor is a *position* in the rows the walk already produced, so it lands
/// on the same commit whatever order made them -- date or `--topo`. An ancestry
/// base (`floor^@` excluded from the walk) cannot do this: on a merge DAG the
/// side branches merged in above the floor are not ancestors of its parent, so
/// they survive the exclusion and leak in as rows far below the floor.
///
/// Empty out means no row was divergent -- e.g. `--no-merges` dropped the only
/// commits the others were missing; the caller reports it like an empty log.
fn window_to_divergent(rows: Vec<CommitRow>, divergent: &HashSet<String>) -> Vec<CommitRow> {
    match rows.iter().rposition(|r| divergent.contains(&r.sha)) {
        Some(i) => rows.into_iter().take(i + 1).collect(),
        None => Vec::new(),
    }
}

/// Per column, the commits it has an *equivalent* of but not the commit itself:
/// same patch, different sha -- a cherry-pick, or a rebase's copy.
///
/// `git cherry <upstream> <head>` is exactly this question: it lists head's
/// commits since the fork and marks `-` on the ones upstream already carries
/// under another sha, comparing patch-ids rather than history. Doing it per
/// ordered pair costs N*(N-1) walks, each bounded by that pair's merge-base,
/// which is the same divergence the table is already showing.
///
/// A pair that cannot be compared (unrelated histories) is skipped rather than
/// fatal: the column simply keeps its `·`, which is what it said before.
fn equivalents(root: &Path, refs: &[String]) -> Vec<HashSet<String>> {
    let mut out = vec![HashSet::new(); refs.len()];
    for (i, upstream) in refs.iter().enumerate() {
        for head in refs.iter() {
            if head == upstream {
                continue;
            }
            let Ok(text) = git_stdout(root, &["cherry", upstream, head]) else {
                continue;
            };
            for line in text.lines() {
                if let Some(sha) = line.strip_prefix("- ") {
                    out[i].insert(sha.trim().to_string());
                }
            }
        }
    }
    out
}

/// Per commit, another sha carrying the same patch: the other half of an `≈`.
///
/// `git cherry` answers whether a copy exists, never which one it is, so the
/// naming is done here: patch-id every commit the refs do not share, group the
/// shas by patch, and a group of more than one is a patch someone picked. Each
/// sha in it names the first of its others -- a patch under three shas has no
/// single answer, and the first is at least a real one.
///
/// The walk is bounded at the refs' common merge-base, since a commit every ref
/// reaches by sha is not a pick to anyone; that is the same divergence `git
/// cherry` bounds each pair by, done once for all of them. Unrelated histories
/// have no such base and no shared work either, so the map comes back empty and
/// the marks keep their `≈` unexplained.
fn pick_ids(root: &Path, refs: &[String]) -> HashMap<String, String> {
    let mut base_args = vec!["merge-base", "--octopus"];
    base_args.extend(refs.iter().map(String::as_str));
    let base = match git_stdout(root, &base_args) {
        Ok(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return HashMap::new(),
    };

    // Merges carry no patch of their own, and `git cherry` skips them too.
    let mut args = vec!["rev-list", "--no-merges"];
    args.extend(refs.iter().map(String::as_str));
    args.push("--not");
    args.push(&base);
    let Some(pairs) = patch_ids(root, &args) else {
        return HashMap::new();
    };

    let mut by_patch: HashMap<String, Vec<String>> = HashMap::new();
    for (patch, sha) in pairs {
        by_patch.entry(patch).or_default().push(sha);
    }
    let mut out = HashMap::new();
    for shas in by_patch.values() {
        for sha in shas {
            if let Some(other) = shas.iter().find(|s| *s != sha) {
                out.insert(sha.clone(), other.clone());
            }
        }
    }
    out
}

/// `(patch-id, commit)` for every commit `rev_args` lists.
///
/// `rev-list | diff-tree --stdin -p | patch-id` is the pipeline `git cherry`
/// runs internally, and the reason for the pipe rather than three `output()`
/// calls: the patch text between the stages is the whole diff of the range,
/// which is worth streaming rather than holding.
///
/// A stage that cannot start, or a git too old for `--stable`, gives `None`:
/// the pick column goes blank, which is what it says for an unpicked commit
/// anyway. Root commits produce no patch and are simply absent.
fn patch_ids(root: &Path, rev_args: &[&str]) -> Option<Vec<(String, String)>> {
    let mut rev = git_cmd(root, rev_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut diff = git_cmd(root, &["diff-tree", "--stdin", "-p"])
        .stdin(Stdio::from(rev.stdout.take()?))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let out = git_cmd(root, &["patch-id", "--stable"])
        .stdin(Stdio::from(diff.stdout.take()?))
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let _ = rev.wait();
    let _ = diff.wait();
    if !out.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| {
                let (patch, sha) = l.split_once(' ')?;
                Some((patch.to_string(), sha.trim().to_string()))
            })
            .collect(),
    )
}

/// Every commit sha reachable from `r`, cut at `base` the same way the rows are.
fn ref_shas(root: &Path, r: &str, base: Option<&str>) -> Result<HashSet<String>, String> {
    let mut args = vec!["rev-list", r];
    if let Some(b) = base {
        args.push("--not");
        args.push(b);
    }
    Ok(git_stdout(root, &args)?
        .lines()
        .map(str::to_string)
        .collect())
}

const CHECK: &str = "✓";
const MISS: &str = "·";
/// Not this commit, but this patch: a cherry-pick or a rebase's copy.
const EQUIV: &str = "≈";
const ELLIPSIS: char = '…';

/// The header over `--pick-id`'s shas.
const PICK_HEAD: &str = "pick";

/// A full sha cut to `n`, the way git abbreviates one.
///
/// No uniqueness check: git's own `--short` picks a length for the repo, and
/// this borrows it rather than second-guessing it per sha.
fn abbrev(sha: &str, n: usize) -> String {
    sha.chars().take(n).collect()
}

/// The narrowest `list` will cut the branch column to, however tight the
/// terminal gets. Wide enough to hold the header and a name's distinguishing
/// head; past this the row is better off wrapping.
const BRANCH_MIN: usize = 12;

/// What a branch has of a given commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mark {
    /// The commit itself.
    Has,
    /// The same patch under a different sha.
    Equivalent,
    /// Neither.
    Missing,
}

impl Mark {
    fn of(sha: &str, has: &HashSet<String>, equiv: &HashSet<String>) -> Mark {
        // Containment wins: a branch that has the commit has it, whatever a
        // patch comparison would also say about an equivalent elsewhere.
        if has.contains(sha) {
            Mark::Has
        } else if equiv.contains(sha) {
            Mark::Equivalent
        } else {
            Mark::Missing
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Mark::Has => CHECK,
            Mark::Equivalent => EQUIV,
            Mark::Missing => MISS,
        }
    }

    fn color(self) -> &'static str {
        match self {
            Mark::Has => GREEN,
            // Yellow: present, but not as the commit in this row.
            Mark::Equivalent => YELLOW,
            Mark::Missing => DIM,
        }
    }
}
/// Below this, a truncated subject says nothing; let the line wrap instead.
const MIN_TEXTW: usize = 24;
/// Enough for a full name; past it, the subject has the better claim.
const AUTHOR_MAX: usize = 16;

/// The terminal's width, or None when stdout is not one.
///
/// No ioctl, so no libc: `tput` reads the same terminfo git's own pager does,
/// and COLUMNS wins when a shell exports it. A terminal that answers neither
/// gets the 80 every terminal is at least as wide as.
fn term_width(is_tty: bool) -> Option<usize> {
    if !is_tty {
        return None;
    }
    if let Some(n) = std::env::var("COLUMNS").ok().and_then(|v| v.parse().ok()) {
        return Some(n);
    }
    let out = Command::new("tput")
        .arg("cols")
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok());
    Some(out.unwrap_or(80))
}

/// An upper bound on the terminal columns `s` will occupy.
///
/// Deliberately not a width table: getting that right needs Unicode data this
/// crate has no dependency for, and a wrong *padding* silently breaks a column.
/// So no column is padded by this -- only the last one is budgeted by it, where
/// over-estimating costs a few characters of subject and under-estimating could
/// only ever wrap. Every non-ASCII char is assumed double-width, which is true
/// of the ones that actually turn up in subjects (emoji, CJK) and merely
/// pessimistic for the rest (accented Latin).
fn width_bound(s: &str) -> usize {
    s.chars()
        .map(|c| match c {
            // Our own marker, and known narrow: counting it wide would let a
            // budgeted string exceed the budget it was just cut to.
            ELLIPSIS => 1,
            c if c.is_ascii() => 1,
            _ => 2,
        })
        .sum()
}

/// Cut `s` to `max` characters, ending in an ellipsis when anything was lost.
fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(1);
    s.chars().take(keep).chain(std::iter::once(ELLIPSIS)).collect()
}

/// Cut `s` to fit `max` terminal columns by `width_bound`'s reckoning.
fn ellipsize_wide(s: &str, max: usize) -> String {
    if width_bound(s) <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    // The ellipsis needs a column of its own, so stop one short of the budget.
    for c in s.chars() {
        let w = if c.is_ascii() { 1 } else { 2 };
        if used + w > max.saturating_sub(1) {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push(ELLIPSIS);
    out
}

/// `commits_2026-07-17_14-30-05.md`: ISO, so the names sort the way the dates
/// do, and stamped to the second so a re-run never silently eats the last one.
///
/// The stamp comes from `date`, for the same reason the terminal width comes
/// from `tput`: turning a unix timestamp into the user's local calendar needs
/// a timezone database this crate has no dependency for.
fn md_filename() -> String {
    let stamp = Command::new("date")
        .arg("+%Y-%m-%d_%H-%M-%S")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            // No `date`: seconds since the epoch still sorts and still differs
            // from the last run, which is all the name owes anyone.
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs().to_string())
                .unwrap_or_else(|_| "report".into())
        });
    format!("commits_{stamp}.md")
}

/// Escape a cell so its content cannot be read as table syntax.
///
/// A `|` in a commit subject would end the cell and shift every column after
/// it -- the markdown twin of the emoji-width bug, and this one silently
/// invents columns rather than merely misaligning them.
fn md_cell(s: &str) -> String {
    s.replace('\\', "\\\\").replace('|', "\\|")
}

/// Write the table as a markdown file, and say where it went.
///
/// Subjects are never truncated here: a file has no right edge to run out of,
/// so the terminal's budget would only lose information the reader asked for.
fn write_md(
    path: &Path,
    rows: &[CommitRow],
    row_files: &[Vec<FileStat>],
    names: &[String],
    sets: &[HashSet<String>],
    equiv: &[HashSet<String>],
    picks: Option<&HashMap<String, String>>,
    cmd: &str,
) -> Result<(), String> {
    let mut out = String::new();
    out.push_str("# git-wt commits\n\n");
    out.push_str(&format!("- Command: `{}`\n", md_cell(cmd)));
    out.push_str(&format!("- Worktrees: {}\n", names.iter()
        .map(|n| format!("`{}`", md_cell(n)))
        .collect::<Vec<_>>()
        .join(", ")));
    out.push_str(&format!("- Commits: {}\n", rows.len()));
    // The glyphs are the whole content of the table; a reader who was not at
    // the terminal has nowhere else to learn them.
    out.push_str("- Legend: `✓` has the commit · `≈` has the same patch under another sha · `·` has neither\n");
    if picks.is_some() {
        out.push_str("- `pick`: the sha that other copy of the patch was committed under\n");
    }
    out.push('\n');

    out.push_str("| commit |");
    if picks.is_some() {
        out.push_str(&format!(" {PICK_HEAD} |"));
    }
    out.push_str(" author | date |");
    for n in names {
        out.push_str(&format!(" {} |", md_cell(n)));
    }
    out.push_str(" subject |\n|---|");
    if picks.is_some() {
        out.push_str("---|");
    }
    out.push_str("---|---|");
    for _ in names {
        out.push_str(":-:|");
    }
    out.push_str("---|\n");

    // The shas the rows print, so a picked sha is one the table itself names.
    let shaw = rows
        .iter()
        .map(|r| r.short.chars().count())
        .max()
        .unwrap_or(0);

    for (i, row) in rows.iter().enumerate() {
        out.push_str(&format!("| `{}` |", md_cell(&row.short)));
        if let Some(p) = picks {
            match p.get(&row.sha) {
                Some(s) => out.push_str(&format!(" `{}` |", md_cell(&abbrev(s, shaw)))),
                None => out.push_str("  |"),
            }
        }
        out.push_str(&format!(
            " {} | {} |",
            md_cell(&row.author),
            md_cell(&row.date)
        ));
        for (set, eq) in sets.iter().zip(equiv) {
            out.push_str(&format!(" {} |", Mark::of(&row.sha, set, eq).glyph()));
        }
        let mut subject = md_cell(&row.text);
        if let Some(file_stats) = row_files.get(i) {
            if !file_stats.is_empty() {
                let mut lines = String::from("<br><br>");
                for f in file_stats {
                    lines.push_str(&format!(
                        "{} {} +{} -{}<br>",
                        f.status,
                        md_cell(&f.path),
                        f.added.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string()),
                        f.removed.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string()),
                    ));
                }
                subject.push_str(&lines);
            }
        }
        out.push_str(&format!(" {} |\n", subject));
    }

    std::fs::write(path, out).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    eprintln!("Wrote {} ({} commits)", path.display(), rows.len());
    Ok(())
}

/// Split `s` at the last column that fits `max`, preferring a word boundary.
///
/// The tail keeps no leading space: it starts a line of its own, where a space
/// would push the text a column out of the subject column it is indented to.
fn split_at_width(s: &str, max: usize) -> (&str, &str) {
    let mut used = 0;
    let mut end = s.len();
    for (i, c) in s.char_indices() {
        let w = if c.is_ascii() { 1 } else { 2 };
        if used + w > max {
            end = i;
            break;
        }
        used += w;
    }
    // A word longer than the whole budget has no boundary to break at -- a sha
    // or a URL, usually -- so it is cut mid-word rather than left to overflow.
    match s[..end].rfind(' ').filter(|b| *b > 0) {
        Some(b) => (&s[..b], s[b + 1..].trim_start()),
        None => (&s[..end], s[end..].trim_start()),
    }
}

/// Break `s` into at most `lines` lines of `max` columns by `width_bound`.
///
/// Only the last line an allowance permits is ellipsized, and only when the
/// subject outruns it: the ellipsis means "there was more", so a line that
/// wrapped must not wear one.
fn wrap_wide(s: &str, max: usize, lines: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    loop {
        if width_bound(rest) <= max {
            out.push(rest.to_string());
            return out;
        }
        // The last line this allowance buys: what is left has to fit in it.
        if out.len() + 1 >= lines {
            out.push(ellipsize_wide(rest, max));
            return out;
        }
        let (head, tail) = split_at_width(rest, max);
        // No progress means no boundary and no room -- a budget under one
        // character's width. Cut the loop rather than spin on it.
        if head.is_empty() {
            out.push(ellipsize_wide(rest, max));
            return out;
        }
        out.push(head.to_string());
        rest = tail;
    }
}

/// Print the table: sha, author, date, a mark per branch, then the subject.
///
/// The subject comes last because it is the only cell holding arbitrary text.
/// Padding a cell means knowing its rendered width, and an emoji subject is
/// wider than its `chars().count()` -- so a padded subject column shifts every
/// column after it, which is precisely the table failing to line up. Last, it
/// is never padded, and no width table is needed to keep the marks straight.
///
/// Widths are measured on the plain text and color applied after, so the ANSI
/// escapes never skew the columns either.
fn render_commits(
    rows: &[CommitRow],
    row_files: &[Vec<FileStat>],
    names: &[String],
    sets: &[HashSet<String>],
    equiv: &[HashSet<String>],
    picks: Option<&HashMap<String, String>>,
    color: bool,
    width: Option<usize>,
    wrap: Wrap,
    subjectw: Option<SubjectWidth>,
) {
    let widths: Vec<usize> = names.iter().map(|n| n.chars().count().max(1)).collect();
    let marksw: usize = widths.iter().map(|w| w + 2).sum();

    let shaw = rows
        .iter()
        .map(|r| r.short.chars().count())
        .chain(std::iter::once("commit".len()))
        .max()
        .unwrap_or(0);

    // A picked sha is abbreviated to the same length the rows' own shas are, so
    // the two columns read as the one kind of thing they are -- and so a sha
    // named here is a sha you can find in the commit column of another row.
    let pickw = picks.map(|_| shaw.max(PICK_HEAD.len()));
    let pickcol = pickw.map_or(0, |w| w + 2);

    // The author column is sized to its longest name, but a name is not worth
    // unbounded width when the subject is competing for the same line; on a
    // terminal it caps, and a piped table keeps every name whole.
    let mut authw = rows
        .iter()
        .map(|r| r.author.chars().count())
        .chain(std::iter::once("author".len()))
        .max()
        .unwrap_or(0);
    if width.is_some() {
        authw = authw.min(AUTHOR_MAX);
    }

    // The date is never cut: half a date is not a date. It is ASCII and a fixed
    // shape, so it costs the same on every row.
    let datew = rows
        .iter()
        .map(|r| r.date.chars().count())
        .chain(std::iter::once("date".len()))
        .max()
        .unwrap_or(0);

    // Everything left of the subject, which is both what the subject has to
    // fit beside and what a wrapped line is indented past to line up under it.
    let fixed = shaw + 2 + pickcol + authw + 2 + datew + marksw + 2;

    // What the subject gets. A width asked for is the width, terminal or not:
    // an explicit one is an answer, where the terminal's is only a default --
    // so '--subject-width 100' on an 80-column terminal runs the line past the
    // edge on purpose, and off a terminal it cuts where nothing was cut before.
    let textw = match subjectw {
        Some(SubjectWidth::Cols(n)) => Some(n),
        Some(SubjectWidth::Full) => None,
        // Only the tail is budgeted, and only to keep a long subject from
        // wrapping where it was not asked to; piped output has no terminal to
        // fit, so it is never cut and never wrapped.
        None => width.map(|w| w.saturating_sub(fixed).max(MIN_TEXTW)),
    };

    let rows: Vec<(CommitRow, Vec<String>)> = rows
        .iter()
        .map(|r| {
            let text = match textw {
                Some(tw) => wrap_wide(&r.text, tw, wrap.lines()),
                None => vec![r.text.clone()],
            };
            let row = CommitRow {
                sha: r.sha.clone(),
                short: r.short.clone(),
                author: ellipsize(&r.author, authw),
                date: r.date.clone(),
                key: r.key.clone(),
                text: r.text.clone(),
            };
            (row, text)
        })
        .collect();
    let rows = &rows;

    // The date is right-aligned so the years line up under --date-human, where
    // an unpadded day makes 'Jan. 1, 2026' a character shorter than
    // 'Sep. 15, 2026'; left-aligned, that ragged edge is the first thing you
    // see. ISO is one width, so the alignment is moot there -- and free.
    let mut head = format!("{:<shaw$}  ", "commit");
    if let Some(w) = pickw {
        head.push_str(&format!("{PICK_HEAD:<w$}  "));
    }
    head.push_str(&format!("{:<authw$}  {:>datew$}", "author", "date"));
    for (n, w) in names.iter().zip(&widths) {
        head.push_str("  ");
        head.push_str(&format!("{n:<w$}"));
    }
    head.push_str("  subject");
    println!("{}", paint(&head, DIM, color));

    for (i, (row, text)) in rows.iter().enumerate() {
        let mut line = format!("{:<shaw$}  ", row.short);
        if let Some(w) = pickw {
            // Blank, not '·': the column names a sha or it has nothing to say,
            // where the marks' '·' is an answer about a branch.
            let cell = picks
                .and_then(|p| p.get(&row.sha))
                .map(|s| abbrev(s, shaw))
                .unwrap_or_default();
            // Yellow, like the '≈' it explains.
            line.push_str(&paint(&format!("{cell:<w$}"), YELLOW, color));
            line.push_str("  ");
        }
        // Dim, so the marks and the subject stay what the eye lands on.
        let meta = format!("{:<authw$}  {:>datew$}", row.author, row.date);
        line.push_str(&paint(&meta, DIM, color));
        for ((set, eq), w) in sets.iter().zip(equiv).zip(&widths) {
            let mark = Mark::of(&row.sha, set, eq);
            // Center the one-cell mark under its header.
            let pad = (w - 1) / 2;
            line.push_str("  ");
            line.push_str(&" ".repeat(pad));
            line.push_str(&paint(mark.glyph(), mark.color(), color));
            line.push_str(&" ".repeat(w - 1 - pad));
        }
        line.push_str("  ");
        line.push_str(&text[0]);
        println!("{}", line.trim_end());
        // The rest of a wrapped subject, indented to the column it belongs to:
        // the row is still one commit, and the marks stay the leftmost thing
        // the eye has to scan.
        for more in &text[1..] {
            println!("{}{}", " ".repeat(fixed), more.trim_end());
        }

        // File block, tab-indented under the commit row. Kept dim so the commit
        // rows remain the primary scan target.
        if let Some(file_stats) = row_files.get(i) {
            if !file_stats.is_empty() {
                let pathw = file_stats
                    .iter()
                    .map(|f| f.path.chars().count())
                    .max()
                    .unwrap_or(0);
                let added_strs: Vec<String> = file_stats
                    .iter()
                    .map(|f| {
                        f.added
                            .map(|n| format!("+{}", n))
                            .unwrap_or_else(|| "-".to_string())
                    })
                    .collect();
                let removed_strs: Vec<String> = file_stats
                    .iter()
                    .map(|f| {
                        f.removed
                            .map(|n| format!("-{}", n))
                            .unwrap_or_else(|| "-".to_string())
                    })
                    .collect();
                let addw = added_strs
                    .iter()
                    .map(|s| width_bound(s))
                    .max()
                    .unwrap_or(1);
                let remw = removed_strs
                    .iter()
                    .map(|s| width_bound(s))
                    .max()
                    .unwrap_or(1);
                println!();
                for (f, (add_s, rem_s)) in file_stats
                    .iter()
                    .zip(added_strs.iter().zip(removed_strs.iter()))
                {
                    let file_line = format!(
                        "\t{}  {:<pathw$}  {:>addw$}  {:>remw$}",
                        f.status, f.path, add_s, rem_s
                    );
                    println!("{}", paint(&file_line, DIM, color));
                }
                println!();
            }
        }
    }
}

/// Is `cmd` an executable file on PATH? Checked before spawning so a missing
/// tool is a clear error rather than an opaque NotFound from the OS.
fn on_path(cmd: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let p = dir.join(cmd);
        p.is_file() && is_executable(&p)
    })
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(_p: &Path) -> bool {
    true
}

// ---------------------------------------------------------------------------
// Create: git-wt add [BRANCH] [flags]
// ---------------------------------------------------------------------------

fn cmd_add(root: &Path, args: &[String]) -> Result<(), String> {
    let mut branch: Option<String> = None;
    let mut name: Option<String> = None;
    let mut dirname: Option<String> = None;
    let mut parentdir: Option<String> = None;
    let mut from: Option<String> = None;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-n" | "--name" => {
                name = Some(it.next().ok_or("--name needs a name")?.clone());
            }
            "--dirname" => {
                dirname = Some(it.next().ok_or("--dirname needs a directory")?.clone());
            }
            "-p" | "--parentdir" => {
                parentdir = Some(it.next().ok_or("--parentdir needs a directory")?.clone());
            }
            "--from" => {
                from = Some(it.next().ok_or("--from needs a ref")?.clone());
            }
            s if s.starts_with("--name=") => name = Some(s["--name=".len()..].to_string()),
            s if s.starts_with("--dirname=") => {
                dirname = Some(s["--dirname=".len()..].to_string())
            }
            s if s.starts_with("--parentdir=") => {
                parentdir = Some(s["--parentdir=".len()..].to_string())
            }
            s if s.starts_with("--from=") => from = Some(s["--from=".len()..].to_string()),
            // A hint for the `wt` wrapper (stay put instead of cd'ing into the
            // new worktree). The binary never cd's, so it just accepts it.
            "--stay" => {}
            s if s.starts_with('-') && s != "-" => {
                return Err(format!("unknown option '{s}'\nTry 'git-wt --help'"));
            }
            s => {
                if branch.is_some() {
                    return Err("too many arguments\nTry 'git-wt --help'".into());
                }
                branch = Some(s.to_string());
            }
        }
    }

    if name.is_some() && dirname.is_some() {
        return Err("--name and --dirname conflict".into());
    }
    if let Some(n) = &name {
        if n.is_empty() {
            return Err("--name cannot be empty".into());
        }
    }
    if let Some(d) = &dirname {
        if d.is_empty() {
            return Err("--dirname cannot be empty".into());
        }
    }

    // No branch -> interactive picker over local branches.
    let branch = match branch {
        Some(b) => b,
        None => pick_branch(root)?,
    };

    let dir = match resolve_add_path(
        root,
        &branch,
        name.as_deref(),
        dirname.as_deref(),
        parentdir.as_deref(),
    )? {
        Some(d) => d,
        None => {
            eprintln!("Aborted.");
            return Ok(());
        }
    };

    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }

    // Refuse to point a new worktree at a branch already checked out; git
    // shares one ref between worktrees, so the two HEADs would drift.
    if let Some(w) = worktrees(root)?
        .into_iter()
        .find(|w| w.branch.as_deref() == Some(branch.as_str()))
    {
        return Err(format!(
            "branch '{branch}' already checked out at {}",
            w.path.display()
        ));
    }

    let has_local = git_quiet(root, &["show-ref", "--verify", &format!("refs/heads/{branch}")]);
    let remote = find_remote_branch(root, &branch);

    // --from only affects creating a NEW branch; if the branch already exists
    // it is silently overridden, so warn + confirm.
    if from.is_some() && (has_local || remote.is_some()) {
        if !confirm(&format!(
            "branch '{branch}' already exists; --from ignored. Continue? [y/N] "
        ))? {
            eprintln!("Aborted.");
            return Ok(());
        }
    }

    // Default base for a NEW branch is the ref checked out where the user is
    // standing (the current worktree), not the primary's HEAD. `--from` wins.
    let default_from = current_ref();
    let from_ref = from.as_deref().unwrap_or(&default_from);
    let dir_s = dir.to_string_lossy().to_string();
    let mut argv: Vec<String> = vec!["worktree".into(), "add".into()];

    if has_local {
        eprintln!("Checking out existing local branch '{branch}'");
        argv.push(dir_s.clone());
        argv.push(branch.clone());
    } else if let Some(r) = &remote {
        eprintln!("Tracking remote branch '{r}/{branch}'");
        argv.extend(["--track".into(), "-b".into(), branch.clone()]);
        argv.push(dir_s.clone());
        argv.push(format!("{r}/{branch}"));
    } else {
        if !confirm(&format!(
            "Branch '{branch}' does not exist. Create it from '{from_ref}'? [y/N] "
        ))? {
            eprintln!("Aborted.");
            return Ok(());
        }
        eprintln!("Creating new branch '{branch}' from '{from_ref}'");
        argv.extend(["-b".into(), branch.clone()]);
        argv.push(dir_s.clone());
        argv.push(from_ref.into());
    }

    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    git_run(root, &refs)?;

    // One-line summary on stderr (never stdout) so interactive users get
    // context without polluting the captured path.
    let summary = if has_local {
        format!("branch {branch}")
    } else if let Some(r) = &remote {
        format!("branch {branch} tracking {r}/{branch}")
    } else {
        format!("branch {branch} from {from_ref}")
    };
    let leaf = leaf_of(&dir);
    let on = color_enabled(std::io::stderr().is_terminal());
    eprintln!("{} {leaf}  ({summary})", paint("Created", GREEN, on));

    // Print the new worktree path on stdout (alone) so scripts can capture it:
    // `dir=$(git-wt add feat/x)`. Status/progress went to stderr.
    println!("{dir_s}");
    Ok(())
}

/// Find a remote whose tracking ref `<remote>/<branch>` exists, so `add`
/// works with any remote name (not just `origin`). Prefers `origin`; otherwise
/// the first configured remote that has the branch.
fn find_remote_branch(root: &Path, branch: &str) -> Option<String> {
    let has = |r: &str| {
        git_quiet(
            root,
            &["show-ref", "--verify", &format!("refs/remotes/{r}/{branch}")],
        )
    };
    if has("origin") {
        return Some("origin".into());
    }
    git_stdout(root, &["remote"])
        .ok()?
        .lines()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .find(|r| has(r))
        .map(String::from)
}

/// Resolve the worktree directory for `add`. Returns `Ok(None)` when the user
/// declines a warn-and-confirm (an override), which the caller treats as an
/// abort rather than an error.
fn resolve_add_path(
    root: &Path,
    branch: &str,
    name: Option<&str>,
    dirname: Option<&str>,
    parentdir: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    let repo = root
        .file_name()
        .ok_or("cannot determine repo folder name")?
        .to_string_lossy()
        .to_string();
    let default_parent = root.parent().ok_or("repo root has no parent directory")?;

    // --dirname with a '/' is a path: sanitize skipped, -p ignored.
    if let Some(d) = dirname {
        if d.contains('/') {
            if parentdir.is_some()
                && !confirm(
                    "--parentdir ignored because --dirname is a path. Continue? [y/N] ",
                )?
            {
                return Ok(None);
            }
            let p = Path::new(d);
            if p.is_absolute() {
                return Ok(Some(p.to_path_buf()));
            }
            return Ok(Some(default_parent.join(p)));
        }
    }

    let parent = match parentdir {
        Some(p) => PathBuf::from(p),
        None => default_parent.to_path_buf(),
    };
    let leaf = match (name, dirname) {
        (Some(n), _) => format!("{repo}-{}", sanitize(n)),
        (_, Some(d)) => sanitize(d),
        _ => format!("{repo}-{}", sanitize(branch)),
    };
    Ok(Some(parent.join(leaf)))
}

// ---------------------------------------------------------------------------
// Branch picker (no BRANCH given to `add`)
// ---------------------------------------------------------------------------

/// Choose a local branch interactively. Prefers fzf's search filter; falls
/// back to a numbered prompt when fzf is not installed.
///
/// Branches already checked out in a worktree can't be added again, so they are
/// dropped from the selectable list and shown separately, for reference.
fn pick_branch(root: &Path) -> Result<String, String> {
    // Sort recently-committed branches to the top so the picker surfaces what
    // you're likely reaching for. Fetch each branch's relative age in the same
    // call (tab-delimited) so the numbered picker needs no per-branch git log.
    let out = git_stdout(
        root,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname:short)%09%(committerdate:relative)",
            "refs/heads",
        ],
    )?;
    // Each line is "<branch>\t<age>"; a missing tab leaves the age empty.
    let branches: Vec<(&str, &str)> = out
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.split_once('\t').unwrap_or((l, "")))
        .collect();
    if branches.is_empty() {
        return Err("no local branches to choose from".into());
    }

    let trees = worktrees(root)?;
    let mut selectable: Vec<(&str, &str)> = Vec::new();
    let mut checked_out: Vec<(&str, &Path)> = Vec::new();
    for (b, age) in &branches {
        match trees.iter().find(|w| w.branch.as_deref() == Some(*b)) {
            Some(w) => checked_out.push((*b, w.path.as_path())),
            None => selectable.push((*b, *age)),
        }
    }

    if !checked_out.is_empty() {
        eprintln!("Already checked out (not selectable):");
        let w = checked_out
            .iter()
            .map(|(b, _)| b.chars().count())
            .max()
            .unwrap_or(0);
        for (b, p) in &checked_out {
            eprintln!("  {:<w$}  {}", b, p.display(), w = w);
        }
        // Separator between the reference list and the selectable choices.
        eprintln!("{}", "─".repeat(48));
    }

    if selectable.is_empty() {
        // Every local branch is checked out; rather than dead-end, offer to
        // create a new branch by name (cmd_add then confirms the base ref).
        return new_branch_prompt();
    }

    let names: Vec<&str> = selectable.iter().map(|(b, _)| *b).collect();
    if let Some(sel) = fzf_pick(root, &names)? {
        return Ok(sel);
    }
    number_pick(&selectable)
}

/// Empty-state fallback: no branch is available to check out, so read a new
/// branch name to create. Empty input / EOF cancels.
fn new_branch_prompt() -> Result<String, String> {
    eprintln!("All local branches are already checked out.");
    eprint!("Enter a new branch name to create (Enter to cancel): ");
    std::io::stderr().flush().ok();
    let mut line = String::new();
    let n = std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("no branch selected".into());
    }
    let name = line.trim();
    if name.is_empty() {
        return Err("no branch selected".into());
    }
    Ok(name.to_string())
}

/// Run fzf over `items`. Returns Ok(None) when fzf is not on PATH so the caller
/// can fall back; an empty/aborted selection is an error.
fn fzf_pick(root: &Path, items: &[&str]) -> Result<Option<String>, String> {
    // Preview the highlighted branch's last commit. fzf shell-quotes {} before
    // substitution, and root is quoted here, so both are safe in `sh -c`.
    let preview = format!(
        "git -C {} log -1 --format='%h  %s%n%an · %ar' {{}} --",
        sh_quote(root)
    );
    let mut child = match Command::new("fzf")
        .args([
            "--prompt",
            "branch> ",
            "--height",
            "40%",
            "--reverse",
            "--preview",
            &preview,
            "--preview-window",
            "down,3,wrap",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return Ok(None), // fzf not available
    };

    child
        .stdin
        .as_mut()
        .ok_or("fzf: no stdin")?
        .write_all(items.join("\n").as_bytes())
        .map_err(|e| e.to_string())?;

    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("no branch selected".into()); // ESC / Ctrl-C in fzf
    }
    let sel = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if sel.is_empty() {
        return Err("no branch selected".into());
    }
    Ok(Some(sel))
}

/// Numbered fallback picker; reads a number from stdin. Each branch is shown
/// with the relative age of its last commit (dimmed on a terminal) for context.
/// `items` are `(branch, age)` pairs already gathered by `pick_branch`.
fn number_pick(items: &[(&str, &str)]) -> Result<String, String> {
    let color = color_enabled(std::io::stderr().is_terminal());
    eprintln!("Available branches (most recent first):");
    let w = items.len().to_string().len();
    let bw = items.iter().map(|(b, _)| b.chars().count()).max().unwrap_or(0);
    for (i, (b, age)) in items.iter().enumerate() {
        let meta = paint(age, DIM, color && !age.is_empty());
        eprintln!("  {:>w$}  {:<bw$}  {}", i + 1, b, meta, w = w, bw = bw);
    }
    eprint!("Select a branch [1-{}], or Enter to cancel: ", items.len());
    std::io::stderr().flush().ok();

    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let t = line.trim();
    if t.is_empty() {
        return Err("no branch selected".into());
    }
    let n: usize = t.parse().map_err(|_| format!("'{t}' is not a number"))?;
    if n == 0 || n > items.len() {
        return Err(format!("no branch #{n}"));
    }
    Ok(items[n - 1].0.to_string())
}

// ---------------------------------------------------------------------------
// Prompt
// ---------------------------------------------------------------------------

/// Print a prompt to stderr and read a yes/no answer from stdin. Requires the
/// user to type and press Enter; empty or anything but y/yes is No. EOF / no
/// tty is No.
fn confirm(prompt: &str) -> Result<bool, String> {
    eprint!("{prompt}");
    std::io::stderr().flush().ok();
    let mut line = String::new();
    let n = std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Ok(false); // EOF / no tty -> treat as No
    }
    let a = line.trim().to_ascii_lowercase();
    Ok(a == "y" || a == "yes")
}

// ---------------------------------------------------------------------------
// Paths and naming
// ---------------------------------------------------------------------------

/// Collapse path-hostile characters to single dashes; trim leading/trailing.
fn sanitize(branch: &str) -> String {
    let mut out = String::with_capacity(branch.len());
    let mut last_dash = false;
    for c in branch.chars() {
        let c = if matches!(c, '/' | ' ' | ':' | '\\') { '-' } else { c };
        if c == '-' {
            if !last_dash {
                out.push('-');
            }
            last_dash = true;
        } else {
            out.push(c);
            last_dash = false;
        }
    }
    out.trim_matches('-').to_string()
}

/// Single-quote a path for safe interpolation into an `sh -c` command line
/// (used to build fzf's --preview). Embedded quotes are escaped `'\''`.
fn sh_quote(p: &Path) -> String {
    format!("'{}'", p.to_string_lossy().replace('\'', "'\\''"))
}

/// Canonical path for comparison; falls back to the input when it can't be
/// resolved (e.g. it no longer exists), so equal paths still compare equal.
fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Last path component (directory leaf) as a display string, or the whole path
/// when it has none.
fn leaf_of(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| p.display().to_string())
}

fn label(w: &Worktree) -> String {
    if w.bare {
        "(bare)".into()
    } else if w.detached {
        "(detached)".into()
    } else {
        w.branch.clone().unwrap_or_else(|| "(unknown)".into())
    }
}

// ---------------------------------------------------------------------------
// git plumbing
// ---------------------------------------------------------------------------

/// Absolute path to the main worktree root, even when invoked from a
/// subdirectory or from inside a linked worktree.
fn repo_root() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let common = git_stdout(&cwd, &["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .map_err(|_| "not inside a git repository".to_string())?;

    let common = PathBuf::from(common.trim());
    // `.../repo/.git` -> `.../repo`; a bare repo has no `.git` component.
    let root = if common.file_name().map(|n| n == ".git").unwrap_or(false) {
        common.parent().ok_or("malformed git dir")?.to_path_buf()
    } else {
        common
    };
    Ok(root)
}

/// The ref checked out in the current directory's worktree: the branch name,
/// or a short commit sha when detached. Falls back to "HEAD" if git can't say.
fn current_ref() -> String {
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(_) => return "HEAD".into(),
    };
    if let Ok(b) = git_stdout(&cwd, &["symbolic-ref", "--short", "-q", "HEAD"]) {
        let b = b.trim();
        if !b.is_empty() {
            return b.to_string();
        }
    }
    // Detached HEAD: use the short commit sha.
    if let Ok(sha) = git_stdout(&cwd, &["rev-parse", "--short", "HEAD"]) {
        let sha = sha.trim();
        if !sha.is_empty() {
            return sha.to_string();
        }
    }
    "HEAD".into()
}

fn worktrees(root: &Path) -> Result<Vec<Worktree>, String> {
    let out = git_stdout(root, &["worktree", "list", "--porcelain"])?;
    let mut trees = Vec::new();
    let mut cur: Option<Worktree> = None;

    for line in out.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            if let Some(w) = cur.take() {
                trees.push(w);
            }
            cur = Some(Worktree {
                path: PathBuf::from(p),
                branch: None,
                detached: false,
                bare: false,
            });
        } else if let Some(w) = cur.as_mut() {
            if let Some(b) = line.strip_prefix("branch ") {
                w.branch = Some(b.strip_prefix("refs/heads/").unwrap_or(b).to_string());
            } else if line == "detached" {
                w.detached = true;
            } else if line == "bare" {
                w.bare = true;
            }
        }
    }
    if let Some(w) = cur {
        trees.push(w);
    }
    Ok(trees)
}

fn git_cmd(dir: &Path, args: &[&str]) -> Command {
    let mut c = Command::new("git");
    c.current_dir(dir).args(args);
    c
}

/// Run git, streaming its output through. Errors carry git's stderr.
fn git_run(dir: &Path, args: &[&str]) -> Result<(), String> {
    let out = git_cmd(dir, args)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    // git's own progress text belongs on stderr, not in our stdout contract.
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        eprintln!("{line}");
    }

    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Run git with the editor disabled. We capture git's output, so a spawned
/// editor would have no terminal and hang; instead git's default commit message
/// is taken as-is (`-m` is how a user overrides it).
fn git_run_no_editor(dir: &Path, args: &[&str]) -> Result<(), String> {
    let out = git_cmd(dir, args)
        .env("GIT_EDITOR", "true")
        .env("GIT_MERGE_AUTOEDIT", "no")
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        eprintln!("{line}");
    }

    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn git_stdout(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = git_cmd(dir, args)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// True when git exits 0. Used for ref existence checks.
fn git_quiet(dir: &Path, args: &[&str]) -> bool {
    git_cmd(dir, args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default spelling: what `commits` prints without a format flag.
    const ISO: DateFmt = DateFmt { human: false, time: false };

    #[test]
    fn sanitize_collapses_separators() {
        assert_eq!(sanitize("feature/login"), "feature-login");
        assert_eq!(sanitize("a/b/c/d"), "a-b-c-d");
        assert_eq!(sanitize("feat//x"), "feat-x");
        assert_eq!(sanitize("has space"), "has-space");
        assert_eq!(sanitize("/leading/"), "leading");
        assert_eq!(sanitize("release-3.2.1"), "release-3.2.1");
    }

    #[test]
    fn add_path_default_is_sibling() {
        let p = resolve_add_path(Path::new("/code/myapp"), "feat/x", None, None, None)
            .unwrap()
            .unwrap();
        assert_eq!(p, PathBuf::from("/code/myapp-feat-x"));
    }

    #[test]
    fn add_path_name_is_suffix() {
        let p = resolve_add_path(Path::new("/code/myapp"), "feat/x", Some("test"), None, None)
            .unwrap()
            .unwrap();
        assert_eq!(p, PathBuf::from("/code/myapp-test"));
    }

    #[test]
    fn add_path_dirname_is_whole_leaf() {
        let p = resolve_add_path(Path::new("/code/myapp"), "feat/x", None, Some("test"), None)
            .unwrap()
            .unwrap();
        assert_eq!(p, PathBuf::from("/code/test"));
    }

    #[test]
    fn add_path_parentdir_overrides() {
        let p = resolve_add_path(Path::new("/code/myapp"), "feat/x", None, None, Some("/work"))
            .unwrap()
            .unwrap();
        assert_eq!(p, PathBuf::from("/work/myapp-feat-x"));
    }

    #[test]
    fn add_path_dirname_absolute_is_verbatim() {
        let p =
            resolve_add_path(Path::new("/code/myapp"), "feat/x", None, Some("/tmp/scratch"), None)
                .unwrap()
                .unwrap();
        assert_eq!(p, PathBuf::from("/tmp/scratch"));
    }

    #[test]
    fn add_path_dirname_relative_path_is_parent_relative() {
        let p = resolve_add_path(Path::new("/code/myapp"), "feat/x", None, Some("sub/test"), None)
            .unwrap()
            .unwrap();
        assert_eq!(p, PathBuf::from("/code/sub/test"));
    }

    #[test]
    fn subseq_matches_in_order() {
        assert!(is_subseq("feature-login", "flogin"));
        assert!(is_subseq("feature-login", "feat"));
        assert!(!is_subseq("feature-login", "zzz"));
        assert!(!is_subseq("abc", "cba"));
    }

    #[test]
    fn branch_like_detection() {
        assert!(branch_like("feat/x"));
        assert!(branch_like("feat-x"));
        assert!(!branch_like("lsit"));
        assert!(!branch_like("foo bar"));
    }

    #[test]
    fn check_index_bounds() {
        assert_eq!(check_index(1, 3), Ok(0));
        assert_eq!(check_index(3, 3), Ok(2));
        assert_eq!(check_index(0, 3), Err("no worktree #0".into()));
        assert_eq!(
            check_index(4, 3),
            Err("no worktree #4; there are 3 (see 'git-wt list')".into())
        );
    }

    #[test]
    fn classify_status_reads_porcelain() {
        assert_eq!(classify_status(""), Status::Clean);
        assert_eq!(classify_status("   \n"), Status::Clean);
        assert_eq!(classify_status(" M src/main.rs"), Status::Dirty);
        assert_eq!(classify_status("?? new.txt"), Status::Untracked);
        // Untracked wins when both are present.
        assert_eq!(classify_status(" M a\n?? b"), Status::Untracked);
    }

    #[test]
    fn paint_wraps_only_when_on() {
        assert_eq!(paint("x", GREEN, false), "x");
        assert_eq!(paint("x", GREEN, true), "\x1b[32mx\x1b[0m");
    }

    #[test]
    fn parse_cols_accepts_status_last_and_merged() {
        assert_eq!(parse_cols("1,4,5").unwrap(), vec![1, 4, 5]);
        assert_eq!(parse_cols("1,2,6").unwrap(), vec![1, 2, 6]);
        assert_eq!(parse_cols("1,7,8").unwrap(), vec![1, 7, 8]);
        assert!(parse_cols("9").is_err());
    }

    #[test]
    fn col_header_uses_last_commit_name() {
        assert_eq!(col_header(5), "last-commit");
        assert_eq!(col_header(7), "merged");
        assert_eq!(col_header(8), "merged-at");
    }

    #[test]
    fn sh_quote_wraps_and_escapes() {
        assert_eq!(sh_quote(Path::new("/code/my app")), "'/code/my app'");
        assert_eq!(sh_quote(Path::new("/a'b")), "'/a'\\''b'");
    }

    #[test]
    fn leaf_of_returns_last_component() {
        assert_eq!(leaf_of(Path::new("/code/myapp-feat-x")), "myapp-feat-x");
        assert_eq!(leaf_of(Path::new("myapp")), "myapp");
    }

    #[test]
    fn render_row_pads_and_tints() {
        let cols = vec![1, 2];
        let row = vec!["1".to_string(), "main".to_string()];
        let widths = vec![1, 7];
        // No color: branch is left-padded to width, no ANSI.
        let plain = render_row(&row, &cols, &widths, Status::Clean, false);
        assert_eq!(plain, "1  main");
        // Color: branch cell tinted green (padding inside the escape).
        let tinted = render_row(&row, &cols, &widths, Status::Clean, true);
        assert_eq!(tinted, "1  \x1b[32mmain\x1b[0m");
    }

    fn merge_args(args: &[&str]) -> Result<MergeArgs, String> {
        let v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse_merge_args(&v)
    }

    fn sync_args(op: SyncOp, args: &[&str]) -> Result<SyncArgs, String> {
        let v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse_sync_args(op, &v)
    }

    #[test]
    fn sync_words_are_exact() {
        assert_eq!(SyncOp::from_word("fetch"), Some(SyncOp::Fetch));
        assert_eq!(SyncOp::from_word("pull"), Some(SyncOp::Pull));
        assert_eq!(SyncOp::from_word("push"), Some(SyncOp::Push));
        // An abbreviation would shadow a branch of the same name.
        assert_eq!(SyncOp::from_word("pu"), None);
        assert_eq!(SyncOp::from_word("--pull"), None);
    }

    #[test]
    fn sync_bare_verb_takes_no_flags() {
        let a = sync_args(SyncOp::Pull, &[]).unwrap();
        assert!(!a.all);
        assert!(a.flags.is_empty());
    }

    #[test]
    fn sync_all_is_worktrees_not_remotes() {
        assert!(sync_args(SyncOp::Fetch, &["--all"]).unwrap().all);
        assert!(sync_args(SyncOp::Push, &["-a"]).unwrap().all);
        // `--all` is ours, so it never reaches git as `fetch --all` (every remote).
        assert!(sync_args(SyncOp::Fetch, &["--all"]).unwrap().flags.is_empty());
    }

    #[test]
    fn sync_shorts_canonicalize() {
        assert_eq!(sync_args(SyncOp::Push, &["-u"]).unwrap().flags, ["--set-upstream"]);
        assert_eq!(sync_args(SyncOp::Push, &["-n"]).unwrap().flags, ["--dry-run"]);
        assert_eq!(sync_args(SyncOp::Fetch, &["-p"]).unwrap().flags, ["--prune"]);
        assert_eq!(sync_args(SyncOp::Pull, &["-p"]).unwrap().flags, ["--prune"]);
    }

    #[test]
    fn sync_flags_are_per_verb() {
        assert!(sync_args(SyncOp::Pull, &["--rebase"]).is_ok());
        assert!(sync_args(SyncOp::Push, &["--rebase"]).is_err());
        assert!(sync_args(SyncOp::Fetch, &["--rebase"]).is_err());
        assert!(sync_args(SyncOp::Push, &["--set-upstream"]).is_ok());
        assert!(sync_args(SyncOp::Pull, &["--set-upstream"]).is_err());
        // -p is prune for fetch/pull; push has no -p at all.
        assert!(sync_args(SyncOp::Push, &["-p"]).is_err());
    }

    #[test]
    fn sync_unknown_flag_is_not_a_passthrough() {
        let e = sync_args(SyncOp::Pull, &["--depth=1"]).unwrap_err();
        assert!(e.contains("unknown option '--depth=1' for pull"));
        // The error hands back the command that would work.
        assert!(e.contains("git -C <dir> pull --depth=1"));
    }

    #[test]
    fn sync_push_force_is_refused() {
        for f in ["--force", "-f"] {
            let e = sync_args(SyncOp::Push, &[f]).unwrap_err();
            assert!(e.contains("no '--force' for push"));
            assert!(e.contains("--force-with-lease"));
        }
        assert!(sync_args(SyncOp::Push, &["--force-with-lease"]).is_ok());
        // fetch --force only refreshes a ref that moved; it overwrites no remote.
        assert!(sync_args(SyncOp::Fetch, &["--force"]).is_ok());
    }

    #[test]
    fn sync_contradictions_are_typos() {
        assert!(sync_args(SyncOp::Pull, &["--rebase", "--no-rebase"]).is_err());
        assert!(sync_args(SyncOp::Pull, &["--rebase", "--ff-only"]).is_err());
        assert!(sync_args(SyncOp::Fetch, &["--tags", "--no-tags"]).is_err());
        assert!(sync_args(SyncOp::Pull, &["--rebase", "--autostash"]).is_ok());
    }

    #[test]
    fn sync_repeated_flag_is_passed_once() {
        let a = sync_args(SyncOp::Fetch, &["--prune", "-p", "--prune"]).unwrap();
        assert_eq!(a.flags, ["--prune"]);
    }

    #[test]
    fn sync_skips_what_the_verb_cannot_mean() {
        let bare = Worktree {
            path: PathBuf::from("/code/myapp.git"),
            branch: None,
            detached: false,
            bare: true,
        };
        let detached = Worktree {
            path: PathBuf::from("/code/myapp-x"),
            branch: None,
            detached: true,
            bare: false,
        };
        let normal = Worktree {
            path: PathBuf::from("/code/myapp"),
            branch: Some("main".into()),
            detached: false,
            bare: false,
        };
        assert_eq!(sync_skip(&bare, SyncOp::Fetch), Some("bare"));
        assert_eq!(sync_skip(&bare, SyncOp::Push), Some("bare"));
        // fetch only moves remote-tracking refs, so a detached HEAD is fine.
        assert_eq!(sync_skip(&detached, SyncOp::Fetch), None);
        assert!(sync_skip(&detached, SyncOp::Pull).is_some());
        assert!(sync_skip(&detached, SyncOp::Push).is_some());
        for op in [SyncOp::Fetch, SyncOp::Pull, SyncOp::Push] {
            assert_eq!(sync_skip(&normal, op), None);
        }
    }

    #[test]
    fn tracked_changes_ignore_untracked_only() {
        assert!(!has_tracked_changes(""));
        assert!(!has_tracked_changes("?? new.txt"));
        assert!(!has_tracked_changes("?? a\n?? b"));
        assert!(has_tracked_changes(" M src/main.rs"));
        assert!(has_tracked_changes("A  staged.rs"));
        // The case classify_status collapses to Untracked: tracked edits are
        // still present, so a merge here needs -f.
        assert!(has_tracked_changes("?? new.txt\n M src/main.rs"));
        assert!(has_tracked_changes(" M src/main.rs\n?? new.txt"));
        assert_eq!(classify_status(" M a\n?? b"), Status::Untracked); // why not classify_status
    }

    #[test]
    fn merge_parses_source_and_options() {
        let a = merge_args(&["2"]).unwrap();
        assert_eq!(a.op, MergeOp::Start("2".into()));
        assert!(!a.no_ff && !a.squash && !a.force && a.message.is_none());

        let a = merge_args(&["feat/x", "--no-ff", "-m", "sync", "-f"]).unwrap();
        assert_eq!(a.op, MergeOp::Start("feat/x".into()));
        assert!(a.no_ff && a.force);
        assert_eq!(a.message.as_deref(), Some("sync"));

        assert_eq!(merge_args(&["2", "--message=hi"]).unwrap().message.as_deref(), Some("hi"));
    }

    #[test]
    fn merge_accepts_bare_and_dashed_resume_words() {
        assert_eq!(merge_args(&["continue"]).unwrap().op, MergeOp::Continue);
        assert_eq!(merge_args(&["--continue"]).unwrap().op, MergeOp::Continue);
        assert_eq!(merge_args(&["abort"]).unwrap().op, MergeOp::Abort);
        assert_eq!(merge_args(&["--abort"]).unwrap().op, MergeOp::Abort);
    }

    /// Every keyword means the same thing bare, dashed, or short.
    #[test]
    fn merge_words_take_optional_dashes_and_shorts() {
        for (bare, dashed, short) in [
            ("continue", "--continue", "-c"),
            ("abort", "--abort", "-a"),
        ] {
            let want = merge_args(&[bare]).unwrap().op;
            assert_eq!(merge_args(&[dashed]).unwrap().op, want, "{dashed}");
            assert_eq!(merge_args(&[short]).unwrap().op, want, "{short}");
        }
        for (bare, dashed, short, want) in [
            ("ours", "--ours", "-o", Side::Ours),
            ("theirs", "--theirs", "-t", Side::Theirs),
        ] {
            for w in [bare, dashed, short] {
                assert_eq!(merge_args(&["2", w]).unwrap().side, Some(want), "{w}");
            }
        }
        for w in ["dry-run", "--dry-run", "-d"] {
            assert!(merge_args(&["2", w]).unwrap().dry_run, "{w}");
        }
    }

    #[test]
    fn merge_side_maps_to_strategy_option() {
        // -X ours / -X theirs, never -s ours: the whole-tree strategy would
        // drop the source's changes and still record a merge.
        assert_eq!(Side::Ours.strategy_option(), "ours");
        assert_eq!(Side::Theirs.strategy_option(), "theirs");
    }

    #[test]
    fn merge_rejects_both_ops_but_allows_repeats() {
        let e = merge_args(&["continue", "abort"]).unwrap_err();
        assert_eq!(e, "continue and abort conflict");
        assert!(merge_args(&["-c", "--abort"]).is_err());
        // Saying the same word twice is redundant, not wrong — same rule as
        // ours/theirs.
        assert_eq!(merge_args(&["continue", "-c"]).unwrap().op, MergeOp::Continue);
    }

    #[test]
    fn merge_rejections_name_the_offending_flag() {
        let e = merge_args(&["abort", "-m", "x", "--squash"]).unwrap_err();
        assert!(e.contains("got -m, --squash"), "{e}");
        let e = merge_args(&["2", "dry-run", "--no-ff", "-f"]).unwrap_err();
        assert!(e.contains("got --no-ff, -f"), "{e}");
    }

    #[test]
    fn merge_rejects_both_sides_but_allows_repeats() {
        assert!(merge_args(&["2", "ours", "theirs"]).is_err());
        assert!(merge_args(&["2", "-o", "--theirs"]).is_err());
        // Saying the same side twice is redundant, not wrong.
        assert_eq!(merge_args(&["2", "ours", "-o"]).unwrap().side, Some(Side::Ours));
    }

    #[test]
    fn merge_resume_rejects_a_side_with_a_pointed_hint() {
        // 'theirs continue' reads as "finish this by taking theirs", which git
        // cannot do — the error has to say so rather than ignore the word.
        let e = merge_args(&["theirs", "continue"]).unwrap_err();
        assert!(e.contains("applied when a merge starts"), "{e}");
        assert!(e.contains("merge abort"), "{e}");
    }

    #[test]
    fn merge_dry_run_rejects_start_only_flags() {
        assert!(merge_args(&["2", "dry-run", "--no-ff"]).is_err());
        assert!(merge_args(&["2", "dry-run", "-m", "x"]).is_err());
        assert!(merge_args(&["2", "dry-run", "-f"]).is_err());
        // --ff-only gates the merge rather than shaping its commit, but a dry
        // run has no merge to gate: merge-tree resolves in memory and never
        // fast-forwards anything, so honoring it is impossible.
        let e = merge_args(&["2", "dry-run", "--ff-only"]).unwrap_err();
        assert!(e.contains("got --ff-only"), "{e}");
        // A side is fine: it changes what the dry run would report.
        assert!(merge_args(&["2", "dry-run", "theirs"]).is_ok());
    }

    #[test]
    fn merge_rejects_bad_combinations() {
        assert!(merge_args(&[]).is_err()); // no source
        assert!(merge_args(&["--continue", "2"]).is_err()); // resume takes no source
        assert!(merge_args(&["--continue", "--no-ff"]).is_err()); // nor options
        assert!(merge_args(&["--continue", "--abort"]).is_err());
        assert!(merge_args(&["2", "--no-ff", "--ff-only"]).is_err());
        assert!(merge_args(&["2", "--squash", "--no-ff"]).is_err());
        assert!(merge_args(&["2", "3"]).is_err()); // too many
        assert!(merge_args(&["2", "--rebase"]).is_err()); // unknown option
        assert!(merge_args(&["-m"]).is_err()); // -m needs a value
    }

    #[test]
    fn unknown_command_messages() {
        assert_eq!(
            unknown_command_msg("show"),
            "unknown command 'show'; use 'git-wt 1 path'"
        );
        assert_eq!(
            unknown_command_msg("remove"),
            "unknown command 'remove'; use 'git-wt 1 remove'"
        );
        assert_eq!(
            unknown_command_msg("merge"),
            "unknown command 'merge'; use 'git-wt 1,2 merge'"
        );
        assert_eq!(
            unknown_command_msg("feat/x"),
            "unknown command 'feat/x'; did you mean 'add feat/x'?"
        );
        assert_eq!(
            unknown_command_msg("merged"),
            "unknown command 'merged'; use 'git-wt 1 merged' or 'git-wt 1,2 merged'"
        );
        assert_eq!(unknown_command_msg("lsit"), "unknown command 'lsit'");
    }

    fn hunk(line: &str) -> (usize, &'static str, usize) {
        let h = parse_hunk_header(line).expect("header should parse");
        (h.line, h.kind, h.count)
    }

    #[test]
    fn omitted_hunk_count_means_one() {
        // '@@ -119 +119 @@' is a one-line change, not a malformed header.
        assert_eq!(hunk("@@ -119 +119 @@"), (119, "modified", 1));
        assert_eq!(parse_range("-119"), Some((119, 1)));
        assert_eq!(parse_range("+42,7"), Some((42, 7)));
    }

    #[test]
    fn zero_hunk_count_is_not_an_edit() {
        // A zero side is a pure insert/delete. Labeling off the new-side
        // number alone would report every deletion as '+0' additions.
        assert_eq!(hunk("@@ -0,0 +290,2 @@"), (290, "added", 2));
        assert_eq!(hunk("@@ -5,3 +4,0 @@"), (4, "deleted", 3));
        assert_eq!(hunk("@@ -119,3 +119,5 @@ fn x() {"), (119, "modified", 5));
    }

    #[test]
    fn patch_counts_skip_the_file_headers() {
        // '--- a/x' / '+++ b/x' are +/- lines to a naive counter.
        let patch = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1,2 @@\n-old\n+new\n+extra\n";
        let mut fd = FileDiff {
            path: "x".into(),
            status: 'M',
            plus: 0,
            minus: 0,
            binary: false,
            hunks: Vec::new(),
        };
        parse_patch_into(patch, &mut fd);
        assert_eq!((fd.plus, fd.minus), (2, 1));
        assert_eq!(fd.hunks.len(), 1);
    }

    #[test]
    fn patch_splits_by_file_and_reads_status_from_dev_null() {
        let patch = "\
diff --git a/add.txt b/add.txt
--- /dev/null
+++ b/add.txt
@@ -0,0 +1 @@
+hi
diff --git a/gone.txt b/gone.txt
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
";
        let files = split_patch(patch);
        assert_eq!(files.len(), 2);
        assert_eq!((files[0].path.as_str(), files[0].status), ("add.txt", 'A'));
        assert_eq!((files[0].plus, files[0].minus), (1, 0));
        assert_eq!((files[1].path.as_str(), files[1].status), ("gone.txt", 'D'));
        assert_eq!((files[1].plus, files[1].minus), (0, 1));
    }

    #[test]
    fn binary_patch_reports_no_counts() {
        let mut fd = FileDiff {
            path: "i.png".into(),
            status: 'M',
            plus: 0,
            minus: 0,
            binary: false,
            hunks: Vec::new(),
        };
        parse_patch_into("Binary files a/i.png and b/i.png differ\n", &mut fd);
        assert!(fd.binary);
        assert_eq!((fd.plus, fd.minus), (0, 0));
    }

    #[test]
    fn summary_matches_gits_phrasing() {
        let f = |p, m| FileDiff {
            path: "x".into(),
            status: 'M',
            plus: p,
            minus: m,
            binary: false,
            hunks: Vec::new(),
        };
        assert_eq!(
            summary(&[f(90, 10), f(345, 38), f(73, 4)]),
            "3 files changed, 508 insertions(+), 52 deletions(-)"
        );
        assert_eq!(summary(&[f(1, 1)]), "1 file changed, 1 insertion(+), 1 deletion(-)");
        assert_eq!(summary(&[f(0, 2)]), "1 file changed, 2 deletions(-)");
    }

    /// `cmd_merged` exit contract: Ok when src is already in dest, Err when not.
    #[test]
    fn merged_reports_ancestor_and_non_ancestor() {
        let tmp = std::env::temp_dir().join(format!(
            "git-wt-merged-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str]) {
            let mut c = std::process::Command::new("git");
            c.current_dir(dir).args(args);
            let out = c.output().unwrap();
            assert!(out.status.success(), "git {:?} failed: {:?}", args, out);
        }

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp,
            &[
                "init",
                "--quiet",
                "--initial-branch=main",
            ],
        );
        git(&tmp, &["config", "user.email", "t@test"]);
        git(&tmp, &["config", "user.name", "t"]);
        std::fs::write(tmp.join("x.txt"), "init").unwrap();
        git(&tmp, &["add", "x.txt"]);
        git(&tmp, &["commit", "--quiet", "-m", "init"]);
        git(&tmp, &["branch", "feat"]);
        git(&tmp, &["checkout", "--quiet", "feat"]);
        std::fs::write(tmp.join("y.txt"), "a").unwrap();
        git(&tmp, &["add", "y.txt"]);
        git(&tmp, &["commit", "--quiet", "-m", "add"]);

        // main is an ancestor of feat.
        assert!(cmd_merged(&tmp, "main", "feat").is_ok());
        // feat is not an ancestor of main: 1 commit ahead.
        let err = cmd_merged(&tmp, "feat", "main").unwrap_err();
        assert!(err.contains("Ahead feat is NOT in main"), "{err}");
        assert!(err.contains("ahead 1"), "{err}");
        // A non-existent ref propagates git's error.
        let err = cmd_merged(&tmp, "no-such-ref", "main").unwrap_err();
        assert!(err.contains("no-such-ref") || err.contains("Not a valid object"), "{err}");

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn commits_args_take_a_limit_and_all() {
        let parse = |args: &[&str]| {
            let v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            parse_commits_args(&v)
        };

        let a = parse(&[]).unwrap();
        assert_eq!(a.limit, None);
        // The default is the first branch's merge-request-style range; the
        // full first-branch log is --all and the full union is --union.
        assert!(!a.union);
        assert!(!a.all);

        assert_eq!(parse(&["-n", "20"]).unwrap().limit, Some(20));
        assert_eq!(parse(&["--limit", "20"]).unwrap().limit, Some(20));
        assert_eq!(parse(&["--limit=5"]).unwrap().limit, Some(5));
        assert!(parse(&["--union"]).unwrap().union);
        assert!(parse(&["--any"]).unwrap().union);
        assert!(parse(&["--all"]).unwrap().all);
        // --all and --union name two different row sources, so they conflict.
        assert!(parse(&["--all", "--union"]).unwrap_err().contains("--union"));

        // A count of zero asks for an empty table, which is never meant.
        assert!(parse(&["-n", "0"]).unwrap_err().contains("show nothing"));
        assert!(parse(&["-n", "x"]).unwrap_err().contains("bad count 'x'"));
        assert!(parse(&["-n"]).unwrap_err().contains("needs a count"));
        assert!(parse(&["--stat"]).unwrap_err().contains("unexpected argument"));

        // The pick column is asked for, never assumed: it costs a second
        // patch-id walk.
        assert!(!parse(&[]).unwrap().pick);
        assert!(parse(&["--pick-id"]).unwrap().pick);
        // And it cannot be asked for and switched off at once.
        assert!(parse(&["--pick-id", "--no-cherry"]).unwrap_err().contains("drop one of them"));

        // --files is also opt-in: it spawns a diff per displayed commit.
        assert!(!parse(&[]).unwrap().files);
        assert!(parse(&["--files"]).unwrap().files);
    }

    #[test]
    fn ellipsize_only_cuts_what_overflows() {
        assert_eq!(ellipsize("short", 10), "short");
        // Exactly the budget is not an overflow: nothing is lost, so no marker.
        assert_eq!(ellipsize("abcde", 5), "abcde");
        assert_eq!(ellipsize("abcdef", 5), "abcd…");
        // The marker costs a character, so the result still fits the budget.
        assert_eq!(ellipsize("abcdef", 5).chars().count(), 5);
        // Counted in characters, not bytes: a multi-byte subject must not be
        // cut mid-codepoint, nor counted as if it were wider than it looks.
        assert_eq!(ellipsize("héllo wörld", 20), "héllo wörld");
        assert_eq!(ellipsize("héllo wörld", 7), "héllo …");
        assert_eq!(ellipsize("日本語のコミット", 4), "日本語…");
    }

    #[test]
    fn date_filters_parse_every_comparison() {
        let f = |s: &str| parse_date_filter(s).unwrap();
        assert_eq!(f(">=2026-01-01"), DateFilter { op: DateOp::Ge, date: "2026-01-01".into() });
        assert_eq!(f("<=2026-01-01"), DateFilter { op: DateOp::Le, date: "2026-01-01".into() });
        assert_eq!(f("=2026-01-01"), DateFilter { op: DateOp::Eq, date: "2026-01-01".into() });
        // A bare date is the '=' everyone means by it.
        assert_eq!(f("2026-01-01"), DateFilter { op: DateOp::Eq, date: "2026-01-01".into() });

        // Bounds are inclusive, so a strict comparison is refused rather than
        // quietly rounded to the inclusive one next door. '>=' must still parse
        // as '>=': the two-character check has to come first.
        assert!(parse_date_filter(">2026-01-01").unwrap_err().contains("use '>='"));
        assert!(parse_date_filter("<2026-01-01").unwrap_err().contains("use '<='"));

        // Only YYYY-MM-DD: a short spelling would compare as a prefix and mean
        // something other than what it reads as.
        assert!(parse_date_filter(">=2026-1-1").unwrap_err().contains("want YYYY-MM-DD"));
        assert!(parse_date_filter(">=2026-01").unwrap_err().contains("want YYYY-MM-DD"));
        assert!(parse_date_filter("2026-13-01").unwrap_err().contains("no such date"));
        assert!(parse_date_filter("2026-01-32").unwrap_err().contains("no such date"));
        // An unquoted '>' is eaten by the shell, so the value arrives empty.
        assert!(parse_date_filter(">=").unwrap_err().contains("redirect"));
    }

    #[test]
    fn date_filters_compare_iso_dates_as_text() {
        let admits = |s: &str, key: &str| parse_date_filter(s).unwrap().admits(key);
        // A bound takes its own day, both ends.
        assert!(admits(">=2026-03-01", "2026-03-01"));
        assert!(admits("<=2026-03-01", "2026-03-01"));
        assert!(!admits(">=2026-03-02", "2026-03-01"));
        assert!(!admits("<=2026-02-28", "2026-03-01"));
        // Ordering is lexicographic, which for zero-padded ISO is chronological
        // -- across months and years, where a naive text compare could not be.
        assert!(admits(">=2026-01-01", "2026-10-01"));
        assert!(admits("<=2026-12-31", "2026-12-31"));
        assert!(!admits(">=2026-01-01", "2025-12-31"));
    }

    #[test]
    fn commits_args_take_the_filters() {
        let parse = |args: &[&str]| {
            let v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            parse_commits_args(&v)
        };

        // Several --date bounds are an AND, which is how a range is spelled.
        let a = parse(&["--date", ">=2026-01-01", "--date", "<=2026-06-01"]).unwrap();
        assert_eq!(a.dates.len(), 2);
        assert_eq!(a.dates[0].op, DateOp::Ge);
        assert_eq!(a.dates[1].op, DateOp::Le);

        // --from-date/--to-date are those same bounds, needing no quoting.
        let a = parse(&["--from-date", "2026-01-01", "--to-date=2026-06-01"]).unwrap();
        assert_eq!(a.dates[0], DateFilter { op: DateOp::Ge, date: "2026-01-01".into() });
        assert_eq!(a.dates[1], DateFilter { op: DateOp::Le, date: "2026-06-01".into() });

        let a = parse(&["--from-id", "abc123", "--to-id=def456"]).unwrap();
        assert_eq!(a.from.as_deref(), Some("abc123"));
        assert_eq!(a.to.as_deref(), Some("def456"));
        assert_eq!(parse(&["--author=nino"]).unwrap().author.as_deref(), Some("nino"));
        assert!(!parse(&[]).unwrap().topo);
        assert!(parse(&["--topo"]).unwrap().topo);
        assert!(parse(&["--topo-order"]).unwrap().topo);
        assert!(!parse(&[]).unwrap().no_merges);
        assert!(parse(&["--no-merges"]).unwrap().no_merges);

        // ISO, no time, unless asked; the flags are independent.
        assert_eq!(parse(&[]).unwrap().fmt, DateFmt { human: false, time: false });
        assert_eq!(parse(&["--show-time"]).unwrap().fmt.spec(), "%Y-%m-%d %H:%M:%S");
        assert_eq!(parse(&["--date-human"]).unwrap().fmt.spec(), "%b. %-d, %Y");
        assert_eq!(
            parse(&["--date-human", "--show-time"]).unwrap().fmt.spec(),
            "%b. %-d, %Y %H:%M:%S"
        );
        // A format flag is not a filter: --date-human must not be read as a
        // bound, nor collide with --date's value parsing.
        assert!(parse(&["--date-human"]).unwrap().dates.is_empty());

        assert!(!parse(&[]).unwrap().reverse);
        assert!(parse(&["--reverse"]).unwrap().reverse);
        assert!(parse(&["--oldest-first"]).unwrap().reverse);

        // --md's path is optional, so the flag after it must not be eaten:
        // 'commits --md --topo' asks for the default name AND topo order.
        assert_eq!(parse(&[]).unwrap().md, None);
        assert_eq!(parse(&["--md"]).unwrap().md, Some(None));
        assert_eq!(parse(&["--md", "out.md"]).unwrap().md, Some(Some("out.md".into())));
        assert_eq!(parse(&["--md=out.md"]).unwrap().md, Some(Some("out.md".into())));
        let a = parse(&["--md", "--topo"]).unwrap();
        assert_eq!(a.md, Some(None), "--topo is a flag, not a filename");
        assert!(a.topo, "--topo must still take effect");

        assert!(parse(&["--from-id"]).unwrap_err().contains("--from-id needs a commit"));
        assert!(parse(&["--from-date", "nope"]).unwrap_err().contains("want YYYY-MM-DD"));
        // A bare --from could be either bound; it names neither.
        assert!(parse(&["--from", "x"]).unwrap_err().contains("'--from-id' takes a commit"));
        // git's spellings point at ours instead of reading as a typo.
        assert!(parse(&["--since", "2026-01-01"]).unwrap_err().contains("--from-date"));
        assert!(parse(&["--until", "2026-01-01"]).unwrap_err().contains("--to-date"));
    }

    #[test]
    fn wrap_reads_a_count_or_full() {
        let parse = |a: &[&str]| {
            parse_commits_args(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>())
        };
        // One line is the table's shape: more of it is asked for, never given.
        assert_eq!(parse(&[]).unwrap().wrap, Wrap::Lines(1));
        assert_eq!(parse(&["--wrap", "2"]).unwrap().wrap, Wrap::Lines(2));
        assert_eq!(parse(&["--wrap=3"]).unwrap().wrap, Wrap::Lines(3));
        assert_eq!(parse(&["-w", "2"]).unwrap().wrap, Wrap::Lines(2));
        assert_eq!(parse(&["--wrap", "full"]).unwrap().wrap, Wrap::Full);
        assert_eq!(parse(&["--wrap=full"]).unwrap().wrap, Wrap::Full);
        assert_eq!(parse(&["--wrap"]).unwrap().wrap, Wrap::Full);
        // The count is optional, so the flag after a bare --wrap must not be
        // eaten -- the same rule --md's optional path follows.
        let a = parse(&["--wrap", "--topo"]).unwrap();
        assert_eq!(a.wrap, Wrap::Full);
        assert!(a.topo, "--topo must still take effect");
        // Zero lines is no subject column, and a word is not a count.
        assert!(parse(&["--wrap=0"]).unwrap_err().contains("1 or more"));
        assert!(parse(&["--wrap=two"]).unwrap_err().contains("1 or more"));
    }

    #[test]
    fn subject_width_is_a_width_not_a_filter() {
        let parse = |a: &[&str]| {
            parse_commits_args(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>())
        };
        // None is the terminal's answer, which is the default it always was.
        assert_eq!(parse(&[]).unwrap().subjectw, None);
        assert_eq!(parse(&["--subject-width", "80"]).unwrap().subjectw, Some(SubjectWidth::Cols(80)));
        assert_eq!(parse(&["--subject-width=80"]).unwrap().subjectw, Some(SubjectWidth::Cols(80)));
        assert_eq!(parse(&["--subjw", "80"]).unwrap().subjectw, Some(SubjectWidth::Cols(80)));
        assert_eq!(parse(&["--subjw=full"]).unwrap().subjectw, Some(SubjectWidth::Full));
        // The count is required, unlike --wrap's: no width is named by a bare
        // flag, and 'full' is the word for wanting all of it.
        assert!(parse(&["--subject-width"]).unwrap_err().contains("needs a column count"));
        assert!(parse(&["--subjw=wide"]).unwrap_err().contains("needs a column count"));
        // Below MIN_TEXTW the column says only 'there was a subject'.
        assert!(parse(&["--subjw=8"]).unwrap_err().contains("columns or more"));
        assert!(parse(&["--subjw=0"]).unwrap_err().contains("needs a column count"));
        // '--subject' is the filter it is not: --author is right there.
        assert!(parse(&["--subject", "fix"]).unwrap_err().contains("--subject-width 80"));
    }

    #[test]
    fn wrapping_a_subject_never_exceeds_its_budget() {
        let s = "fix(portal-sales): validate the uploaded masterfile rows";
        // Every line fits, and the words survive the break.
        for line in wrap_wide(s, 20, 3) {
            assert!(width_bound(&line) <= 20, "{line:?}");
        }
        assert_eq!(wrap_wide(s, 20, usize::MAX).join(" "), s, "full loses nothing");
        // One line is the old behavior exactly: cut, with an ellipsis.
        let one = wrap_wide(s, 20, 1);
        assert_eq!(one.len(), 1);
        assert!(one[0].ends_with(ELLIPSIS), "{one:?}");
        // Only the last line an allowance permits wears the ellipsis: the
        // others wrapped, and an ellipsis there would claim text was lost.
        let two = wrap_wide(s, 20, 2);
        assert_eq!(two.len(), 2);
        assert!(!two[0].ends_with(ELLIPSIS), "{two:?}");
        assert!(two[1].ends_with(ELLIPSIS), "{two:?}");
        // A subject that fits takes one line whatever it is allowed.
        assert_eq!(wrap_wide("short one", 20, 3), vec!["short one"]);
        // An emoji is two columns wide and one char: the budget counts columns.
        for line in wrap_wide("🚀🚀🚀🚀🚀🚀 ship it", 6, 4) {
            assert!(width_bound(&line) <= 6, "{line:?}");
        }
        // A word longer than the budget has no boundary to break at, so it is
        // cut rather than left to overflow -- and the wrap still terminates.
        let long = wrap_wide("aaaaaaaaaaaaaaaaaaaaaaaa tail", 8, usize::MAX);
        assert!(long.len() > 1, "{long:?}");
        assert!(long.iter().all(|l| width_bound(l) <= 8), "{long:?}");
        assert_eq!(long.last().unwrap(), "tail");
    }

    #[test]
    fn wrapped_lines_start_at_the_subject_column() {
        // A leading space would push the text one column past the indent the
        // continuation line is padded to -- the table failing to line up.
        let (head, tail) = split_at_width("feat: add the thing", 10);
        assert_eq!(head, "feat: add");
        assert_eq!(tail, "the thing");
    }

    #[test]
    fn md_cells_cannot_invent_columns() {
        assert_eq!(md_cell("plain subject"), "plain subject");
        // A '|' would end the cell and shift every column after it -- the
        // markdown twin of the emoji-width bug, and a silent one.
        assert_eq!(md_cell("fix: a|pipe"), "fix: a\\|pipe");
        assert_eq!(md_cell("a|b|c"), "a\\|b\\|c");
        // The backslash goes first, or escaping the pipe would leave a stray
        // '\' that eats the escape we just added.
        assert_eq!(md_cell("back\\slash"), "back\\\\slash");
        assert_eq!(md_cell("both\\|here"), "both\\\\\\|here");
        // Emoji and CJK pass through: a file has no columns to misalign.
        assert_eq!(md_cell("🚀 ship 日本語"), "🚀 ship 日本語");
    }

    #[test]
    fn md_filename_is_stamped_and_sorts() {
        let name = md_filename();
        assert!(name.starts_with("commits_"), "{name}");
        assert!(name.ends_with(".md"), "{name}");
        // No path separator: it lands in the cwd, and cannot be read as a
        // directory that may not exist.
        assert!(!name.contains('/'), "{name}");
    }

    #[test]
    fn width_bound_never_under_counts_a_subject() {
        // ASCII is exact.
        assert_eq!(width_bound("abc"), 3);
        // An emoji is two columns wide but one char: counting chars is what
        // shifted every column after an emoji subject.
        assert_eq!("🚀 fix".chars().count(), 5);
        assert_eq!(width_bound("🚀 fix"), 6);
        // CJK, likewise.
        assert_eq!(width_bound("日本語"), 6);
        // Pessimistic on accented Latin -- costs a character of subject, never
        // an overflow, which is the safe direction for a budget.
        assert_eq!(width_bound("é"), 2);
    }

    #[test]
    fn ellipsize_wide_budgets_in_columns_not_chars() {
        assert_eq!(ellipsize_wide("abcdef", 10), "abcdef");
        assert_eq!(ellipsize_wide("abcdef", 4), "abc…");
        // Two emoji = 4 columns, so a 4-column budget fits them whole: exactly
        // the budget is not an overflow.
        assert_eq!(ellipsize_wide("🚀🚀", 4), "🚀🚀");
        // Never cut mid-emoji: the char is atomic, so a budget that cannot fit
        // it drops it rather than splitting it.
        assert_eq!(ellipsize_wide("🚀🚀", 3), "🚀…");
        // The result always fits the budget it was given.
        for max in 2..12 {
            let out = ellipsize_wide("🚀 (ci): add validate stage", max);
            assert!(width_bound(&out) <= max, "{max}: {out:?}");
        }
    }

    #[test]
    fn a_piped_table_has_no_width_to_fit() {
        // Not a terminal: the subject is the payload for `| grep`, so it must
        // arrive whole however long it is.
        assert_eq!(term_width(false), None);
    }

    #[test]
    fn commit_rows_stop_at_the_common_ancestor() {
        let tmp = std::env::temp_dir().join(format!("git-wt-commits-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str]) {
            let mut c = std::process::Command::new("git");
            c.current_dir(dir).args(args);
            let out = c.output().unwrap();
            assert!(out.status.success(), "git {:?} failed: {:?}", args, out);
        }
        // A fixed author date: the date column's format is part of the
        // contract, and "now" cannot be asserted against.
        fn commit(dir: &std::path::Path, name: &str, when: &str) {
            std::fs::write(dir.join(format!("{name}.txt")), name).unwrap();
            git(dir, &["add", "-A"]);
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(["commit", "--quiet", "-m", name])
                .env("GIT_AUTHOR_DATE", when)
                .env("GIT_COMMITTER_DATE", when)
                .output()
                .unwrap();
            assert!(out.status.success(), "commit {name} failed: {out:?}");
        }

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"]);
        git(&tmp, &["config", "user.email", "t@test"]);
        git(&tmp, &["config", "user.name", "t"]);
        commit(&tmp, "shared", "2025-12-20T10:00:00");
        git(&tmp, &["branch", "feat"]);
        git(&tmp, &["checkout", "--quiet", "feat"]);
        commit(&tmp, "on-feat", "2026-09-15T10:00:00");
        git(&tmp, &["checkout", "--quiet", "main"]);
        commit(&tmp, "on-main", "2026-01-01T10:00:00");

        let refs = vec!["main".to_string(), "feat".to_string()];

        // --all keeps the old default: the first ref's log, whole -- exactly
        // 'git log --oneline main', shared history included. feat's own commit
        // is not a row, it is a missing mark on feat's column.
        let all_rows = commit_rows(&tmp, &refs[..1], None, None, Order::Date, ISO, false).unwrap();
        let subjects: Vec<&str> = all_rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(all_rows.len(), 2, "{subjects:?}");
        assert!(subjects.iter().any(|t| t.ends_with("on-main")), "{subjects:?}");
        assert!(subjects.iter().any(|t| t.ends_with("shared")), "{subjects:?}");
        assert!(!subjects.iter().any(|t| t.ends_with("on-feat")), "{subjects:?}");
        // Each field is parsed off its own tab, so nothing can shift into the
        // wrong column. The date is the format the table promises, single-digit
        // days unpadded.
        assert!(all_rows.iter().all(|r| r.author == "t"), "{:?}", all_rows[0].author);
        assert!(all_rows.iter().all(|r| !r.short.is_empty()));
        // ISO by default: the shape --from-date takes, so a date read off the
        // table pastes straight back into a filter.
        let dates: Vec<&str> = all_rows.iter().map(|r| r.date.as_str()).collect();
        assert_eq!(dates, ["2026-01-01", "2025-12-20"], "{dates:?}");

        // The default slice: rows are commits in main that feat is missing,
        // from the oldest such commit up to main's tip. Here feat forked at
        // the root, so only 'on-main' is missing from feat; 'shared' is
        // older than the missing commit and is therefore excluded.
        let divergent = divergent_set(&tmp, &refs[0], &refs[1..]).unwrap();
        assert!(!divergent.is_empty(), "feat must be missing something from main");
        let full = commit_rows(&tmp, &refs[..1], None, None, Order::Date, ISO, false).unwrap();
        let rows = window_to_divergent(full, &divergent);
        let subjects: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(rows.len(), 1, "{subjects:?}");
        assert!(subjects.iter().any(|t| t.ends_with("on-main")), "{subjects:?}");
        assert!(!subjects.iter().any(|t| t.ends_with("shared")), "{subjects:?}");
        assert!(!subjects.iter().any(|t| t.ends_with("on-feat")), "{subjects:?}");

        // The columns answer for a branch's entire history. The only row is
        // 'on-main'; feat does not have it.
        let feat_all = ref_shas(&tmp, "feat", None).unwrap();
        for row in &rows {
            assert!(!feat_all.contains(&row.sha), "{}", row.text);
        }

        // The divergent set is main's commits feat is missing: here just
        // 'on-main', and it is the floor the slice stops at.
        let on_main_row = rows.iter().find(|r| r.text.ends_with("on-main")).unwrap();
        assert!(divergent.contains(&on_main_row.sha));
        assert_eq!(divergent.len(), 1);


        // --union: every ref contributes rows, so feat's commit is one too, and
        // the shared commit is checked on both.
        let union = commit_rows(&tmp, &refs, None, None, Order::Date, ISO, false).unwrap();
        let subjects: Vec<&str> = union.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(union.len(), 3, "{subjects:?}");
        // --author-date-order, so the rows descend by the date they print.
        assert!(union[0].text.ends_with("on-feat"), "{:?}", union[0].text);
        let shared = union.iter().find(|r| r.text.ends_with("shared")).unwrap();
        assert!(ref_shas(&tmp, "main", None).unwrap().contains(&shared.sha));
        assert!(feat_all.contains(&shared.sha));

        // -n caps the rows, newest first.
        let capped = commit_rows(&tmp, &refs, None, Some(1), Order::Date, ISO, false).unwrap();
        assert_eq!(capped.len(), 1);

        // --from-id/--to-id include the commit they name. That is the whole
        // point of the flags, and the easy thing to get wrong: 'X..' excludes
        // X, so the bound is built from X's *parents* instead.
        let on_main = rows.iter().find(|r| r.text.ends_with("on-main")).unwrap();
        let older = older_than(&tmp, &on_main.sha).unwrap();
        assert!(!older.contains(&on_main.sha), "--from-id must keep its own commit");
        let within = reachable_from(&tmp, &on_main.sha).unwrap();
        assert!(within.contains(&on_main.sha), "--to-id must keep its own commit");
        // 'shared' is on-main's parent: strictly older, and reachable from it.
        let shared = union.iter().find(|r| r.text.ends_with("shared")).unwrap();
        assert!(older.contains(&shared.sha));
        assert!(within.contains(&shared.sha));
        // The root commit has no parents, so nothing is older than it -- the
        // case where 'X^' would have failed outright.
        assert!(older_than(&tmp, &shared.sha).unwrap().is_empty());

        // A commit that does not resolve is named by the flag that wanted it.
        let err = commit_of(&tmp, "no-such-commit", "--from-id").unwrap_err();
        assert_eq!(err, "--from-id: no commit 'no-such-commit'");

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn commits_default_slice_uses_earliest_divergence() {
        let tmp = std::env::temp_dir().join(format!(
            "git-wt-commits-slice-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str], when: &str) {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_AUTHOR_DATE", when)
                .env("GIT_COMMITTER_DATE", when)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {:?} failed: {:?}", args, out);
        }
        let commit = |dir: &std::path::Path, name: &str, when: &str| {
            std::fs::write(dir.join(format!("{name}.txt")), name).unwrap();
            git(dir, &["add", "-A"], when);
            git(dir, &["commit", "--quiet", "-m", name], when);
        };

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"], "");
        git(&tmp, &["config", "user.email", "t@test"], "");
        git(&tmp, &["config", "user.name", "t"], "");
        commit(&tmp, "A", "2025-12-20T10:00:00");
        commit(&tmp, "B", "2025-12-21T10:00:00");
        git(&tmp, &["branch", "feat"], "");
        git(&tmp, &["branch", "fix"], "");
        git(&tmp, &["checkout", "--quiet", "feat"], "");
        commit(&tmp, "on-feat", "2025-12-22T10:00:00");
        git(&tmp, &["checkout", "--quiet", "fix"], "");
        commit(&tmp, "on-fix", "2025-12-23T10:00:00");
        git(&tmp, &["checkout", "--quiet", "main"], "");
        commit(&tmp, "C", "2025-12-24T10:00:00");
        commit(&tmp, "D", "2025-12-25T10:00:00");

        let refs = vec![
            "main".to_string(),
            "feat".to_string(),
            "fix".to_string(),
        ];

        // feat and fix both forked at B, so the commits main has that either of
        // them misses are C and D; the earliest is C. The default slice should
        // include C and D (commits strictly after B), but not B or A.
        let divergent = divergent_set(&tmp, &refs[0], &refs[1..]).unwrap();
        assert!(divergent.contains(sha_by_subject(&tmp, "main", "C").as_str()));
        assert!(divergent.contains(sha_by_subject(&tmp, "main", "D").as_str()));
        assert_eq!(divergent.len(), 2);

        let full = commit_rows(
            &tmp, &refs[..1], None, None, Order::Date, ISO, false,
        ).unwrap();
        let rows = window_to_divergent(full, &divergent);
        let subjects: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(subjects, ["D", "C"], "{subjects:?}");

        // The full first-branch log with --all.
        let all_rows = commit_rows(
            &tmp, &refs[..1], None, None, Order::Date, ISO, false,
        ).unwrap();
        let all_subjects: Vec<&str> = all_rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(all_subjects, ["D", "C", "B", "A"], "{all_subjects:?}");

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn commits_window_does_not_leak_merge_side_branches() {
        // The bug positional truncation fixes: on a merge DAG, the floor is a
        // commit on a side branch merged into the target late. An ancestry base
        // (`floor^@` excluded) only prunes the floor's own parent line, so a
        // shared commit on the *other* merge parent -- older than the floor,
        // and one the source branch also has -- leaks in as a row below it.
        let tmp = std::env::temp_dir().join(format!(
            "git-wt-window-leak-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str], when: &str) {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_AUTHOR_DATE", when)
                .env("GIT_COMMITTER_DATE", when)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?} failed: {out:?}");
        }
        let commit = |dir: &std::path::Path, name: &str, when: &str| {
            std::fs::write(dir.join(format!("{name}.txt")), name).unwrap();
            git(dir, &["add", "-A"], when);
            git(dir, &["commit", "--quiet", "-m", name], when);
        };

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"], "");
        git(&tmp, &["config", "user.email", "t@test"], "");
        git(&tmp, &["config", "user.name", "t"], "");
        // A -> MAINLINE on main; feat forks at MAINLINE (so feat has both).
        commit(&tmp, "A", "2025-12-20T10:00:00");
        git(&tmp, &["branch", "other"], "");
        commit(&tmp, "MAINLINE", "2025-12-21T10:00:00");
        git(&tmp, &["branch", "feat"], "");
        // A side branch off A, then merged back into main as the FLOOR merge.
        // FLOOR's first parent is MAINLINE, its second is SIDE (parent A) --
        // MAINLINE is not an ancestor of SIDE.
        git(&tmp, &["checkout", "--quiet", "other"], "");
        commit(&tmp, "SIDE", "2025-12-22T10:00:00");
        git(&tmp, &["checkout", "--quiet", "main"], "");
        git(
            &tmp,
            &["merge", "--no-ff", "--quiet", "-m", "FLOOR", "other"],
            "2025-12-23T10:00:00",
        );

        let refs = vec!["main".to_string(), "feat".to_string()];

        // main has SIDE and FLOOR that feat is missing; MAINLINE is shared.
        let divergent = divergent_set(&tmp, &refs[0], &refs[1..]).unwrap();
        assert!(divergent.contains(sha_by_subject(&tmp, "main", "SIDE").as_str()));
        assert!(divergent.contains(sha_by_subject(&tmp, "main", "FLOOR").as_str()));
        assert_eq!(divergent.len(), 2, "MAINLINE is shared, not divergent");

        let full = commit_rows(&tmp, &refs[..1], None, None, Order::Date, ISO, false).unwrap();
        let rows = window_to_divergent(full, &divergent);
        let subjects: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        // FLOOR down to SIDE, and nothing below: MAINLINE must not leak in even
        // though it is reachable from main outside SIDE's ancestry.
        assert_eq!(subjects, ["FLOOR", "SIDE"], "{subjects:?}");

        std::fs::remove_dir_all(&tmp).ok();
    }

    fn sha_by_subject(
        root: &std::path::Path,
        branch: &str,
        subject: &str,
    ) -> String {
        let rows = commit_rows(
            root,
            &[branch.to_string()],
            None,
            None,
            Order::Date,
            ISO,
            false,
        )
        .unwrap();
        rows.iter()
            .find(|r| r.text == subject)
            .map(|r| r.sha.clone())
            .unwrap_or_else(|| panic!("no row for subject '{}'", subject))
    }

    #[test]
    fn topo_groups_the_branches_that_date_order_interleaves() {
        let tmp = std::env::temp_dir().join(format!("git-wt-topo-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str], when: &str) {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_AUTHOR_DATE", when)
                .env("GIT_COMMITTER_DATE", when)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?} failed: {out:?}");
        }
        let commit = |name: &str, when: &str| {
            git(&tmp, &["commit", "--quiet", "--allow-empty", "-m", name], when);
        };

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"], "");
        git(&tmp, &["config", "user.email", "t@test"], "");
        git(&tmp, &["config", "user.name", "t"], "");

        // Two branches whose commits alternate in time: main in the even
        // months, feat in the odd ones. The orders disagree maximally here.
        commit("base", "2026-01-01T10:00:00");
        git(&tmp, &["branch", "feat"], "");
        commit("main-02", "2026-02-01T10:00:00");
        commit("main-04", "2026-04-01T10:00:00");
        git(&tmp, &["checkout", "--quiet", "feat"], "");
        commit("feat-03", "2026-03-01T10:00:00");
        commit("feat-05", "2026-05-01T10:00:00");

        let refs = vec!["main".to_string(), "feat".to_string()];
        let subjects = |o: Order| -> Vec<String> {
            commit_rows(&tmp, &refs, None, None, o, ISO, false)
                .unwrap()
                .iter()
                .map(|r| r.text.clone())
                .collect()
        };

        // By date: strictly newest-first, so the branches interleave and a
        // row's neighbors are the commits written around the same time.
        assert_eq!(
            subjects(Order::Date),
            ["feat-05", "main-04", "feat-03", "main-02", "base"]
        );
        // By topology: each branch's line stays in one block, so the table
        // reads as one branch's story then the other's.
        assert_eq!(
            subjects(Order::Topo),
            ["feat-05", "feat-03", "main-04", "main-02", "base"]
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn same_day_rows_are_ordered_by_time_of_day() {
        let tmp = std::env::temp_dir().join(format!("git-wt-sameday-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str], when: &str) {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_AUTHOR_DATE", when)
                .env("GIT_COMMITTER_DATE", when)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?} failed: {out:?}");
        }
        let commit = |name: &str, when: &str| {
            git(&tmp, &["commit", "--quiet", "--allow-empty", "-m", name], when);
        };

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"], "");
        git(&tmp, &["config", "user.email", "t@test"], "");
        git(&tmp, &["config", "user.name", "t"], "");

        // Two branches, four commits, one calendar day. The column prints the
        // day, so every row looks tied; only the time can order them.
        commit("base", "2026-07-01T10:00:00");
        git(&tmp, &["branch", "feat"], "");
        commit("main-09h", "2026-07-17T09:00:00");
        commit("main-17h", "2026-07-17T17:00:00");
        git(&tmp, &["checkout", "--quiet", "feat"], "");
        commit("feat-13h", "2026-07-17T13:00:00");
        commit("feat-21h", "2026-07-17T21:00:00");

        let refs = vec!["main".to_string(), "feat".to_string()];
        let rows = commit_rows(&tmp, &refs, None, None, Order::Date, ISO, false).unwrap();
        let seen: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();

        // Ordering reads the full timestamp, not the printed day: the branches
        // interleave by hour even though all four rows show '2026-07-17'.
        assert_eq!(seen, ["feat-21h", "main-17h", "feat-13h", "main-09h", "base"]);
        assert!(rows[..4].iter().all(|r| r.date == "2026-07-17"));

        // The filter key is the day, so one '=' bound takes every hour in it.
        let day = parse_date_filter("=2026-07-17").unwrap();
        assert_eq!(rows.iter().filter(|r| day.admits(&r.key)).count(), 4);

        // --show-time is what tells those four rows apart, 24-hour so they sort
        // the way they read; the day stays ISO beside it.
        let timed = DateFmt { human: false, time: true };
        let rows = commit_rows(&tmp, &refs, None, None, Order::Date, timed, false).unwrap();
        let stamps: Vec<&str> = rows[..4].iter().map(|r| r.date.as_str()).collect();
        assert_eq!(
            stamps,
            [
                "2026-07-17 21:00:00",
                "2026-07-17 17:00:00",
                "2026-07-17 13:00:00",
                "2026-07-17 09:00:00",
            ]
        );

        // --date-human is the old spelling, single-digit days unpadded.
        let human = DateFmt { human: true, time: false };
        let rows = commit_rows(&tmp, &refs, None, None, Order::Date, human, false).unwrap();
        assert_eq!(rows[4].date, "Jul. 1, 2026");
        // The filter key never changes shape, whatever the column is spelled
        // as: --date compares ISO no matter what you are looking at.
        assert_eq!(rows[4].key, "2026-07-01");

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn a_cherry_picked_patch_is_neither_present_nor_missing() {
        let tmp = std::env::temp_dir().join(format!("git-wt-cherry-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str]) -> String {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_AUTHOR_DATE", "2026-07-17T10:00:00")
                .env("GIT_COMMITTER_DATE", "2026-07-17T10:00:00")
                .env("GIT_EDITOR", "true")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?} failed: {out:?}");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }
        let commit = |name: &str, file: &str| {
            std::fs::write(tmp.join(file), name).unwrap();
            git(&tmp, &["add", "-A"]);
            git(&tmp, &["commit", "--quiet", "-m", name]);
        };

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"]);
        git(&tmp, &["config", "user.email", "t@test"]);
        git(&tmp, &["config", "user.name", "t"]);
        commit("base", "base.txt");
        git(&tmp, &["checkout", "--quiet", "-b", "feat"]);
        commit("shared-fix", "fix.txt");
        let feat_fix = git(&tmp, &["rev-parse", "HEAD"]);
        commit("feat-only", "only.txt");
        let feat_only = git(&tmp, &["rev-parse", "HEAD"]);
        git(&tmp, &["checkout", "--quiet", "main"]);
        // main needs work of its own first: onto the same parent, with the
        // dates pinned, a pick reproduces every input of the original and so
        // reproduces its sha -- the same commit, not a copy of it.
        commit("main-work", "mainwork.txt");
        // main takes the fix by cherry-pick: same patch, its own sha.
        git(&tmp, &["cherry-pick", &feat_fix]);
        let main_fix = git(&tmp, &["rev-parse", "HEAD"]);
        assert_ne!(feat_fix, main_fix, "a pick makes a new commit");

        let refs = vec!["main".to_string(), "feat".to_string()];
        let equiv = equivalents(&tmp, &refs);
        let sets: Vec<HashSet<String>> = refs
            .iter()
            .map(|r| ref_shas(&tmp, r, None).unwrap())
            .collect();
        let mark = |sha: &str, col: usize| Mark::of(sha, &sets[col], &equiv[col]);
        let (main_col, feat_col) = (0, 1);

        // Each side has its own sha of the fix, and an equivalent of the
        // other's: same patch, so neither '✓' nor '·' is the truth.
        assert_eq!(mark(&main_fix, main_col), Mark::Has);
        assert_eq!(mark(&main_fix, feat_col), Mark::Equivalent);
        assert_eq!(mark(&feat_fix, feat_col), Mark::Has);
        assert_eq!(mark(&feat_fix, main_col), Mark::Equivalent);

        // The commit main really is missing stays missing: '≈' must mean
        // something, so it cannot leak onto work nobody picked.
        assert_eq!(mark(&feat_only, feat_col), Mark::Has);
        assert_eq!(mark(&feat_only, main_col), Mark::Missing);

        // --no-cherry is the old answer: equivalence unasked, so the picked
        // commit reads as absent again.
        let none = vec![HashSet::new(); refs.len()];
        assert_eq!(Mark::of(&feat_fix, &sets[main_col], &none[main_col]), Mark::Missing);

        // --pick-id's column: each copy of the fix names the other's sha, and
        // the work nobody picked names nothing.
        let picks = pick_ids(&tmp, &refs);
        assert_eq!(picks.get(&main_fix), Some(&feat_fix));
        assert_eq!(picks.get(&feat_fix), Some(&main_fix));
        assert_eq!(picks.get(&feat_only), None);
        // Every '≈' the marks report is a sha the column can name: the two
        // answers come from one patch comparison and must not disagree.
        for (col, r) in refs.iter().enumerate() {
            for sha in ref_shas(&tmp, r, None).unwrap() {
                if mark(&sha, col) == Mark::Equivalent {
                    assert!(picks.contains_key(&sha), "no pick for '≈' {sha}");
                }
            }
        }

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn containment_beats_equivalence_in_a_mark() {
        let has: HashSet<String> = ["a".to_string()].into_iter().collect();
        let equiv: HashSet<String> = ["a".to_string(), "b".to_string()].into_iter().collect();
        // A branch holding both the commit and a copy of its patch still just
        // has the commit; '≈' would understate it.
        assert_eq!(Mark::of("a", &has, &equiv), Mark::Has);
        assert_eq!(Mark::of("b", &has, &equiv), Mark::Equivalent);
        assert_eq!(Mark::of("c", &has, &equiv), Mark::Missing);
        assert_eq!(Mark::Has.glyph(), "✓");
        assert_eq!(Mark::Equivalent.glyph(), "≈");
        assert_eq!(Mark::Missing.glyph(), "·");
    }

    #[test]
    fn no_merges_drops_only_the_merge_commits() {
        let tmp = std::env::temp_dir().join(format!("git-wt-merges-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str]) {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_AUTHOR_DATE", "2026-07-17T10:00:00")
                .env("GIT_COMMITTER_DATE", "2026-07-17T10:00:00")
                .env("GIT_MERGE_AUTOEDIT", "no")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?} failed: {out:?}");
        }

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"]);
        git(&tmp, &["config", "user.email", "t@test"]);
        git(&tmp, &["config", "user.name", "t"]);
        git(&tmp, &["commit", "--quiet", "--allow-empty", "-m", "base"]);
        git(&tmp, &["checkout", "--quiet", "-b", "side"]);
        git(&tmp, &["commit", "--quiet", "--allow-empty", "-m", "on-side"]);
        git(&tmp, &["checkout", "--quiet", "main"]);
        git(&tmp, &["commit", "--quiet", "--allow-empty", "-m", "on-main"]);
        // A real merge commit: two parents, no work of its own.
        git(&tmp, &["merge", "--no-ff", "-m", "merge-side", "side"]);

        let refs = vec!["main".to_string()];
        let rows = |no_merges: bool| -> Vec<String> {
            commit_rows(&tmp, &refs, None, None, Order::Date, ISO, no_merges)
                .unwrap()
                .iter()
                .map(|r| r.text.clone())
                .collect()
        };

        let all = rows(false);
        assert!(all.contains(&"merge-side".to_string()), "{all:?}");
        assert_eq!(all.len(), 4);

        // Only the merge goes: the commits it joined are still there, which is
        // the point -- the work survives, the bookkeeping row does not.
        let kept = rows(true);
        assert!(!kept.contains(&"merge-side".to_string()), "{kept:?}");
        assert_eq!(kept.len(), 3);
        for c in ["base", "on-side", "on-main"] {
            assert!(kept.contains(&c.to_string()), "{c} should survive: {kept:?}");
        }
    }

    #[test]
    fn rows_follow_ancestry_even_when_the_dates_disagree() {
        let tmp = std::env::temp_dir().join(format!("git-wt-order-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str], when: &str) {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_AUTHOR_DATE", when)
                .env("GIT_COMMITTER_DATE", when)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?} failed: {out:?}");
        }

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"], "");
        git(&tmp, &["config", "user.email", "t@test"], "");
        git(&tmp, &["config", "user.name", "t"], "");

        // The parent is authored in May, its child in January: a rebase, a
        // cherry-pick, or a bad clock all produce exactly this.
        git(&tmp, &["commit", "--quiet", "--allow-empty", "-m", "parent"], "2026-05-01T10:00:00");
        git(&tmp, &["commit", "--quiet", "--allow-empty", "-m", "child"], "2026-01-01T10:00:00");

        let refs = vec!["main".to_string()];
        let rows = commit_rows(&tmp, &refs, None, None, Order::Date, ISO, false).unwrap();
        let seen: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();

        // Ancestry wins: the child is listed above the parent it descends from,
        // so reading down the table is reading real history. The date column
        // ascends across that pair, which is the wrong clock showing through --
        // not the rows lying about what came from what.
        assert_eq!(seen, ["child", "parent"], "a parent must never precede its child");
        assert_eq!(rows[0].key, "2026-01-01");
        assert_eq!(rows[1].key, "2026-05-01");

        std::fs::remove_dir_all(&tmp).ok();
    }
}
