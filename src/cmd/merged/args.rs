use clap::{ArgAction, Args};

/// Check merge status of branches.
#[derive(Args, Debug)]
pub(crate) struct MergedArgs {
    /// The worktree(s) to use as reference, e.g. `1` or `1,2`.
    pub worktree_or_branch_list: Option<String>,

    /// Alternative spelling of the positional target; errors if both given.
    #[arg(short = 't', long = "target", value_name = "TARGET_LIST")]
    pub target_flag: Option<String>,

    /// With one target, compare against this branch or worktree number
    /// instead of listing every other worktree.
    pub source: Option<String>,

    /// List every worktree and whether it is already merged into the target.
    #[arg(short, long)]
    pub others: bool,

    /// Include the worktree path in the --others table.
    #[arg(short, long)]
    pub show_path: bool,

    /// Extra worktree targets, appended to this command's target list.
    /// Repeatable: `-b 2 -b 3` and `-b 2,3` mean the same thing.
    #[arg(short, long, action = ArgAction::Append, value_name = "TARGET_LIST")]
    pub branch: Vec<String>,

}
