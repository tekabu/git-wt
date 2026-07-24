use clap::{ArgAction, Parser, Subcommand};

use crate::cmd::add::args::AddArgs;
use crate::cmd::diff::args::DiffArgs;
use crate::cmd::doctor::args::DoctorArgs;
use crate::cmd::list::args::ListArgs;
use crate::cmd::meld::args::MeldArgs;
use crate::cmd::merge::args::MergeArgs;
use crate::cmd::merged::args::MergedArgs;
use crate::cmd::remove::args::RemoveArgs;
use crate::cmd::switch::args::{PathArgs, SwitchArgs};
use crate::cmd::sync::args::SyncArgs;
use crate::worktree::{Worktree};

/// git-wt — create and manage git worktrees in sibling directories.
#[derive(Parser, Debug)]
#[command(
    name = "git-wt",
    version,
    about = "Worktrees in sibling directories named <repo>-<branch>",
    disable_help_flag = true
)]
pub(crate) struct Cli {
    /// Print help. Combine with -f/--full for the full manual (-hf).
    #[arg(short = 'h', long = "help", action = ArgAction::SetTrue)]
    pub(crate) help: bool,

    /// Print the full manual instead of the flag summary; alone or with -h.
    #[arg(short = 'f', long = "full", action = ArgAction::SetTrue)]
    pub(crate) full: bool,

    /// Add a comma-separated worktree list of *other* branches to bring into
    /// the command, alongside the target. Can be given multiple times.
    ///
    /// Not `-t/--target`: that flag exists per-command instead (see each
    /// command's own `*Args`), because `merge` already claims `-t` for
    /// `theirs` and a global flag can't be un-global for one subcommand.
    #[arg(short, long, action = ArgAction::Append, global = true, value_name = "TARGET_LIST")]
    pub(crate) branch: Vec<String>,

    /// When no verb is given, the target list selects a worktree to switch to
    /// (shows the worktree list when omitted).
    #[arg(value_name = "TARGET_LIST")]
    pub(crate) targets: Option<String>,

    #[command(subcommand)]
    pub(crate) command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    /// Create a new worktree.
    #[command(alias = "a")]
    Add(AddArgs),

    /// List worktrees.
    #[command(alias = "ls")]
    List(ListArgs),

    /// Switch to a worktree.
    #[command(aliases = ["cd", "s"])]
    Switch(SwitchArgs),

    /// Print a worktree's path.
    Path(PathArgs),

    /// Remove a worktree.
    #[command(aliases = ["rm"])]
    Remove(RemoveArgs),

    /// Fetch across worktrees.
    Fetch(SyncArgs),

    /// Pull across worktrees.
    #[command(alias = "p")]
    Pull(SyncArgs),

    /// Push across worktrees.
    Push(SyncArgs),

    /// Diff two worktrees.
    Diff(DiffArgs),

    /// Open meld on 2-3 worktrees.
    Meld(MeldArgs),

    /// Commit table across worktrees.
    #[command(alias = "c")]
    Commits {
        /// Target list followed by git log options and filters. When the first
        /// token resolves as a worktree list it is consumed as the target;
        /// otherwise the current worktree is used and the whole tail is passed
        /// through.
        #[arg(allow_hyphen_values = true, num_args = 0.., value_name = "TARGET/OPTIONS")]
        rest: Vec<String>,
    },

    /// File history table across worktrees.
    #[command(alias = "l")]
    Log {
        /// Optional target, path, and git log options. When the first token
        /// resolves as a worktree list it is consumed as the target; otherwise
        /// it is kept as part of the path/options passed through to git log.
        #[arg(allow_hyphen_values = true, num_args = 0.., value_name = "TARGET/PATH/OPTIONS")]
        rest: Vec<String>,
    },

    /// Merge a source into a worktree.
    Merge(MergeArgs),

    /// Check merge status.
    #[command(alias = "m")]
    Merged(MergedArgs),

    /// Report worktree issues.
    Doctor(DoctorArgs),

    /// Print version.
    Version,
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
/// `heads/2` is still the way to mean the branch. Anything else is matched
/// against the checked-out branches.
pub(crate) fn resolve_target_list(trees: &[Worktree], parts: &[String]) -> Result<Vec<usize>, String> {
    let mut out = Vec::new();
    for part in parts {
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

/// Fold a command's own positional target into the global `-t/--target` flag:
/// they name the same thing, so only one may be given.
pub(crate) fn effective_target(
    positional: Option<String>,
    flag: Option<&String>,
) -> Result<Option<String>, String> {
    match (positional, flag) {
        (Some(p), Some(f)) => Err(format!(
            "target given twice: '{p}' and '-t/--target {f}'; use one or the other"
        )),
        (Some(p), None) => Ok(Some(p)),
        (None, Some(f)) => Ok(Some(f.clone())),
        (None, None) => Ok(None),
    }
}

/// Combine a positional target list and any `-b/--branch` values into a single
/// flat list of raw target parts.
pub(crate) fn gather_targets(
    targets: Option<&String>,
    branch: &[String],
) -> Result<Vec<String>, String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(t) = targets {
        parts.extend(t.split(',').map(String::from));
    }
    for b in branch {
        parts.extend(b.split(',').map(String::from));
    }
    if parts.iter().any(|p| p.is_empty()) {
        return Err("bad worktree list; want numbers or branches, e.g. '1,2' or 'main,2'".into());
    }
    Ok(parts)
}

/// Pull a `-b`/`--branch`/`--branch=VALUE` flag out of an argument list,
/// wherever it sits, and return the args with it removed alongside its value.
///
/// `commits`/`log`/`merge` capture their tail as a raw, `allow_hyphen_values`
/// catch-all positional (`rest: Vec<String>`); once that positional starts
/// consuming (i.e. a target token precedes the flag), clap's greedy matching
/// swallows even global options like `-b`/`-t` into it instead of routing
/// them to the global field. This (and `extract_target_flag`) is the fallback
/// that recovers them from `rest` in that case; when `-b`/`-t` precede every
/// positional token, clap's global handling already caught them and this
/// simply finds nothing.
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

/// The `-t`/`--target` twin of `extract_branch_flag`: pulls `-t`/`--target`/
/// `--target=VALUE` out of a raw catch-all `rest`, wherever it sits.
pub(crate) fn extract_target_flag(
    args: &[String],
) -> Result<(Vec<String>, Option<String>), String> {
    let mut out = Vec::with_capacity(args.len());
    let mut val: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "-t" || a == "--target" {
            if val.is_some() {
                return Err(format!("'{a}' given twice"));
            }
            let v = args
                .get(i + 1)
                .ok_or_else(|| format!("'{a}' needs a value, e.g. '{a} 1'"))?;
            val = Some(v.clone());
            i += 2;
            continue;
        }
        if let Some(v) = a.strip_prefix("--target=") {
            if val.is_some() {
                return Err("'--target' given twice".into());
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

/// The worktree number a lone leading token names, when it names one at all.
///
/// The single-target twin of `resolve_target_list`, and deliberately quieter:
/// a lone word reaches here only after every verb has failed to match, so a
/// miss is not "no such branch" but "not a target either".
pub(crate) fn resolve_target(trees: &[Worktree], tok: &str) -> Option<usize> {
    if tok.parse::<usize>().is_ok() {
        return None; // A number is the caller's own path, already handled.
    }
    let want = tok.strip_prefix("heads/").unwrap_or(tok);
    worktree_on_branch(trees, want).map(|i| i + 1)
}

/// Warn on stderr when a bare one-letter alias is also the name of a
/// checked-out branch.
pub(crate) fn warn_if_alias_shadows_branch(trees: &[Worktree], tok: &str, full_word: &str) {
    if worktree_on_branch(trees, tok).is_some() {
        eprintln!(
            "warning: branch '{tok}' is checked out here; '{tok}' is read as the '{full_word}' \
             alias, not the branch\nhint: 'heads/{tok}' reaches the branch's worktree"
        );
    }
}

/// The verb token the user actually typed (`git-wt <VERB> ...`), read from
/// raw argv rather than the parsed `Commands` variant -- clap normalizes an
/// alias like `s` to the canonical `Switch` before it ever reaches us, so by
/// the time a match arm runs there is no way to tell "typed `switch`" apart
/// from "typed `s`". `warn_if_alias_shadows_branch` needs exactly that
/// distinction: the warning only makes sense when the alias itself was typed.
///
/// Skips the two global flags that can precede the verb (`-h`/`-f`, no
/// value; `-b`/`--branch`, one value) so `git-wt -b 2 pull` still finds
/// `pull`. Anything else unrecognized before the verb is skipped rather than
/// mistaken for it, since it is the parser's job (already run) to reject it.
pub(crate) fn typed_verb() -> Option<String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-h" | "--help" | "-f" | "--full" => i += 1,
            "-b" | "--branch" => i += 2,
            s if s.starts_with("--branch=") => i += 1,
            s if !s.starts_with('-') => return Some(s.to_string()),
            _ => i += 1,
        }
    }
    None
}

/// Map a 1-based index to a 0-based one, or an error.
pub(crate) fn check_index(n: usize, len: usize) -> Result<usize, String> {
    if n == 0 {
        return Err("no worktree #0".into());
    }
    if n > len {
        return Err(format!("no worktree #{n}; there are {len} (see 'git-wt list')"));
    }
    Ok(n - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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

    fn trees_on(branches: &[&str]) -> Vec<Worktree> {
        branches
            .iter()
            .map(|b| Worktree {
                path: PathBuf::from(format!("/tmp/{b}")),
                branch: Some((*b).to_string()),
                detached: false,
                bare: false,
                locked: None,
                prunable: None,
            })
            .collect()
    }

    #[test]
    fn target_list_tokenizes_without_judging_the_parts() {
        assert_eq!(
            parse_target_list("1,main"),
            Ok(Some(vec!["1".into(), "main".into()]))
        );
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
        assert_eq!(
            resolve_target_list(&trees, &["2".into(), "main".into()]),
            Ok(vec![2, 1])
        );
    }

    #[test]
    fn a_bare_number_is_the_worktree_not_a_branch_of_that_name() {
        let trees = trees_on(&["main", "feat/x", "2"]);
        assert_eq!(resolve_target_list(&trees, &["2".into()]), Ok(vec![2]));
        assert_eq!(resolve_target_list(&trees, &["heads/2".into()]), Ok(vec![3]));
    }

    #[test]
    fn an_out_of_range_number_stays_a_number() {
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
    fn gather_targets_concatenates_positional_and_branch_flags() {
        assert_eq!(
            gather_targets(Some(&"1,2".into()), &["main".into()]),
            Ok(vec!["1".into(), "2".into(), "main".into()])
        );
        assert_eq!(
            gather_targets(None, &["3".into(), "feat/x".into()]),
            Ok(vec!["3".into(), "feat/x".into()])
        );
        assert!(gather_targets(Some(&"1,".into()), &[]).is_err());
    }

    #[test]
    fn a_lone_branch_names_its_worktree() {
        let trees = trees_on(&["main", "feat/x"]);
        assert_eq!(resolve_target(&trees, "feat/x"), Some(2));
        assert_eq!(resolve_target(&trees, "heads/main"), Some(1));
        assert_eq!(resolve_target(&trees, "feat/gone"), None);
    }

    #[test]
    fn a_lone_number_is_left_to_the_caller() {
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
        assert_eq!(resolve_target_list(&trees, &["2".into()]), Ok(vec![2]));
    }
}
