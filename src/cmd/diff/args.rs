use clap::Args;

use crate::cmd::args::{
    AOnlyFiles, BOnlyFiles, DiffStat, HunkNumbers, LiveDiff, Meld, NameOnly, NameStatus,
    PathFilter, SourceDest,
};

/// Diff two worktrees. Grammar deliberately mirrors `merge`'s: the one bare
/// positional is always the *source* -- the other side to compare against
/// -- and the destination is never positional, only `-d/--destination`,
/// defaulting to the current worktree. No `-t/--target` (dropped: this
/// isn't a list of two-or-more, it's exactly a source and a dest, same as
/// `merge`) and no `-b/--branch` either (nothing left to append to, same
/// reason `merge` has none).
#[derive(Args, Debug)]
pub(crate) struct DiffArgs {
    /// The other worktree to compare against; defaults to the current one
    /// if `-d/--destination` also isn't given.
    pub targets: Option<String>,

    #[command(flatten)]
    pub sd: SourceDest,

    #[command(flatten)]
    pub live: LiveDiff,

    #[command(flatten)]
    pub hunks: HunkNumbers,

    #[command(flatten)]
    pub a_only: AOnlyFiles,

    #[command(flatten)]
    pub b_only: BOnlyFiles,

    #[command(flatten)]
    pub path: PathFilter,

    #[command(flatten)]
    pub meld: Meld,

    #[command(flatten)]
    pub name_only: NameOnly,

    #[command(flatten)]
    pub name_status: NameStatus,

    #[command(flatten)]
    pub stat: DiffStat,

    /// `..` (tip-vs-tip) or `...` (fork, the default); a single stray word
    /// here otherwise is a bad range or a path someone spelled the git way
    /// (`-- PATH...` or a bare path), both of which `cmd_diff` rejects itself
    /// so the message can say which. Not `allow_hyphen_values`: a real unknown
    /// flag like `-w` should stay clap's own rejection, not land here.
    #[arg(value_name = "RANGE")]
    pub range: Option<String>,
}
