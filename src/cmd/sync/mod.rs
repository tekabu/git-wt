pub(crate) mod args;

use std::io::IsTerminal;
use std::path::Path;

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

    #[allow(dead_code)]
    fn from_word(tok: &str) -> Option<SyncOp> {
        match tok {
            "fetch" => Some(SyncOp::Fetch),
            "pull" => Some(SyncOp::Pull),
            "push" => Some(SyncOp::Push),
            _ => None,
        }
    }

    pub(crate) fn flags(self) -> &'static [&'static str] {
        match self {
            SyncOp::Fetch => &["--prune", "--tags", "--no-tags", "--force"],
            SyncOp::Pull => &["--rebase", "--no-rebase", "--ff-only", "--prune", "--autostash"],
            SyncOp::Push => &["--set-upstream", "--force-with-lease", "--tags", "--dry-run"],
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SyncParsedArgs {
    pub(crate) op: SyncOp,
    pub(crate) all: bool,
    pub(crate) flags: Vec<String>,
}

pub(crate) const ALL_HINT: &str = "hint: 'git-wt <N> fetch' for one worktree, 'git-wt fetch --all' for every one";

pub(crate) fn parse_sync_args(op: SyncOp, args: &[String]) -> Result<SyncParsedArgs, String> {
    let mut all = false;
    let mut flags: Vec<String> = Vec::new();
    let word = op.word();

    for a in args {
        let canon = match a.as_str() {
            "--all" | "-a" => {
                all = true;
                continue;
            }
            "-u" if op == SyncOp::Push => "--set-upstream",
            "-p" if op != SyncOp::Push => "--prune",
            "-n" if op == SyncOp::Push => "--dry-run",
            "--rb" if op == SyncOp::Pull => "--rebase",
            "--nr" if op == SyncOp::Pull => "--no-rebase",
            "--as" if op == SyncOp::Pull => "--autostash",
            "--nt" if op == SyncOp::Fetch => "--no-tags",
            "--fl" if op == SyncOp::Push => "--force-with-lease",
            "-f" | "--force" if op == SyncOp::Push => {
                return Err("no '--force' for push: it overwrites a remote branch without \
                     checking what is on it\nhint: '--force-with-lease' refuses when the remote \
                     moved since you last saw it"
                    .into());
            }
            s => s,
        };
        if !op.flags().contains(&canon) {
            return Err(format!(
                "unknown option '{a}' for {word}\n\
                 hint: {word} takes {}\n\
                 any other git flag is yours to run: 'git -C <dir> {word} {a}'",
                op.flags().join(", ")
            ));
        }
        let canon = canon.to_string();
        if !flags.contains(&canon) {
            flags.push(canon);
        }
    }

    for (a, b) in [("--rebase", "--no-rebase"), ("--tags", "--no-tags")] {
        if flags.iter().any(|f| f == a) && flags.iter().any(|f| f == b) {
            return Err(format!("'{a}' and '{b}' contradict each other"));
        }
    }
    if op == SyncOp::Pull && flags.iter().any(|f| f == "--rebase") && flags.iter().any(|f| f == "--ff-only") {
        return Err("'--rebase' and '--ff-only' contradict each other".into());
    }

    Ok(SyncParsedArgs { op, all, flags })
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
            "which remote? this repo has {}, and none is called 'origin'\n\
             hint: 'git -C <dir> push -u <remote> <branch>' names it",
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

fn no_upstream_hint(op: SyncOp, e: &str, idx: usize) -> String {
    let is_no_upstream = match op {
        SyncOp::Push => e.contains("has no upstream branch"),
        SyncOp::Pull => e.contains("no tracking information"),
        SyncOp::Fetch => false,
    };
    if !is_no_upstream {
        return e.to_string();
    }
    format!("{e}\nhint: 'git-wt {} push -u' sets the upstream from here", idx + 1)
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
                let e = no_upstream_hint(args.op, &e, i);
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

    fn sync_args(op: SyncOp, args: &[&str]) -> Result<SyncParsedArgs, String> {
        let v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse_sync_args(op, &v)
    }

    #[test]
    fn sync_words_are_exact() {
        assert_eq!(SyncOp::from_word("fetch"), Some(SyncOp::Fetch));
        assert_eq!(SyncOp::from_word("pull"), Some(SyncOp::Pull));
        assert_eq!(SyncOp::from_word("push"), Some(SyncOp::Push));
        assert_eq!(SyncOp::from_word("pu"), None);
        assert_eq!(SyncOp::from_word("--pull"), None);
    }

    #[test]
    fn sync_bare_verb_takes_no_flags() {
        let a = sync_args(SyncOp::Pull, &[]).unwrap();
        assert!(!a.all);
        assert!(a.flags.is_empty());
    }

    #[test]
    fn sync_all_is_worktrees_not_remotes() {
        assert!(sync_args(SyncOp::Fetch, &["--all"]).unwrap().all);
        assert!(sync_args(SyncOp::Push, &["-a"]).unwrap().all);
        assert!(sync_args(SyncOp::Fetch, &["--all"]).unwrap().flags.is_empty());
    }

    #[test]
    fn sync_shorts_canonicalize() {
        assert_eq!(sync_args(SyncOp::Push, &["-u"]).unwrap().flags, ["--set-upstream"]);
        assert_eq!(sync_args(SyncOp::Push, &["-n"]).unwrap().flags, ["--dry-run"]);
        assert_eq!(sync_args(SyncOp::Fetch, &["-p"]).unwrap().flags, ["--prune"]);
        assert_eq!(sync_args(SyncOp::Pull, &["-p"]).unwrap().flags, ["--prune"]);
    }

    #[test]
    fn sync_short_aliases_canonicalize_the_same_as_long_form() {
        assert_eq!(
            sync_args(SyncOp::Pull, &["--rb"]).unwrap().flags,
            sync_args(SyncOp::Pull, &["--rebase"]).unwrap().flags
        );
        assert_eq!(
            sync_args(SyncOp::Pull, &["--nr"]).unwrap().flags,
            sync_args(SyncOp::Pull, &["--no-rebase"]).unwrap().flags
        );
        assert_eq!(
            sync_args(SyncOp::Pull, &["--as"]).unwrap().flags,
            sync_args(SyncOp::Pull, &["--autostash"]).unwrap().flags
        );
        assert_eq!(
            sync_args(SyncOp::Fetch, &["--nt"]).unwrap().flags,
            sync_args(SyncOp::Fetch, &["--no-tags"]).unwrap().flags
        );
        assert_eq!(
            sync_args(SyncOp::Push, &["--fl"]).unwrap().flags,
            sync_args(SyncOp::Push, &["--force-with-lease"]).unwrap().flags
        );
    }

    #[test]
    fn sync_flags_are_per_verb() {
        assert!(sync_args(SyncOp::Pull, &["--rebase"]).is_ok());
        assert!(sync_args(SyncOp::Push, &["--rebase"]).is_err());
        assert!(sync_args(SyncOp::Fetch, &["--rebase"]).is_err());
        assert!(sync_args(SyncOp::Push, &["--set-upstream"]).is_ok());
        assert!(sync_args(SyncOp::Pull, &["--set-upstream"]).is_err());
        assert!(sync_args(SyncOp::Push, &["-p"]).is_err());
    }

    #[test]
    fn sync_unknown_flag_is_not_a_passthrough() {
        let e = sync_args(SyncOp::Pull, &["--depth=1"]).unwrap_err();
        assert!(e.contains("unknown option '--depth=1' for pull"));
        assert!(e.contains("git -C <dir> pull --depth=1"));
    }

    #[test]
    fn sync_push_force_is_refused() {
        for f in ["--force", "-f"] {
            let e = sync_args(SyncOp::Push, &[f]).unwrap_err();
            assert!(e.contains("no '--force' for push"));
            assert!(e.contains("--force-with-lease"));
        }
        assert!(sync_args(SyncOp::Push, &["--force-with-lease"]).is_ok());
        assert!(sync_args(SyncOp::Fetch, &["--force"]).is_ok());
    }

    #[test]
    fn sync_contradictions_are_typos() {
        assert!(sync_args(SyncOp::Pull, &["--rebase", "--no-rebase"]).is_err());
        assert!(sync_args(SyncOp::Pull, &["--rebase", "--ff-only"]).is_err());
        assert!(sync_args(SyncOp::Fetch, &["--tags", "--no-tags"]).is_err());
        assert!(sync_args(SyncOp::Pull, &["--rebase", "--autostash"]).is_ok());
    }

    #[test]
    fn sync_repeated_flag_is_passed_once() {
        let a = sync_args(SyncOp::Fetch, &["--prune", "-p", "--prune"]).unwrap();
        assert_eq!(a.flags, ["--prune"]);
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
