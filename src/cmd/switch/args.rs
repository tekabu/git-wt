use clap::Args;

/// Switch to a worktree.
#[derive(Args, Debug)]
pub(crate) struct SwitchArgs {
    /// Worktree number or branch name; see 'git-wt list' to pick one.
    pub target: Option<String>,

    /// Alternative spelling of the positional target; errors if both given.
    #[arg(short = 't', long = "target", value_name = "TARGET_LIST")]
    pub target_flag: Option<String>,
}

/// Print a worktree's path.
#[derive(Args, Debug)]
pub(crate) struct PathArgs {
    /// Worktree number or branch name; omit for the current worktree.
    pub target: Option<String>,

    /// Alternative spelling of the positional target; errors if both given.
    #[arg(short = 't', long = "target", value_name = "TARGET_LIST")]
    pub target_flag: Option<String>,
}
