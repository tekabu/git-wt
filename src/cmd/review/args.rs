use clap::Args;

use crate::cmd::args::{Meld, SourceDest};
use crate::cmd::commits::args::ReviewFlags;

/// `git-wt review <SOURCE> [-d <DEST>]`: what would merging SOURCE into DEST
/// bring over, and would it merge?
///
/// A verb of its own rather than an option on `merge`, because it answers a
/// question instead of performing an action: it writes nothing, and its exit
/// code reports the verdict (0 clean, 1 conflict) rather than success. Being
/// its own subcommand is also what lets it own a whole flag vocabulary --
/// `ReviewFlags` is the `commits` table's, and several of its short letters
/// (`-a` for `--all`, `-c` for `--commits`) are spoken for by merge options
/// under `merge`.
///
/// The source/destination grammar deliberately mirrors `merge`'s: the lone
/// positional is the source, and `-d/--destination` (alias `--dest`) is the
/// destination, defaulting to the current worktree -- its own field here, not
/// `commits`'/`log`'s shared `-t/--target`, since `commits`' own `--date` is
/// already sitting on `-d` and a review's destination is worth more than a
/// short spelling of one filter. `-s/--source` is likewise its own field
/// rather than the shared `-b/--branch`, since a review's `-b` is the one
/// already spoken for by `merge`'s own `-s/--source` rename -- keeping both
/// verbs on the same letter for "the other spelling of source".
#[derive(Args, Debug)]
pub(crate) struct ReviewArgs {
    /// Branch or worktree number whose commits would come over.
    #[arg(value_name = "SOURCE")]
    pub(crate) source: Option<String>,

    #[command(flatten)]
    pub(crate) sd: SourceDest,

    /// Open meld on the files the range touches instead of printing the table.
    #[command(flatten)]
    pub(crate) meld: Meld,

    #[command(flatten)]
    pub(crate) flags: ReviewFlags,
}
