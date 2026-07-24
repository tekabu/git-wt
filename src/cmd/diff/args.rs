use clap::Args;

/// Diff two worktrees.
#[derive(Args, Debug)]
pub(crate) struct DiffArgs {
    /// The two worktrees to compare, e.g. `1,2`.
    pub targets: Option<String>,

    /// Alternative spelling of the positional target; errors if both given.
    #[arg(short = 't', long = "target", value_name = "TARGET_LIST")]
    pub target_flag: Option<String>,

    /// Diff-mode words and git flags (`..`, `...`, `live`, `hunks`,
    /// `--name-only`, `--name-status`, `--stat`, `--`, pathspecs...).
    #[arg(allow_hyphen_values = true, value_name = "ARGS")]
    pub rest: Vec<String>,
}
