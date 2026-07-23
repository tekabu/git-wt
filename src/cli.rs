use std::path::Path;

use crate::cmd::commits::cmd_commits;
use crate::cmd::log::cmd_log;
use crate::cmd::diff::cmd_diff;
use crate::cmd::list::{cmd_list, parse_cols, ListMode};
use crate::cmd::meld::cmd_meld;
use crate::cmd::merge::{cmd_merge, parse_merge_args, resolve_merge_source, MergeOp};
use crate::cmd::merged::{cmd_merged, cmd_merged_others};
use crate::cmd::remove::cmd_remove;
use crate::cmd::sync::{cmd_sync, parse_sync_args, SyncOp};
use crate::worktree::{current_ref, label, ref_of, worktrees, Worktree};

/// Message for a leading word that is neither a number nor a known verb.
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
            let mut delete_branch = false;
            for a in &rest[1..] {
                match a.as_str() {
                    "-y" => yes = true,
                    "-f" | "--force" => force = true,
                    "-D" | "--delete-branch" => delete_branch = true,
                    other => {
                        return Err(format!("unexpected argument '{other}' for remove"));
                    }
                }
            }
            cmd_remove(root, &trees, idx, yes, force, delete_branch)
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
        // A single target is that worktree's own log, nothing else: naming one
        // branch is asking about one branch, and quietly pulling in the
        // worktree you happen to be standing in answers a wider question than
        // was asked. 'git-wt {n},<M> commits' is still how you compare two.
        "commits" | "c" => cmd_commits(root, &trees, &[idx], &rest[1..]),
        // A single target's own history of the path, no comparison columns --
        // the same "one worktree, no mark columns" rule 'commits' follows.
        "log" | "l" => cmd_log(root, &trees, &[idx], &rest[1..]),
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
        "merged" | "m" => {
            let args = &rest[1..];
            // `--others` asks for a table, not a yes/no answer.
            if args.iter().any(|a| a == "--others" || a == "--ot" || a == "-o") {
                let show_path = show_path_from_rest(args);
                let extra: Vec<&str> = args
                    .iter()
                    .map(String::as_str)
                    .filter(|a| {
                        *a != "--others" && *a != "--ot" && *a != "-o" && *a != "-p" && *a != "--show-path"
                    })
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
        "fetch" | "pull" | "push" | "p" => {
            // "p" is the one-letter alias for `pull` specifically; `fetch`
            // and `push` keep only their full words.
            let op = if action == "p" {
                SyncOp::Pull
            } else {
                SyncOp::from_word(action).expect("matched above")
            };
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
            "unknown action '{other}' (switch, path, remove, diff, commits, log, merge, meld, \
             merged, fetch, pull, push; aliases: c=commits, l=log, m=merged, p=pull)"
        )),
    }
}

/// Recognize a comma-separated target list like `1,2,3` or `main,2`. Returns
/// Ok(None) when the token is not one at all (so the caller keeps looking), and
/// an error when it clearly meant to be one but is malformed (`1,,2`).
///
/// Parts are returned as written; `resolve_target_list` turns them into the
/// worktree numbers the rest of the grammar runs on.
pub(crate) fn parse_target_list(tok: &str) -> Result<Option<Vec<String>>, String> {
    if !tok.contains(',') {
        return Ok(None);
    }
    let mut out = Vec::new();
    for part in tok.split(',') {
        if part.is_empty() {
            return Err(format!(
                "bad worktree list '{tok}'; want numbers or branches, e.g. '1,2' or 'main,2'"
            ));
        }
        out.push(part.to_string());
    }
    Ok(Some(out))
}

/// Turn each part of a target list into a 1-based worktree number, so
/// everything downstream keeps working on numbers alone.
///
/// A bare number in range is that worktree, even when a branch shares the name;
/// this is the same worktree-wins rule `merge` and `merged` already apply, and
/// `heads/2` is still the way to mean the branch. Anything else is matched
/// against the checked-out branches. It has to be a *worktree*: a list action
/// diffs, melds or sweeps real directories, so a branch nobody has checked out
/// has no path to give. `git-wt <N> merge <branch>` remains the way to name one.
pub(crate) fn resolve_target_list(trees: &[Worktree], parts: &[String]) -> Result<Vec<usize>, String> {
    let mut out = Vec::new();
    for part in parts {
        // A number is a worktree number, full stop -- in range or not. Out of
        // range it is check_index that should say "no worktree #9", not a
        // branch lookup reporting a branch nobody was asking for; `heads/9` is
        // still there for a branch that really is called `9`.
        if let Ok(n) = part.parse::<usize>() {
            out.push(n);
            continue;
        }
        let want = part.strip_prefix("heads/").unwrap_or(part);
        let hit = worktree_on_branch(trees, want).ok_or_else(|| {
            format!("no worktree on branch '{want}' (see 'git-wt list')")
        })?;
        out.push(hit + 1);
    }
    Ok(out)
}

/// Pull a `-b`/`--branch`/`--branch=VALUE` flag out of an argument list,
/// wherever it sits, and return the args with it removed alongside its value.
///
/// Wherever it sits: `--branch` names extra targets to fold into whatever
/// command it rides along with, not a target of its own, so it has to come
/// out before the command's own parser sees an argument it doesn't know.
pub(crate) fn extract_branch_flag(
    args: &[String],
) -> Result<(Vec<String>, Option<String>), String> {
    let mut out = Vec::with_capacity(args.len());
    let mut val: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "-b" || a == "--branch" {
            if val.is_some() {
                return Err(format!("'{a}' given twice"));
            }
            let v = args
                .get(i + 1)
                .ok_or_else(|| format!("'{a}' needs a value, e.g. '{a} 1,2'"))?;
            val = Some(v.clone());
            i += 2;
            continue;
        }
        if let Some(v) = a.strip_prefix("--branch=") {
            if val.is_some() {
                return Err("'--branch' given twice".into());
            }
            val = Some(v.to_string());
            i += 1;
            continue;
        }
        out.push(a.clone());
        i += 1;
    }
    Ok((out, val))
}

/// A `--branch` value, resolved to 0-based worktree indexes the same way any
/// other comma list is: numbers or branch names, validated the same way.
pub(crate) fn branch_targets(trees: &[Worktree], val: &str) -> Result<Vec<usize>, String> {
    let parts: Vec<String> = val.split(',').map(String::from).collect();
    if parts.iter().any(|p| p.is_empty()) {
        return Err(format!(
            "bad worktree list '{val}'; want numbers or branches, e.g. '1,2' or 'main,2'"
        ));
    }
    let ns = resolve_target_list(trees, &parts)?;
    ns.into_iter().map(|n| check_index(n, trees.len())).collect()
}

/// The index of the worktree with `branch` checked out, if any.
pub(crate) fn worktree_on_branch(trees: &[Worktree], branch: &str) -> Option<usize> {
    trees.iter().position(|w| w.branch.as_deref() == Some(branch))
}

/// Warn on stderr when a bare one-letter alias is also the name of a
/// checked-out branch: the alias wins, same as every other verb wins over a
/// same-named branch, but a single letter is common enough as a real branch
/// name that a silent shadow is worth flagging.
pub(crate) fn warn_if_alias_shadows_branch(trees: &[Worktree], tok: &str, full_word: &str) {
    if worktree_on_branch(trees, tok).is_some() {
        eprintln!(
            "warning: branch '{tok}' is checked out here; '{tok}' is read as the '{full_word}' \
             alias, not the branch\nhint: 'heads/{tok}' reaches the branch's worktree"
        );
    }
}

/// The worktree number a lone leading token names, when it names one at all.
///
/// The single-target twin of `resolve_target_list`, and deliberately quieter:
/// a lone word reaches here only after every verb has failed to match, so a
/// miss is not "no such branch" but "not a target either" -- the caller still
/// owns the message, and `unknown_command_msg` keeps its `add <branch>` hint
/// for a word that merely looks branch-shaped.
pub(crate) fn resolve_target(trees: &[Worktree], tok: &str) -> Option<usize> {
    if tok.parse::<usize>().is_ok() {
        return None; // A number is the caller's own path, already handled.
    }
    let want = tok.strip_prefix("heads/").unwrap_or(tok);
    worktree_on_branch(trees, want).map(|i| i + 1)
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
        Some("commits") | Some("c") => cmd_commits(root, &trees, &idxs, &rest[1..]),
        Some("log") | Some("l") => cmd_log(root, &trees, &idxs, &rest[1..]),
        // `1,2 merge`: the list reads dest-first, so 2 merges into 1.
        Some("merge") => {
            // The list already names the source, so a resume word contradicts
            // it: there is nothing for `continue` to take a source from.
            // Check this before the count so an over-long list with `continue`
            // gets the more useful resume-word message.
            // Only what `merge` itself will parse: `--review` ends that, and
            // past it `-c` is `--commits`, not `continue`.
            let mut mine = rest[1..]
                .iter()
                .take_while(|a| *a != "review" && *a != "--review");
            if let Some(word) = mine.find_map(|a| resume_word(a)) {
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
        Some("merged") | Some("m") => {
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
        Some(w) if SyncOp::from_word(w).is_some() || w == "p" => {
            // "p" is the one-letter alias for `pull` specifically.
            let op = if w == "p" {
                SyncOp::Pull
            } else {
                SyncOp::from_word(w).expect("matched above")
            };
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
            "'{other}' takes a single worktree; only 'commits', 'log', 'diff', 'meld', 'merge', \
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

const SEARCH_MISSING: &str = "--search needs a term, e.g. '--search main'";

/// Parse `list` arguments (an optional SEARCH plus `--col`) then list. `SEARCH`
/// is a bare positional or `--search`/`--search=`, never both -- either one
/// only highlights matches now, it no longer drops the rows that miss. Shared
/// by `list`/`ls`, the no-args default, and a bare leading `--col`/`--files`.
pub(crate) fn list_from_args(root: &Path, args: &[String]) -> Result<(), String> {
    let mut search: Option<String> = None;
    let mut cols: Option<Vec<usize>> = None;
    let mut mode = ListMode::Normal;
    let mut show_path = false;
    let mut files = false;
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
            "--files" | "-f" => files = true,
            "--search" => {
                if search.is_some() {
                    return Err("too many arguments\nTry 'git-wt --help'".into());
                }
                let v = it.next().ok_or(SEARCH_MISSING)?;
                if v.trim().is_empty() {
                    return Err(SEARCH_MISSING.into());
                }
                search = Some(v.to_string());
            }
            s if s.starts_with("--search=") => {
                if search.is_some() {
                    return Err("too many arguments\nTry 'git-wt --help'".into());
                }
                let v = &s["--search=".len()..];
                if v.trim().is_empty() {
                    return Err(SEARCH_MISSING.into());
                }
                search = Some(v.to_string());
            }
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
    cmd_list(root, search.as_deref(), cols, mode, show_path, files, None)
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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


    /// Worktrees on the given branches, in order, for the resolver tests.
    fn trees_on(branches: &[&str]) -> Vec<Worktree> {
        branches
            .iter()
            .map(|b| Worktree {
                path: PathBuf::from(format!("/tmp/{b}")),
                branch: Some((*b).to_string()),
                detached: false,
                bare: false,
            })
            .collect()
    }

    #[test]
    fn target_list_tokenizes_without_judging_the_parts() {
        // A branch name is a legal part; only the caller knows if it resolves.
        assert_eq!(
            parse_target_list("1,main"),
            Ok(Some(vec!["1".into(), "main".into()]))
        );
        // No comma is not a list at all -- the caller keeps looking.
        assert_eq!(parse_target_list("main"), Ok(None));
    }

    #[test]
    fn an_empty_part_is_a_malformed_list() {
        let want = Err("bad worktree list '1,,2'; want numbers or branches, \
                        e.g. '1,2' or 'main,2'"
            .into());
        assert_eq!(parse_target_list("1,,2"), want);
        assert!(parse_target_list("1,").is_err());
        assert!(parse_target_list(",1").is_err());
    }

    #[test]
    fn a_branch_resolves_to_its_worktree_number() {
        let trees = trees_on(&["main", "feat/x", "feat/y"]);
        assert_eq!(
            resolve_target_list(&trees, &["main".into(), "feat/y".into()]),
            Ok(vec![1, 3])
        );
        // Numbers and branches mix freely, and a number passes through as-is.
        assert_eq!(
            resolve_target_list(&trees, &["2".into(), "main".into()]),
            Ok(vec![2, 1])
        );
    }

    #[test]
    fn a_bare_number_is_the_worktree_not_a_branch_of_that_name() {
        // Worktree #2 is `feat/x`, but a branch is also literally named `2`.
        let trees = trees_on(&["main", "feat/x", "2"]);
        // The number wins...
        assert_eq!(resolve_target_list(&trees, &["2".into()]), Ok(vec![2]));
        // ...and `heads/` is how you reach the branch instead.
        assert_eq!(resolve_target_list(&trees, &["heads/2".into()]), Ok(vec![3]));
    }

    #[test]
    fn an_out_of_range_number_stays_a_number() {
        // It must not fall through to a branch lookup: the useful complaint is
        // "no worktree #9", which check_index makes downstream, not "no worktree
        // on branch '9'". Same for a `+` that usize::from_str happens to accept.
        let trees = trees_on(&["main", "feat/x"]);
        assert_eq!(resolve_target_list(&trees, &["9".into()]), Ok(vec![9]));
        assert_eq!(resolve_target_list(&trees, &["+9".into()]), Ok(vec![9]));
        assert_eq!(
            check_index(9, trees.len()),
            Err("no worktree #9; there are 2 (see 'git-wt list')".into())
        );
    }

    #[test]
    fn a_branch_no_worktree_holds_is_rejected() {
        let trees = trees_on(&["main", "feat/x"]);
        assert_eq!(
            resolve_target_list(&trees, &["main".into(), "feat/gone".into()]),
            Err("no worktree on branch 'feat/gone' (see 'git-wt list')".into())
        );
    }

    #[test]
    fn a_lone_branch_names_its_worktree() {
        let trees = trees_on(&["main", "feat/x"]);
        assert_eq!(resolve_target(&trees, "feat/x"), Some(2));
        assert_eq!(resolve_target(&trees, "heads/main"), Some(1));
        // A miss is None, not an error: the caller still owns the message, so
        // `unknown_command_msg` keeps its 'did you mean add ...' hint.
        assert_eq!(resolve_target(&trees, "feat/gone"), None);
    }

    #[test]
    fn a_lone_number_is_left_to_the_caller() {
        // main's own `first.parse::<usize>()` arm already ran by then; resolving
        // it here too would give a second, divergent path to the same worktree.
        let trees = trees_on(&["main", "2"]);
        assert_eq!(resolve_target(&trees, "2"), None);
        assert_eq!(resolve_target(&trees, "heads/2"), Some(2));
    }

    #[test]
    fn a_detached_worktree_matches_no_branch_name() {
        let mut trees = trees_on(&["main", "feat/x"]);
        trees[1].branch = None;
        trees[1].detached = true;
        assert!(resolve_target_list(&trees, &["feat/x".into()]).is_err());
        // Its number still reaches it; whether it has a ref is the action's call.
        assert_eq!(resolve_target_list(&trees, &["2".into()]), Ok(vec![2]));
    }
}
