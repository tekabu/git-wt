use clap::{ArgAction, Args};

/// Switch to a worktree.
#[derive(Args, Debug)]
pub(crate) struct SwitchArgs {
    /// Worktree number or branch name; see 'git-wt list' to pick one.
    pub target: Option<String>,

    /// Alternative spelling of the positional target; errors if both given.
    #[arg(short = 't', long = "target", value_name = "TARGET")]
    pub target_flag: Option<String>,

    /// Extra worktree targets, appended to this command's target list.
    /// Repeatable: `-b 2 -b 3` and `-b 2,3` mean the same thing.
    #[arg(short, long, action = ArgAction::Append, value_name = "TARGET_LIST")]
    pub branch: Vec<String>,

}

/// Print a worktree's path.
#[derive(Args, Debug)]
pub(crate) struct PathArgs {
    /// Worktree number or branch name; omit for the current worktree.
    pub target: Option<String>,

    /// Alternative spelling of the positional target; errors if both given.
    #[arg(short = 't', long = "target", value_name = "TARGET")]
    pub target_flag: Option<String>,
}
