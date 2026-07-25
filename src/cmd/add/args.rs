use clap::Args;

use crate::cmd::args::{AddDirname, AddFromRef, AddName, AddParentdir, AddStayHidden};

/// Create a new worktree from a branch.
#[derive(Args, Debug)]
pub(crate) struct AddArgs {
    /// Branch to check out; omit to pick interactively.
    #[arg(value_name = "BRANCH")]
    pub branch_name: Option<String>,

    #[command(flatten)]
    pub name: AddName,

    #[command(flatten)]
    pub dirname: AddDirname,

    #[command(flatten)]
    pub parentdir: AddParentdir,

    #[command(flatten)]
    pub from: AddFromRef,

    #[command(flatten)]
    pub stay: AddStayHidden,
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
        assert!(a.name.name.is_none());

        let a = parse(&["feature/login", "--name", "review"]);
        assert_eq!(a.name.name.as_deref(), Some("review"));

        let a = parse(&["feature/login", "--parentdir", "/work", "--from", "develop"]);
        assert_eq!(a.parentdir.parentdir.as_deref(), Some("/work"));
        assert_eq!(a.from.from.as_deref(), Some("develop"));
    }

    #[test]
    fn add_args_reject_unknown_flags() {
        assert!(TestAdd::try_parse_from(["git-wt", "--bogus"]).is_err());
    }
}
