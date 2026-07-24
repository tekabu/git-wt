pub(crate) mod args;

use std::io::IsTerminal;
use std::path::Path;

use crate::cmd::remove::args::RemoveArgs;
use crate::git::git_run;
use crate::ui::{color_enabled, confirm, paint, GREEN};
use crate::worktree::{canon, label, leaf_of, Worktree};

pub(crate) fn cmd_remove(
    root: &Path,
    trees: &[Worktree],
    idx: usize,
    args: RemoveArgs,
) -> Result<(), String> {
    let wanted = &trees[idx];

    if idx == 0 || wanted.bare {
        return Err("refusing to remove the main worktree".into());
    }

    if args.delete_branch && wanted.branch.is_none() {
        return Err("worktree has no branch to delete".into());
    }

    let inside = match std::env::current_dir() {
        Ok(cwd) => canon(&cwd).starts_with(canon(&wanted.path)),
        Err(_) => false,
    };

    let path_s = wanted.path.to_string_lossy().to_string();
    if !args.yes {
        let prompt = match (&wanted.branch, args.delete_branch) {
            (Some(b), true) => format!(
                "Remove worktree '{}' at {path_s} and delete branch '{b}'? [y/N] ",
                label(wanted)
            ),
            _ => format!("Remove worktree '{}' at {path_s}? [y/N] ", label(wanted)),
        };
        if !confirm(&prompt)? {
            eprintln!("Aborted.");
            return Ok(());
        }
    }

    let mut argv = vec!["worktree", "remove"];
    if args.force {
        argv.push("--force");
    }
    argv.push(&path_s);

    git_run(root, &argv).map_err(|e| {
        if !args.force && e.contains("contains modified or untracked files") {
            format!("{e}\nhint: re-run with -f to discard them")
        } else {
            e
        }
    })?;

    git_run(root, &["worktree", "prune"])?;

    let leaf = leaf_of(&wanted.path);
    let branch_note = match &wanted.branch {
        Some(b) if args.delete_branch => {
            let flag = if args.force { "-D" } else { "-d" };
            git_run(root, &["branch", flag, b])?;
            format!("branch {b} deleted")
        }
        Some(b) => format!("branch {b} kept"),
        None => "detached".into(),
    };
    let on = color_enabled(std::io::stderr().is_terminal());
    eprintln!("{} {leaf}  ({branch_note})", paint("Removed", GREEN, on));

    if inside {
        if let Some(main) = trees.first() {
            println!("{}", main.path.display());
        }
    }
    Ok(())
}
