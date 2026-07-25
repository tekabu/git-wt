use clap::{ArgAction, Args};

use crate::ui::{BRANCH_MIN, MIN_TEXTW, PATH_MIN};



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
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// How wide a mark column's branch-name header gets cut to.
///
/// A branch name is not bounded the way a subject line is -- an issue-shaped
/// one can run past a hundred characters -- and unlike the subject, it is a
/// header: every row under it pays for its width, not just the one line. Cut
/// by default, so a long name cannot drag the marks and the subject off the
/// right edge on every row; `Full` opts back in when the whole name is the
/// point (piped to `grep`, or read off a terminal wide enough to spare it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BranchWidth {
    /// Exactly this many columns. Below it the header carries only an
    /// ellipsis, which names nothing -- see `parse_branchw`.
    Cols(usize),
    /// However many the branch name is. Nothing is cut.
    Full,
}

/// How wide `log`'s `path` column gets, when a single name runs past
/// `PATH_MAX`. A rename's or a multi-path call's *other* names already wrap
/// onto their own line rather than compete for this width -- this only
/// bounds one name at a time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PathWidth {
    /// Exactly this many columns. Below it a cut name says nothing -- see
    /// `parse_pathw`.
    Cols(usize),
    /// However many the path is. Nothing is cut.
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

/// Which of the three verbs that render through this parser and table asked
/// for it.
///
/// Not a `review: bool` any more: `log` is a third shape, not a second
/// variation on the first. It shares `Review`'s reason for existing (four
/// behaviours gated by one enum instead of a growing pile of bools) and adds
/// its own: `--filename`, `--all`, `--all-files` are not words `log` knows at
/// all, because the path already is what they would otherwise ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Commits,
    Review,
    Log,
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
    /// Only commits whose subject or body contains this, case-folded. A
    /// substring, not the subsequence --author matches with: a name is one word
    /// typed from memory, where a message is prose -- a subsequence over prose
    /// matches nearly all of it.
    pub(crate) message: Option<String>,
    /// Highlight only -- never drops a row. Lit wherever it appears: sha,
    /// author, date, subject, file paths. Unlike `--message` it names nothing
    /// to filter on, so a row with no match still prints, plain.
    pub(crate) search: Option<String>,
    /// Only commits touching a path containing this, case-folded.
    pub(crate) filename: Option<String>,
    /// Show every file a commit touched, not only the paths --filename
    /// matched. Off by default: the filter named a path, so the block answers
    /// with that path.
    pub(crate) all_files: bool,
    pub(crate) topo: bool,
    /// Keep merge commits. Off by default: a merge carries no work of its own,
    /// and on a branch that merges often they are most of the table. On under
    /// `merge --review`, where the range is bounded by the merge about to
    /// happen and a merge inside it is the cargo -- same reasoning, different
    /// range, opposite answer.
    pub(crate) merges: bool,
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
    /// Drop the per-commit file blocks for a single consolidated one below the
    /// table: every file the shown commits touched, its counts summed. The
    /// commits stay as rows -- this is a file view, not a squash of history.
    pub(crate) squash: bool,
    /// Terminal lines a subject may take. Moot off a terminal: nothing is cut.
    pub(crate) wrap: Wrap,
    /// Columns the subject gets. None lets the terminal decide, as it always has.
    pub(crate) subjectw: Option<SubjectWidth>,
    /// Columns a branch-name header gets cut to. None is the default cap
    /// (`BRANCH_HEAD_MAX`); `Full` never cuts.
    pub(crate) branchw: Option<BranchWidth>,
    /// `log` only: columns its `path` column gets cut to. None is the
    /// default cap (`PATH_MAX`); `Full` never cuts.
    pub(crate) pathw: Option<PathWidth>,
    /// `log` only: don't follow a rename, even with one path given. The
    /// escape hatch for the empty-result note ("it may live under another
    /// name") -- an explicit opt-out for the one case `log` would otherwise
    /// always take, a single path's history across whatever it used to be
    /// called.
    pub(crate) no_follow: bool,
}

/// The flags `commits` and `log` share, declared for real: clap knows every
/// one of these by name and rejects anything else itself, before either verb
/// ever runs -- the outer catch-all `rest`/`allow_hyphen_values` this used to
/// require is gone for these two verbs.
///
/// Validated values (a date, a width above its floor, a non-zero limit) keep
/// running through the same parser functions the hand-rolled loop above
/// uses, wired in as clap `value_parser`s -- one source of truth for what
/// each value means, whichever verb collected it. What is *not* kept is the
/// hand-rolled loop's bespoke wording for a missing or empty value (e.g.
/// "--author needs a name, e.g. '--author alex'"): clap's own message covers
/// that case now, in exchange for not hand-writing one closure per flag.
#[derive(Args, Debug, Default)]
pub(crate) struct CommonCommitsFlags {
    /// Extra worktree targets, appended to the target list.
    #[arg(short, long, action = ArgAction::Append, value_name = "TARGET_LIST")]
    pub(crate) branch: Vec<String>,
    #[arg(short = 't', long = "target", value_name = "TARGET_LIST")]
    pub(crate) target_flag: Option<String>,

    #[arg(short = 'n', long, value_parser = parse_limit, value_name = "N")]
    pub(crate) limit: Option<usize>,
    #[arg(short = 'd', long = "date", value_parser = parse_date_filter, value_name = "DATE")]
    pub(crate) date: Vec<DateFilter>,
    #[arg(long = "date-since", visible_alias = "ds", value_parser = iso_date, value_name = "DATE")]
    pub(crate) date_since: Option<String>,
    #[arg(long = "date-until", visible_alias = "du", value_parser = iso_date, value_name = "DATE")]
    pub(crate) date_until: Option<String>,
    #[arg(long = "commit-since", visible_alias = "cs", value_name = "COMMIT")]
    pub(crate) commit_since: Option<String>,
    #[arg(long = "commit-until", visible_alias = "cu", value_name = "COMMIT")]
    pub(crate) commit_until: Option<String>,
    #[arg(short = 'c', long = "commits", value_delimiter = ',', value_name = "IDS")]
    pub(crate) commits: Vec<String>,
    #[arg(long = "author", visible_alias = "au", value_name = "NAME")]
    pub(crate) author: Option<String>,
    #[arg(short = 'm', long, value_name = "TERM")]
    pub(crate) message: Option<String>,
    #[arg(long, value_name = "TERM")]
    pub(crate) search: Option<String>,

    #[arg(long, visible_alias = "topo-order")]
    pub(crate) topo: bool,
    #[arg(long)]
    pub(crate) merges: bool,
    #[arg(long, visible_alias = "oldest-first")]
    pub(crate) reverse: bool,
    #[arg(long = "no-cherry", visible_alias = "nc")]
    pub(crate) no_cherry: bool,
    #[arg(long = "pick-id", visible_alias = "pi")]
    pub(crate) pick: bool,
    #[arg(short = 'f', long)]
    pub(crate) files: bool,
    #[arg(long)]
    pub(crate) squash: bool,
    #[arg(long)]
    pub(crate) union: bool,
    #[arg(long)]
    pub(crate) time: bool,
    #[arg(long = "date-human", visible_alias = "dh")]
    pub(crate) date_human: bool,

    /// The count is optional -- a bare `--wrap` means "the whole subject".
    #[arg(short = 'w', long, value_parser = parse_wrap, num_args = 0..=1, default_missing_value = "full", value_name = "N")]
    pub(crate) wrap: Option<Wrap>,
    #[arg(long = "subject-width", visible_alias = "subjw", value_parser = parse_subjectw, value_name = "COLS")]
    pub(crate) subjectw: Option<SubjectWidth>,
    #[arg(long = "branch-width", visible_alias = "branchw", value_parser = parse_branchw, value_name = "COLS")]
    pub(crate) branchw: Option<BranchWidth>,
    /// The path is optional -- a bare `--md` means the timestamped default name.
    #[arg(long, num_args = 0..=1, default_missing_value = "", value_name = "PATH")]
    pub(crate) md: Option<String>,
}

/// `commits`' own flags, on top of what it shares with `log`.
#[derive(Args, Debug)]
pub(crate) struct CommitsFlags {
    #[arg(value_name = "TARGET")]
    pub(crate) target: Option<String>,
    #[command(flatten)]
    pub(crate) common: CommonCommitsFlags,

    #[arg(short = 'a', long)]
    pub(crate) all: bool,
    #[arg(long = "all-files", visible_alias = "af")]
    pub(crate) all_files: bool,
    #[arg(long = "filename", visible_alias = "fn", value_name = "TERM")]
    pub(crate) filename: Option<String>,
}

/// `log`'s own flags, on top of what it shares with `commits`.
///
/// `leading` is the run of non-flag tokens before the first `-`: an optional
/// target followed by any number of paths, or paths alone. Which is which
/// still needs the live worktree list to answer -- the same ambiguity
/// `main.rs` already resolves for every other raw-target verb -- so it stays
/// one positional `Vec<String>` rather than a `target` field of its own.
#[derive(Args, Debug)]
pub(crate) struct LogFlags {
    #[arg(value_name = "TARGET/PATH", num_args = 0..)]
    pub(crate) leading: Vec<String>,
    #[command(flatten)]
    pub(crate) common: CommonCommitsFlags,

    #[arg(long = "no-follow")]
    pub(crate) no_follow: bool,
    #[arg(long = "path-width", visible_alias = "pathw", value_parser = parse_pathw, value_name = "COLS")]
    pub(crate) pathw: Option<PathWidth>,
}

impl CommonCommitsFlags {
    /// Fold the shared fields into a `RawCommitsArgs`, leaving the
    /// verb-specific ones (`all`, `all_files`, `filename`, `no_follow`,
    /// `pathw`, and the merges default) for the caller to fill in.
    fn into_raw(self) -> RawCommitsArgs {
        let mut dates = self.date;
        if let Some(d) = self.date_since {
            dates.push(DateFilter { op: DateOp::Ge, date: d });
        }
        if let Some(d) = self.date_until {
            dates.push(DateFilter { op: DateOp::Le, date: d });
        }
        RawCommitsArgs {
            limit: self.limit,
            dates,
            commit_since: self.commit_since,
            commit_until: self.commit_until,
            commits: self.commits,
            author: self.author,
            message: self.message,
            search: self.search,
            filename: None,
            all_files: false,
            topo: self.topo,
            merges: self.merges,
            fmt: DateFmt { human: self.date_human, time: self.time },
            md: self.md.map(|s| if s.is_empty() { None } else { Some(s) }),
            reverse: self.reverse,
            no_cherry: self.no_cherry,
            pick: self.pick,
            union: self.union,
            all: false,
            files: self.files,
            squash: self.squash,
            wrap: self.wrap,
            subjectw: self.subjectw,
            branchw: self.branchw,
            pathw: None,
            no_follow: false,
        }
    }
}

impl CommitsFlags {
    pub(crate) fn into_args(self) -> Result<CommitsArgs, String> {
        let mut raw = self.common.into_raw();
        raw.all = self.all;
        raw.all_files = self.all_files;
        raw.filename = self.filename;
        finalize_commits_args(Mode::Commits, raw)
    }
}

impl LogFlags {
    pub(crate) fn into_args(self) -> Result<CommitsArgs, String> {
        let mut raw = self.common.into_raw();
        raw.no_follow = self.no_follow;
        raw.pathw = self.pathw;
        finalize_commits_args(Mode::Log, raw)
    }
}

/// `merge --review`'s own flags: the same shared vocabulary `commits`/`log`
/// declare, plus the three words only a review knows (`--no-merges`, `--all`,
/// `--all-files`, `--filename` -- the same set plain `commits` has, since a
/// review is that mode with the merges default flipped).
///
/// What is *not* kept from the old hand-rolled `Mode::Review` parser is
/// `merge_word_msg`'s cross-referenced wording for a merge option typed after
/// `--review` (`'--dry-run' and '--review' answer the same question'`, and
/// friends) -- declared here, a merge option like `--dry-run` is simply not a
/// word this struct knows, so it gets clap's own unknown-argument message
/// naming `review`, the same simplification `commits`/`log` already made.
///
/// `merge --review` no longer carries a tail at all (`--review` is a plain
/// bool on `MergeOptions`, conflicting with every other merge option), so
/// every field here is at its default in practice; the struct stays because
/// `finalize_commits_args` still needs the merges-kept-by-default rule and
/// the review-only row-source refusal, and a literal is a worse way to say
/// "these are the defaults" than the type that already means it.
#[derive(Args, Debug, Default)]
pub(crate) struct ReviewFlags {
    #[command(flatten)]
    pub(crate) common: CommonCommitsFlags,

    #[arg(long = "no-merges")]
    pub(crate) no_merges: bool,
    #[arg(short = 'a', long)]
    pub(crate) all: bool,
    #[arg(long = "all-files", visible_alias = "af")]
    pub(crate) all_files: bool,
    #[arg(long = "filename", visible_alias = "fn", value_name = "TERM")]
    pub(crate) filename: Option<String>,
}

impl ReviewFlags {
    pub(crate) fn into_args(self) -> Result<CommitsArgs, String> {
        let mut raw = self.common.into_raw();
        raw.merges = !self.no_merges;
        raw.all = self.all;
        raw.all_files = self.all_files;
        raw.filename = self.filename;
        finalize_commits_args(Mode::Review, raw)
    }
}

/// `CommitsArgs`, before the cross-field rules below have run: `wrap` still
/// unresolved to its mode default, `all`/`files`/`all_files` not yet folded
/// against the flags that imply or forbid them.
///
/// The declaratively-parsed `commits`/`log` paths (`CommitsFlags`/`LogFlags`)
/// build one of these directly from typed clap fields, same as the
/// hand-rolled token loop above does field by field; both hand it to
/// `finalize_commits_args` so the cross-field rules exist in exactly one
/// place.
pub(crate) struct RawCommitsArgs {
    pub(crate) limit: Option<usize>,
    pub(crate) dates: Vec<DateFilter>,
    pub(crate) commit_since: Option<String>,
    pub(crate) commit_until: Option<String>,
    pub(crate) commits: Vec<String>,
    pub(crate) author: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) search: Option<String>,
    pub(crate) filename: Option<String>,
    pub(crate) all_files: bool,
    pub(crate) topo: bool,
    pub(crate) merges: bool,
    pub(crate) fmt: DateFmt,
    pub(crate) md: Option<Option<String>>,
    pub(crate) reverse: bool,
    pub(crate) no_cherry: bool,
    pub(crate) pick: bool,
    pub(crate) union: bool,
    pub(crate) all: bool,
    pub(crate) files: bool,
    pub(crate) squash: bool,
    pub(crate) wrap: Option<Wrap>,
    pub(crate) subjectw: Option<SubjectWidth>,
    pub(crate) branchw: Option<BranchWidth>,
    pub(crate) pathw: Option<PathWidth>,
    pub(crate) no_follow: bool,
}

/// The cross-field rules every `CommitsArgs` source (the hand-rolled token
/// loop above, and the clap-declared `commits`/`log` structs) has to run
/// once its own flags are collected: `--review`'s row-source refusal,
/// `--pick-id`/`--no-cherry`, the implied `--all` a lower bound sets, the
/// `--message`/`--filename` implications on `wrap`/`files`, and
/// `--all-files`'s "needs something to widen" check.
///
/// Extracted from `parse_commits_args_with` so a struct built straight from
/// typed clap fields gets the same rules as one built token by token,
/// without a second copy of them.
pub(crate) fn finalize_commits_args(mode: Mode, raw: RawCommitsArgs) -> Result<CommitsArgs, String> {
    let review = mode == Mode::Review;
    let RawCommitsArgs {
        limit, dates, commit_since, commit_until, commits, author, message, search, filename,
        all_files, topo, merges, fmt, md, reverse, no_cherry, pick, union, mut all, mut files,
        squash, wrap, subjectw, branchw, pathw, no_follow,
    } = raw;
    if review {
        for (flag, what) in [(all, "--all"), (union, "--union")] {
            if flag {
                return Err(format!(
                    "no '{what}' under '--review': the rows are the range 'dest..src', \
                     which is the one source a review has"
                ));
            }
        }
    }
    if pick && no_cherry {
        return Err(
            "--pick-id needs the patch comparison that --no-cherry skips: drop one of them"
                .to_string(),
        );
    }
    if all && union {
        return Err("--all and --union are two different row sources: use one of them".into());
    }
    let names_a_floor = !commits.is_empty()
        || commit_since.is_some()
        || dates.iter().any(|d| d.op != DateOp::Le);
    all = all || (names_a_floor && !union);

    let wrap = wrap.unwrap_or(if message.is_some() { Wrap::Full } else { Wrap::Lines(1) });
    files = files || filename.is_some();
    if all_files && filename.is_none() {
        return Err(ALL_FILES_MSG.into());
    }

    Ok(CommitsArgs {
        limit, dates, commit_since, commit_until, commits, author, message, search, filename, all_files,
        topo, merges, fmt, md,
        reverse, no_cherry, pick, union,
        all, files, squash, wrap, subjectw, branchw, pathw, no_follow,
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
            "--subject-width needs {MIN_TEXTW} columns or more: below that, a cut subject says nothing\n  got: '{n}'"
        )),
        _ => Err(format!("{SUBJW_BAD}\n  got: '{v}'")),
    }
}

/// Read `--branch-width`'s value: a column count, or 'full' for no cut at all.
pub(crate) fn parse_branchw(v: &str) -> Result<BranchWidth, String> {
    if v.eq_ignore_ascii_case("full") || v.eq_ignore_ascii_case("all") {
        return Ok(BranchWidth::Full);
    }
    match v.parse::<usize>() {
        // Below BRANCH_MIN a cut name is indistinguishable from any other
        // branch cut to the same head -- the column would stop telling them
        // apart, which is the one job a header has.
        Ok(n) if n >= BRANCH_MIN => Ok(BranchWidth::Cols(n)),
        Ok(n) if n > 0 => Err(format!(
            "--branch-width needs {BRANCH_MIN} columns or more: below that, a cut name says nothing\n  got: '{n}'"
        )),
        _ => Err(format!("{BRANCHW_BAD}\n  got: '{v}'")),
    }
}

/// Read `--path-width`'s value: a column count, or 'full' for no cut at all.
pub(crate) fn parse_pathw(v: &str) -> Result<PathWidth, String> {
    if v.eq_ignore_ascii_case("full") || v.eq_ignore_ascii_case("all") {
        return Ok(PathWidth::Full);
    }
    match v.parse::<usize>() {
        Ok(n) if n >= PATH_MIN => Ok(PathWidth::Cols(n)),
        Ok(n) if n > 0 => Err(format!(
            "--path-width needs {PATH_MIN} columns or more: below that, a cut path says nothing\n  got: '{n}'"
        )),
        _ => Err(format!("{PATHW_BAD}\n  got: '{v}'")),
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

pub(crate) const WRAP_BAD: &str = "--wrap needs a line count of 1 or more, or 'full', e.g. '--wrap 2'";
pub(crate) const SUBJW_BAD: &str = "--subject-width needs a column count, or 'full', e.g. '--subject-width 80'";
pub(crate) const BRANCHW_BAD: &str = "--branch-width needs a column count, or 'full', e.g. '--branch-width 20'";
pub(crate) const PATHW_BAD: &str = "--path-width needs a column count, or 'full', e.g. '--path-width 60'";
pub(crate) const ALL_FILES_MSG: &str =
    "--all-files needs a '--filename TERM' to widen: on its own the file block is already whole";

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
    let shown = if bare.is_empty() { "2026-01-01" } else { bare };
    format!("no '{op}' in --date; it takes one day, e.g. '--date {shown}'")
}

/// Validate a `YYYY-MM-DD` date, which is the only shape the compare is sound
/// for: shorter spellings would compare as prefixes and quietly mean something
/// else.
pub(crate) fn iso_date(s: &str) -> Result<String, String> {
    let bad = || {
        // An empty value usually means the shell ate an unquoted '>' -- which
        // no longer belongs here at all, so say where the bounds live.
        if s.is_empty() {
            "a date is missing; want YYYY-MM-DD".to_string()
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
    let (y, m, d) = (num(0..4), num(5..7), num(8..10));
    if !(1..=12).contains(&m) {
        return Err(format!("no such date '{s}'"));
    }
    // Day bound is month-specific: a flat 1..=31 would pass 2026-02-31, which
    // then matches nothing and reads as "no commits" rather than a typo.
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let last = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if leap => 29,
        _ => 28,
    };
    if !(1..=last).contains(&d) {
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
    use clap::Parser;

    #[derive(Parser)]
    struct CommitsWrap {
        #[command(flatten)]
        o: CommitsFlags,
    }
    #[derive(Parser)]
    struct LogWrap {
        #[command(flatten)]
        o: LogFlags,
    }
    #[derive(Parser)]
    struct ReviewWrap {
        #[command(flatten)]
        o: ReviewFlags,
    }

    fn commits(args: &[&str]) -> Result<CommitsArgs, String> {
        let argv = std::iter::once("commits".to_string()).chain(args.iter().map(|s| s.to_string()));
        CommitsWrap::try_parse_from(argv).map_err(|e| e.to_string())?.o.into_args()
    }
    fn log(args: &[&str]) -> Result<CommitsArgs, String> {
        let argv = std::iter::once("log".to_string()).chain(args.iter().map(|s| s.to_string()));
        LogWrap::try_parse_from(argv).map_err(|e| e.to_string())?.o.into_args()
    }
    fn review(args: &[&str]) -> Result<CommitsArgs, String> {
        let argv = std::iter::once("review".to_string()).chain(args.iter().map(|s| s.to_string()));
        ReviewWrap::try_parse_from(argv).map_err(|e| e.to_string())?.o.into_args()
    }

    #[test]
    fn iso_date_rejects_days_the_month_does_not_have() {
        assert!(iso_date("2026-02-31").is_err());
        assert!(iso_date("2026-04-31").is_err());
        assert!(iso_date("2026-02-29").is_err(), "2026 is not a leap year");
        assert!(iso_date("2026-13-01").is_err());
        assert!(iso_date("2026-01-00").is_err());

        assert!(iso_date("2026-01-31").is_ok());
        assert!(iso_date("2026-04-30").is_ok());
        assert!(iso_date("2026-02-28").is_ok());
        assert!(iso_date("2024-02-29").is_ok());
        assert!(iso_date("2000-02-29").is_ok());
        assert!(iso_date("1900-02-29").is_err());
    }

    #[test]
    fn date_takes_one_day_and_no_operator() {
        let f = |s: &str| parse_date_filter(s).unwrap();
        assert_eq!(f("2026-01-01"), DateFilter { op: DateOp::Eq, date: "2026-01-01".into() });
        for given in [">=2026-01-01", ">2026-01-01", "=2026-01-01", "<=2026-01-01", "<2026-01-01"] {
            let err = parse_date_filter(given).unwrap_err();
            assert!(err.contains("in --date"), "{given}: {err}");
            assert!(err.contains("2026-01-01"), "{given}: {err}");
        }
        assert!(parse_date_filter("2026-1-1").unwrap_err().contains("want YYYY-MM-DD"));
        assert!(parse_date_filter("").unwrap_err().contains("a date is missing"));
    }

    #[test]
    fn date_filters_compare_iso_dates_as_text() {
        let admits = |op: DateOp, d: &str, key: &str| DateFilter { op, date: d.into() }.admits(key);
        assert!(admits(DateOp::Ge, "2026-03-01", "2026-03-01"));
        assert!(admits(DateOp::Le, "2026-03-01", "2026-03-01"));
        assert!(!admits(DateOp::Ge, "2026-03-02", "2026-03-01"));
        assert!(admits(DateOp::Ge, "2026-01-01", "2026-10-01"));
    }

    #[test]
    fn parse_limit_rejects_zero_and_non_numbers() {
        assert_eq!(parse_limit("20").unwrap(), 20);
        assert!(parse_limit("0").unwrap_err().contains("show nothing"));
        assert!(parse_limit("x").unwrap_err().contains("bad count 'x'"));
    }

    #[test]
    fn widths_enforce_their_floor() {
        assert!(parse_subjectw("8").unwrap_err().contains("columns or more"));
        assert_eq!(parse_subjectw("full").unwrap(), SubjectWidth::Full);
        assert!(parse_branchw("8").unwrap_err().contains("columns or more"));
        assert!(parse_pathw("4").unwrap_err().contains("columns or more"));
        assert!(parse_wrap("0").unwrap_err().contains("1 or more"));
        assert_eq!(parse_wrap("full").unwrap(), Wrap::Full);
    }

    #[test]
    fn commits_rejects_undeclared_flags() {
        assert!(commits(&["1", "--oneline"]).is_err());
        assert!(commits(&["1", "--stat"]).is_err());
    }

    #[test]
    fn commits_knows_its_own_flags() {
        assert_eq!(commits(&["--limit", "5"]).unwrap().limit, Some(5));
        assert!(commits(&["--all"]).unwrap().all);
        assert!(commits(&["--filename", "ui.rs"]).unwrap().filename.is_some());
        assert!(commits(&["--filename", "ui.rs", "--all-files"]).unwrap().all_files);
        assert!(commits(&["--all-files"]).unwrap_err().contains("--filename"));
        assert!(commits(&["--pick-id", "--no-cherry"]).unwrap_err().contains("drop one of them"));
        assert!(commits(&["--all", "--union"]).unwrap_err().contains("--union"));
    }

    #[test]
    fn commits_bundles_short_flags() {
        let a = commits(&["-af"]).unwrap();
        assert!(a.all && a.files);
    }

    #[test]
    fn commits_optional_value_flags() {
        assert_eq!(commits(&[]).unwrap().wrap, Wrap::Lines(1));
        assert_eq!(commits(&["--wrap"]).unwrap().wrap, Wrap::Full);
        assert_eq!(commits(&["--wrap", "2"]).unwrap().wrap, Wrap::Lines(2));
        assert_eq!(commits(&["--md"]).unwrap().md, Some(None));
        assert_eq!(commits(&["--md", "out.md"]).unwrap().md, Some(Some("out.md".into())));
    }

    #[test]
    fn log_does_not_know_the_flags_the_path_already_answers() {
        for w in ["--filename", "--all", "-a", "--all-files"] {
            assert!(log(&[w]).is_err(), "{w}");
        }
        assert!(commits(&["--all"]).unwrap().all);
        assert!(log(&["--union"]).unwrap().union);
        assert!(log(&["-f"]).unwrap().files);
    }

    #[test]
    fn log_only_words() {
        assert!(!log(&[]).unwrap().no_follow);
        assert!(log(&["--no-follow"]).unwrap().no_follow);
        assert!(commits(&["--no-follow"]).is_err());
        assert_eq!(log(&["--path-width", "60"]).unwrap().pathw, Some(PathWidth::Cols(60)));
        assert!(commits(&["--path-width", "60"]).is_err());
    }

    #[test]
    fn log_leading_run_is_the_positional() {
        let a = LogWrap::try_parse_from(["log", "src/a.rs", "src/b.rs", "--union"]).unwrap();
        assert_eq!(a.o.leading, vec!["src/a.rs".to_string(), "src/b.rs".to_string()]);
        assert!(a.o.common.union);
    }

    #[test]
    fn review_merges_default_to_kept() {
        assert!(review(&[]).unwrap().merges);
        assert!(!review(&["--no-merges"]).unwrap().merges);
        assert!(!commits(&[]).unwrap().merges);
    }

    #[test]
    fn review_refuses_the_flags_that_would_redefine_its_range() {
        let e = review(&["--all"]).unwrap_err();
        assert!(e.contains("no '--all' under '--review'"), "{e}");
        assert!(e.contains("dest..src"), "{e}");
        assert!(review(&["-f"]).unwrap().files);
    }

    #[test]
    fn review_rejects_merge_vocabulary_as_an_unknown_argument() {
        // The bespoke cross-referenced wording is gone with the hand-rolled
        // parser; a merge option here is simply not a word `review` knows.
        assert!(review(&["--dry-run"]).is_err());
        assert!(review(&["--continue"]).is_err());
    }
}
