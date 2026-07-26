use clap::Args;

use crate::cmd::args::WorktreeFlag;

/// Switch to a worktree. Resolves to exactly one, so unlike every list-
/// accepting verb, there is no `-b/--branch` here to append a second target
/// with -- only the positional and `-w/--worktree` name it, both singular.
#[derive(Args, Debug)]
pub(crate) struct SwitchArgs {
    /// Worktree number or branch name; see 'git-wt list' to pick one.
    pub target: Option<String>,

    #[command(flatten)]
    pub target_flag: WorktreeFlag,
}

/// Print a worktree's path.
#[derive(Args, Debug)]
pub(crate) struct PathArgs {
    /// Worktree number or branch name; omit for the current worktree.
    pub target: Option<String>,

    #[command(flatten)]
    pub target_flag: WorktreeFlag,
}
