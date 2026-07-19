use crate::ui::MIN_TEXTW;



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
    pub(crate) fn flag(self) -> &'static str {
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
    pub(crate) human: bool,
    /// Append the time, 24-hour.
    pub(crate) time: bool,
}

impl DateFmt {
    /// The strftime git is asked for. `%-d` drops the day's leading zero, which
    /// only the human spelling wants; ISO is padded by definition.
    pub(crate) fn spec(self) -> &'static str {
        match (self.human, self.time) {
            (false, false) => "%Y-%m-%d",
            (false, true) => "%Y-%m-%d %H:%M:%S",
            (true, false) => "%b. %-d, %Y",
            (true, true) => "%b. %-d, %Y %H:%M:%S",
        }
    }
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
    pub(crate) op: DateOp,
    pub(crate) date: String,
}

impl DateFilter {
    /// ISO dates sort lexicographically, so a string compare *is* a date
    /// compare -- no timezone arithmetic, no calendar library.
    pub(crate) fn admits(&self, key: &str) -> bool {
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
    pub(crate) fn lines(self) -> usize {
        match self {
            Wrap::Lines(n) => n,
            Wrap::Full => usize::MAX,
        }
    }
}

/// Options for `commits`.
#[derive(Debug)]
pub(crate) struct CommitsArgs {
    pub(crate) limit: Option<usize>,
    pub(crate) dates: Vec<DateFilter>,
    /// Lower date bound named by a commit: that commit's day, and after.
    pub(crate) commit_since: Option<String>,
    /// Upper date bound named by a commit: that commit's day, and before.
    pub(crate) commit_until: Option<String>,
    /// Only these commits, by sha prefix. Empty means every row.
    pub(crate) commits: Vec<String>,
    pub(crate) author: Option<String>,
    pub(crate) topo: bool,
    pub(crate) no_merges: bool,
    pub(crate) fmt: DateFmt,
    /// `Some(None)` is `--md` with no path: a timestamped name in the cwd.
    pub(crate) md: Option<Option<String>>,
    pub(crate) reverse: bool,
    pub(crate) no_cherry: bool,
    /// Print the sha the '≈' copy of each row carries elsewhere.
    pub(crate) pick: bool,
    /// Rows come from every worktree at once, not the first one's log alone.
    pub(crate) union: bool,
    /// Full first-branch log instead of the merge-request-style range.
    pub(crate) all: bool,
    /// Add the changed files under each displayed commit.
    pub(crate) files: bool,
    /// Terminal lines a subject may take. Moot off a terminal: nothing is cut.
    pub(crate) wrap: Wrap,
    /// Columns the subject gets. None lets the terminal decide, as it always has.
    pub(crate) subjectw: Option<SubjectWidth>,
}

/// The error for a flag that still exists under another name.
fn renamed(old: &str, new: &str) -> String {
    format!("'{old}' is now '{new}'")
}

/// Split a `--commits` value on commas into the list, rejecting an empty id --
/// `af48509,,f9e2427` is a typo, and an empty prefix would match every row.
fn push_commit_ids(into: &mut Vec<String>, v: &str) -> Result<(), String> {
    for part in v.split(',') {
        let id = part.trim();
        if id.is_empty() {
            return Err(format!("bad commit list '{v}'; want ids, e.g. 'af48509,f9e2427'"));
        }
        into.push(id.to_string());
    }
    Ok(())
}

/// Short flags that carry no value, so any number of them can share one dash.
const FLAG_SHORTS: &str = "af";
/// Short flags that read the next argument (`-w`'s is optional), so at most one
/// can appear in a bundle and only as its last letter.
const VALUE_SHORTS: &str = "ndwc";

/// Split `-af` into `-a -f` so short flags can be bundled the way every other
/// unix tool bundles them.
///
/// A value-taking flag has to come last -- `-fn 20` is the only reading of a
/// bundle that ends in one, and `-nf 20` would have to hand '20' to both. Rather
/// than pick for the user, that spelling is an error naming the one that works.
/// Anything that is not a short bundle (`--all`, a path, a lone `-`) is passed
/// through untouched for the parser proper to judge.
pub(crate) fn expand_short_bundles(args: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::with_capacity(args.len());
    for a in args {
        let is_bundle = a.len() > 2 && a.starts_with('-') && !a.starts_with("--");
        if !is_bundle {
            out.push(a.clone());
            continue;
        }
        let letters: Vec<char> = a.chars().skip(1).collect();
        // Not a bundle at all if any letter names nothing: leave it whole so the
        // parser reports the argument the user actually typed.
        if !letters
            .iter()
            .all(|c| FLAG_SHORTS.contains(*c) || VALUE_SHORTS.contains(*c))
        {
            out.push(a.clone());
            continue;
        }
        for (i, c) in letters.iter().enumerate() {
            if VALUE_SHORTS.contains(*c) && i + 1 != letters.len() {
                let rest: String = letters.iter().filter(|o| *o != c).collect();
                return Err(format!(
                    "'-{c}' takes a value, so it has to come last in '{a}'\n\
                     hint: '-{rest}{c} <value>'"
                ));
            }
            out.push(format!("-{c}"));
        }
    }
    Ok(out)
}

pub(crate) fn parse_commits_args(args: &[String]) -> Result<CommitsArgs, String> {
    let args = expand_short_bundles(args)?;
    let mut limit = None;
    let mut dates = Vec::new();
    let mut commit_since = None;
    let mut commit_until = None;
    let mut commits: Vec<String> = Vec::new();
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
            "--files" | "-f" => files = true,
            "--union" | "--any" => union = true,
            "--all" | "-a" => all = true,
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
            "--time" => fmt.time = true,
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
            "--date-since" => {
                let v = it.next().ok_or(FROM_DATE_MISSING)?;
                dates.push(DateFilter { op: DateOp::Ge, date: iso_date(v)? });
            }
            s if s.starts_with("--date-since=") => {
                dates.push(DateFilter { op: DateOp::Ge, date: iso_date(&s["--date-since=".len()..])? });
            }
            "--date-until" => {
                let v = it.next().ok_or(TO_DATE_MISSING)?;
                dates.push(DateFilter { op: DateOp::Le, date: iso_date(v)? });
            }
            s if s.starts_with("--date-until=") => {
                dates.push(DateFilter { op: DateOp::Le, date: iso_date(&s["--date-until=".len()..])? });
            }
            "--author" => author = Some(it.next().ok_or(AUTHOR_MISSING)?.clone()),
            s if s.starts_with("--author=") => author = Some(s["--author=".len()..].to_string()),
            "--commit-since" => commit_since = Some(it.next().ok_or(COMMIT_SINCE_MISSING)?.clone()),
            s if s.starts_with("--commit-since=") => commit_since = Some(s["--commit-since=".len()..].to_string()),
            "--commit-until" => commit_until = Some(it.next().ok_or(COMMIT_UNTIL_MISSING)?.clone()),
            s if s.starts_with("--commit-until=") => commit_until = Some(s["--commit-until=".len()..].to_string()),
            // The rows named outright, rather than a window they fall in. A
            // comma-separated list, and repeatable, so both spellings work.
            "--commits" | "-c" => {
                let v = it.next().ok_or(COMMITS_MISSING)?;
                push_commit_ids(&mut commits, v)?;
            }
            s if s.starts_with("--commits=") => {
                push_commit_ids(&mut commits, &s["--commits=".len()..])?;
            }
            // A bare --from names neither of the two things it could bound, and
            // guessing which was meant would be worse than saying so.
            "--from" | "--to" => {
                let (c, d) = if *a == "--from" {
                    ("--commit-since", "--date-since")
                } else {
                    ("--commit-until", "--date-until")
                };
                return Err(format!(
                    "no '{a}' for commits; '{c}' takes a commit, '{d}' takes a date"
                ));
            }
            // The names these bounds used to carry. A rename is a thing to be
            // told about once, not a word that reads as a typo.
            "--from-date" => return Err(renamed("--from-date", "--date-since")),
            "--to-date" => return Err(renamed("--to-date", "--date-until")),
            "--from-id" => return Err(renamed("--from-id", "--commit-since")),
            "--to-id" => return Err(renamed("--to-id", "--commit-until")),
            "--show-time" => return Err(renamed("--show-time", "--time")),
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
    // The default rows are a slice with a floor: the first branch's log from its
    // earliest divergent commit up to its tip. Only the bottom is cut, and that
    // is what decides which filters have to widen the source.
    //
    // A lower bound or a named commit can point BELOW that floor, so on the
    // default rows they would report "nothing matched" when the truth is "older
    // than these rows". They widen to the full log on their own.
    //
    // An upper bound cannot: the top edge is the tip either way, so --date-until
    // and --commit-until only ever trim rows the slice already has. They are a
    // post-filter, and post-filters do not get to redefine the source. A range
    // still widens, because its lower bound does.
    //
    // --author is the same kind of thing: it matches many commits and named none
    // of them, so "who wrote in this range" stays the question. Say --all when
    // you mean the whole log.
    //
    // Checked after the conflict above, so an implied --all can never collide
    // with a --union the user actually typed.
    let names_a_floor = !commits.is_empty()
        || commit_since.is_some()
        || dates.iter().any(|d| d.op != DateOp::Le);
    let all = all || (names_a_floor && !union);
    Ok(CommitsArgs {
        limit, dates, commit_since, commit_until, commits, author, topo, no_merges, fmt, md,
        reverse, no_cherry, pick, union,
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
pub(crate) const DATE_MISSING: &str =
    "--date needs a day, e.g. '--date 2026-01-01'\n\
     hint: for a range use --date-since / --date-until";
pub(crate) const FROM_DATE_MISSING: &str = "--date-since needs a date, e.g. '--date-since 2026-01-01'";
pub(crate) const TO_DATE_MISSING: &str = "--date-until needs a date, e.g. '--date-until 2026-06-30'";
pub(crate) const COMMIT_SINCE_MISSING: &str =
    "--commit-since needs a commit, e.g. '--commit-since 5568a21'";
pub(crate) const COMMIT_UNTIL_MISSING: &str =
    "--commit-until needs a commit, e.g. '--commit-until HEAD~3'";
pub(crate) const COMMITS_MISSING: &str =
    "--commits needs one or more commits, e.g. '--commits af48509,f9e2427'";
pub(crate) const AUTHOR_MISSING: &str = "--author needs a name, e.g. '--author nino'";
pub(crate) const SINCE_MSG: &str = "no '--since' for commits; use '--date-since 2026-01-01'";
pub(crate) const UNTIL_MSG: &str = "no '--until' for commits; use '--date-until 2026-06-30'";

/// Parse `>=2026-01-01`, `<=2026-06-30`, `=2026-01-01`, or a bare date (`=`).
pub(crate) fn parse_date_filter(s: &str) -> Result<DateFilter, String> {
    // One day, named plainly. The comparisons live in --date-since and
    // --date-until, which say which end they are and cost the shell nothing:
    // an operator here would have to be quoted every single time, and '>' is
    // eaten as a redirect the moment it is not.
    let t = s.trim();
    if let Some(op) = t.chars().next().filter(|c| matches!(c, '>' | '<' | '=')) {
        return Err(operator_msg(op, t));
    }
    Ok(DateFilter { op: DateOp::Eq, date: iso_date(t)? })
}

/// A comparison in `--date`'s value names a bound that has its own flag.
pub(crate) fn operator_msg(op: char, given: &str) -> String {
    let bare = given.trim_start_matches(['>', '<', '=']).trim();
    let flag = if op == '<' { "--date-until" } else { "--date-since" };
    let shown = if bare.is_empty() { "2026-01-01" } else { bare };
    format!(
        "no '{op}' in --date; it takes one day, e.g. '--date {shown}'\n\
         hint: for a bound use '{flag} {shown}'"
    )
}

/// Validate a `YYYY-MM-DD` date, which is the only shape the compare is sound
/// for: shorter spellings would compare as prefixes and quietly mean something
/// else.
pub(crate) fn iso_date(s: &str) -> Result<String, String> {
    let bad = || {
        // An empty value usually means the shell ate an unquoted '>' -- which
        // no longer belongs here at all, so say where the bounds live.
        if s.is_empty() {
            "a date is missing; want YYYY-MM-DD\n\
             hint: --date takes one day, --date-since / --date-until take bounds"
                .to_string()
        } else {
            format!("bad date '{s}'; want YYYY-MM-DD, e.g. '2026-01-01'")
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

pub(crate) fn parse_limit(s: &str) -> Result<usize, String> {
    match s.parse::<usize>() {
        Ok(0) => Err("-n 0 would show nothing".into()),
        Ok(n) => Ok(n),
        Err(_) => Err(format!("bad count '{s}'; want a number, e.g. '-n 20'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `parse_commits_args` over string literals.
    fn parse(args: &[&str]) -> Result<CommitsArgs, String> {
        let v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse_commits_args(&v)
    }

    #[test]
    fn commits_args_take_a_limit_and_all() {
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
    fn short_flags_alias_their_long_forms() {
        assert!(parse(&["-a"]).unwrap().all);
        assert!(parse(&["-f"]).unwrap().files);
    }

    #[test]
    fn short_flags_bundle_under_one_dash() {
        for spelling in [&["-af"][..], &["-fa"][..], &["-a", "-f"][..]] {
            let got = parse(spelling).unwrap();
            assert!(got.all && got.files, "{spelling:?}");
        }
    }

    #[test]
    fn a_value_taking_short_ends_the_bundle() {
        // Last letter: the value is unambiguously its own.
        let got = parse(&["-fn", "5"]).unwrap();
        assert!(got.files);
        assert_eq!(got.limit, Some(5));
        // Anywhere else, '5' would have to belong to two flags at once. Say so,
        // and say which spelling works.
        let err = parse(&["-nf", "5"]).unwrap_err();
        assert!(err.contains("has to come last"), "{err}");
        assert!(err.contains("-fn <value>"), "{err}");
    }

    #[test]
    fn a_bundle_of_nonsense_is_reported_whole() {
        // '-xz' names no flag of ours, so the error quotes what was typed
        // rather than an invented '-x' the user never wrote.
        let err = parse(&["-xz"]).unwrap_err();
        assert!(err.contains("'-xz'"), "{err}");
    }

    #[test]
    fn date_takes_one_day_and_no_operator() {
        let f = |s: &str| parse_date_filter(s).unwrap();
        // One day, and the filter that day makes: --date is exact, full stop.
        assert_eq!(f("2026-01-01"), DateFilter { op: DateOp::Eq, date: "2026-01-01".into() });

        // The comparisons moved to their own flags, so an operator here names
        // a bound that has a better spelling -- and the error says which.
        for (given, flag) in [
            (">=2026-01-01", "--date-since"),
            (">2026-01-01", "--date-since"),
            ("=2026-01-01", "--date-since"),
            ("<=2026-01-01", "--date-until"),
            ("<2026-01-01", "--date-until"),
        ] {
            let err = parse_date_filter(given).unwrap_err();
            assert!(err.contains("in --date"), "{given}: {err}");
            assert!(err.contains(flag), "{given}: {err}");
            // The day survives into the hint, so the fix is copy-pasteable.
            assert!(err.contains("2026-01-01"), "{given}: {err}");
        }

        // Only YYYY-MM-DD: a short spelling would compare as a prefix and mean
        // something other than what it reads as.
        assert!(parse_date_filter("2026-1-1").unwrap_err().contains("want YYYY-MM-DD"));
        assert!(parse_date_filter("2026-01").unwrap_err().contains("want YYYY-MM-DD"));
        assert!(parse_date_filter("2026-13-01").unwrap_err().contains("no such date"));
        assert!(parse_date_filter("2026-01-32").unwrap_err().contains("no such date"));
        // An unquoted '>' is eaten by the shell, so the value arrives empty.
        assert!(parse_date_filter("").unwrap_err().contains("--date-since"));
    }

    #[test]
    fn date_filters_compare_iso_dates_as_text() {
        let admits = |op: DateOp, d: &str, key: &str| DateFilter { op, date: d.into() }.admits(key);
        // A bound takes its own day, both ends.
        assert!(admits(DateOp::Ge, "2026-03-01", "2026-03-01"));
        assert!(admits(DateOp::Le, "2026-03-01", "2026-03-01"));
        assert!(!admits(DateOp::Ge, "2026-03-02", "2026-03-01"));
        assert!(!admits(DateOp::Le, "2026-02-28", "2026-03-01"));
        // Ordering is lexicographic, which for zero-padded ISO is chronological
        // -- across months and years, where a naive text compare could not be.
        assert!(admits(DateOp::Ge, "2026-01-01", "2026-10-01"));
        assert!(admits(DateOp::Le, "2026-12-31", "2026-12-31"));
        assert!(!admits(DateOp::Ge, "2026-01-01", "2025-12-31"));
    }

    #[test]
    fn commits_args_take_the_filters() {
        let parse = |args: &[&str]| {
            let v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            parse_commits_args(&v)
        };

        // A range is --date-since plus --date-until; --date itself is one day.
        let a = parse(&["--date", "2026-01-01"]).unwrap();
        assert_eq!(a.dates, vec![DateFilter { op: DateOp::Eq, date: "2026-01-01".into() }]);

        // --date-since/--date-until are those same bounds, needing no quoting.
        let a = parse(&["--date-since", "2026-01-01", "--date-until=2026-06-01"]).unwrap();
        assert_eq!(a.dates[0], DateFilter { op: DateOp::Ge, date: "2026-01-01".into() });
        assert_eq!(a.dates[1], DateFilter { op: DateOp::Le, date: "2026-06-01".into() });

        let a = parse(&["--commit-since", "abc123", "--commit-until=def456"]).unwrap();
        assert_eq!(a.commit_since.as_deref(), Some("abc123"));
        assert_eq!(a.commit_until.as_deref(), Some("def456"));
        assert_eq!(parse(&["--author=nino"]).unwrap().author.as_deref(), Some("nino"));
        assert!(!parse(&[]).unwrap().topo);
        assert!(parse(&["--topo"]).unwrap().topo);
        assert!(parse(&["--topo-order"]).unwrap().topo);
        assert!(!parse(&[]).unwrap().no_merges);
        assert!(parse(&["--no-merges"]).unwrap().no_merges);

        // ISO, no time, unless asked; the flags are independent.
        assert_eq!(parse(&[]).unwrap().fmt, DateFmt { human: false, time: false });
        assert_eq!(parse(&["--time"]).unwrap().fmt.spec(), "%Y-%m-%d %H:%M:%S");
        assert_eq!(parse(&["--date-human"]).unwrap().fmt.spec(), "%b. %-d, %Y");
        assert_eq!(
            parse(&["--date-human", "--time"]).unwrap().fmt.spec(),
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

        assert!(parse(&["--commit-since"]).unwrap_err().contains("--commit-since needs a commit"));
        assert!(parse(&["--date-since", "nope"]).unwrap_err().contains("want YYYY-MM-DD"));
        // A bare --from could be either bound; it names neither.
        assert!(parse(&["--from", "x"]).unwrap_err().contains("'--commit-since' takes a commit"));
        assert!(parse(&["--to", "x"]).unwrap_err().contains("'--commit-until' takes a commit"));
        // git's spellings point at ours instead of reading as a typo.
        assert!(parse(&["--since", "2026-01-01"]).unwrap_err().contains("--date-since"));
        assert!(parse(&["--until", "2026-01-01"]).unwrap_err().contains("--date-until"));
    }

    #[test]
    fn renamed_flags_say_their_new_name() {
        // A rename is worth being told about once. Silence would read as a
        // typo, and quietly accepting the old name would keep two spellings
        // alive forever.
        for (old, new) in [
            ("--from-date", "--date-since"),
            ("--to-date", "--date-until"),
            ("--from-id", "--commit-since"),
            ("--to-id", "--commit-until"),
            ("--show-time", "--time"),
        ] {
            let err = parse(&[old]).unwrap_err();
            assert_eq!(err, format!("'{old}' is now '{new}'"));
        }
    }

    #[test]
    fn a_lower_bound_widens_the_source_and_an_upper_one_does_not() {
        // The default rows are cut at the bottom, so anything that names a
        // floor -- a commit, a day, a lower bound -- can point below it and
        // has to widen the source to mean what it says.
        assert!(parse(&["--commits", "abc123"]).unwrap().all);
        assert!(parse(&["--commit-since", "abc123"]).unwrap().all);
        assert!(parse(&["--date", "2026-01-01"]).unwrap().all);
        assert!(parse(&["--date-since", "2026-01-01"]).unwrap().all);

        // An upper bound only ever trims the top, which the slice already ends
        // at: a post-filter, and a post-filter does not redefine the source.
        assert!(!parse(&["--date-until", "2026-01-01"]).unwrap().all);
        assert!(!parse(&["--commit-until", "abc123"]).unwrap().all);
        // ...but a range widens, because its lower bound does.
        assert!(parse(&["--date-since", "2026-01-01", "--date-until", "2026-06-01"]).unwrap().all);
        assert!(parse(&["--commit-since", "abc", "--commit-until", "def"]).unwrap().all);
        // And an upper bound still takes --all when it is asked for.
        assert!(parse(&["--date-until", "2026-01-01", "--all"]).unwrap().all);

        // --author matches many commits and named none of them, so the slice
        // stays the question; --all is there to be typed.
        assert!(!parse(&["--author", "nino"]).unwrap().all);
        assert!(parse(&["--author", "nino", "--all"]).unwrap().all);

        // Nothing is implied without a selector, and a --union the user typed
        // is never overridden -- nor does the implied --all trip its guard.
        assert!(!parse(&[]).unwrap().all);
        let a = parse(&["--union", "--date", "2026-01-01"]).unwrap();
        assert!(a.union && !a.all);
    }

    #[test]
    fn commits_names_rows_outright() {
        // A list, a repeat, and both at once all reach the same place.
        assert!(parse(&[]).unwrap().commits.is_empty());
        assert_eq!(parse(&["--commits", "abc123"]).unwrap().commits, vec!["abc123"]);
        assert_eq!(parse(&["-c", "abc123"]).unwrap().commits, vec!["abc123"]);
        assert_eq!(
            parse(&["--commits", "abc123,def456"]).unwrap().commits,
            vec!["abc123", "def456"]
        );
        assert_eq!(
            parse(&["--commits=abc123", "-c", "def456"]).unwrap().commits,
            vec!["abc123", "def456"]
        );
        // An empty id would be a prefix of every sha, so the typo is named.
        assert!(parse(&["--commits", "abc,,def"]).unwrap_err().contains("bad commit list"));
        assert!(parse(&["--commits"]).unwrap_err().contains("needs one or more commits"));
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
}
