use clap::Args;

/// Report worktree issues.
#[derive(Args, Debug)]
pub(crate) struct DoctorArgs {
    /// Attempt to fix what is found.
    #[arg(short, long)]
    pub repair: bool,
}
