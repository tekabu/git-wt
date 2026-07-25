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
    #[arg(long, overrides_with = "live")]
    pub live: bool,

    /// Show line numbers per changed hunk instead of just +/- counts.
    #[arg(long, overrides_with = "hunks")]
    pub hunks: bool,

    /// Only files that exist in the first worktree and not the second.
    #[arg(short = 'A', long = "a-only", action = ArgAction::SetTrue, overrides_with = "a_only")]
    pub a_only: bool,

    /// Only files that exist in the second worktree and not the first.
    #[arg(short = 'B', long = "b-only", action = ArgAction::SetTrue, overrides_with = "b_only")]
    pub b_only: bool,

    /// Comma-separated paths to limit the diff to; the plain-words spelling of
    /// git's trailing `-- PATH...`, which still works.
    #[arg(short = 'p', long = "path", value_name = "PATH_LIST", overrides_with = "path")]
    pub path: Option<String>,

    /// Copy each side's changed files to a tmp dir and open meld on them,
    /// waiting for it to exit, instead of printing a text diff.
    #[arg(short = 'm', long = "meld", action = ArgAction::SetTrue, overrides_with = "meld")]
    pub meld: bool,

    /// List names only, not the +/- counts.
    #[arg(long = "name-only", overrides_with = "name_only")]
    pub name_only: bool,

    /// List names with their one-letter status (A/M/D).
    #[arg(long = "name-status", overrides_with = "name_status")]
    pub name_status: bool,

    /// A bar-chart summary instead of per-file counts.
    #[arg(long, overrides_with = "stat")]
    pub stat: bool,

    /// Extra worktree targets, appended to this command's target list.
    /// Repeatable: `-b 2 -b 3` and `-b 2,3` mean the same thing.
    #[arg(short, long, action = ArgAction::Append, value_name = "TARGET_LIST")]
    pub branch: Vec<String>,

    /// `..` (tip-vs-tip) or `...` (fork, the default); a single stray word
    /// here otherwise is a bad range or a path someone spelled the git way
    /// (`-- PATH...` or a bare path), both of which `cmd_diff` rejects itself
    /// so the message can say which. Not `allow_hyphen_values`: a real unknown
    /// flag like `-w` should stay clap's own rejection, not land here.
    #[arg(value_name = "RANGE")]
    pub range: Option<String>,
}
