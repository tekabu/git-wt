//! git-wt — create and manage git worktrees under `<repo>/.worktrees/`,
//! named `<repo-folder>-<sanitized-branch>`.
//!
//! Installed on PATH as `git-wt`, so it is also reachable as `git wt`.

mod cli;
mod cmd;
mod git;
mod ids;
mod ui;
mod worktree;

use crate::cli::{
    check_index, effective_target, gather_targets, resolve_worktree_or_branch_list, typed_verb,
    warn_if_alias_shadows_branch, worktree_on_branch, Cli, Commands,
};
use clap::{CommandFactory, Parser};
use crate::cmd::add::cmd_add;
use crate::cmd::commits::cmd_commits;
use crate::cmd::compare::cmd_compare;
use crate::cmd::diff::cmd_diff;
use crate::cmd::doctor::cmd_doctor;
use crate::cmd::list::cmd_list;
use crate::cmd::log::cmd_log;
use crate::cmd::meld::cmd_meld;
use crate::cmd::merge::{build_merge_parsed_args, cmd_merge, resolve_merge_source};
use crate::cmd::merged::{cmd_merged, cmd_merged_others};
use crate::cmd::remove::cmd_remove;
use crate::cmd::review::cmd_review;
use crate::cmd::switch::{cmd_path, cmd_switch};
use crate::cmd::sync::{cmd_sync, fetch_parsed, pull_parsed, push_parsed};
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
            if let Some(t) = cli.worktree_or_branch_list {
                return cmd_switch(
                    &root,
                    crate::cmd::switch::args::SwitchArgs {
                        target: Some(t),
                        target_flag: Default::default(),
                    },
                );
            }
            return cmd_list(
                &root,
                crate::cmd::list::args::ListArgs {
                    search: None,
                    col: cli.list_col,
                    long: cli.list_long,
                    short: cli.list_short,
                    show_path: cli.list_show_path,
                    files: Default::default(),
                    less: cli.list_pager,
                },
            );
        }
    };

    // Grammar is verb-first: the optional positional target list comes after
    // the subcommand. Any target captured at the top level means the user
    // wrote target-first.
    if cli.worktree_or_branch_list.is_some() {
        return Err("unexpected argument for the verb".into());
    }

    // The one-letter alias each of these matched, if that's the spelling the
    // user actually typed -- see `typed_verb`'s docs for why the parsed
    // `Commands` variant alone can't tell us that.
    let typed = typed_verb();
    let typed_alias = |a: &str| typed.as_deref() == Some(a);

    match cmd {
        Commands::Add(args) => {
            if typed_alias("a") {
                warn_if_alias_shadows_branch(&trees, "a", "add");
            }
            cmd_add(&root, args)
        }

        Commands::List(args) => cmd_list(&root, args),

        Commands::Switch(args) => {
            let target = effective_target(args.target, args.target_flag.target_flag.as_ref())?;
            // No `-b` to append with, but the target itself could still be a
            // comma-separated list (e.g. a stray '1,2'), so the cardinality
            // check stays.
            let idxs = resolve_targets(&trees, target.as_ref(), &[], false, false)?;
            if idxs.len() > 1 {
                return Err(format!(
                    "switch takes one worktree, got {}",
                    idxs.len()
                ));
            }
            if typed_alias("s") {
                warn_if_alias_shadows_branch(&trees, "s", "switch");
            }
            let args = crate::cmd::switch::args::SwitchArgs { target, target_flag: Default::default() };
            cmd_switch(&root, args)
        }

        Commands::Path(args) => {
            let target = effective_target(args.target, args.target_flag.target_flag.as_ref())?;
            cmd_path(
                &root,
                crate::cmd::switch::args::PathArgs { target, target_flag: Default::default() },
            )
        }

        Commands::Remove(args) => {
            let target = effective_target(args.target.clone(), args.target_flag.target_flag.as_ref())?;
            // No `-b` to append with, but the target itself could still be a
            // comma-separated list, so the cardinality check stays.
            let idxs = resolve_targets(&trees, target.as_ref(), &[], true, false)?;
            if idxs.len() > 1 {
                return Err(format!("remove takes one worktree, got {}", idxs.len()));
            }
            let idx = idxs.into_iter().next().expect("len 1");
            let args = crate::cmd::remove::args::RemoveArgs {
                target,
                target_flag: Default::default(),
                ..args
            };
            cmd_remove(&root, &trees, idx, args)
        }

        Commands::Fetch(args) => {
            let common = args.common.clone();
            run_sync(&trees, common, fetch_parsed(&args))
        }

        Commands::Pull(args) => {
            if typed_alias("p") {
                warn_if_alias_shadows_branch(&trees, "p", "pull");
            }
            let common = args.common.clone();
            run_sync(&trees, common, pull_parsed(&args))
        }

        Commands::Push(args) => {
            let common = args.common.clone();
            run_sync(&trees, common, push_parsed(&args)?)
        }

        Commands::Diff(args) => {
            // Grammar mirrors merge's: the bare positional is the source --
            // the other worktree to compare -- never the destination. The
            // destination is never positional, only `-d/--destination`,
            // defaulting to the current worktree.
            if args.sd.destination_flag.as_deref().is_some_and(|t| t.contains(',')) {
                return Err("diff's '-d/--destination' takes exactly one target".into());
            }
            let positional = args.targets.clone();
            if let Some(t) = positional.as_deref().filter(|t| t.contains(',')) {
                return Err(format!(
                    "diff takes one source, got the list '{t}'; the destination is '-d/--destination'"
                ));
            }
            if let Some(s) = args.sd.source_flag.as_deref() {
                if s.contains(',') {
                    return Err("diff's '-s/--source' takes exactly one source branch".into());
                }
                if let Some(w) = positional.as_deref() {
                    return Err(format!(
                        "source given twice: '{w}' and '-s/--source {s}'; use one or the other"
                    ));
                }
            }
            let dest_idx = match args.sd.destination_flag.as_deref() {
                Some(t) => {
                    let ns = resolve_worktree_or_branch_list(&trees, &[t.to_string()])?;
                    check_index(ns[0], &trees)?
                }
                None => current_worktree_index(&trees)
                    .ok_or("not inside a worktree; use 'git-wt diff <SOURCE> -d <DEST>'")?,
            };
            let tok = args
                .sd
                .source_flag
                .clone()
                .or(positional)
                .ok_or("diff needs a source: 'git-wt diff <SOURCE>' (or '-s <SOURCE>')")?;
            let ns = resolve_worktree_or_branch_list(&trees, &[tok])?;
            let src_idx = check_index(ns[0], &trees)?;
            cmd_diff(&root, &trees, &[dest_idx, src_idx], &args)
        }

        Commands::Meld(args) => {
            let has_position = args.position.left.is_some()
                || args.position.right.is_some()
                || args.position.center.is_some();
            let has_list = args.worktree_or_branch_list.is_some() || !args.branch.extra_ref.is_empty();
            if has_position && has_list {
                return Err(
                    "meld's '--left'/'--right'/'--center' and the worktree list \
                     (positional/-x) are alternatives; use one or the other"
                        .into(),
                );
            }
            if has_position {
                let left = args
                    .position
                    .left
                    .as_deref()
                    .ok_or("meld needs '--left' along with '--right'/'--center'")?;
                return crate::cmd::meld::cmd_meld_position(
                    &trees,
                    left,
                    args.position.right.as_deref(),
                    args.position.center.as_deref(),
                    &args,
                );
            }
            let idxs = resolve_targets(&trees, args.worktree_or_branch_list.as_ref(), &args.branch.extra_ref, true, true)?;
            if idxs.len() < 2 {
                return Err(format!(
                    "meld needs 2 or 3 worktrees, got {}",
                    idxs.len()
                ));
            }
            if idxs.len() > 3 {
                return Err(format!(
                    "meld takes at most 3 worktrees, got {}",
                    idxs.len()
                ));
            }
            cmd_meld(&root, &trees, &idxs, &args)
        }

        Commands::Compare(args) => {
            let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
            cmd_compare(&cwd, &args)
        }

        Commands::Merge(mut args) => {
            // Grammar, deliberately the same shape as `git merge <thing>`:
            // the one bare positional is always the *source* to merge in,
            // whether it is a worktree number, a branch that has a worktree,
            // or a plain branch name. The destination is never positional --
            // it is `-d/--destination`, defaulting to the current worktree.
            // One token, one meaning, and it is the same meaning `git merge`
            // gives it.
            if args.sd.destination_flag.as_deref().is_some_and(|t| t.contains(',')) {
                return Err("merge's '-d/--destination' takes exactly one target".into());
            }
            let positional = args.options.source.take();
            if let Some(t) = positional.as_deref().filter(|t| t.contains(',')) {
                return Err(format!(
                    "merge takes one source, got the list '{t}'; the destination is '-d/--destination'"
                ));
            }

            // `-s/--source` on merge is a second spelling of the source, not
            // the "extra target" `-b` is under every other verb, so it cannot
            // double up with the positional.
            if let Some(b) = args.sd.source_flag.as_deref() {
                if b.contains(',') {
                    return Err("merge's '-s/--source' takes exactly one source branch".into());
                }
                if let Some(w) = positional.as_deref() {
                    return Err(format!(
                        "source given twice: '{w}' and '-s/--source {b}'; use one or the other"
                    ));
                }
            }

            let dest_idx = match args.sd.destination_flag.as_deref() {
                Some(t) => {
                    let ns = resolve_worktree_or_branch_list(&trees, &[t.to_string()])?;
                    check_index(ns[0], &trees)?
                }
                None => current_worktree_index(&trees)
                    .ok_or("not inside a worktree; use 'git-wt merge <SOURCE> -d <DEST>'")?,
            };

            // A source naming a worktree becomes that worktree's ref; one
            // that names no worktree here is a branch in its own right and
            // goes on unresolved, so a branch with no worktree still merges.
            // A *number* has no such second reading -- it is a worktree or
            // it is nothing -- so it reports its own miss rather than being
            // handed on as a would-be branch called "99".
            args.options.source = match args.sd.source_flag.take().or(positional) {
                None => None,
                Some(tok) => {
                    let idx = if tok.parse::<usize>().is_ok() {
                        let ns = resolve_worktree_or_branch_list(&trees, std::slice::from_ref(&tok))?;
                        Some(check_index(ns[0], &trees)?)
                    } else {
                        worktree_on_branch(&trees, tok.strip_prefix("heads/").unwrap_or(&tok))
                    };
                    match idx {
                        Some(i) if i == dest_idx => {
                            return Err(format!(
                                "worktree #{} is both the source and the target",
                                trees[i].id
                            ))
                        }
                        Some(i) => Some(ref_of(&trees[i])?),
                        None => Some(tok),
                    }
                }
            };
            let parsed = build_merge_parsed_args(args.options)?;
            cmd_merge(&root, &trees, dest_idx, &parsed)
        }

        Commands::Review(mut args) => {
            // Same source/dest grammar `merge` has -- the positional is the
            // source, `-d/--destination` the destination, defaulting to the
            // current worktree -- because it is the same question about the
            // same pair. `-d` is review's own field, not the shared commits
            // vocabulary (`--date` already sits on that letter there);
            // `-s/--source` is review's own field too, the same letter
            // merge's own second source spelling uses.
            if !args.flags.common.branch.extra_ref.is_empty() {
                return Err(
                    "review has no '-x/--reference'; name the source with '-s/--source' or the positional".into(),
                );
            }
            if args.sd.destination_flag.as_deref().is_some_and(|t| t.contains(',')) {
                return Err("review's '-d/--destination' takes exactly one target".into());
            }
            if let Some(t) = args.source.as_deref().filter(|t| t.contains(',')) {
                return Err(format!(
                    "review takes one source, got the list '{t}'; the destination is '-d/--destination'"
                ));
            }
            if let Some(s) = args.sd.source_flag.as_deref().filter(|s| s.contains(',')) {
                return Err(format!(
                    "review's '-s/--source' takes exactly one source branch, got the list '{s}'"
                ));
            }
            if let (Some(w), Some(s)) = (args.source.as_deref(), args.sd.source_flag.as_deref()) {
                return Err(format!(
                    "source given twice: '{w}' and '-s/--source {s}'; use one or the other"
                ));
            }
            let dest_idx = match args.sd.destination_flag.take().as_deref() {
                Some(t) => {
                    let ns = resolve_worktree_or_branch_list(&trees, &[t.to_string()])?;
                    check_index(ns[0], &trees)?
                }
                None => current_worktree_index(&trees)
                    .ok_or("not inside a worktree; use 'git-wt review <SOURCE> -d <DEST>'")?,
            };
            let tok = args
                .sd
                .source_flag
                .take()
                .or(args.source)
                .ok_or("review needs a source: 'git-wt review <SOURCE>' (or '-s <SOURCE>')")?;
            let idx = if tok.parse::<usize>().is_ok() {
                let ns = resolve_worktree_or_branch_list(&trees, std::slice::from_ref(&tok))?;
                Some(check_index(ns[0], &trees)?)
            } else {
                worktree_on_branch(&trees, tok.strip_prefix("heads/").unwrap_or(&tok))
            };
            let src = match idx {
                Some(i) if i == dest_idx => {
                    return Err(format!(
                        "worktree #{} is both the source and the target",
                        trees[i].id
                    ))
                }
                Some(i) => ref_of(&trees[i])?,
                None => tok,
            };
            let src = resolve_merge_source(&root, &trees, &src)?;
            cmd_review(&root, &trees, dest_idx, &src, args.meld.meld, args.flags)
        }

        Commands::Merged(args) => {
            if typed_alias("m") {
                warn_if_alias_shadows_branch(&trees, "m", "merged");
            }
            let target = effective_target(args.worktree_or_branch_list.clone(), args.target_flag.as_ref())?;
            let idxs = resolve_targets(&trees, target.as_ref(), &args.branch, false, false)?;
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
                    return Err(format!("'{src}' is already checked out in worktree {}", trees[idxs[0]].id));
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
                    return Err(format!("'{raw_src}' is already checked out in worktree {}", trees[idx].id));
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
                    "merged takes one or two worktrees, got {}",
                    idxs.len()
                ));
            };
            let dest = ref_of(&trees[idx])?;
            let src = crate::worktree::current_ref();
            cmd_merged(&root, &src, &dest)
        }

        Commands::Commits(args) => {
            let target = effective_target(args.worktree_or_branch_list.clone(), args.target_flag.target_flag.as_ref())?;
            let idxs = resolve_targets(&trees, target.as_ref(), &args.common.branch.extra_ref, true, false)?;
            if idxs.is_empty() {
                return Err("not inside a worktree; use 'git-wt commits <N>[,...]'".into());
            }
            if typed_alias("c") {
                warn_if_alias_shadows_branch(&trees, "c", "commits");
            }
            cmd_commits(&root, &trees, &idxs, args)
        }

        Commands::Log(args) => {
            // `log` is ambiguous: its leading positional run may open with a
            // target, or be paths outright. If the first token resolves as a
            // worktree list, it is consumed as the target; otherwise the whole
            // run is paths and the current worktree is used.
            let target_token = match args.leading.first() {
                Some(first)
                    if gather_targets(Some(first), &[])
                        .and_then(|p| resolve_worktree_or_branch_list(&trees, &p))
                        .is_ok() =>
                {
                    Some(first.clone())
                }
                _ => None,
            };
            let paths: Vec<String> = if target_token.is_some() {
                args.leading[1..].to_vec()
            } else {
                args.leading.clone()
            };
            let target_token = effective_target(target_token, args.target_flag.target_flag.as_ref())?;
            let mut idxs = resolve_targets(&trees, target_token.as_ref(), &args.common.branch.extra_ref, true, false)?;
            if idxs.is_empty() {
                idxs = current_or_empty(&trees, "log")?;
            }
            if typed_alias("l") {
                warn_if_alias_shadows_branch(&trees, "l", "log");
            }
            cmd_log(&root, &trees, &idxs, &paths, args)
        }

        Commands::Doctor(args) => {
            cmd_doctor(&root, &trees, args)
        }

        Commands::Version => unreachable!(),
    }
}

/// Shared body of the fetch/pull/push arms: `--all` is now a plain declared
/// bool on each verb's own struct, so unlike the old catch-all it is always
/// caught by clap regardless of where it sits -- no more merging two sources
/// of "all" to cover the position it could hide in.
fn run_sync(
    trees: &[crate::worktree::Worktree],
    common: crate::cmd::sync::args::SyncCommon,
    parsed: crate::cmd::sync::SyncParsedArgs,
) -> Result<(), String> {
    if parsed.all && (common.worktree_or_branch_list.is_some() || !common.branch.extra_ref.is_empty()) {
        return Err("'--all' is every worktree, so a target list has nothing to add".into());
    }
    let idxs = if parsed.all {
        (0..trees.len()).collect()
    } else {
        let mut idxs = resolve_targets(trees, common.worktree_or_branch_list.as_ref(), &common.branch.extra_ref, true, false)?;
        if idxs.is_empty() {
            let cur = current_worktree_index(trees).ok_or_else(|| {
                format!("not inside a worktree; use 'git-wt <N> {}'", parsed.op.word())
            })?;
            idxs.push(cur);
        }
        idxs
    };
    cmd_sync(trees, &idxs, &parsed)
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
        let ns = resolve_worktree_or_branch_list(trees, &parts)?;
        for n in ns {
            let i = check_index(n, trees)?;
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
        let ns = resolve_worktree_or_branch_list(trees, &bparts)?;
        for n in ns {
            let i = check_index(n, trees)?;
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
        let i = check_index(n, trees)?;
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
