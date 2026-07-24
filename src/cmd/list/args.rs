use clap::Args;

/// List worktrees.
#[derive(Args, Debug, Default)]
pub(crate) struct ListArgs {
    /// Optional search term; only highlights matches.
    pub search: Option<String>,

    /// Pick and order columns: 1=id,2=branch,3=dir,4=status,5=last-commit,6=merged,7=merged-ref,8=merged-at,9=push,10=pull.
    #[arg(short, long, value_name = "COLS")]
    pub col: Option<String>,

    /// Long output (id, branch, dir, status, last-commit, merged, push, pull).
    #[arg(short, long)]
    pub long: bool,

    /// Short output (id, branch, status).
    #[arg(short, long)]
    pub short: bool,

    /// Include the directory/path column.
    #[arg(short = 'p', long = "path")]
    pub show_path: bool,

    /// List uncommitted files under each worktree.
    #[arg(short, long)]
    pub files: bool,

    /// Page output through less instead of printing straight to the screen.
    #[arg(long)]
    pub less: bool,
}
