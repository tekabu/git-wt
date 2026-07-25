use clap::{ArgAction, Args};

use crate::cmd::commits::args::{
    parse_branchw, parse_date_filter, parse_limit, parse_pathw, parse_subjectw, parse_wrap, iso_date,
    BranchWidth, DateFilter as DateFilterValue, PathWidth, SubjectWidth, Wrap,
};

/// Conclude or undo a resumable multi-step git operation. Currently only
/// `merge` has one (a conflicted merge stopped mid-way).
///
/// The `conflicts_with_all` lists below are `merge`'s own vocabulary
/// (`message`/`no_ff`/`ff_only`/`squash`/`force`/`ours`/`theirs`/`dry_run`),
/// baked into this struct rather than left to the caller: clap's
/// conflict-enumeration ("cannot be used with: --message, --squash") only
/// reports every offender at once when they're all named from *one* arg's
/// own list, so keeping them here (instead of scattering one `conflicts_with`
/// onto each of those other fields) is what makes `merge --abort -m x
/// --squash` name both `--message` and `--squash` in a single error.
///
/// The tradeoff: a second verb flattening `ResumeOp` needs that exact set of
/// sibling field names (or clap panics at startup naming the missing one) --
/// fine while `merge` is the only consumer, worth splitting or
/// parameterizing the day a second one actually shows up with a different
/// vocabulary.
#[derive(Args, Debug, Default)]
pub(crate) struct ResumeOp {
    /// Conclude a conflicted operation.
    #[arg(
        short = 'c',
        long = "continue",
        overrides_with = "continue",
        conflicts_with_all = ["abort", "message", "no_ff", "ff_only", "squash", "force", "ours", "theirs", "dry_run"]
    )]
    pub(crate) r#continue: bool,

    /// Undo a conflicted operation.
    #[arg(
        short = 'a',
        long,
        overrides_with = "abort",
        conflicts_with_all = ["message", "no_ff", "ff_only", "squash", "force", "ours", "theirs", "dry_run"]
    )]
    pub(crate) abort: bool,
}

/// Which side wins a conflicting hunk. Generic across any two-way compare
/// that can conflict -- `merge` is the one verb that has this today, but the
/// pair reads the same way for a future rebase or cherry-pick wrapper.
#[derive(Args, Debug, Default)]
pub(crate) struct ConflictSide {
    /// Resolve conflicts by keeping the destination's side.
    #[arg(long, overrides_with = "ours", conflicts_with = "theirs")]
    pub(crate) ours: bool,

    /// Resolve conflicts by keeping the source's side.
    #[arg(long, overrides_with = "theirs")]
    pub(crate) theirs: bool,
}

/// Override the safety check an operation would otherwise refuse to run
/// past. `-F/--force`, uppercase, shared by every verb where it means
/// exactly this -- `merge` (force a merge commit into a dirty worktree) and
/// `sync`'s fetch (force a non-fast-forward update to a remote-tracking
/// ref) today; `remove` joins once its own `-f/--force` is uppercased to
/// match.
///
/// `sync`'s push does *not* flatten this: its `-F/--force` is a deliberately
/// hidden stub (`sync/args.rs`) that exists only to give the letter a real
/// rejection message instead of clap's generic "unknown argument" --
/// push's actual force option is `--force-with-lease`. That is a different
/// arg (hidden, no real effect) wearing the same letter on purpose, not a
/// second user of this one, so it stays a local declaration.
#[derive(Args, Debug, Default)]
pub(crate) struct Force {
    #[arg(short = 'F', long, overrides_with = "force")]
    pub(crate) force: bool,
}

/// Stage/consolidate instead of leaving the normal per-item output: `merge`
/// squashes the source's commits into one uncommitted change; `commits`/
/// `log`/`review` squash the per-commit file blocks into one summary below
/// the table. Different effect, same bool, same letterless `--squash`
/// (alias `--sq`) everywhere -- worth one field instead of four copies.
///
/// `merge`'s `no_ff` conflict lives on `no_ff` itself
/// (`conflicts_with = "squash"`), not here: `commits` has no `no_ff` field,
/// so a `conflicts_with` naming it can't sit on this shared struct without
/// panicking there. Declaring it on either side of a pair is enough for
/// clap to enforce it regardless of order, so `no_ff` alone carries it.
#[derive(Args, Debug, Default)]
pub(crate) struct Squash {
    #[arg(long, visible_alias = "sq")]
    pub(crate) squash: bool,
}

/// A merge-shaped source/destination pair: one branch or worktree number to
/// bring in, one worktree to bring it into. `merge` and `review` both ask
/// this exact question about the same pair (`review` just answers it
/// instead of acting on it), with identical letters and shape in both --
/// `-s/--source` as the second spelling of the positional source, and
/// `-d/--destination` (alias `--dest`) as the destination, defaulting to
/// the current worktree.
#[derive(Args, Debug, Default)]
pub(crate) struct SourceDest {
    /// Second spelling of the source branch or worktree number.
    #[arg(short = 's', long = "source", value_name = "SOURCE")]
    pub(crate) source_flag: Option<String>,

    /// Dest, defaulting to the current worktree. One target only (# or branch).
    #[arg(short = 'd', long = "destination", alias = "dest", value_name = "DEST")]
    pub(crate) destination_flag: Option<String>,
}

/// Open meld instead of printing the normal output. `-M/--meld`, uppercase,
/// shared by `compare`/`diff`/`review` -- all three had `-m, --meld` before
/// this, but lowercase `-m` collides with `--message` under `review`
/// (`ReviewFlags` flattens `CommonCommitsFlags`, which owns `-m`), which is
/// why `review`'s was long-only. Uppercasing frees the letter for all three
/// to share one declaration instead of `compare`/`diff` keeping `-m` and
/// `review` going without.
#[derive(Args, Debug, Default)]
pub(crate) struct Meld {
    #[arg(short = 'M', long, action = ArgAction::SetTrue, overrides_with = "meld")]
    pub(crate) meld: bool,
}

// -- the commits table's vocabulary: everything `commits`/`log`/`review` --
// currently reach via `CommonCommitsFlags`/`ReviewFlags`, pulled apart here
// field group by field group so a collision (like `-w` wanting to mean both
// `--wrap-subject` and a future `--worktree`) is visible at the declaration
// site instead of buried in one 30-field struct. None of this is wired into
// `commits`/`log`/`review` yet -- still `CommonCommitsFlags`'s own fields
// there, kept in sync by hand until each verb is switched over one at a time.

/// Extra worktree targets, appended to the target list. `commits`/`log` take
/// it; `review` refuses it at runtime (`main.rs`: "review has no
/// '-b/--branch'") since it has its own `-s/--source` for the one thing
/// `-b` would otherwise append to.
/// future: branch to ref
#[derive(Args, Debug, Default)]
pub(crate) struct ExtraBranch {
    #[arg(
        short = 'x', 
        long, 
        visible_aliases = ["extra", "ex"], 
        num_args = 1..,
        value_delimiter = ',', 
        value_name = "BRANCH_LIST")]
    pub(crate) extra_branch: Vec<String>,
}

/// Alternative spelling of the leading positional target. `commits`/`log`
/// declare this identically (both `#[arg(short = 't', long = "target")]`,
/// each on their own struct today, copy-pasted); `review` does *not* use
/// this -- its destination is `-d/--destination` (`SourceDest`), a different
/// letter and a different concept (dest, not "alternative to a positional
/// target"), so it never flattens this one.
/// future: retired
#[derive(Args, Debug, Default)]
pub(crate) struct TargetFlag {
    #[arg(short = 't', long = "target", value_name = "TARGET")]
    pub(crate) target_flag: Option<String>,
}

/// `log`-only: don't follow a rename, even with one path given. The escape
/// hatch for the empty-result note ("it may live under another name") --
/// an explicit opt-out for the one case `log` would otherwise always take,
/// a single path's history across whatever it used to be called.
#[derive(Args, Debug, Default)]
pub(crate) struct NoFollow {
    #[arg(long = "no-follow")]
    pub(crate) no_follow: bool,
}

/// `log`-only: columns its `path` column gets cut to. None is the default
/// cap; `Full` never cuts.
#[derive(Args, Debug, Default)]
pub(crate) struct PathWidthArg {
    #[arg(long = "path-width", visible_aliases = ["pathw", "pw"], value_parser = parse_pathw, value_name = "COLS")]
    pub(crate) pathw: Option<PathWidth>,
}

#[derive(Args, Debug, Default)]
pub(crate) struct Limit {
    #[arg(short = 'n', long, value_parser = parse_limit, value_name = "LIMIT")]
    pub(crate) limit: Option<usize>,
}

/// One exact day, `--date YYYY-MM-DD`. Repeatable (each is an AND); the
/// open-ended comparisons are `DateBounds`, not this.
#[derive(Args, Debug, Default)]
pub(crate) struct DateExact {
    #[arg(short = 'D', long = "date", value_parser = parse_date_filter, value_name = "DATE")]
    pub(crate) date: Vec<DateFilterValue>,
}

/// The two open-ended date bounds, always offered as a pair.
#[derive(Args, Debug, Default)]
pub(crate) struct DateBounds {
    #[arg(long = "date-since", visible_aliases = ["ds", "since"], value_parser = iso_date, value_name = "DATE")]
    pub(crate) date_since: Option<String>,

    #[arg(long = "date-until", visible_aliases = ["du", "until"], value_parser = iso_date, value_name = "DATE")]
    pub(crate) date_until: Option<String>,
}

/// The two commit-named bounds, always offered as a pair -- same shape as
/// `DateBounds`, but the bound is a commit (sha/ref), not a literal date.
#[derive(Args, Debug, Default)]
pub(crate) struct CommitBounds {
    #[arg(long = "commit-since", visible_alias = "cs", value_name = "COMMIT")]
    pub(crate) commit_since: Option<String>,

    #[arg(long = "commit-until", visible_alias = "cu", value_name = "COMMIT")]
    pub(crate) commit_until: Option<String>,
}

/// Only these commits, by sha prefix.
#[derive(Args, Debug, Default)]
pub(crate) struct CommitsShaFilter {
    #[arg(
        short = 'i',
        long = "commits",
        alias = "ids",
        num_args = 1..,
        value_delimiter = ',',
        value_name = "SHA_LIST"
    )]
    pub(crate) commits: Vec<String>,
}

#[derive(Args, Debug, Default)]
pub(crate) struct Author {
    #[arg(long = "author", visible_aliases = ["au", "dev", "user"], value_name = "NAME")]
    pub(crate) author: Option<String>,
}

/// Only commits whose subject or body contains this, case-folded. Distinct
/// from `merge`'s own `-m/--message` (the merge commit's message to write)
/// despite sharing the letter and long name -- same shape, unrelated
/// meaning, so this stays its own struct rather than merge's `MergeOptions`
/// field, never shared with `merge`.
#[derive(Args, Debug, Default)]
pub(crate) struct MessageFilter {
    #[arg(short = 'm', long, value_name = "MESSAGE")]
    pub(crate) message: Option<String>,
}

/// Highlight only -- never drops a row. Lit wherever it appears: sha,
/// author, date, subject, file paths.
#[derive(Args, Debug, Default)]
pub(crate) struct SearchFilter {
    #[arg(short = 'S', long = "search", value_name = "SEARCH")]
    pub(crate) search: Option<String>,
}

#[derive(Args, Debug, Default)]
pub(crate) struct TopoOrder {
    #[arg(long, visible_aliases = ["topo", "to"])]
    pub(crate) topo_order: bool,
}

/// Keep merge commits in the table. Off by default under `commits`/`log`; on
/// by default under `review` (a review's range is bounded by the merge about
/// to happen, and a merge inside it is the cargo) -- same field, opposite
/// default, so the default lives with each verb's own `finalize_commits_args`
/// call, not here.
#[derive(Args, Debug, Default)]
pub(crate) struct KeepMerges {
    #[arg(long)]
    pub(crate) merges: bool,
}

#[derive(Args, Debug, Default)]
pub(crate) struct Reverse {
    #[arg(long, visible_aliases = ["oldest-first", "of", "rev"])]
    pub(crate) reverse: bool,
}

#[derive(Args, Debug, Default)]
pub(crate) struct NoCherry {
    #[arg(long = "no-cherry", visible_alias = "nc")]
    pub(crate) no_cherry: bool,
}

#[derive(Args, Debug, Default)]
pub(crate) struct PickId {
    #[arg(long = "pick-id", visible_alias = "pi")]
    pub(crate) pick: bool,
}

/// Add the changed files under each displayed commit.
/// future: use as filenames list
#[derive(Args, Debug, Default)]
pub(crate) struct ShowFiles {
    #[arg(short = 'f', long)]
    pub(crate) files: bool,
}

/// Rows come from every worktree at once, not the first one's log alone.
/// `commits`/`log` take it; `review` refuses it at runtime (its rows are
/// always the fixed range `dest..src`, never a union of worktrees).
#[derive(Args, Debug, Default)]
pub(crate) struct UnionRows {
    #[arg(long, visible_alias = "un")]
    pub(crate) union: bool,
}

#[derive(Args, Debug, Default)]
pub(crate) struct ShowTime {
    #[arg(long)]
    pub(crate) time: bool,
}

#[derive(Args, Debug, Default)]
pub(crate) struct DateHuman {
    #[arg(long = "date-human", visible_alias = "dh")]
    pub(crate) date_human: bool,
}

/// The count is optional -- a bare `--wrap` means "the whole subject". Short
/// `-w`: a future `commits`/`log` rename of `-t/--target` to `-w/--worktree`
/// (see `docs/note.txt`) would collide with this letter -- exactly the kind
/// of clash this file exists to surface before it's wired anywhere.
#[derive(Args, Debug, Default)]
pub(crate) struct SubjectWrap {
    #[arg(short = 'w', long = "wrap-subject", visible_aliases = ["wraps", "ws"], value_parser = parse_wrap, num_args = 0..=1, default_missing_value = "full", value_name = "N")]
    pub(crate) wrap_subject: Option<Wrap>,
}

#[derive(Args, Debug, Default)]
pub(crate) struct SubjectWidthArg {
    #[arg(long = "subject-width", visible_aliases = ["subjw", "sw"], value_parser = parse_subjectw, value_name = "COLS")]
    pub(crate) subject_width: Option<SubjectWidth>,
}

#[derive(Args, Debug, Default)]
pub(crate) struct BranchWidthArg {
    #[arg(long = "branch-width", visible_aliases = ["branchw", "bw"], value_parser = parse_branchw, value_name = "COLS")]
    pub(crate) branch_width: Option<BranchWidth>,
}

/// The path is optional -- a bare `--md` means the timestamped default name.
#[derive(Args, Debug, Default)]
pub(crate) struct MdExport {
    #[arg(long, num_args = 0..=1, default_missing_value = "", value_name = "PATH")]
    pub(crate) md: Option<String>,
}

/// `commits`/`review` both declare this identically today (`log` doesn't,
/// since a path already answers what "the whole log" would): the row source
/// is either the merge-request-style default range, or the full log. Only
/// `commits` can actually turn it on -- `review`'s own `finalize_commits_args`
/// call refuses it, since a review's range is fixed at `dest..src`.
#[derive(Args, Debug, Default)]
pub(crate) struct AllRows {
    #[arg(short = 'a', long, visible_aliases = ["all", "ar"])]
    pub(crate) show_all_rows: bool,
}

/// Show every file a commit touched, not only the paths `--filename`
/// matched. `commits`/`review` both declare this identically.
#[derive(Args, Debug, Default)]
pub(crate) struct AllFiles {
    #[arg(long = "all-files", visible_alias = "af")]
    pub(crate) show_all_files: bool,
}

/// Only commits touching a path containing this, case-folded. `commits`'
/// value name is historically `FILENAME`; `review`'s is `TERM` -- cosmetic
/// only, unified here to `TERM` since neither reads as more correct than
/// the other.
/// future: accept a list of paths, locally handle if need only single value
#[derive(Args, Debug, Default)]
pub(crate) struct FilenameFilter {
    #[arg(long = "filename", visible_alias = "fn", value_name = "TERM")]
    pub(crate) filename: Option<String>,
}

/// `review`-only: keep merge commits by default is `review`'s whole reason
/// for a `merges` field at all, but the *word* that flips it back off is its
/// own -- `commits`/`log` have no such default to undo.
#[derive(Args, Debug, Default)]
pub(crate) struct NoMerges {
    #[arg(long = "no-merges", visible_aliases = ["nm", "no-merge"])]
    pub(crate) no_merges: bool,
}

#[cfg(test)]
mod collision_tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    /// `merge` is already wired to these groups (`MergeArgs`/`MergeOptions`),
    /// so this exercises the real struct: a regression guard, not a forecast.
    #[derive(Parser)]
    struct MergeWrap {
        #[command(flatten)]
        o: crate::cmd::merge::args::MergeArgs,
    }

    #[test]
    fn merge_has_no_flag_collisions() {
        MergeWrap::command().debug_assert();
    }

    /// `review`/`commits`/`log` aren't wired to these groups yet -- still
    /// `CommonCommitsFlags`/`ReviewFlags` in their own `args.rs` files. These
    /// three mock up the shape each would have *if* switched over, purely so
    /// a collision (or a `conflicts_with` naming a field that combo doesn't
    /// have) is caught here, before the wiring, rather than after.
    #[derive(Parser)]
    struct ReviewVocab {
        #[command(flatten)] sd: SourceDest,
        #[command(flatten)] meld: Meld,
        #[command(flatten)] branch: ExtraBranch,
        #[command(flatten)] limit: Limit,
        #[command(flatten)] date: DateExact,
        #[command(flatten)] date_bounds: DateBounds,
        #[command(flatten)] commit_bounds: CommitBounds,
        #[command(flatten)] shas: CommitsShaFilter,
        #[command(flatten)] author: Author,
        #[command(flatten)] message: MessageFilter,
        #[command(flatten)] search: SearchFilter,
        #[command(flatten)] topo: TopoOrder,
        #[command(flatten)] keep_merges: KeepMerges,
        #[command(flatten)] reverse: Reverse,
        #[command(flatten)] no_cherry: NoCherry,
        #[command(flatten)] pick: PickId,
        #[command(flatten)] files: ShowFiles,
        #[command(flatten)] squash: Squash,
        #[command(flatten)] union: UnionRows,
        #[command(flatten)] time: ShowTime,
        #[command(flatten)] date_human: DateHuman,
        #[command(flatten)] wrap: SubjectWrap,
        #[command(flatten)] subject_width: SubjectWidthArg,
        #[command(flatten)] branch_width: BranchWidthArg,
        #[command(flatten)] md: MdExport,
        #[command(flatten)] all: AllRows,
        #[command(flatten)] all_files: AllFiles,
        #[command(flatten)] filename: FilenameFilter,
        #[command(flatten)] no_merges: NoMerges,
    }

    #[derive(Parser)]
    struct CommitsVocab {
        #[command(flatten)] target_flag: TargetFlag,
        #[command(flatten)] branch: ExtraBranch,
        #[command(flatten)] limit: Limit,
        #[command(flatten)] date: DateExact,
        #[command(flatten)] date_bounds: DateBounds,
        #[command(flatten)] commit_bounds: CommitBounds,
        #[command(flatten)] shas: CommitsShaFilter,
        #[command(flatten)] author: Author,
        #[command(flatten)] message: MessageFilter,
        #[command(flatten)] search: SearchFilter,
        #[command(flatten)] topo: TopoOrder,
        #[command(flatten)] keep_merges: KeepMerges,
        #[command(flatten)] reverse: Reverse,
        #[command(flatten)] no_cherry: NoCherry,
        #[command(flatten)] pick: PickId,
        #[command(flatten)] files: ShowFiles,
        #[command(flatten)] squash: Squash,
        #[command(flatten)] union: UnionRows,
        #[command(flatten)] time: ShowTime,
        #[command(flatten)] date_human: DateHuman,
        #[command(flatten)] wrap: SubjectWrap,
        #[command(flatten)] subject_width: SubjectWidthArg,
        #[command(flatten)] branch_width: BranchWidthArg,
        #[command(flatten)] md: MdExport,
        #[command(flatten)] all: AllRows,
        #[command(flatten)] all_files: AllFiles,
        #[command(flatten)] filename: FilenameFilter,
    }

    #[derive(Parser)]
    struct LogVocab {
        #[command(flatten)] target_flag: TargetFlag,
        #[command(flatten)] branch: ExtraBranch,
        #[command(flatten)] limit: Limit,
        #[command(flatten)] date: DateExact,
        #[command(flatten)] date_bounds: DateBounds,
        #[command(flatten)] commit_bounds: CommitBounds,
        #[command(flatten)] shas: CommitsShaFilter,
        #[command(flatten)] author: Author,
        #[command(flatten)] message: MessageFilter,
        #[command(flatten)] search: SearchFilter,
        #[command(flatten)] topo: TopoOrder,
        #[command(flatten)] keep_merges: KeepMerges,
        #[command(flatten)] reverse: Reverse,
        #[command(flatten)] no_cherry: NoCherry,
        #[command(flatten)] pick: PickId,
        #[command(flatten)] files: ShowFiles,
        #[command(flatten)] squash: Squash,
        #[command(flatten)] union: UnionRows,
        #[command(flatten)] time: ShowTime,
        #[command(flatten)] date_human: DateHuman,
        #[command(flatten)] wrap: SubjectWrap,
        #[command(flatten)] subject_width: SubjectWidthArg,
        #[command(flatten)] branch_width: BranchWidthArg,
        #[command(flatten)] md: MdExport,
        #[command(flatten)] no_follow: NoFollow,
        #[command(flatten)] pathw: PathWidthArg,
    }

    #[test]
    fn review_vocab_has_no_flag_collisions() {
        ReviewVocab::command().debug_assert();
    }

    #[test]
    fn commits_vocab_has_no_flag_collisions() {
        CommitsVocab::command().debug_assert();
    }

    #[test]
    fn log_vocab_has_no_flag_collisions() {
        LogVocab::command().debug_assert();
    }
}
