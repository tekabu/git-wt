use clap::Args;

use crate::cmd::args::Repair;

/// Report worktree issues.
#[derive(Args, Debug)]
pub(crate) struct DoctorArgs {
    #[command(flatten)]
    pub repair: Repair,
}
