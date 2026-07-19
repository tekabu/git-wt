use std::io::IsTerminal;
use std::path::Path;

use crate::ui::confirm;
use crate::git::git_run;
use crate::ui::{color_enabled, paint, GREEN};
use crate::worktree::{canon, label, leaf_of, Worktree};

pub(crate) fn cmd_remove(
    root: &Path,
    trees: &[Worktree],
    idx: usize,
    yes: bool,
    force: bool,
) -> Result<(), String> {
    let wanted = &trees[idx];

    // The main worktree is the first entry git reports; a bare one is never
    // a checkout to remove.
    if idx == 0 || wanted.bare {
        return Err("refusing to remove the main worktree".into());
    }

    // Was the shell standing inside the tree we're about to remove? Capture it
    // before removal (canonicalize needs the dir to still exist). Only then does
    // a wrapper need to cd back to main; otherwise it should stay put.
    let inside = match std::env::current_dir() {
        Ok(cwd) => canon(&cwd).starts_with(canon(&wanted.path)),
        Err(_) => false,
    };

    let path_s = wanted.path.to_string_lossy().to_string();
    if !yes
        && !confirm(&format!(
            "Remove worktree '{}' at {path_s}? [y/N] ",
            label(wanted)
        ))?
    {
        eprintln!("Aborted.");
        return Ok(());
    }

    let mut argv = vec!["worktree", "remove"];
    if force {
        argv.push("--force");
    }
    argv.push(&path_s);

    git_run(root, &argv).map_err(|e| {
        if !force && e.contains("contains modified or untracked files") {
            format!("{e}\nhint: re-run with -f to discard them")
        } else {
            e
        }
    })?;

    git_run(root, &["worktree", "prune"])?;

    // The `remove` verb only detaches the worktree; the branch itself stays.
    let leaf = leaf_of(&wanted.path);
    let branch_note = match &wanted.branch {
        Some(b) => format!("branch {b} kept"),
        None => "detached".into(),
    };
    let on = color_enabled(std::io::stderr().is_terminal());
    eprintln!("{} {leaf}  ({branch_note})", paint("Removed", GREEN, on));

    // Only when the shell was inside the removed tree does its cwd now dangle,
    // so print the main path for a wrapper to cd back. Removing some other tree
    // leaves you where you are — print nothing, so the wrapper stays put.
    if inside {
        if let Some(main) = trees.first() {
            println!("{}", main.path.display());
        }
    }
    Ok(())
}
