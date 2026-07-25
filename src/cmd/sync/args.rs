use clap::{ArgAction, Args};

/// Fields every sync verb shares: the target list and the worktree-list
/// append flag, same shape as every other multi-target command.
#[derive(Args, Clone, Debug, Default)]
pub(crate) struct SyncCommon {
    /// Worktree list; omit with --all to target every worktree.
    pub targets: Option<String>,

    /// Alternative spelling of the positional target; errors if both given.
    #[arg(short = 't', long = "target", value_name = "TARGET_LIST")]
    pub target_flag: Option<String>,

    /// Extra worktree targets, appended to this command's target list.
    #[arg(short, long, action = ArgAction::Append, value_name = "TARGET_LIST")]
    pub branch: Vec<String>,

    /// Run in every worktree.
    #[arg(short, long, overrides_with = "all")]
    pub all: bool,
}

/// Fetch across worktrees.
#[derive(Args, Debug, Default)]
pub(crate) struct FetchArgs {
    #[command(flatten)]
    pub common: SyncCommon,

    /// Remove remote-tracking refs the remote no longer has.
    #[arg(short = 'p', long, overrides_with = "prune")]
    pub prune: bool,

    /// Fetch tags too.
    #[arg(long, overrides_with = "tags", conflicts_with = "no_tags")]
    pub tags: bool,

    /// Skip tags even if the remote default would fetch them.
    #[arg(long = "no-tags", alias = "nt", overrides_with = "no_tags")]
    pub no_tags: bool,

    /// Fetch even a non-fast-forward update to a remote-tracking ref.
    #[arg(short = 'F', long, overrides_with = "force")]
    pub force: bool,
}

/// Pull across worktrees.
#[derive(Args, Debug, Default)]
pub(crate) struct PullArgs {
    #[command(flatten)]
    pub common: SyncCommon,

    /// Rebase local commits onto the fetched history instead of merging.
    #[arg(long, alias = "rb", overrides_with = "rebase", conflicts_with_all = ["no_rebase", "ff_only"])]
    pub rebase: bool,

    /// Merge even if the repo's config defaults to rebase.
    #[arg(long = "no-rebase", alias = "nr", overrides_with = "no_rebase")]
    pub no_rebase: bool,

    /// Refuse anything that isn't a fast-forward.
    #[arg(long = "ff-only", overrides_with = "ff_only")]
    pub ff_only: bool,

    /// Remove remote-tracking refs the remote no longer has.
    #[arg(short = 'p', long, overrides_with = "prune")]
    pub prune: bool,

    /// Stash-restore local changes around the pull automatically.
    #[arg(long, alias = "as", overrides_with = "autostash")]
    pub autostash: bool,
}

/// Push across worktrees.
#[derive(Args, Debug, Default)]
pub(crate) struct PushArgs {
    #[command(flatten)]
    pub common: SyncCommon,

    /// Record the pushed branch's upstream if it has none yet.
    #[arg(short = 'u', long, overrides_with = "set_upstream")]
    pub set_upstream: bool,

    /// Push a non-fast-forward update, refused if the remote moved since fetch.
    #[arg(long = "force-with-lease", alias = "fl", overrides_with = "force_with_lease")]
    pub force_with_lease: bool,

    /// Push tags too.
    #[arg(long, overrides_with = "tags")]
    pub tags: bool,

    /// Show what would be pushed without pushing it.
    #[arg(short = 'n', long = "dry-run", overrides_with = "dry_run")]
    pub dry_run: bool,

    /// Declared only to give '-F/--force' its own explanatory rejection --
    /// see `cmd_sync`'s push arm -- rather than clap's generic "unknown
    /// argument", since typing it here is a plausible, dangerous mistake.
    #[arg(short = 'F', long, hide = true, overrides_with = "force")]
    pub force: bool,
}
