pub(crate) mod args;

use std::path::Path;

use crate::cli::{check_index, parse_target_list, resolve_target_list};
use crate::cmd::switch::args::{PathArgs, SwitchArgs};
use crate::worktree::{label, worktrees};

/// `switch` / `cd` / `s`: print the path of a worktree.
pub(crate) fn cmd_switch(root: &Path, args: SwitchArgs) -> Result<(), String> {
    match args.target {
        Some(t) => print_path(root, &t),
        None => Err("switch needs a target; see 'git-wt list'".into()),
    }
}

/// `path` / `show`: print a worktree's path (default: current).
pub(crate) fn cmd_path(root: &Path, args: PathArgs) -> Result<(), String> {
    match args.target {
        Some(t) => print_path(root, &t),
        None => {
            let trees = worktrees(root)?;
            let idx = crate::worktree::current_worktree_index(&trees)
                .ok_or("not inside a worktree")?;
            eprintln!("{}", label(&trees[idx]));
            println!("{}", trees[idx].path.display());
            Ok(())
        }
    }
}

fn print_path(root: &Path, tok: &str) -> Result<(), String> {
    let trees = worktrees(root)?;
    if let Some(parts) = parse_target_list(tok)? {
        let ns = resolve_target_list(&trees, &parts)?;
        if ns.len() != 1 {
            return Err(format!("switch takes a single worktree, not '{tok}'"));
        }
        let idx = check_index(ns[0], trees.len())?;
        eprintln!("{}", label(&trees[idx]));
        println!("{}", trees[idx].path.display());
        return Ok(());
    }
    if let Some(n) = tok.parse::<usize>().ok() {
        let idx = check_index(n, trees.len())?;
        eprintln!("{}", label(&trees[idx]));
        println!("{}", trees[idx].path.display());
        return Ok(());
    }
    // Branch name.
    let idx = crate::cli::resolve_target(&trees, tok)
        .ok_or_else(|| format!("no worktree named '{tok}'"))?;
    let idx = check_index(idx, trees.len())?;
    eprintln!("{}", label(&trees[idx]));
    println!("{}", trees[idx].path.display());
    Ok(())
}
