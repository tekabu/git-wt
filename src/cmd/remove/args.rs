use clap::Args;

use crate::cmd::args::{DeleteBranch, RemoveForceLegacy, SkipConfirm, WorktreeFlag};

/// Remove a worktree. Resolves to exactly one, so -- same reasoning as
/// `switch` -- no `-b/--branch` here to append a second target with; only
/// the positional and `-w/--worktree` name it, both singular.
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
}
