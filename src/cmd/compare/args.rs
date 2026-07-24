use clap::{ArgAction, Args};

/// Compare files against a branch or commit.
#[derive(Args, Debug)]
pub(crate) struct CompareArgs {
    /// Comma-separated relative file paths to compare.
    #[arg(short = 'f', long = "file", required = true, value_name = "FILE_LIST")]
    pub files: String,

    /// Commit to compare against; alternative to the global -b/--branch,
    /// which names a single branch for the same purpose here.
    #[arg(short = 'c', long = "commit", value_name = "COMMIT")]
    pub commit: Option<String>,

    /// Copy the branch/commit's versions to a temp dir and open meld,
    /// waiting for it to exit. Without this, the diff prints to stdout.
    #[arg(short = 'm', long = "meld", action = ArgAction::SetTrue)]
    pub meld: bool,
}
