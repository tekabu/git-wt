use clap::{ArgAction, Args};

/// Diff two worktrees.
#[derive(Args, Debug)]
pub(crate) struct DiffArgs {
    /// The two worktrees to compare, e.g. `1,2`.
    pub targets: Option<String>,

    /// Alternative spelling of the positional target; errors if both given.
    #[arg(short = 't', long = "target", value_name = "TARGET_LIST")]
    pub target_flag: Option<String>,

    /// Compare literal file contents on disk instead of committed state.
    #[arg(long)]
    pub live: bool,

    /// Show line numbers per changed hunk instead of just +/- counts.
    #[arg(long)]
    pub hunks: bool,

    /// Only files that exist in the first worktree and not the second.
    #[arg(short = 'A', long = "a-only", action = ArgAction::SetTrue)]
    pub a_only: bool,

    /// Only files that exist in the second worktree and not the first.
    #[arg(short = 'B', long = "b-only", action = ArgAction::SetTrue)]
    pub b_only: bool,

    /// Comma-separated paths to limit the diff to; the plain-words spelling of
    /// git's trailing `-- PATH...`, which still works.
    #[arg(short = 'p', long = "path", value_name = "PATH_LIST")]
    pub path: Option<String>,

    /// Copy each side's changed files to a tmp dir and open meld on them,
    /// waiting for it to exit, instead of printing a text diff.
    #[arg(short = 'm', long = "meld", action = ArgAction::SetTrue)]
    pub meld: bool,

    /// Diff-mode words and git flags (`..`, `...`, `--name-only`,
    /// `--name-status`, `--stat`, `--`, pathspecs...).
    #[arg(allow_hyphen_values = true, value_name = "ARGS")]
    pub rest: Vec<String>,
}
