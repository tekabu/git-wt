use clap::Args;

/// Check merge status of branches.
#[derive(Args, Debug)]
pub(crate) struct MergedArgs {
    /// The worktree(s) to use as reference, e.g. `1` or `1,2`.
    pub targets: Option<String>,

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
}
