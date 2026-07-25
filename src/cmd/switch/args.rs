use clap::Args;

use crate::cmd::args::{BranchTargets, WorktreeFlag};

/// Switch to a worktree.
#[derive(Args, Debug)]
pub(crate) struct SwitchArgs {
    /// Worktree number or branch name; see 'git-wt list' to pick one.
    pub target: Option<String>,

    #[command(flatten)]
    pub target_flag: WorktreeFlag,

    #[command(flatten)]
    pub branch: BranchTargets,
}

/// Print a worktree's path.
#[derive(Args, Debug)]
pub(crate) struct PathArgs {
    /// Worktree number or branch name; omit for the current worktree.
    pub target: Option<String>,

    #[command(flatten)]
    pub target_flag: WorktreeFlag,
}
