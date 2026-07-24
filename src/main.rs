//! git-wt — create and manage git worktrees in sibling directories named
//! `<repo-folder>-<sanitized-branch>`.
//!
//! Installed on PATH as `git-wt`, so it is also reachable as `git wt`.

mod cli;
mod cmd;
mod git;
mod ui;
mod worktree;

use crate::cli::{
    check_index, effective_target, extract_branch_flag, extract_target_flag, gather_targets,
    resolve_target_list, typed_verb, warn_if_alias_shadows_branch, worktree_on_branch, Cli,
    Commands,
};
use clap::{CommandFactory, Parser};
use crate::cmd::add::cmd_add;
use crate::cmd::commits::cmd_commits;
use crate::cmd::diff::cmd_diff;
use crate::cmd::doctor::cmd_doctor;
use crate::cmd::list::cmd_list;
use crate::cmd::log::cmd_log;
use crate::cmd::meld::cmd_meld;
use crate::cmd::merge::{cmd_merge, parse_merge_args};
use crate::cmd::merged::{cmd_merged, cmd_merged_others};
use crate::cmd::remove::cmd_remove;
use crate::cmd::switch::{cmd_path, cmd_switch};
use crate::cmd::sync::{cmd_sync, parse_sync_args, SyncOp, ALL_HINT};
use crate::worktree::{current_worktree_index, ref_of, repo_root, worktrees};
use crate::git::git_stdout;

const VERSION: &str = env!("CARGO_PKG_VERSION");

// Rust starts every process with SIGPIPE ignored, so a write into a reader
// that quit early surfaces as an `Err` on the write call -- which `println!`
// turns into a panic instead of the quiet exit every other Unix tool makes.
// Putting the handler back to its default restores that.
extern "C" {
    fn signal(signum: i32, handler: usize) -> usize;
}
const SIGPIPE: i32 = 13;
const SIG_DFL: usize = 0;

fn main() {
    unsafe {
        signal(SIGPIPE, SIG_DFL);
    }
    let code = match run() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();

    if cli.help || cli.full {
        if cli.full {
            print!("{}", include_str!("../docs/MANUAL.md"));
        } else {
            Cli::command().print_help().map_err(|e| e.to_string())?;
            println!();
        }
        return Ok(());
    }

    let root = repo_root()?;
    let trees = worktrees(&root)?;

    // No verb: default to switch. A bare target list selects one worktree;
    // no target list shows the worktree list.
    let cmd = match cli.command {
        Some(Commands::Version) => {
            println!("git-wt {VERSION}");
            return Ok(());
        }
        Some(c) => c,
        None => {
            if let Some(t) = cli.targets {
                return cmd_switch(
                    &root,
                    crate::cmd::switch::args::SwitchArgs { target: Some(t), target_flag: None },
                );
            }
            return cmd_list(&root, crate::cmd::list::args::ListArgs::default());
        }
    };

    // The new grammar is verb-first: the optional positional target list comes
    // after the subcommand. Any target captured at the top level means the user
    // wrote target-first (e.g. `git-wt 2 diff`), which has been retired.
    if cli.targets.is_some() {
        return Err("target list must come after the verb, e.g. 'git-wt diff 1,2'".into());
    }

    // The one-letter alias each of these matched, if that's the spelling the
    // user actually typed -- see `typed_verb`'s docs for why the parsed
    // `Commands` variant alone can't tell us that.
    let typed = typed_verb();
    let typed_alias = |a: &str| typed.as_deref() == Some(a);

    match cmd {
        Commands::Add(args) => {
            if !cli.branch.is_empty() {
                return Err("'add' does not take '-b/--branch'".into());
            }
            if typed_alias("a") {
                warn_if_alias_shadows_branch(&trees, "a", "add");
            }
            cmd_add(&root, args)
        }

        Commands::List(args) => {
            if cli.targets.is_some() || !cli.branch.is_empty() {
                return Err("'list' does not take worktree targets".into());
            }
            cmd_list(&root, args)
        }

        Commands::Switch(args) => {
            let target = effective_target(args.target, args.target_flag.as_ref())?;
            let idxs = resolve_targets(&trees, target.as_ref(), &cli.branch, false, false)?;
            if idxs.len() > 1 {
                return Err(format!(
                    "switch takes one worktree, got {}\nhint: use 'git-wt commits {}' to compare",
                    idxs.len(),
                    idxs.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(",")
                ));
            }
            if typed_alias("s") {
                warn_if_alias_shadows_branch(&trees, "s", "switch");
            }
            let mut args = crate::cmd::switch::args::SwitchArgs { target, target_flag: None };
            if args.target.is_none() && !cli.branch.is_empty() {
                args.target = Some(cli.branch.join(","));
            }
            cmd_switch(&root, args)
        }

        Commands::Path(args) => {
            if !cli.branch.is_empty() {
                return Err("'path' does not combine with '-b/--branch'".into());
            }
            let target = effective_target(args.target, args.target_flag.as_ref())?;
            cmd_path(&root, crate::cmd::switch::args::PathArgs { target, target_flag: None })
        }

        Commands::Remove(args) => {
            let target = effective_target(args.target.clone(), args.target_flag.as_ref())?;
            let idxs = resolve_targets(&trees, target.as_ref(), &cli.branch, true, false)?;
            if idxs.len() > 1 {
                return Err(format!("remove takes one worktree, got {}", idxs.len()));
            }
            let idx = idxs.into_iter().next().expect("len 1");
            let args = crate::cmd::remove::args::RemoveArgs { target, target_flag: None, ..args };
            cmd_remove(&root, &trees, idx, args)
        }

        Commands::Fetch(ref args) | Commands::Pull(ref args) | Commands::Push(ref args) => {
            let op = match &cmd {
                Commands::Fetch(_) => SyncOp::Fetch,
                Commands::Pull(_) => {
                    if typed_alias("p") {
                        warn_if_alias_shadows_branch(&trees, "p", "pull");
                    }
                    SyncOp::Pull
                }
                Commands::Push(_) => SyncOp::Push,
                _ => unreachable!(),
            };
            let target = effective_target(args.targets.clone(), args.target_flag.as_ref())?;
            if args.all && (target.is_some() || !cli.branch.is_empty()) {
                return Err(format!(
                    "'--all' is every worktree, so a target list has nothing to add\n\
                     hint: 'git-wt {} --all', or drop it to sweep just the ones you named",
                    op.word()
                ));
            }
            let idxs = if args.all {
                (0..trees.len()).collect()
            } else {
                let mut idxs = resolve_targets(&trees, target.as_ref(), &cli.branch, true, false)?;
                if idxs.is_empty() {
                    let cur = current_worktree_index(&trees)
                        .ok_or_else(|| format!("not inside a worktree; use 'git-wt <N> {}'\n{ALL_HINT}", op.word()))?;
                    idxs.push(cur);
                }
                idxs
            };
            let mut parsed = parse_sync_args(op, &args.flags)?;
            parsed.all = parsed.all || args.all;
            cmd_sync(&trees, &idxs, &parsed)
        }

        Commands::Diff(args) => {
            let target = effective_target(args.targets.clone(), args.target_flag.as_ref())?;
            let idxs = resolve_targets(&trees, target.as_ref(), &cli.branch, true, false)?;
            if idxs.len() != 2 {
                return Err(format!(
                    "diff takes exactly two worktrees, got {}\n\
                     hint: 'git-wt diff 1,2' or 'git-wt diff 1 -b 2'",
                    idxs.len()
                ));
            }
            cmd_diff(&root, &trees, &idxs, &args)
        }

        Commands::Meld(args) => {
            let target = effective_target(args.targets.clone(), args.target_flag.as_ref())?;
            let idxs = resolve_targets(&trees, target.as_ref(), &cli.branch, true, true)?;
            if idxs.len() < 2 {
                return Err(format!(
                    "meld needs 2 or 3 worktrees, got {}\n\
                     hint: 'git-wt meld 1,2' or 'git-wt meld 1,2,3'",
                    idxs.len()
                ));
            }
            if idxs.len() > 3 {
                return Err(format!(
                    "meld takes at most 3 worktrees, got {}\n\
                     hint: 'git-wt meld 1,2' or 'git-wt meld 1,2,3'",
                    idxs.len()
                ));
            }
            cmd_meld(&root, &trees, &idxs, &args)
        }

        Commands::Merge(args) => {
            let (rest, branch) = split_rest_branch(args.rest, &cli.branch)?;
            let (target_token, merge_rest) = if let Some(first) = rest.first() {
                if first.starts_with('-') {
                    (None, rest)
                } else if gather_targets(Some(first), &[])
                    .and_then(|p| resolve_target_list(&trees, &p))
                    .is_ok()
                {
                    (Some(first.clone()), rest[1..].to_vec())
                } else {
                    (None, rest)
                }
            } else {
                (None, rest)
            };

            // `-b/--branch` on merge means the source to merge, not an
            // "other target" the way it does elsewhere: `git-wt merge -b 2`
            // is "merge 2 into <target, default current>", so it takes
            // exactly one branch and skips the generic dest/source idxs
            // dance below entirely.
            if !branch.is_empty() {
                if branch.len() > 1 || branch.iter().any(|b| b.contains(',')) {
                    return Err("merge's '-b/--branch' takes exactly one source branch".into());
                }
                if target_token.as_deref().is_some_and(|t| t.contains(',')) {
                    return Err(
                        "merge: can't combine a 'dest,source' target list with '-b/--branch'".into(),
                    );
                }
                let dest_idx = match &target_token {
                    Some(t) => {
                        let ns = resolve_target_list(&trees, &[t.clone()])?;
                        check_index(ns[0], trees.len())?
                    }
                    None => current_worktree_index(&trees)
                        .ok_or("not inside a worktree; use 'git-wt merge <N> -b <BRANCH>'")?,
                };
                let src_tok = &branch[0];
                let src_ns = resolve_target_list(&trees, &[src_tok.clone()])?;
                let src_idx = check_index(src_ns[0], trees.len())?;
                if src_idx == dest_idx {
                    return Err(format!("branch '{src_tok}' is already the target"));
                }
                let mut merge_argv = vec![ref_of(&trees[src_idx])?];
                merge_argv.extend(merge_rest.iter().cloned());
                let parsed = parse_merge_args(&merge_argv)?;
                return cmd_merge(&root, &trees, dest_idx, &parsed);
            }

            let idxs = resolve_targets(&trees, target_token.as_ref(), &branch, true, false)?;
            if idxs.is_empty() {
                return Err("not inside a worktree; use 'git-wt merge <N>[,<M>]'".into());
            }
            if idxs.len() > 2 {
                return Err(format!(
                    "merge takes exactly two worktrees, got {}\n\
                     hint: 'git-wt merge 1,2' or 'git-wt merge 1 <BRANCH>'",
                    idxs.len()
                ));
            }
            // A single resolved worktree named by a *branch* (not a plain
            // number) is ambiguous between "destination, still needs a
            // source" and "source, destination is current" -- the grammar
            // reads a bare branch as the latter (`git-wt merge <BRANCH>`).
            // A number always keeps its long-standing meaning, destination,
            // whatever flags or resume words follow it (`git-wt merge 2
            // --abort`, `git-wt merge 2 continue`).
            let is_branch_word = target_token
                .as_deref()
                .is_some_and(|t| t.parse::<usize>().is_err());
            let (dest_idx, mut merge_argv) = if idxs.len() == 1 && is_branch_word {
                let cur = current_worktree_index(&trees)
                    .ok_or("not inside a worktree; use 'git-wt merge <N>[,<M>]'")?;
                if idxs[0] == cur {
                    return Err(
                        "merge needs a source: 'git-wt <N>,<M> merge' (or 'git-wt <N> merge <BRANCH>', or continue/abort)"
                            .into(),
                    );
                }
                (cur, vec![ref_of(&trees[idxs[0]])?])
            } else if idxs.len() == 2 {
                (idxs[0], vec![ref_of(&trees[idxs[1]])?])
            } else {
                (idxs[0], Vec::new())
            };
            merge_argv.extend(merge_rest.iter().cloned());
            let parsed = parse_merge_args(&merge_argv)?;
            cmd_merge(&root, &trees, dest_idx, &parsed)
        }

        Commands::Merged(args) => {
            if typed_alias("m") {
                warn_if_alias_shadows_branch(&trees, "m", "merged");
            }
            let target = effective_target(args.targets.clone(), args.target_flag.as_ref())?;
            let idxs = resolve_targets(&trees, target.as_ref(), &cli.branch, false, false)?;
            if args.others {
                let idx = if idxs.len() == 1 {
                    idxs[0]
                } else if idxs.is_empty() {
                    current_worktree_index(&trees)
                        .ok_or("not inside a worktree; use 'git-wt merged --others <N>'")?
                } else {
                    return Err(format!("--others takes one worktree, got {}", idxs.len()));
                };
                return cmd_merged_others(&root, &trees, idx, args.show_path);
            }
            if idxs.len() == 2 {
                if args.source.is_some() {
                    return Err("merged takes no arguments after a two-worktree list".into());
                }
                let dest = ref_of(&trees[idxs[0]])?;
                let src = ref_of(&trees[idxs[1]])?;
                if src == dest {
                    return Err(format!("'{src}' is already checked out in worktree {}", idxs[0] + 1));
                }
                return cmd_merged(&root, &src, &dest);
            }
            if let Some(raw_src) = args.source {
                let idx = if idxs.len() == 1 {
                    idxs[0]
                } else if idxs.is_empty() {
                    current_worktree_index(&trees)
                        .ok_or("not inside a worktree; use 'git-wt merged <N> <BRANCH>'")?
                } else {
                    return Err(format!("merged takes one worktree with a branch source, got {}", idxs.len()));
                };
                let dest = ref_of(&trees[idx])?;
                let src = resolve_source(&root, &trees, &raw_src)?;
                if src == dest {
                    return Err(format!("'{raw_src}' is already checked out in worktree {}", idx + 1));
                }
                return cmd_merged(&root, &src, &dest);
            }
            let idx = if idxs.len() == 1 {
                idxs[0]
            } else if idxs.is_empty() {
                current_worktree_index(&trees)
                    .ok_or("not inside a worktree; use 'git-wt merged <N>'")?
            } else {
                return Err(format!(
                    "merged takes one or two worktrees, got {}\n\
                     hint: 'git-wt merged 1,2' or 'git-wt merged 1 <BRANCH>'",
                    idxs.len()
                ));
            };
            let dest = ref_of(&trees[idx])?;
            let src = crate::worktree::current_ref();
            cmd_merged(&root, &src, &dest)
        }

        Commands::Commits { rest } => {
            let (rest, branch, embedded_target) = split_rest_flags(rest, &cli.branch)?;
            let (target_token, commit_rest) = if let Some(first) = rest.first() {
                if first.starts_with('-') {
                    (None, rest)
                } else if gather_targets(Some(first), &[])
                    .and_then(|p| resolve_target_list(&trees, &p))
                    .is_ok()
                {
                    (Some(first.clone()), rest[1..].to_vec())
                } else {
                    (None, rest)
                }
            } else {
                (None, rest)
            };
            let target_token = effective_target(target_token, embedded_target.as_ref())?;
            let idxs = resolve_targets(
                &trees,
                target_token.as_ref(),
                &branch,
                true,
                false,
            )?;
            if idxs.is_empty() {
                return Err("not inside a worktree; use 'git-wt commits <N>[,...]'".into());
            }
            if typed_alias("c") {
                warn_if_alias_shadows_branch(&trees, "c", "commits");
            }
            cmd_commits(&root, &trees, &idxs, &commit_rest)
        }

        Commands::Log { rest } => {
            let (rest, branch, embedded_target) = split_rest_flags(rest, &cli.branch)?;
            // `log` is ambiguous: its first positional may be a target, a path, or
            // a git option. If it resolves as a worktree list, consume it as the
            // target; otherwise keep it as part of the path/options passed to git.
            let (target_token, log_rest) = if let Some(first) = rest.first() {
                if first.starts_with('-') {
                    (None, rest)
                } else if gather_targets(Some(first), &[])
                    .and_then(|p| resolve_target_list(&trees, &p))
                    .is_ok()
                {
                    (Some(first.clone()), rest[1..].to_vec())
                } else {
                    (None, rest)
                }
            } else {
                (None, rest)
            };
            let target_token = effective_target(target_token, embedded_target.as_ref())?;
            let mut idxs = resolve_targets(
                &trees,
                target_token.as_ref(),
                &branch,
                true,
                false,
            )?;
            if idxs.is_empty() {
                idxs = current_or_empty(&trees, "log")?;
            }
            if typed_alias("l") {
                warn_if_alias_shadows_branch(&trees, "l", "log");
            }
            cmd_log(&root, &trees, &idxs, &log_rest)
        }

        Commands::Doctor(args) => {
            if !cli.branch.is_empty() {
                return Err("'doctor' does not take '-b/--branch'".into());
            }
            cmd_doctor(&root, &trees, args)
        }

        Commands::Version => unreachable!(),
    }
}

/// Recover `-b`/`--branch` and `-t`/`--target` from a raw catch-all `rest`
/// (see `extract_branch_flag`'s doc comment for why they can end up there),
/// and fold them into whatever the global flags already caught.
fn split_rest_flags(
    rest: Vec<String>,
    cli_branch: &[String],
) -> Result<(Vec<String>, Vec<String>, Option<String>), String> {
    let (rest, embedded_branch) = extract_branch_flag(&rest)?;
    let (rest, embedded_target) = extract_target_flag(&rest)?;
    let mut branch = cli_branch.to_vec();
    if let Some(b) = embedded_branch {
        branch.push(b);
    }
    Ok((rest, branch, embedded_target))
}

/// `merge`'s own `rest` twin: extracts `-b/--branch` the same way, but never
/// `-t/--target` -- `merge` already spells `-t` for `theirs` in its own hand
/// parser, so that letter stays reserved and merge's target is positional-only.
fn split_rest_branch(
    rest: Vec<String>,
    cli_branch: &[String],
) -> Result<(Vec<String>, Vec<String>), String> {
    let (rest, embedded_branch) = extract_branch_flag(&rest)?;
    let mut branch = cli_branch.to_vec();
    if let Some(b) = embedded_branch {
        branch.push(b);
    }
    Ok((rest, branch))
}

/// Resolve a positional target list plus any `-b` values to 0-based worktree
/// indexes. Duplicates are removed, order is preserved.
fn resolve_targets(
    trees: &[crate::worktree::Worktree],
    targets: Option<&String>,
    branch: &[String],
    default_current: bool,
    allow_duplicates: bool,
) -> Result<Vec<usize>, String> {
    if !allow_duplicates {
        if let Some(t) = targets {
            let items: Vec<&str> = t.split(',').collect();
            if let Some(dup) = items.iter().enumerate().find_map(|(i, p)| {
                if items[..i].contains(p) {
                    Some(*p)
                } else {
                    None
                }
            }) {
                return if dup.parse::<usize>().is_ok() {
                    Err(format!("worktree #{} listed twice", dup))
                } else {
                    Err(format!("branch '{}' listed twice", dup))
                };
            }
        }
    }
    // The positional target list defaults to the current worktree when it is
    // absent entirely -- not merely when the combined (positional + `-b`)
    // result would otherwise be empty. This is what makes `-b` purely
    // additive: `git-wt commits -b 2` must reach `<cur>,2`, not just `2`.
    let mut idxs = Vec::new();
    if let Some(t) = targets {
        let parts: Vec<String> = t.split(',').map(String::from).collect();
        let ns = resolve_target_list(trees, &parts)?;
        for n in ns {
            let i = check_index(n, trees.len())?;
            if allow_duplicates || !idxs.contains(&i) {
                idxs.push(i);
            }
        }
    } else if default_current {
        if let Some(cur) = current_worktree_index(trees) {
            idxs.push(cur);
        }
    }
    if !branch.is_empty() {
        let bparts: Vec<String> = branch.iter().flat_map(|b| b.split(',').map(String::from)).collect();
        if bparts.iter().any(|p| p.is_empty()) {
            return Err("bad worktree list; want numbers or branches, e.g. '1,2' or 'main,2'".into());
        }
        let ns = resolve_target_list(trees, &bparts)?;
        for n in ns {
            let i = check_index(n, trees.len())?;
            if allow_duplicates || !idxs.contains(&i) {
                idxs.push(i);
            }
        }
    }
    Ok(idxs)
}

fn current_or_empty(trees: &[crate::worktree::Worktree], verb: &str) -> Result<Vec<usize>, String> {
    current_worktree_index(trees)
        .map(|i| vec![i])
        .ok_or_else(|| format!("not inside a worktree; use 'git-wt {verb} <N>[,...]'"))
}

/// Resolve a `merged` source token: a 1-based worktree number, a branch name
/// checked out in a worktree, or any git ref/branch name.
fn resolve_source(
    root: &std::path::Path,
    trees: &[crate::worktree::Worktree],
    tok: &str,
) -> Result<String, String> {
    if let Ok(n) = tok.parse::<usize>() {
        let i = check_index(n, trees.len())?;
        if trees[i].branch.is_none() {
            return Err(format!("no worktree or branch '{tok}'"));
        }
        return ref_of(&trees[i]);
    }
    if let Some(i) = worktree_on_branch(trees, tok) {
        return ref_of(&trees[i]);
    }
    if git_stdout(root, &["rev-parse", "--verify", tok]).is_ok() {
        return Ok(tok.to_string());
    }
    Err(format!("no worktree or branch '{tok}'"))
}
