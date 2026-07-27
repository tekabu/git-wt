use clap::Args;

use crate::cmd::args::Repair;

/// Report worktree issues.
#[derive(Args, Debug)]
pub(crate) struct DoctorArgs {
    #[command(flatten)]
    pub repair: Repair,

    #[command(flatten)]
    pub migrate: Migrate,

    #[command(flatten)]
    pub syncname: SyncName,
}

/// `-m/--migrate`: relocate worktrees living outside the current default
/// location (bare siblings of the repo root, the since-retired
/// `<repo>/.worktrees/`, or anywhere else) into `<repo>-worktrees`, via
/// `git worktree move` so the admin link stays correct. Runs instead of the
/// usual report/repair flow. Purely a location fix -- it does not touch a
/// worktree's folder name, even one still carrying an old repo-prefixed name
/// from before folders were named by branch alone; that is `-s/--syncname`'s
/// job.
#[derive(Args, Debug, Default)]
pub(crate) struct Migrate {
    #[arg(short, long)]
    pub(crate) migrate: bool,
}

/// `-s/--syncname`: rename every worktree folder (wherever it lives) to
/// match its branch alone, the same scheme `add` uses for new worktrees --
/// enforced separately from `--migrate` since a worktree can already sit in
/// the right place under an old, repo-prefixed name. Runs instead of the
/// usual report/repair flow; combine with `--migrate` to fix location and
/// name in one pass.
#[derive(Args, Debug, Default)]
pub(crate) struct SyncName {
    #[arg(short, long)]
    pub(crate) syncname: bool,
}
