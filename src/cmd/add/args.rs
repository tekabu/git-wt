use clap::Args;

/// Create a new worktree from a branch.
#[derive(Args, Debug)]
pub(crate) struct AddArgs {
    /// Branch to check out; omit to pick interactively.
    #[arg(value_name = "BRANCH")]
    pub branch_name: Option<String>,

    /// Suffix only: leaf becomes `<repo>-NAME`.
    #[arg(short, long)]
    pub name: Option<String>,

    /// Whole leaf, verbatim (sanitized); with '/' it is a path.
    #[arg(long)]
    pub dirname: Option<String>,

    /// Parent directory (default: primary worktree's parent).
    #[arg(short, long)]
    pub parentdir: Option<String>,

    /// Base ref for a new branch.
    #[arg(long)]
    pub from: Option<String>,

    /// Hint for the shell wrapper: do not cd into the new worktree.
    /// The binary itself never changes directory, so this is accepted and ignored.
    #[arg(short, long, hide = true)]
    pub stay: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestAdd {
        #[command(flatten)]
        args: AddArgs,
    }

    fn parse(args: &[&str]) -> AddArgs {
        TestAdd::try_parse_from(std::iter::once("git-wt").chain(args.iter().copied()))
            .unwrap()
            .args
    }

    #[test]
    fn add_args_take_branch_and_flags() {
        let a = parse(&["feature/login"]);
        assert_eq!(a.branch_name.as_deref(), Some("feature/login"));
        assert!(a.name.is_none());

        let a = parse(&["feature/login", "--name", "review"]);
        assert_eq!(a.name.as_deref(), Some("review"));

        let a = parse(&["feature/login", "-p", "/work", "--from", "develop"]);
        assert_eq!(a.parentdir.as_deref(), Some("/work"));
        assert_eq!(a.from.as_deref(), Some("develop"));
    }

    #[test]
    fn add_args_reject_unknown_flags() {
        assert!(TestAdd::try_parse_from(["git-wt", "--bogus"]).is_err());
    }
}
