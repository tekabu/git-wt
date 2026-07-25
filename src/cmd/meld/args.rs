use clap::Args;

use crate::cmd::args::{BranchTargets, MeldBase, MeldDiffOnly, TargetListFlag, ThreeWay};

/// Open meld on 2-3 worktree directories.
#[derive(Args, Clone, Debug, Default)]
pub(crate) struct MeldArgs {
    /// The worktrees to compare, e.g. `1,2` or `1,2,3`.
    pub targets: Option<String>,

    #[command(flatten)]
    pub target_flag: TargetListFlag,

    #[command(flatten)]
    pub diff: MeldDiffOnly,

    #[command(flatten)]
    pub three_way: ThreeWay,

    #[command(flatten)]
    pub base: MeldBase,

    /// Diff only: `..` (tip-vs-tip, default under --diff) or `...` (fork).
    #[arg(value_name = "RANGE")]
    pub range: Option<String>,

    #[command(flatten)]
    pub branch: BranchTargets,
}
