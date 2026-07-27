use clap::Args;

use crate::cmd::args::Repair;

/// Report worktree issues.
#[derive(Args, Debug)]
pub(crate) struct DoctorArgs {
    #[command(flatten)]
    pub repair: Repair,

    #[command(flatten)]
    pub migrate: Migrate,
}

/// `-m/--migrate`: relocate worktrees created before `.worktrees/` became
/// the default (siblings of the repo root, or anywhere else) into
/// `<repo>/.worktrees/`, via `git worktree move` so the admin link stays
/// correct. Runs instead of the usual report/repair flow.
#[derive(Args, Debug, Default)]
pub(crate) struct Migrate {
    #[arg(short, long)]
    pub(crate) migrate: bool,
}
