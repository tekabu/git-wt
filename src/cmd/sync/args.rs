use clap::Args;

use crate::cmd::args::{
    Autostash, BranchTargets, FetchTags, Force, ForceWithLease, Prune, PullFfOnly, PullRebase,
    DryRun, PushTags, SetUpstream, SyncAll, TargetListFlag,
};

/// Fields every sync verb shares: the target list and the worktree-list
/// append flag, same shape as every other multi-target command.
#[derive(Args, Clone, Debug, Default)]
pub(crate) struct SyncCommon {
    /// Worktree list; omit with --all to target every worktree.
    pub targets: Option<String>,

    #[command(flatten)]
    pub target_flag: TargetListFlag,

    #[command(flatten)]
    pub branch: BranchTargets,

    #[command(flatten)]
    pub all: SyncAll,
}

/// Fetch across worktrees.
#[derive(Args, Debug, Default)]
pub(crate) struct FetchArgs {
    #[command(flatten)]
    pub common: SyncCommon,

    #[command(flatten)]
    pub prune: Prune,

    #[command(flatten)]
    pub tags: FetchTags,

    #[command(flatten)]
    pub force: Force,
}

/// Pull across worktrees.
#[derive(Args, Debug, Default)]
pub(crate) struct PullArgs {
    #[command(flatten)]
    pub common: SyncCommon,

    #[command(flatten)]
    pub rebase: PullRebase,

    #[command(flatten)]
    pub ff_only: PullFfOnly,

    #[command(flatten)]
    pub prune: Prune,

    #[command(flatten)]
    pub autostash: Autostash,
}

/// Push across worktrees.
#[derive(Args, Debug, Default)]
pub(crate) struct PushArgs {
    #[command(flatten)]
    pub common: SyncCommon,

    #[command(flatten)]
    pub set_upstream: SetUpstream,

    #[command(flatten)]
    pub force_with_lease: ForceWithLease,

    #[command(flatten)]
    pub tags: PushTags,

    #[command(flatten)]
    pub dry_run: DryRun,

    /// Declared only to give '-F/--force' its own explanatory rejection --
    /// see `cmd_sync`'s push arm -- rather than clap's generic "unknown
    /// argument", since typing it here is a plausible, dangerous mistake.
    /// Deliberately local, not the shared `Force` group -- see `Force`'s
    /// doc comment.
    #[arg(short = 'F', long, hide = true, overrides_with = "force")]
    pub force: bool,
}
