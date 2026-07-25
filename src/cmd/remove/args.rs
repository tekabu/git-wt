use clap::Args;

use crate::cmd::args::{BranchTargets, DeleteBranch, RemoveForceLegacy, SkipConfirm, WorktreeFlag};

/// Remove a worktree.
#[derive(Args, Debug)]
pub(crate) struct RemoveArgs {
    /// Worktree number or branch name to remove.
    pub target: Option<String>,

    #[command(flatten)]
    pub target_flag: WorktreeFlag,

    #[command(flatten)]
    pub yes: SkipConfirm,

    /// Discard uncommitted/untracked changes; with -D, force-delete the branch.
    #[command(flatten)]
    pub force: RemoveForceLegacy,

    #[command(flatten)]
    pub delete_branch: DeleteBranch,

    #[command(flatten)]
    pub branch: BranchTargets,
}
