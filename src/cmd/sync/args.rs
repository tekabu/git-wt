use clap::Args;

use crate::cmd::args::{
    Autostash, ExtraRef, FetchTags, Force, ForceWithLease, Prune, PullFfOnly, PullRebase,
    DryRun, PushTags, SetUpstream, SyncAll,
};

/// Fields every sync verb shares: the target list and the worktree-list
/// append flag, same shape as every other multi-target command.
///
/// No `-t/--target`: it was a pure alternate spelling of the positional
/// (verified byte-identical error text on a bad worktree number), and the
/// positional alone already does what it did -- name the list directly,
/// with no default-to-current-worktree fallback. `-x/--reference` (alias
/// `--ref`) doesn't replace it; `-x` is additive only (always layers on top
/// of the positional or the current-worktree default), so it can't express
/// "just this worktree, not current" -- only the bare positional can, same
/// as before.
///
/// `-x`, not the old `-b/--branch`: same `resolve_worktree_or_branch_list`-
/// based resolution either way (worktree number or an existing worktree's
/// branch name -- no sha fallback here, see `ExtraRef`'s own doc comment),
/// just the shared struct instead of a fourth copy.
#[derive(Args, Clone, Debug, Default)]
pub(crate) struct SyncCommon {
    /// Worktree list; omit with --all to target every worktree.
    pub worktree_or_branch_list: Option<String>,

    #[command(flatten)]
    pub branch: ExtraRef,

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
