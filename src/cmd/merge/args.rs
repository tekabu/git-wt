use clap::{ArgAction, Args};

/// Merge a source branch into a worktree.
#[derive(Args, Debug)]
pub(crate) struct MergeArgs {
    /// Destination worktree, optionally with source worktree (`1` or `1,2`),
    /// followed by merge options, source branch/number, resume flags
    /// (`--continue`, `--abort`), and the `--review` hand-off. When the first
    /// token resolves as a worktree list it is consumed as the target;
    /// otherwise the current worktree is used and the whole tail passed
    /// through as options. The source goes in the comma list (`1,feat/x`) or
    /// in `-b` (`1 -b feat/x`), never in a separate bare word (`1 feat/x`).
    #[arg(allow_hyphen_values = true, num_args = 0.., value_name = "TARGETS/OPTIONS")]
    pub rest: Vec<String>,

    /// The one source branch to merge. Unlike every other verb, merge's `-b`
    /// is not an "extra target": it names what gets merged in.
    #[arg(short, long, action = ArgAction::Append, value_name = "BRANCH")]
    pub branch: Vec<String>,
}

/// The merge-option vocabulary: what `parse_merge_args` re-parses `rest` (with
/// `--review`'s tail already sliced off, untouched -- it hands off to a wholly
/// different vocabulary, commits' filters) into via clap, in place of a
/// hand-written token loop.
///
/// Every start-only flag declares `conflicts_with_all = ["r#continue" ->
/// "continue", "abort"]` itself, so "continue/abort takes no merge options"
/// is enforced by clap at parse time -- not a hand-built accumulator -- for
/// every one of them except `review`, which isn't a field here at all (its
/// tail is sliced off before this struct ever sees the tokens) and whose
/// "review takes no merge options" check stays a small accumulator in
/// `parse_merge_args` for exactly that reason.
#[derive(clap::Args, Debug, Default)]
pub(crate) struct MergeOptions {
    /// Source branch or worktree number to merge in.
    #[arg(value_name = "SOURCE", conflicts_with_all = ["continue", "abort"])]
    pub source: Option<String>,

    /// Conclude a conflicted merge.
    #[arg(
        short = 'c',
        long = "continue",
        overrides_with = "continue",
        conflicts_with_all = ["abort", "message", "no_ff", "ff_only", "squash", "force", "ours", "theirs", "dry_run"]
    )]
    pub r#continue: bool,

    /// Undo a conflicted merge.
    #[arg(
        short = 'a',
        long,
        overrides_with = "abort",
        conflicts_with_all = ["message", "no_ff", "ff_only", "squash", "force", "ours", "theirs", "dry_run"]
    )]
    pub abort: bool,

    /// Resolve conflicts by keeping the destination's side.
    #[arg(short = 'o', long, overrides_with = "ours", conflicts_with = "theirs")]
    pub ours: bool,

    /// Resolve conflicts by keeping the source's side.
    #[arg(long, overrides_with = "theirs")]
    pub theirs: bool,

    /// Preview the merge without committing it.
    #[arg(short = 'd', long = "dry-run", overrides_with = "dry_run")]
    pub dry_run: bool,

    /// Commit message for the merge.
    #[arg(short = 'm', long, overrides_with = "message")]
    pub message: Option<String>,

    /// Always make a merge commit, even when a fast-forward would do.
    #[arg(long = "no-ff", alias = "nf", overrides_with = "no_ff", conflicts_with = "ff_only")]
    pub no_ff: bool,

    /// Refuse anything that isn't a fast-forward.
    #[arg(long = "ff-only", alias = "fo", overrides_with = "ff_only")]
    pub ff_only: bool,

    /// Squash the source's commits into one uncommitted change.
    #[arg(long, overrides_with = "squash", conflicts_with = "no_ff")]
    pub squash: bool,

    /// Force a merge commit even into a worktree with uncommitted changes.
    #[arg(short, long, overrides_with = "force")]
    pub force: bool,
}
