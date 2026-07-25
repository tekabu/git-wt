use clap::Args;

use crate::cmd::args::{
    AOnlyFiles, BOnlyFiles, BranchTargets, DiffStat, HunkNumbers, LiveDiff, Meld,
    NameOnly, NameStatus, PathFilter, TargetListFlag,
};

/// Diff two worktrees.
#[derive(Args, Debug)]
pub(crate) struct DiffArgs {
    /// The two worktrees to compare, e.g. `1,2`.
    pub targets: Option<String>,

    #[command(flatten)]
    pub target_flag: TargetListFlag,

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

    #[command(flatten)]
    pub branch: BranchTargets,

    /// `..` (tip-vs-tip) or `...` (fork, the default); a single stray word
    /// here otherwise is a bad range or a path someone spelled the git way
    /// (`-- PATH...` or a bare path), both of which `cmd_diff` rejects itself
    /// so the message can say which. Not `allow_hyphen_values`: a real unknown
    /// flag like `-w` should stay clap's own rejection, not land here.
    #[arg(value_name = "RANGE")]
    pub range: Option<String>,
}
