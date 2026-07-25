use clap::{ArgAction, Args};

/// Remove a worktree.
#[derive(Args, Debug)]
pub(crate) struct RemoveArgs {
    /// Worktree number or branch name to remove.
    pub target: Option<String>,

    /// Alternative spelling of the positional target; errors if both given.
    #[arg(short = 't', long = "target", value_name = "TARGET")]
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

    /// Extra worktree targets, appended to this command's target list.
    /// Repeatable: `-b 2 -b 3` and `-b 2,3` mean the same thing.
    #[arg(short, long, action = ArgAction::Append, value_name = "TARGET_LIST")]
    pub branch: Vec<String>,

}
