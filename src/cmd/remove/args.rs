use clap::Args;

/// Remove a worktree.
#[derive(Args, Debug)]
pub(crate) struct RemoveArgs {
    /// Worktree number or branch name to remove.
    pub target: Option<String>,

    /// Alternative spelling of the positional target; errors if both given.
    #[arg(short = 't', long = "target", value_name = "TARGET_LIST")]
    pub target_flag: Option<String>,

    /// Skip the confirmation prompt.
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Discard uncommitted/untracked changes; with -D, force-delete the branch.
    #[arg(short, long)]
    pub force: bool,

    /// Delete the worktree's branch too.
    #[arg(short = 'D', long = "delete-branch")]
    pub delete_branch: bool,
}
