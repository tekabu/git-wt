use clap::Args;

use crate::cmd::args::{ExtraRef, Meld, PathFilter};

/// Compare files against a ref.
///
/// `r#ref` is `ExtraRef` (`-x/--reference`, alias `--ref`), the shared
/// branch/worktree-number/commit-sha list -- retired `CompareRef`'s own
/// `-r/--ref` (required, exactly one). `files` is the shared `PathFilter`
/// (`-p/--path`) -- retired `CompareFiles`' own `-f/--file` (required,
/// comma-split `String`). Neither shared struct is `required`, so "at least
/// one" (`files`) and "exactly one" (`r#ref`) are now `cmd_compare`'s own
/// checks rather than clap's.
#[derive(Args, Debug)]
pub(crate) struct CompareArgs {
    #[command(flatten)]
    pub files: PathFilter,

    #[command(flatten)]
    pub r#ref: ExtraRef,

    #[command(flatten)]
    pub meld: Meld,
}
