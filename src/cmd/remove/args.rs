use clap::Args;

use crate::cmd::args::{DeleteBranch, RemoveForceLegacy, SkipConfirm, WorktreeFlag};

/// Remove one or more worktrees. Targets are comma- or space-separated
/// (`git-wt remove 1,2` or `git-wt remove 1 2`); no `-b/--branch` here to
/// append a second target with -- the positional list and `-w/--worktree`
/// already cover it between them.
#[derive(Args, Debug)]
pub(crate) struct RemoveArgs {
    /// Worktree number(s) or branch name(s) to remove, comma- or
    /// space-separated.
    #[arg(value_name = "TARGET", num_args = 0..)]
    pub target: Vec<String>,

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
