use clap::Args;

use crate::cmd::args::{ColSelect, ListFiles, LongOutput, Pager, ShortOutput, ShowPathCol};

/// List worktrees.
#[derive(Args, Debug, Default)]
pub(crate) struct ListArgs {
    /// Optional search term; only highlights matches.
    pub search: Option<String>,

    #[command(flatten)]
    pub col: ColSelect,

    #[command(flatten)]
    pub long: LongOutput,

    #[command(flatten)]
    pub short: ShortOutput,

    #[command(flatten)]
    pub show_path: ShowPathCol,

    #[command(flatten)]
    pub files: ListFiles,

    #[command(flatten)]
    pub less: Pager,
}
