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

/// `-x/--reference` (alias `--ref`), one struct, four different jobs
/// depending on who flattens it -- every current meaning, in one place so
/// nobody has to chase them through each verb's own file:
///
/// - **`commits`/`log`** (`CommonCommitsFlags.branch`): *appends* to the
///   worktree/branch list the positional/`-w` already named. Retired the
///   old `-b/--branch` here (`docs/note.txt`'s planned rename).
/// - **`meld`**: same append role, into the 2-3-worktree list `cmd_meld`
///   compares. Retired `meld`'s own old `-b/--branch` too.
/// - **`sync`**'s `fetch`/`pull`/`push` (`SyncCommon.branch`): same append
///   role again, into the worktree list `run_sync` acts on. Retired the
///   third copy of `-b/--branch`.
/// - **`compare`**: *not* append -- this is `compare`'s only source of a
///   ref at all (no positional), and `cmd_compare` enforces "exactly one"
///   itself (clap can't, since this struct is list-shaped) rather than via
///   `required = true` the way the retired `CompareRef` did.
/// - **`review`**: refused at runtime (`main.rs`: "review has no
///   '-x/--reference'") -- it has its own `-s/--source` for the one thing
///   `-x` would otherwise append to, and nothing else to append *to*.
///
/// The long form says `reference`, not `branch`, because of a real split in
/// how the value gets resolved, not just a naming choice: `compare` resolves
/// its own value directly via `git rev-parse`, which accepts anything git
/// does -- a bare sha included. Every other consumer above goes through the
/// shared `resolve_worktree_or_branch_list` (`cli.rs`), which only ever
/// tries a worktree number or an *existing worktree's* branch name -- a bare
/// sha errors there ("no worktree on branch '<sha>'"), it does not fall
/// through to a raw git rev. So "or a commit sha" is true for `compare`
/// alone, not the other four.
#[derive(Args, Clone, Debug, Default)]
pub(crate) struct ExtraRef {
    #[arg(
        short = 'x',
        long = "reference",
        alias = "ref",
        num_args = 1..,
        value_delimiter = ',',
        value_name = "REF_LIST"
    )]
    pub(crate) extra_ref: Vec<String>,
}

/// Alternative spelling of the leading positional target. `-w/--worktree`
/// (alias `--wt`), shared by `switch`/`path`/`remove`/`commits`/`log` -- the
/// one struct for all five now; the old plain `-t/--target` `TargetFlag` is
/// retired. `review` uses neither -- its destination is `-d/--destination`
/// (`SourceDest`), a different concept entirely (dest, not "alternative to
/// a positional target").
#[derive(Args, Debug, Default)]
pub(crate) struct WorktreeFlag {
    #[arg(short = 'w', long = "worktree", alias = "wt", value_name = "TARGET")]
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

/// Add the changed files under each displayed commit. Just the toggle --
/// whether the block appears at all. `-f` is free within `commits`/`log`/
/// `review`'s own vocabulary (nothing else there uses it), so this gets its
/// old short back.
#[derive(Args, Debug, Default)]
pub(crate) struct ShowFiles {
    #[arg(short = 'f', long = "files", visible_aliases = ["show-files", "sf"])]
    pub(crate) files: bool,
}

/// When a filter (`-p/--path`) narrowed the commits shown and trimmed their
/// file blocks to just the matches, widen each block back to every file
/// those commits touched instead. Distinct job from `ShowFiles`: this one
/// never turns the block *on* by itself -- it only says what's in it once
/// something else already has (`ShowFiles`, or `--path` implying it).
#[derive(Args, Debug, Default)]
pub(crate) struct ShowAllFiles {
    #[arg(long = "all-files", visible_alias = "af")]
    pub(crate) all_files: bool,
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

/// The count is optional -- a bare `--wrap-subject` means "the whole
/// subject". Long-only now: `-w` moved to `commits`/`log`'s `WorktreeFlag`
/// (`docs/note.txt`'s planned rename), so this short is retired the same
/// way `merge`'s `dry_run` gave up `-d` for `--destination`.
#[derive(Args, Debug, Default)]
pub(crate) struct SubjectWrap {
    #[arg(long = "wrap-subject", visible_aliases = ["wraps", "ws"], value_parser = parse_wrap, num_args = 0..=1, default_missing_value = "full", value_name = "N")]
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
    #[arg(short = 'a', long = "all", visible_alias = "ar")]
    pub(crate) show_all_rows: bool,
}

/// Only rows touching a path containing any of these, case-folded.
/// Comma- or space-separated, same acceptance as `CommitsShaFilter`'s `-i`.
/// `-p/--path`; `AddParentdir`'s and `Prune`'s shorts were retired to make
/// room within this vocabulary. `list`'s `ShowPathCol` also uses `-p/--path`
/// -- a different concept (column toggle vs. path-list filter) that never
/// shares a `Command` with this one, so it's a safe second user, not a
/// collision. `diff`'s own `DiffPathFilter` is retired outright in favor of
/// this one, its "exactly one path" rule now `cmd_diff`'s own check rather
/// than clap's `Option<String>` shape. Whether a caller wants "directory
/// only" or "full filename only" matching is likewise left to that caller --
/// this struct only collects the list.
#[derive(Args, Debug, Default)]
pub(crate) struct PathFilter {
    #[arg(
        short = 'p',
        long = "path",
        num_args = 1..,
        value_delimiter = ',',
        value_name = "PATH_LIST"
    )]
    pub(crate) filename: Vec<String>,
}

/// `review`-only: keep merge commits by default is `review`'s whole reason
/// for a `merges` field at all, but the *word* that flips it back off is its
/// own -- `commits`/`log` have no such default to undo.
#[derive(Args, Debug, Default)]
pub(crate) struct NoMerges {
    #[arg(long = "no-merges", visible_aliases = ["nm", "no-merge"])]
    pub(crate) no_merges: bool,
}

// -- the rest of the verbs: `add`, `list`, `switch`, `path`, `remove`,
// `diff`, `meld`, `compare`, `sync` (fetch/pull/push), `doctor`. Same
// exercise as the commits table above -- each verb's live `args.rs` is
// untouched, this is only where a collision (same short/long, different
// meaning) gets caught before it's ever wired. Letters get reused a lot
// across this vocabulary (`-p`, `-n`, `-d`, `-r`, `-f` each mean 3-4
// different things depending on verb) -- that is fine as long as the two
// meanings never share one flattened `Command`, which is called out wherever
// it's worth a reader knowing.

// -- add --

#[derive(Args, Debug, Default)]
pub(crate) struct AddName {
    #[arg(short, long)]
    pub(crate) name: Option<String>,
}

#[derive(Args, Debug, Default)]
pub(crate) struct AddDirname {
    #[arg(long)]
    pub(crate) dirname: Option<String>,
}

/// `--parentdir`, long-only -- `-p` was retired here so `PathFilter` could
/// be the one user of the letter.
#[derive(Args, Debug, Default)]
pub(crate) struct AddParentdir {
    #[arg(long)]
    pub(crate) parentdir: Option<String>,
}

#[derive(Args, Debug, Default)]
pub(crate) struct AddFromRef {
    #[arg(long)]
    pub(crate) from: Option<String>,
}

/// Hidden shell-wrapper hint; the binary itself never reads it. `-s`
/// collides letter-wise with `SourceDest`'s `-s/--source` and `list`'s
/// `-s/--short` -- different verbs, never combined.
#[derive(Args, Debug, Default)]
pub(crate) struct AddStayHidden {
    #[arg(short, long, hide = true)]
    pub(crate) stay: bool,
}

// -- list --

#[derive(Args, Debug, Default)]
pub(crate) struct ColSelect {
    #[arg(short, long, value_name = "COLS")] // todo: space and comma sep, add alias cs
    pub(crate) col: Option<String>,
}

#[derive(Args, Debug, Default)]
pub(crate) struct LongOutput {
    #[arg(short, long)]
    pub(crate) long: bool,
}

/// `-s/--short`. Same letter as `SourceDest`'s `-s/--source` and `add`'s
/// `-s/--stay` -- different verbs, never combined.
#[derive(Args, Debug, Default)]
pub(crate) struct ShortOutput {
    #[arg(short, long)]
    pub(crate) short: bool,
}

/// `-p/--path`: include the directory/path *column*. Shared by `list` and
/// `merged` (its `--others` table's own path column, previously a bare
/// `-p/--show-path`). Safe to reuse `-p` despite `PathFilter` also claiming
/// it: neither `list` nor `merged` ever shares a `Command` with `commits`/
/// `log`/`review`/`diff`/`compare`, so the two never collide -- different
/// concept (a column toggle here, a path-list filter there), never the same
/// verb.
#[derive(Args, Debug, Default)]
pub(crate) struct ShowPathCol {
    #[arg(short = 'p', long = "path")]
    pub(crate) show_path: bool,
}

/// `-f/--files`: list uncommitted files under each *worktree*. A different
/// concept from `ShowFiles`' `-f/--files` (add the changed-files block under
/// each *commit*, `commits`/`log`/`review`) -- same letter/long, unrelated
/// meaning, never the same verb. `compare`'s own file list moved off `-f`
/// entirely, onto the shared `PathFilter` (`-p/--path`).
#[derive(Args, Debug, Default)]
pub(crate) struct ListFiles {
    #[arg(short, long)]
    pub(crate) files: bool,
}

#[derive(Args, Debug, Default)]
pub(crate) struct Pager {
    #[arg(long, visible_alias = "pager")]
    pub(crate) less: bool,
}

// -- remove --

#[derive(Args, Debug, Default)]
pub(crate) struct SkipConfirm {
    #[arg(short = 'y', long)]
    pub(crate) yes: bool,
}

/// `-D/--delete-branch`. Uppercase `-D` also names `DateExact`'s `--date`
/// short above -- different verbs, never combined.
#[derive(Args, Debug, Default)]
pub(crate) struct DeleteBranch {
    #[arg(short = 'D', long = "delete-branch")]
    pub(crate) delete_branch: bool,
}

/// `remove`'s current, still-lowercase `-f/--force`. Deliberately *not*
/// folded into the shared `Force` (`-F`) group yet -- see `Force`'s own doc
/// comment: this is the one that joins once it's uppercased to match.
/// Kept here anyway so the pending migration has a named home.
#[derive(Args, Debug, Default)]
pub(crate) struct RemoveForceLegacy { // todo: remove this use Force, wire to verb remove
    #[arg(short, long)]
    pub(crate) force: bool,
}

// -- diff --

#[derive(Args, Debug, Default)]
pub(crate) struct LiveDiff {
    #[arg(long, overrides_with = "live")]
    pub(crate) live: bool,
}

#[derive(Args, Debug, Default)]
pub(crate) struct HunkNumbers {
    #[arg(long, overrides_with = "hunks")]
    pub(crate) hunks: bool,
}

#[derive(Args, Debug, Default)]
pub(crate) struct AOnlyFiles {
    #[arg(short = 'A', long = "a-only", action = ArgAction::SetTrue, overrides_with = "a_only")]
    pub(crate) a_only: bool,
}

#[derive(Args, Debug, Default)]
pub(crate) struct BOnlyFiles {
    #[arg(short = 'B', long = "b-only", action = ArgAction::SetTrue, overrides_with = "b_only")]
    pub(crate) b_only: bool,
}

#[derive(Args, Debug, Default)]
pub(crate) struct NameOnly {
    #[arg(long = "name-only", overrides_with = "name_only")]
    pub(crate) name_only: bool,
}

#[derive(Args, Debug, Default)]
pub(crate) struct NameStatus {
    #[arg(long = "name-status", overrides_with = "name_status")]
    pub(crate) name_status: bool,
}

#[derive(Args, Debug, Default)]
pub(crate) struct DiffStat {
    #[arg(long, overrides_with = "stat")]
    pub(crate) stat: bool,
}

// -- meld (verb) --

/// `-d/--diff`: filter to files that differ. Lowercase `-d` collides
/// letter-wise with `SourceDest`'s `-d/--destination` -- different verbs,
/// never combined.
#[derive(Args, Clone, Debug, Default)]
pub(crate) struct MeldDiffOnly {
    #[arg(short, long, overrides_with = "diff")]
    pub(crate) diff: bool,
}

#[derive(Args, Clone, Debug, Default)]
pub(crate) struct ThreeWay {
    #[arg(long = "3way", visible_alias = "tw")]
    pub(crate) three_way: bool,
}

#[derive(Args, Clone, Debug, Default)]
pub(crate) struct MeldBase {
    #[arg(long, value_name = "REF")]
    pub(crate) base: Option<String>,
}

/// Explicit slot assignment, an alternative to naming worktrees by position
/// in the list: each of `--left`/`--right`/`--center` takes a worktree
/// number *or* a raw filesystem path (not just a worktree -- meld can
/// compare an arbitrary directory this way). `--center` is optional; when
/// it's given and `--right` isn't, `cmd_meld` treats `--center`'s value as
/// `--right` instead (a two-way meld with the "middle" slot doubling as the
/// right side) rather than erroring for a missing right.
#[derive(Args, Clone, Debug, Default)]
pub(crate) struct MeldPosition {
    #[arg(long, value_name = "PATH_OR_N")]
    pub(crate) left: Option<String>,

    #[arg(long, value_name = "PATH_OR_N")]
    pub(crate) right: Option<String>,

    #[arg(long, value_name = "PATH_OR_N")]
    pub(crate) center: Option<String>,
}

// -- compare --

// -- sync (fetch/pull/push) --

/// `-a/--all`: run in every worktree. A different concept from `AllRows`'
/// `-a/--all` (commits row source, full log vs. the default range) -- same
/// letter/long, unrelated meaning, never the same verb.
#[derive(Args, Clone, Debug, Default)]
pub(crate) struct SyncAll {
    #[arg(short, long, overrides_with = "all")]
    pub(crate) all: bool,
}

/// `--prune`, long-only, identical today on both `fetch` and `pull` -- a real
/// shared pair within `sync`'s own domain. `-p` was retired here so
/// `PathFilter` could be the one user of the letter.
#[derive(Args, Debug, Default)]
pub(crate) struct Prune {
    #[arg(long, overrides_with = "prune")]
    pub(crate) prune: bool,
}

/// `fetch`-only: `tags`/`no_tags` always travel as a pair (the exclusion is
/// declared on `tags`). `push` has its own bare `tags` with no such partner
/// (see `PushTags`), so this stays fetch's own rather than one shared field.
#[derive(Args, Debug, Default)]
pub(crate) struct FetchTags {
    #[arg(long, overrides_with = "tags", conflicts_with = "no_tags")]
    pub(crate) tags: bool,

    #[arg(long = "no-tags", alias = "nt", overrides_with = "no_tags")]
    pub(crate) no_tags: bool,
}

#[derive(Args, Debug, Default)]
pub(crate) struct PushTags {
    #[arg(long, overrides_with = "tags")]
    pub(crate) tags: bool,
}

/// `pull`-only. `rebase`'s `conflicts_with_all` names `ff_only`, which lives
/// on the separate `PullFfOnly` below -- fine as long as both end up
/// flattened into the same `Command`, same rule as `ResumeOp`.
#[derive(Args, Debug, Default)]
pub(crate) struct PullRebase {
    #[arg(long, alias = "rb", overrides_with = "rebase", conflicts_with_all = ["no_rebase", "ff_only"])]
    pub(crate) rebase: bool,

    #[arg(long = "no-rebase", alias = "nr", overrides_with = "no_rebase")]
    pub(crate) no_rebase: bool,
}

/// `pull`'s own `--ff-only`, no short. `merge` has an `--ff-only` too (with
/// an `-fo` alias) meaning almost the same thing -- refuse a non-fast-
/// forward -- for a different operation; not unified into one struct here
/// since that would mean touching `merge/args.rs`, out of scope for this
/// pass, just noted as a plausible future unification.
#[derive(Args, Debug, Default)]
pub(crate) struct PullFfOnly {
    #[arg(long = "ff-only", overrides_with = "ff_only")]
    pub(crate) ff_only: bool,
}

#[derive(Args, Debug, Default)]
pub(crate) struct Autostash {
    #[arg(long, alias = "as", overrides_with = "autostash")]
    pub(crate) autostash: bool,
}

#[derive(Args, Debug, Default)]
pub(crate) struct SetUpstream {
    #[arg(short = 'u', long, overrides_with = "set_upstream")]
    pub(crate) set_upstream: bool,
}

#[derive(Args, Debug, Default)]
pub(crate) struct ForceWithLease {
    #[arg(long = "force-with-lease", alias = "fl", overrides_with = "force_with_lease")]
    pub(crate) force_with_lease: bool,
}

/// Preview without doing it. `--dry-run`, long-only, shared by `merge` and
/// `push`. `push`'s own hidden `-F/--force` rejection stub stays local to
/// `sync/args.rs` (see `Force`'s doc comment) -- unrelated to this one.
#[derive(Args, Debug, Default)]
pub(crate) struct DryRun {
    #[arg(long = "dry-run", overrides_with = "dry_run")]
    pub(crate) dry_run: bool,
}

// -- merged --

/// List every worktree and whether it is already merged into the target,
/// instead of the single reference-vs-source check. `merged`-only; nothing
/// else in this file uses `-o`.
#[derive(Args, Debug, Default)]
pub(crate) struct MergedOthers {
    #[arg(short, long)]
    pub(crate) others: bool,
}

// -- doctor --

/// `-r/--repair`. `compare` no longer sits on `-r` (its ref moved to
/// `ExtraRef`'s `-x/--reference`) -- this is now the one user of the letter.
#[derive(Args, Debug, Default)]
pub(crate) struct Repair {
    #[arg(short, long)]
    pub(crate) repair: bool,
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
        #[command(flatten)] branch: ExtraRef,
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
        #[command(flatten)] all_files: ShowAllFiles,
        #[command(flatten)] filename: PathFilter,
        #[command(flatten)] no_merges: NoMerges,
    }

    #[derive(Parser)]
    struct CommitsVocab {
        #[command(flatten)] target_flag: WorktreeFlag,
        #[command(flatten)] branch: ExtraRef,
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
        #[command(flatten)] all_files: ShowAllFiles,
        #[command(flatten)] filename: PathFilter,
    }

    #[derive(Parser)]
    struct LogVocab {
        #[command(flatten)] target_flag: WorktreeFlag,
        #[command(flatten)] branch: ExtraRef,
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
