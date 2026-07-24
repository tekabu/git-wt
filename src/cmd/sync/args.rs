use clap::Args;

/// Fetch, pull, or push across worktrees.
#[derive(Args, Debug)]
pub(crate) struct SyncArgs {
    /// Worktree list; omit with --all to target every worktree.
    pub targets: Option<String>,

    /// Alternative spelling of the positional target; errors if both given.
    #[arg(short = 't', long = "target", value_name = "TARGET_LIST")]
    pub target_flag: Option<String>,

    /// Run in every worktree.
    #[arg(short, long)]
    pub all: bool,

    /// Git flags for the verb (curated list; see --help).
    #[arg(allow_hyphen_values = true, trailing_var_arg = true, value_name = "FLAGS")]
    pub flags: Vec<String>,
}
