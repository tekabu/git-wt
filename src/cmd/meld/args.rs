use clap::Args;

use crate::cmd::args::{MeldBase, MeldDiffOnly, MeldPosition, ThreeWay};
use crate::cmd::args::ExtraRef;

/// Open meld on 2-3 worktree directories.
///
/// Two independent ways to name the worktrees: the positional/`-x` list
/// (order picks left/right/center by position), or `MeldPosition`'s
/// explicit `--left`/`--right`/`--center` (each a worktree number or a raw
/// filesystem path). `cmd_meld` rejects mixing both in one call.
///
/// No `-t/--target` -- `-x` already covers everything it did (comma- or
/// space-separated, and works with no positional at all) plus the append
/// case `-t` couldn't, so a second spelling of "the list" added nothing.
#[derive(Args, Clone, Debug, Default)]
pub(crate) struct MeldArgs {
    /// The worktrees to compare, e.g. `1,2` or `1,2,3`.
    pub worktree_or_branch_list: Option<String>,

    #[command(flatten)]
    pub diff: MeldDiffOnly,

    #[command(flatten)]
    pub three_way: ThreeWay,

    #[command(flatten)]
    pub base: MeldBase,

    /// Diff only: `..` (tip-vs-tip, default under --diff) or `...` (fork).
    #[arg(value_name = "RANGE")]
    pub range: Option<String>,

    /// Extra worktree targets, appended to the list. `-x/--reference`
    /// (alias `--ref`), same shape `commits`/`log` use -- retired the old
    /// `-b/--branch` here.
    #[command(flatten)]
    pub branch: ExtraRef,

    #[command(flatten)]
    pub position: MeldPosition,
}
