use std::collections::{HashMap, HashSet};
use std::io::IsTerminal;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::git::{git_cmd, git_stdout};
use crate::list::is_subseq;
use crate::ui::{
    abbrev, color_enabled, ellipsize, paint, term_width, width_bound, wrap_wide, AUTHOR_MAX, CHECK,
    DIM, EQUIV, GREEN, MIN_TEXTW, MISS, PICK_HEAD, YELLOW,
};
use crate::worktree::{label, ref_of, Worktree};

/// Which of the two readings of "the story" the rows are in.
///
/// Both keep ancestry: git shows no parent before its children either way, so
/// neither can misreport what came from what. They differ in what fills the
/// gaps between unrelated commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Order {
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
pub(crate) struct DateFmt {
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
#[derive(Clone)]
pub(crate) struct CommitRow {
    /// Full sha, for the set lookups; never printed.
    sha: String,
    short: String,
    text: String,
    author: String,
    /// Author date as printed: `2026-01-31`, or whatever `DateFmt` asked for.
    date: String,
    /// The same date as `YYYY-MM-DD`, which `--date` compares against.
    key: String,
    /// Author date as a Unix timestamp, compared numerically. The default
    /// view's floor is found on this, not on the day-granular `key`, so two
    /// commits on the same day still order against each other and the window
    /// does not swallow a whole day of shared history.
    stamp: String,
}

/// One file touched by a commit, with status and line-count summary.
#[derive(Debug, Clone)]
pub(crate) struct FileStat {
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
pub(crate) enum DateOp {
    Eq,
    Ge,
    Le,
}

/// One `--date` bound. Several are an AND: `--date '>=A' --date '<B'`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DateFilter {
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
pub(crate) enum SubjectWidth {
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
pub(crate) enum Wrap {
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
pub(crate) struct CommitsArgs {
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

pub(crate) fn parse_commits_args(args: &[String]) -> Result<CommitsArgs, String> {
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
pub(crate) fn parse_subjectw(v: &str) -> Result<SubjectWidth, String> {
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
pub(crate) fn parse_wrap(v: &str) -> Result<Wrap, String> {
    if v.eq_ignore_ascii_case("full") || v.eq_ignore_ascii_case("all") {
        return Ok(Wrap::Full);
    }
    match v.parse::<usize>() {
        // Zero lines is no subject column, which no one means by 'wrap'.
        Ok(0) | Err(_) => Err(format!("{WRAP_BAD}\n  got: '{v}'")),
        Ok(n) => Ok(Wrap::Lines(n)),
    }
}

pub(crate) const WRAP_BAD: &str = "--wrap needs a line count of 1 or more, or 'full', e.g. '--wrap 2'\n\
     hint: a bare '--wrap' is 'full'";
pub(crate) const SUBJW_MISSING: &str = "--subject-width needs a column count, or 'full', e.g. '--subject-width 80'";
pub(crate) const SUBJW_BAD: &str = "--subject-width needs a column count, or 'full', e.g. '--subject-width 80'\n\
     hint: 'full' never cuts the subject, however wide it is";
pub(crate) const SUBJECT_MSG: &str = "no '--subject' for commits: it would read as a filter, and it is a width\n\
     hint: '--subject-width 80' widens the column; '--author NAME' filters rows";
pub(crate) const DATE_MISSING: &str = "--date needs a comparison, e.g. --date '>=2026-01-01'\n\
     hint: quote it, or the shell reads '>' as a redirect";
pub(crate) const FROM_DATE_MISSING: &str = "--from-date needs a date, e.g. '--from-date 2026-01-01'";
pub(crate) const TO_DATE_MISSING: &str = "--to-date needs a date, e.g. '--to-date 2026-06-30'";
pub(crate) const FROM_MISSING: &str = "--from-id needs a commit, e.g. '--from-id 5568a21'";
pub(crate) const TO_MISSING: &str = "--to-id needs a commit, e.g. '--to-id HEAD~3'";
pub(crate) const AUTHOR_MISSING: &str = "--author needs a name, e.g. '--author nino'";
pub(crate) const SINCE_MSG: &str = "no '--since' for commits; use '--from-date 2026-01-01'";
pub(crate) const UNTIL_MSG: &str = "no '--until' for commits; use '--to-date 2026-06-30'";

/// Parse `>=2026-01-01`, `<=2026-06-30`, `=2026-01-01`, or a bare date (`=`).
pub(crate) fn parse_date_filter(s: &str) -> Result<DateFilter, String> {
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
pub(crate) fn strict_msg(op: char, incl: &str, flag: &str) -> String {
    format!(
        "no '{op}' comparison; bounds are inclusive: use '{incl}' (or {flag})\n\
         hint: a day either side is '{incl}' on the next day"
    )
}

/// Validate a `YYYY-MM-DD` date, which is the only shape the compare is sound
/// for: shorter spellings would compare as prefixes and quietly mean something
/// else.
pub(crate) fn iso_date(s: &str) -> Result<String, String> {
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

pub(crate) const QUOTE_HINT: &str =
    "quote the comparison -- --date '>=2026-01-01' -- or the shell reads '>' as a redirect";

pub(crate) fn parse_limit(s: &str) -> Result<usize, String> {
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
pub(crate) fn cmd_commits(
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
    //   default: the first branch's log, cut at its earliest divergent commit
    //            -- a merge-request view of what it has that the others do not,
    //            from the furthest divergence up to its tip. Shared commits
    //            newer than that floor stay in; the floor is a date, not a
    //            position or an ancestry base, so a merge DAG's older side
    //            branches cannot leak past it and --topo only regroups the same
    //            rows rather than changing which ones show.
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
    // Default view: keep the log down to its earliest divergent date, shared
    // commits above the floor included. A date threshold, so --topo shows the
    // same rows this does, only regrouped.
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
pub(crate) fn commit_files(root: &Path, sha: &str) -> Result<Vec<FileStat>, String> {
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

pub(crate) fn commit_rows(
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
        "--format=%H%x09%aN%x09%ad%x09%as%x09%h%x09%at%x09%s",
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
            let mut f = line.splitn(7, '\t');
            Some(CommitRow {
                sha: f.next()?.to_string(),
                author: f.next()?.to_string(),
                date: f.next()?.to_string(),
                key: f.next()?.to_string(),
                short: f.next()?.to_string(),
                stamp: f.next()?.to_string(),
                text: f.next()?.to_string(),
            })
        })
        .collect())
}

/// Resolve `r` to a commit, or say which flag could not find it.
pub(crate) fn commit_of(root: &Path, r: &str, flag: &str) -> Result<String, String> {
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
pub(crate) fn older_than(root: &Path, c: &str) -> Result<HashSet<String>, String> {
    Ok(git_stdout(root, &["rev-list", &format!("{c}^@")])?
        .lines()
        .map(str::to_string)
        .collect())
}

/// `c` and everything it can reach, so `--to <c>` includes `<c>`.
pub(crate) fn reachable_from(root: &Path, c: &str) -> Result<HashSet<String>, String> {
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
pub(crate) fn divergent_set(root: &Path, target: &str, sources: &[String]) -> Result<HashSet<String>, String> {
    let mut out = HashSet::new();
    for src in sources {
        let range = format!("{src}..{target}");
        for sha in git_stdout(root, &["rev-list", &range])?.lines() {
            out.insert(sha.to_string());
        }
    }
    Ok(out)
}

/// Keep the first branch's log down to its earliest divergent commit: find the
/// oldest date among the divergent rows, then keep every row at least that new.
///
/// A date threshold, not a cut at a position, so the window is the same set of
/// commits whatever order produced the rows -- `--topo` regroups them, it does
/// not change which ones show. A positional cut would not: topo orders a shared
/// commit below the floor where date order keeps it above, so the two would
/// disagree on the row count. And unlike an ancestry base (`floor^@` excluded
/// from the walk) the threshold cannot leak a merge DAG's older side branches
/// past the floor -- they are older than it, so it drops them.
///
/// The floor is the oldest divergent timestamp, so every divergent row clears
/// the threshold by construction; the shared rows above it are the in-between
/// history. Timestamp, not day: a shared commit older than the floor but landed
/// on the same date stays out. Empty out means no row was divergent -- e.g.
/// `--no-merges` dropped the only commits the others were missing; the caller
/// reports it like an empty log.
pub(crate) fn window_to_divergent(rows: Vec<CommitRow>, divergent: &HashSet<String>) -> Vec<CommitRow> {
    let stamp = |r: &CommitRow| r.stamp.parse::<i64>().unwrap_or(i64::MIN);
    let Some(floor) = rows
        .iter()
        .filter(|r| divergent.contains(&r.sha))
        .map(stamp)
        .min()
    else {
        return Vec::new();
    };
    rows.into_iter().filter(|r| stamp(r) >= floor).collect()
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
pub(crate) fn equivalents(root: &Path, refs: &[String]) -> Vec<HashSet<String>> {
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
pub(crate) fn pick_ids(root: &Path, refs: &[String]) -> HashMap<String, String> {
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
pub(crate) fn patch_ids(root: &Path, rev_args: &[&str]) -> Option<Vec<(String, String)>> {
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
pub(crate) fn ref_shas(root: &Path, r: &str, base: Option<&str>) -> Result<HashSet<String>, String> {
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

/// What a branch has of a given commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mark {
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

/// `commits_2026-07-17_14-30-05.md`: ISO, so the names sort the way the dates
/// do, and stamped to the second so a re-run never silently eats the last one.
///
/// The stamp comes from `date`, for the same reason the terminal width comes
/// from `tput`: turning a unix timestamp into the user's local calendar needs
/// a timezone database this crate has no dependency for.
pub(crate) fn md_filename() -> String {
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
pub(crate) fn md_cell(s: &str) -> String {
    s.replace('\\', "\\\\").replace('|', "\\|")
}

/// Write the table as a markdown file, and say where it went.
///
/// Subjects are never truncated here: a file has no right edge to run out of,
/// so the terminal's budget would only lose information the reader asked for.
pub(crate) fn write_md(
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
pub(crate) fn render_commits(
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
                stamp: r.stamp.clone(),
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
    // Legend above the header: the marks are the point of the table and the
    // '≈'/'·' distinction is not self-evident, so name each glyph once up top.
    let legend = format!(
        "{} {}   {} {}   {} {}",
        paint(CHECK, GREEN, color),
        paint("has commit", DIM, color),
        paint(EQUIV, YELLOW, color),
        paint("same patch, other sha", DIM, color),
        paint(MISS, DIM, color),
        paint("neither", DIM, color),
    );
    println!("{}", legend);

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


#[cfg(test)]
mod tests {
    use super::*;

    /// The default spelling: what `commits` prints without a format flag.
    const ISO: DateFmt = DateFmt { human: false, time: false };


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
        let rows = window_to_divergent(full.clone(), &divergent);
        let subjects: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        // FLOOR down to SIDE, and nothing below: MAINLINE must not leak in even
        // though it is reachable from main outside SIDE's ancestry.
        assert_eq!(subjects, ["FLOOR", "SIDE"], "{subjects:?}");

        // The window is a set, not a slice of one ordering: feeding the rows in
        // any order -- as --topo would -- keeps the same commits, so --topo can
        // only regroup the table, never change its row count.
        let mut scrambled = full;
        scrambled.reverse();
        let sorted_shas = |v: Vec<CommitRow>| {
            let mut s: Vec<String> = v.into_iter().map(|r| r.sha).collect();
            s.sort();
            s
        };
        assert_eq!(
            sorted_shas(window_to_divergent(scrambled, &divergent)),
            sorted_shas(rows),
            "window must be order-independent",
        );

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
