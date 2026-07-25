use clap::{ArgAction, Args};

/// Compare files against a ref.
#[derive(Args, Debug)]
pub(crate) struct CompareArgs {
    /// Comma-separated relative file paths to compare.
    #[arg(short = 'f', long = "file", required = true, value_name = "FILE_LIST")]
    pub files: String,

    /// Ref to compare against: any single thing `git rev-parse` resolves to a
    /// commit -- local branch, remote-tracking branch, tag, sha, `HEAD~3`.
    #[arg(short = 'r', long = "ref", required = true, value_name = "REF")]
    pub r#ref: String,

    /// Copy the ref's versions to a temp dir and open meld,
    /// waiting for it to exit. Without this, the diff prints to stdout.
    #[arg(short = 'm', long = "meld", action = ArgAction::SetTrue, overrides_with = "meld")]
    pub meld: bool,
}
