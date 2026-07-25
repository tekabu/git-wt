use clap::Args;

/// Merge a source branch into a worktree.
///
/// The one positional (`MergeOptions.source`) is genuinely ambiguous --
/// `merge 1` means "dest 1, source stays unnamed" and `merge feat/x` means
/// "dest is current, source is feat/x" -- and telling them apart needs the
/// live worktree list, so it stays a single `Option<String>` for `main.rs` to
/// resolve at dispatch time rather than a `target` field of its own here.
#[derive(Args, Debug)]
pub(crate) struct MergeArgs {
    /// Override for dest, the worktree `git merge` runs in -- current
    /// worktree if unset. One target only (# or branch); unlike every other
    /// verb's `-t`, the dest,source pair only ever comes from the positional.
    #[arg(short = 't', long = "target", value_name = "TARGET")]
    pub target_flag: Option<String>,

    /// Branch to merge in, the source `git merge` runs with. One target
    /// only (# or branch); unlike every other verb, merge's `-b` is not an
    /// "extra target" -- it names what gets merged in, not dest.
    #[arg(short, long, value_name = "BRANCH")]
    pub branch: Option<String>,

    #[command(flatten)]
    pub options: MergeOptions,
}

/// The merge-option vocabulary. Every start-only flag declares
/// `conflicts_with_all` itself, so "X takes no other merge options" is
/// enforced by clap at parse time -- not a hand-built accumulator -- for
/// every one of them, `review` included: unlike a bare `--review <tail>`
/// hand-off, this `review` is a plain flag with no vocabulary of its own, so
/// there is nothing left to re-parse once clap has rejected everything it
/// conflicts with.
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
        conflicts_with_all = ["abort", "message", "no_ff", "ff_only", "squash", "force", "ours", "theirs", "dry_run", "review"]
    )]
    pub r#continue: bool,

    /// Undo a conflicted merge.
    #[arg(
        short = 'a',
        long,
        overrides_with = "abort",
        conflicts_with_all = ["message", "no_ff", "ff_only", "squash", "force", "ours", "theirs", "dry_run", "review"]
    )]
    pub abort: bool,

    /// Show the range this merge would bring over (`dest..src`) instead of
    /// running it. A mode, not an option: it takes no merge option of its
    /// own, and needs the same `SOURCE` a real merge does.
    #[arg(
        long,
        conflicts_with_all = ["message", "no_ff", "ff_only", "squash", "force", "ours", "theirs", "dry_run"]
    )]
    pub review: bool,

    /// Open meld on the files `dest..src` touches, instead of the table.
    /// Only means anything under `--review`.
    #[arg(long, requires = "review")]
    pub meld: bool,

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
