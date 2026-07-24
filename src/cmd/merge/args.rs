use clap::Args;

/// Merge a source branch into a worktree.
#[derive(Args, Debug)]
pub(crate) struct MergeArgs {
    /// Destination worktree, optionally with source worktree (`1` or `1,2`),
    /// followed by merge options, source branch/number, resume flags
    /// (`--continue`, `--abort`), and the `--review` hand-off. When the first
    /// token resolves as a worktree list it is consumed as the target;
    /// otherwise the current worktree is used and the whole tail passed
    /// through as options. A target and a separate bare source word
    /// (`1 feat/x`) is no longer accepted -- use a comma list (`1,feat/x`)
    /// or `-b` (`1 -b feat/x`).
    #[arg(allow_hyphen_values = true, num_args = 0.., value_name = "TARGETS/OPTIONS")]
    pub rest: Vec<String>,
}
