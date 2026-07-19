use std::path::Path;

use crate::cmd::commits::cmd_commits;
use crate::cmd::diff::cmd_diff;
use crate::cmd::list::{cmd_list, parse_cols, ListMode};
use crate::cmd::meld::cmd_meld;
use crate::cmd::merge::{cmd_merge, parse_merge_args, resolve_merge_source, MergeOp};
use crate::cmd::merged::{cmd_merged, cmd_merged_others};
use crate::cmd::remove::cmd_remove;
use crate::cmd::sync::{cmd_sync, parse_sync_args, SyncOp};
use crate::worktree::{current_ref, here_index, label, ref_of, worktrees};

/// Legacy verb-first forms get a migration hint; branch-like words get an
/// `add` suggestion.
pub(crate) fn unknown_command_msg(tok: &str) -> String {
    match tok {
        "show" => "unknown command 'show'; use 'git-wt 1 path'".into(),
        "remove" | "rm" => format!("unknown command '{tok}'; use 'git-wt 1 remove'"),
        "merge" => "unknown command 'merge'; use 'git-wt 1,2 merge'".into(),
        "merged" => "unknown command 'merged'; use 'git-wt 1 merged' or 'git-wt 1,2 merged'".into(),
        "commits" => "unknown command 'commits'; use 'git-wt 1,2 commits'".into(),
        _ if branch_like(tok) => format!("unknown command '{tok}'; did you mean 'add {tok}'?"),
        _ => format!("unknown command '{tok}'"),
    }
}

/// A word looks like a branch when it has a `/` or `-` and no whitespace.
pub(crate) fn branch_like(s: &str) -> bool {
    !s.chars().any(char::is_whitespace) && (s.contains('/') || s.contains('-'))
}

// ---------------------------------------------------------------------------
// Target dispatch: git-wt <N> [action]
// ---------------------------------------------------------------------------

pub(crate) fn dispatch_target(root: &Path, n: usize, rest: &[String]) -> Result<(), String> {
    let trees = worktrees(root)?;
    let idx = check_index(n, trees.len())?;

    let action = rest.first().map(String::as_str).unwrap_or("switch");
    match action {
        "switch" | "cd" | "path" | "show" => {
            if rest.len() > 1 {
                return Err("too many arguments\nTry 'git-wt --help'".into());
            }
            // The branch is status, so it goes to stderr; the path is the
            // stdout contract (`cd "$(git-wt 1 path)"` stays clean).
            eprintln!("{}", label(&trees[idx]));
            println!("{}", trees[idx].path.display());
            Ok(())
        }
        "remove" | "rm" => {
            let mut yes = false;
            let mut force = false;
            for a in &rest[1..] {
                match a.as_str() {
                    "-y" => yes = true,
                    "-f" | "--force" => force = true,
                    other => {
                        return Err(format!("unexpected argument '{other}' for remove"));
                    }
                }
            }
            cmd_remove(root, &trees, idx, yes, force)
        }
        // `1 diff 2` was the old grammar; point at the list form meld already
        // uses. `merge` keeps the single-target form for branch sources and for
        // `continue`/`abort`; only a worktree-number source now uses the list.
        // A source equal to the destination is left to `cmd_merge`, which gives
        // the clearer "already checked out" error and preserves the documented
        // worktree-wins rule for digit branch names.
        "diff" => Err(format!(
            "diff takes a worktree list: 'git-wt {n},<M> diff'"
        )),
        // The same reading 'merged' gives a single target: N against the
        // worktree you are standing in. A one-column table would just be
        // 'git log', so the second column is the one you are already in.
        "commits" => {
            let Some(here) = here_index(&trees) else {
                return Err(format!(
                    "not inside a worktree, so there is no second branch to compare \
                     against\nhint: 'git-wt {n},<M> commits' names both"
                ));
            };
            if here == idx {
                return Err(format!(
                    "worktree #{n} is the one you are standing in, so the table would \
                     compare it with itself\nhint: 'git-wt {n},<M> commits' names both"
                ));
            }
            cmd_commits(root, &trees, &[here, idx], &rest[1..])
        }
        "merge" => {
            let args = parse_merge_args(&rest[1..])?;
            if let MergeOp::Start(src) = &args.op {
                if let Ok(m) = src.parse::<usize>() {
                    if m != n && (1..=trees.len()).contains(&m) {
                        return Err(format!(
                            "merge takes a worktree list: 'git-wt {n},{m} merge' \
                             (or use 'heads/{m}' for a branch of the same name)"
                        ));
                    }
                }
            }
            cmd_merge(root, &trees, idx, &args)
        }
        "merged" => {
            let args = &rest[1..];
            // `--others` asks for a table, not a yes/no answer.
            if args.iter().any(|a| a == "--others") {
                let show_path = show_path_from_rest(args);
                let extra: Vec<&str> = args
                    .iter()
                    .map(String::as_str)
                    .filter(|a| *a != "--others" && *a != "-p" && *a != "--show-path")
                    .collect();
                if !extra.is_empty() {
                    return Err(format!(
                        "--others takes no arguments (got '{}')\nTry 'git-wt --help'",
                        extra.join("', '")
                    ));
                }
                return cmd_merged_others(root, &trees, idx, show_path);
            }
            if args.len() > 1 {
                return Err("too many arguments\nTry 'git-wt --help'".into());
            }
            // A worktree-number source uses the list form, as merge and diff do;
            // the single form stays for a branch source, which a list of numbers
            // cannot name. A source equal to the destination falls through to the
            // self-check below for its clearer "already checked out" error.
            if let Some(src) = args.first() {
                if let Ok(m) = src.parse::<usize>() {
                    if m != n && (1..=trees.len()).contains(&m) {
                        return Err(format!(
                            "merged takes a worktree list: 'git-wt {n},{m} merged' \
                             (or use 'heads/{m}' for a branch of the same name)"
                        ));
                    }
                }
            }
            let has_explicit_source = !args.is_empty();
            let src = if has_explicit_source {
                // "git-wt N merged BRANCH" reads dest-first, like merge.
                resolve_merge_source(root, &trees, &args[0])?
            } else {
                // "git-wt N merged" asks whether N's branch is already in the
                // branch we are standing in now.
                ref_of(&trees[idx])?
            };
            let dest = if has_explicit_source {
                ref_of(&trees[idx])?
            } else {
                current_ref()
            };
            // Reject the explicit self-check (1 merged 1) the same way merge
            // does; the bare form (1 merged) intentionally asks about itself.
            if has_explicit_source && src == dest {
                return Err(format!("'{src}' is already checked out in worktree {}", idx + 1));
            }
            cmd_merged(root, &src, &dest)
        }
        "fetch" | "pull" | "push" => {
            let op = SyncOp::from_word(action).expect("matched above");
            let args = parse_sync_args(op, &rest[1..])?;
            // `--all` is every worktree, so a target contradicts it; the target
            // is the more specific thing said, so name the form that keeps it.
            if args.all {
                return Err(format!(
                    "'--all' is every worktree, so worktree #{n} has nothing to add\n\
                     hint: 'git-wt {action} --all', or 'git-wt {n} {action}' for just this one"
                ));
            }
            cmd_sync(&trees, &[idx], &args)
        }
        // A single target can't be melded, but say so in meld's own terms.
        "meld" => cmd_meld(root, &trees, &[idx], &rest[1..]),
        // An option in the action slot is never right, whatever the option is:
        // each action carries its own, after its own verb.
        other if other.starts_with('-') => Err(format!(
            "'{other}' is an option, not an action; options follow the action, \
             e.g. 'git-wt {n} remove -f' or 'git-wt {n},2 diff --stat'"
        )),
        other => Err(format!(
            "unknown action '{other}' (switch, path, remove, diff, commits, merge, meld, \
             merged, fetch, pull, push)"
        )),
    }
}

/// Recognize a comma-separated target list like `1,2,3`. Returns Ok(None) when
/// the token is not one at all (so the caller keeps looking), and an error when
/// it clearly meant to be one but is malformed (`1,,2`, `1,x`).
pub(crate) fn parse_target_list(tok: &str) -> Result<Option<Vec<usize>>, String> {
    if !tok.contains(',') {
        return Ok(None);
    }
    let mut out = Vec::new();
    for part in tok.split(',') {
        let n: usize = part
            .parse()
            .map_err(|_| format!("bad worktree list '{tok}'; want numbers, e.g. '1,2'"))?;
        out.push(n);
    }
    Ok(Some(out))
}

pub(crate) fn dispatch_targets(root: &Path, ns: &[usize], rest: &[String]) -> Result<(), String> {
    let trees = worktrees(root)?;
    let mut idxs = Vec::new();
    for &n in ns {
        idxs.push(check_index(n, trees.len())?);
    }

    match rest.first().map(String::as_str) {
        Some("meld") => cmd_meld(root, &trees, &idxs, &rest[1..]),
        Some("diff") => cmd_diff(root, &trees, &idxs, &rest[1..]),
        Some("commits") => cmd_commits(root, &trees, &idxs, &rest[1..]),
        // `1,2 merge`: the list reads dest-first, so 2 merges into 1.
        Some("merge") => {
            // The list already names the source, so a resume word contradicts
            // it: there is nothing for `continue` to take a source from.
            // Check this before the count so an over-long list with `continue`
            // gets the more useful resume-word message.
            if let Some(word) = rest[1..].iter().find_map(|a| resume_word(a)) {
                return Err(format!(
                    "'{word}' takes no source, so a worktree list has nothing to name\n\
                     hint: 'git-wt {n} merge {word}'",
                    n = ns[0]
                ));
            }
            if idxs.len() != 2 {
                return Err(format!(
                    "merge takes exactly two worktrees, not {}: 'git-wt <N>,<M> merge' \
                     merges M into N",
                    idxs.len()
                ));
            }
            // Hand the source to the single-target parser as the positional it
            // already understands, so both spellings share one code path.
            let mut argv = vec![ns[1].to_string()];
            argv.extend_from_slice(&rest[1..]);
            let args = parse_merge_args(&argv)?;
            cmd_merge(root, &trees, idxs[0], &args)
        }
        // `1,2 merged` == "is 2 already in 1?" — same dest-first reading as merge.
        Some("merged") => {
            if idxs.len() != 2 {
                return Err(format!(
                    "merged takes exactly two worktrees, not {}: 'git-wt <N>,<M> merged' \
                     asks whether M is already in N",
                    idxs.len()
                ));
            }
            if rest.len() > 1 {
                return Err("merged takes no arguments\nTry 'git-wt --help'".into());
            }
            if idxs[0] == idxs[1] {
                return Err(format!("worktree #{} listed twice", idxs[0] + 1));
            }
            let dest = ref_of(&trees[idxs[0]])?;
            let src = ref_of(&trees[idxs[1]])?;
            cmd_merged(root, &src, &dest)
        }
        // `1,3 pull`: the sweep, narrowed to the worktrees you named.
        Some(w) if SyncOp::from_word(w).is_some() => {
            let op = SyncOp::from_word(w).expect("matched above");
            let args = parse_sync_args(op, &rest[1..])?;
            if args.all {
                return Err(format!(
                    "'--all' is every worktree, so a worktree list has nothing to add\n\
                     hint: 'git-wt {w} --all', or drop it to sweep just the ones you named"
                ));
            }
            cmd_sync(&trees, &idxs, &args)
        }
        // A list only makes sense for actions that take more than one worktree.
        Some(other) => Err(format!(
            "'{other}' takes a single worktree; only 'commits', 'diff', 'meld', 'merge', \
             'merged', 'fetch', 'pull' and 'push' take a list"
        )),
        None => Err("a worktree list needs an action, e.g. 'git-wt 1,2 diff'".into()),
    }
}

/// The resume word a token spells, in any of its accepted forms, or None.
///
/// Only `continue`/`abort` qualify: they act on a merge that already exists, so
/// they name no source and a worktree list has nothing to hand them. The other
/// keywords — `ours`, `theirs`, `dry-run` — all describe a merge that is about
/// to start, so they combine with a list perfectly well.
pub(crate) fn resume_word(tok: &str) -> Option<&'static str> {
    match tok {
        "continue" | "--continue" | "-c" => Some("continue"),
        "abort" | "--abort" | "-a" => Some("abort"),
        _ => None,
    }
}

/// Map a 1-based index to a 0-based one, or an error.
pub(crate) fn check_index(n: usize, len: usize) -> Result<usize, String> {
    if n == 0 {
        return Err("no worktree #0".into());
    }
    if n > len {
        return Err(format!(
            "no worktree #{n}; there are {len} (see 'git-wt list')"
        ));
    }
    Ok(n - 1)
}

/// True when a slice of arguments contains `-p` or `--show-path`.
pub(crate) fn show_path_from_rest(args: &[String]) -> bool {
    args.iter().any(|a| a == "-p" || a == "--show-path")
}

/// Parse `list` arguments (an optional SEARCH plus `--col`) then list. Shared
/// by `list`/`ls`, the no-args default, and a bare leading `--col`.
pub(crate) fn list_from_args(root: &Path, args: &[String]) -> Result<(), String> {
    let mut search: Option<String> = None;
    let mut cols: Option<Vec<usize>> = None;
    let mut mode = ListMode::Normal;
    let mut show_path = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--col" | "-c" => {
                let v = it.next().ok_or("--col needs columns, e.g. 1,2,3")?;
                cols = Some(parse_cols(v)?);
            }
            s if s.starts_with("--col=") => cols = Some(parse_cols(&s["--col=".len()..])?),
            "--long" | "-l" => mode = ListMode::Long,
            "--short" | "-s" => mode = ListMode::Short,
            "--show-path" | "-p" => show_path = true,
            s if s.starts_with('-') && s != "-" => {
                return Err(format!("unknown option '{s}'\nTry 'git-wt --help'"));
            }
            s => {
                if search.is_some() {
                    return Err("too many arguments\nTry 'git-wt --help'".into());
                }
                search = Some(s.to_string());
            }
        }
    }
    cmd_list(root, search.as_deref(), cols, mode, show_path)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_like_detection() {
        assert!(branch_like("feat/x"));
        assert!(branch_like("feat-x"));
        assert!(!branch_like("lsit"));
        assert!(!branch_like("foo bar"));
    }


    #[test]
    fn check_index_bounds() {
        assert_eq!(check_index(1, 3), Ok(0));
        assert_eq!(check_index(3, 3), Ok(2));
        assert_eq!(check_index(0, 3), Err("no worktree #0".into()));
        assert_eq!(
            check_index(4, 3),
            Err("no worktree #4; there are 3 (see 'git-wt list')".into())
        );
    }


    #[test]
    fn unknown_command_messages() {
        assert_eq!(
            unknown_command_msg("show"),
            "unknown command 'show'; use 'git-wt 1 path'"
        );
        assert_eq!(
            unknown_command_msg("remove"),
            "unknown command 'remove'; use 'git-wt 1 remove'"
        );
        assert_eq!(
            unknown_command_msg("merge"),
            "unknown command 'merge'; use 'git-wt 1,2 merge'"
        );
        assert_eq!(
            unknown_command_msg("feat/x"),
            "unknown command 'feat/x'; did you mean 'add feat/x'?"
        );
        assert_eq!(
            unknown_command_msg("merged"),
            "unknown command 'merged'; use 'git-wt 1 merged' or 'git-wt 1,2 merged'"
        );
        assert_eq!(unknown_command_msg("lsit"), "unknown command 'lsit'");
    }

}
