pub(crate) mod args;

use std::io::IsTerminal;
use std::path::Path;

use crate::cmd::sync::args::{FetchArgs, PullArgs, PushArgs};
use crate::git::{git_quiet, git_run, git_stdout};
use crate::ui::{color_enabled, paint, DIM, GREEN, RED};
use crate::worktree::{label, Worktree};

/// The three remote verbs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyncOp {
    Fetch,
    Pull,
    Push,
}

impl SyncOp {
    pub(crate) fn word(self) -> &'static str {
        match self {
            SyncOp::Fetch => "fetch",
            SyncOp::Pull => "pull",
            SyncOp::Push => "push",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SyncParsedArgs {
    pub(crate) op: SyncOp,
    pub(crate) all: bool,
    pub(crate) flags: Vec<String>,
}

/// Each verb's own struct is already clap-validated (unknown flags, `-a`
/// position, and every hand-rolled contradiction check below are gone: clap's
/// `conflicts_with`/`conflicts_with_all` on the struct fields refuse them at
/// parse time). What's left here is just reading the typed bools into the
/// canonical flag strings `sync_argv`/`cmd_sync` already work with.
pub(crate) fn fetch_parsed(a: &FetchArgs) -> SyncParsedArgs {
    let mut flags = Vec::new();
    if a.prune.prune {
        flags.push("--prune".to_string());
    }
    if a.tags.tags {
        flags.push("--tags".to_string());
    }
    if a.tags.no_tags {
        flags.push("--no-tags".to_string());
    }
    if a.force.force {
        flags.push("--force".to_string());
    }
    SyncParsedArgs { op: SyncOp::Fetch, all: a.common.all.all, flags }
}

pub(crate) fn pull_parsed(a: &PullArgs) -> SyncParsedArgs {
    let mut flags = Vec::new();
    if a.rebase.rebase {
        flags.push("--rebase".to_string());
    }
    if a.rebase.no_rebase {
        flags.push("--no-rebase".to_string());
    }
    if a.ff_only.ff_only {
        flags.push("--ff-only".to_string());
    }
    if a.prune.prune {
        flags.push("--prune".to_string());
    }
    if a.autostash.autostash {
        flags.push("--autostash".to_string());
    }
    SyncParsedArgs { op: SyncOp::Pull, all: a.common.all.all, flags }
}

/// `-F/--force` is declared on `PushArgs` (see its doc comment) purely so this
/// can name the danger instead of clap's generic "unexpected argument": it
/// overwrites a remote branch without checking what is on it, and force is a
/// real word on the other two verbs, so a typo here is a plausible mistake.
pub(crate) fn push_parsed(a: &PushArgs) -> Result<SyncParsedArgs, String> {
    if a.force {
        return Err("no '--force' for push: it overwrites a remote branch without \
             checking what is on it"
            .into());
    }
    let mut flags = Vec::new();
    if a.set_upstream.set_upstream {
        flags.push("--set-upstream".to_string());
    }
    if a.force_with_lease.force_with_lease {
        flags.push("--force-with-lease".to_string());
    }
    if a.tags.tags {
        flags.push("--tags".to_string());
    }
    if a.dry_run.dry_run {
        flags.push("--dry-run".to_string());
    }
    Ok(SyncParsedArgs { op: SyncOp::Push, all: a.common.all.all, flags })
}

pub(crate) fn sync_skip(w: &Worktree, op: SyncOp) -> Option<&'static str> {
    if w.bare {
        return Some("bare");
    }
    if op != SyncOp::Fetch && w.detached {
        return Some("detached HEAD, no branch to sync");
    }
    None
}

pub(crate) fn default_remote(dir: &Path) -> Result<String, String> {
    let remotes: Vec<String> = git_stdout(dir, &["remote"])?
        .lines()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(String::from)
        .collect();
    match remotes.len() {
        0 => Err("no remote to push to; add one with 'git remote add origin <url>'".into()),
        1 => Ok(remotes.into_iter().next().expect("len 1")),
        _ if remotes.iter().any(|r| r == "origin") => Ok("origin".into()),
        _ => Err(format!(
            "which remote? this repo has {}, and none is called 'origin'",
            remotes.join(", ")
        )),
    }
}

pub(crate) fn sync_argv(w: &Worktree, args: &SyncParsedArgs) -> Result<Vec<String>, String> {
    let mut argv: Vec<String> = vec![args.op.word().to_string()];
    argv.extend(args.flags.iter().cloned());

    if args.op == SyncOp::Push && args.flags.iter().any(|f| f == "--set-upstream") {
        let branch = w
            .branch
            .as_deref()
            .ok_or("no branch to set an upstream for")?;
        if !git_quiet(
            &w.path,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"],
        ) {
            argv.push(default_remote(&w.path)?);
            argv.push(branch.to_string());
        }
    }
    Ok(argv)
}



pub(crate) fn cmd_sync(
    trees: &[Worktree],
    idxs: &[usize],
    args: &SyncParsedArgs,
) -> Result<(), String> {
    for (i, a) in idxs.iter().enumerate() {
        if idxs[i + 1..].contains(a) {
            return Err(format!("worktree #{} listed twice", a + 1));
        }
    }

    let word = args.op.word();
    let on = color_enabled(std::io::stderr().is_terminal());
    let sweep = idxs.len() > 1;

    let mut failed: Vec<(String, String)> = Vec::new();
    let mut skipped: Vec<(String, &'static str)> = Vec::new();
    let mut ok = 0usize;

    for &i in idxs {
        let w = &trees[i];
        let name = label(w);
        if let Some(why) = sync_skip(w, args.op) {
            eprintln!("{} {name} ({why})", paint("skip", DIM, on));
            skipped.push((name, why));
            continue;
        }
        eprintln!("{} {name}", paint(word, GREEN, on));
        let res = sync_argv(w, args).and_then(|argv| {
            let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
            git_run(&w.path, &argv)
        });
        match res {
            Ok(()) => ok += 1,
            Err(e) => {
                if sweep {
                    eprintln!("{} {e}", paint("error:", RED, on));
                }
                failed.push((name, e));
            }
        }
    }

    if !sweep {
        return match failed.pop() {
            Some((_, e)) => Err(e),
            None => Ok(()),
        };
    }

    eprintln!(
        "\n{word}: {ok} ok, {} failed, {} skipped",
        failed.len(),
        skipped.len()
    );
    if failed.is_empty() {
        return Ok(());
    }
    let names: Vec<&str> = failed.iter().map(|(n, _)| n.as_str()).collect();
    Err(format!("{word} failed in {}: {}", failed.len(), names.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestFetch {
        #[command(flatten)]
        args: FetchArgs,
    }
    #[derive(Parser, Debug)]
    struct TestPull {
        #[command(flatten)]
        args: PullArgs,
    }
    #[derive(Parser, Debug)]
    struct TestPush {
        #[command(flatten)]
        args: PushArgs,
    }

    fn fetch(args: &[&str]) -> Result<FetchArgs, String> {
        TestFetch::try_parse_from(std::iter::once("git-wt").chain(args.iter().copied()))
            .map(|c| c.args)
            .map_err(|e| e.to_string())
    }
    fn pull(args: &[&str]) -> Result<PullArgs, String> {
        TestPull::try_parse_from(std::iter::once("git-wt").chain(args.iter().copied()))
            .map(|c| c.args)
            .map_err(|e| e.to_string())
    }
    fn push(args: &[&str]) -> Result<PushArgs, String> {
        TestPush::try_parse_from(std::iter::once("git-wt").chain(args.iter().copied()))
            .map(|c| c.args)
            .map_err(|e| e.to_string())
    }

    #[test]
    fn sync_bare_verb_takes_no_flags() {
        let a = pull(&[]).unwrap();
        assert!(!a.common.all.all);
        assert!(pull_parsed(&a).flags.is_empty());
    }

    #[test]
    fn sync_all_is_worktrees_not_remotes() {
        assert!(fetch(&["--all"]).unwrap().common.all.all);
        assert!(push(&["-a"]).unwrap().common.all.all);
        assert!(fetch_parsed(&fetch(&["--all"]).unwrap()).flags.is_empty());
    }

    #[test]
    fn all_is_found_wherever_it_sits() {
        // Unlike the old catch-all tail, `--all` is a plain declared bool now:
        // clap catches it regardless of what else came first.
        let a = fetch(&["--prune", "--all"]).unwrap();
        assert!(a.common.all.all);
        assert_eq!(fetch_parsed(&a).flags, ["--prune"]);
        let a = fetch(&["--all", "--prune"]).unwrap();
        assert!(a.common.all.all);
        assert_eq!(fetch_parsed(&a).flags, ["--prune"]);
    }

    #[test]
    fn sync_shorts_canonicalize() {
        assert_eq!(push_parsed(&push(&["-u"]).unwrap()).unwrap().flags, ["--set-upstream"]);
        assert_eq!(fetch_parsed(&fetch(&["--prune"]).unwrap()).flags, ["--prune"]);
        assert_eq!(pull_parsed(&pull(&["--prune"]).unwrap()).flags, ["--prune"]);
    }

    #[test]
    fn sync_short_aliases_canonicalize_the_same_as_long_form() {
        assert_eq!(
            pull_parsed(&pull(&["--rb"]).unwrap()).flags,
            pull_parsed(&pull(&["--rebase"]).unwrap()).flags
        );
        assert_eq!(
            pull_parsed(&pull(&["--nr"]).unwrap()).flags,
            pull_parsed(&pull(&["--no-rebase"]).unwrap()).flags
        );
        assert_eq!(
            pull_parsed(&pull(&["--as"]).unwrap()).flags,
            pull_parsed(&pull(&["--autostash"]).unwrap()).flags
        );
        assert_eq!(
            fetch_parsed(&fetch(&["--nt"]).unwrap()).flags,
            fetch_parsed(&fetch(&["--no-tags"]).unwrap()).flags
        );
        assert_eq!(
            push_parsed(&push(&["--fl"]).unwrap()).unwrap().flags,
            push_parsed(&push(&["--force-with-lease"]).unwrap()).unwrap().flags
        );
    }

    #[test]
    fn sync_flags_are_per_verb() {
        // Each verb only declares its own flags, so clap itself refuses the
        // others -- there is no shared vocabulary left to check by hand.
        assert!(pull(&["--rebase"]).is_ok());
        assert!(push(&["--rebase"]).is_err());
        assert!(fetch(&["--rebase"]).is_err());
        assert!(push(&["--set-upstream"]).is_ok());
        assert!(pull(&["--set-upstream"]).is_err());
        assert!(push(&["-p"]).is_err());
    }

    #[test]
    fn sync_unknown_flag_is_not_a_passthrough() {
        assert!(pull(&["--depth=1"]).is_err());
    }

    #[test]
    fn sync_push_force_is_refused() {
        for f in ["--force", "-F"] {
            let a = push(&[f]).unwrap();
            let e = push_parsed(&a).unwrap_err();
            assert!(e.contains("no '--force' for push"));
        }
        assert!(push_parsed(&push(&["--force-with-lease"]).unwrap()).is_ok());
        // fetch's own '--force' means something else entirely and is fine.
        assert!(fetch(&["--force"]).unwrap().force.force);
    }

    #[test]
    fn sync_contradictions_are_typos() {
        // clap's own `conflicts_with`/`conflicts_with_all` refuse these at
        // parse time now; there is nothing left for `push`/`fetch_parsed` etc.
        // to check by hand.
        assert!(pull(&["--rebase", "--no-rebase"]).is_err());
        assert!(pull(&["--rebase", "--ff-only"]).is_err());
        assert!(fetch(&["--tags", "--no-tags"]).is_err());
        assert!(pull(&["--rebase", "--autostash"]).is_ok());
    }

    #[test]
    fn sync_skips_what_the_verb_cannot_mean() {
        let bare = Worktree {
            path: PathBuf::from("/code/myapp.git"),
            branch: None,
            detached: false,
            bare: true,
            locked: None,
            prunable: None,
        };
        let detached = Worktree {
            path: PathBuf::from("/code/myapp-x"),
            branch: None,
            detached: true,
            bare: false,
            locked: None,
            prunable: None,
        };
        let normal = Worktree {
            path: PathBuf::from("/code/myapp"),
            branch: Some("main".into()),
            detached: false,
            bare: false,
            locked: None,
            prunable: None,
        };
        assert_eq!(sync_skip(&bare, SyncOp::Fetch), Some("bare"));
        assert_eq!(sync_skip(&bare, SyncOp::Push), Some("bare"));
        assert_eq!(sync_skip(&detached, SyncOp::Fetch), None);
        assert!(sync_skip(&detached, SyncOp::Pull).is_some());
        assert!(sync_skip(&detached, SyncOp::Push).is_some());
        for op in [SyncOp::Fetch, SyncOp::Pull, SyncOp::Push] {
            assert_eq!(sync_skip(&normal, op), None);
        }
    }
}
