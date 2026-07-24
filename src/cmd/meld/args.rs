use clap::Args;

/// Open meld on 2-3 worktree directories.
#[derive(Args, Clone, Debug, Default)]
pub(crate) struct MeldArgs {
    /// The worktrees to compare, e.g. `1,2` or `1,2,3`.
    pub targets: Option<String>,

    /// Alternative spelling of the positional target; errors if both given.
    #[arg(short = 't', long = "target", value_name = "TARGET_LIST")]
    pub target_flag: Option<String>,

    /// Diff only: filter to files that differ, extracted into temp dirs.
    #[arg(short, long)]
    pub diff: bool,

    /// Diff only: three-way with auto base.
    #[arg(long = "3way")]
    pub three_way: bool,

    /// Diff only: explicit base ref (branch, commit, or worktree number).
    #[arg(long, value_name = "REF")]
    pub base: Option<String>,

    /// Diff only: `..` (tip-vs-tip, default under --diff) or `...` (fork).
    #[arg(value_name = "RANGE")]
    pub range: Option<String>,
}
