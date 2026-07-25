use clap::Args;

use crate::cmd::args::{ConflictSide, DryRun, Force, ResumeOp, SourceDest, Squash};

/// Merge a source branch into a worktree.
///
/// The one positional (`MergeOptions.source`) is the source, and it may name a
/// worktree number, a branch that has a worktree, or a branch that has none.
/// Telling those apart needs the live worktree list, so it stays a single
/// `Option<String>` for `main.rs` to resolve at dispatch time.
///
/// `-s/--source` and `-d/--destination` come from the shared `SourceDest` --
/// `review` flattens the same struct for the same pair.
#[derive(Args, Debug)]
pub(crate) struct MergeArgs {
    #[command(flatten)]
    pub sd: SourceDest,

    #[command(flatten)]
    pub options: MergeOptions,
}

/// The merge-option vocabulary. Every start-only flag declares
/// `conflicts_with_all` itself, so "X takes no other merge options" is
/// enforced by clap at parse time rather than by a hand-built accumulator.
#[derive(clap::Args, Debug, Default)]
pub(crate) struct MergeOptions {
    /// Source branch or worktree number to merge in.
    ///
    /// Only ever a source -- the destination is `-t/--target`, never this
    /// positional -- so a resume word, which takes no source, rules it out
    /// declaratively. Resuming in some other worktree is `-t <N> --continue`.
    #[arg(value_name = "SOURCE", conflicts_with_all = ["continue", "abort"])]
    pub source: Option<String>,

    /// Conclude or undo a conflicted merge; see `ResumeOp` for why its
    /// `conflicts_with_all` names merge's own vocabulary directly.
    #[command(flatten)]
    pub resume: ResumeOp,

    #[command(flatten)]
    pub side: ConflictSide,

    /// Preview the merge without committing it. Shared `DryRun` (`-n`, also
    /// `push`'s) -- gives merge a short spelling it didn't have before (its
    /// old `-d` moved to `-d/--destination`, see `SourceDest`).
    #[command(flatten)]
    pub dry_run: DryRun,

    /// Commit message for the merge.
    #[arg(short = 'm', long, overrides_with = "message")]
    pub message: Option<String>,

    /// Always make a merge commit, even when a fast-forward would do.
    ///
    /// Carries `squash`'s exclusion (rather than the reverse) since `Squash`
    /// is shared with `commits`/`log`/`review`, none of which have a
    /// `no_ff` field a `conflicts_with` could name; declaring the pair on
    /// either side is enough for clap to enforce it regardless of order.
    #[arg(
        long = "no-ff",
        alias = "nf",
        overrides_with = "no_ff",
        conflicts_with_all = ["ff_only", "squash"]
    )]
    pub no_ff: bool,

    /// Refuse anything that isn't a fast-forward.
    #[arg(long = "ff-only", alias = "fo", overrides_with = "ff_only")]
    pub ff_only: bool,

    #[command(flatten)]
    pub squash: Squash,

    #[command(flatten)]
    pub force: Force,
}
