use std::io::IsTerminal;
use std::path::Path;

use crate::git::{git_quiet, git_run, git_stdout};
use crate::ui::{color_enabled, paint, DIM, GREEN, RED};
use crate::worktree::{label, Worktree};

// ---------------------------------------------------------------------------
// Sync: git-wt <N> fetch|pull|push, git-wt fetch|pull|push --all
// ---------------------------------------------------------------------------

/// The three remote verbs. They share a shape — run git in one worktree's
/// directory, over and over — and differ only in the word and the flags.
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

    /// The verb a token spells, or None. Only the exact word: these are actions,
    /// and an abbreviation would collide with a branch name.
    pub(crate) fn from_word(tok: &str) -> Option<SyncOp> {
        match tok {
            "fetch" => Some(SyncOp::Fetch),
            "pull" => Some(SyncOp::Pull),
            "push" => Some(SyncOp::Push),
            _ => None,
        }
    }

    /// The flags this verb accepts, in help order. Anything else is an error
    /// rather than a passthrough, the same rule `diff` follows.
    pub(crate) fn flags(self) -> &'static [&'static str] {
        match self {
            SyncOp::Fetch => &["--prune", "--tags", "--no-tags", "--force"],
            SyncOp::Pull => &["--rebase", "--no-rebase", "--ff-only", "--prune", "--autostash"],
            SyncOp::Push => &["--set-upstream", "--force-with-lease", "--tags", "--dry-run"],
        }
    }
}

/// What a `fetch`/`pull`/`push` invocation asked for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SyncArgs {
    pub(crate) op: SyncOp,
    /// Every worktree, rather than the ones a target list named.
    pub(crate) all: bool,
    /// Curated git flags, already canonical (`-u` has become `--set-upstream`).
    pub(crate) flags: Vec<String>,
}

pub(crate) const ALL_HINT: &str = "hint: 'git-wt <N> fetch' for one worktree, 'git-wt fetch --all' for every one";

pub(crate) fn parse_sync_args(op: SyncOp, args: &[String]) -> Result<SyncArgs, String> {
    let mut all = false;
    let mut flags: Vec<String> = Vec::new();
    let word = op.word();

    for a in args {
        let canon = match a.as_str() {
            "--all" | "-a" => {
                all = true;
                continue;
            }
            // Short forms, only where git has one and it is unambiguous here.
            "-u" if op == SyncOp::Push => "--set-upstream",
            "-p" if op != SyncOp::Push => "--prune",
            "-n" if op == SyncOp::Push => "--dry-run",
            "--rb" if op == SyncOp::Pull => "--rebase",
            "--nr" if op == SyncOp::Pull => "--no-rebase",
            "--as" if op == SyncOp::Pull => "--autostash",
            "--nt" if op == SyncOp::Fetch => "--no-tags",
            "--fl" if op == SyncOp::Push => "--force-with-lease",
            // `git push --force` is the one flag we refuse outright: it is
            // `--force-with-lease` minus the check that makes it safe, and a
            // sweep would apply it to every branch at once.
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

    // git would take both and let the last one win, silently. Two spellings of
    // opposite intent in one command line is a typo, not a preference.
    for (a, b) in [("--rebase", "--no-rebase"), ("--tags", "--no-tags")] {
        if flags.iter().any(|f| f == a) && flags.iter().any(|f| f == b) {
            return Err(format!("'{a}' and '{b}' contradict each other"));
        }
    }
    if op == SyncOp::Pull && flags.iter().any(|f| f == "--rebase") && flags.iter().any(|f| f == "--ff-only") {
        return Err("'--rebase' and '--ff-only' contradict each other".into());
    }

    Ok(SyncArgs { op, all, flags })
}

/// Why a worktree cannot take the verb at all, or None when it can.
///
/// A bare worktree has no working tree to pull into and no branch to push; a
/// detached HEAD has a commit but no branch, so there is no upstream to name.
/// `fetch` only touches remote-tracking refs, so it works on both.
pub(crate) fn sync_skip(w: &Worktree, op: SyncOp) -> Option<&'static str> {
    if w.bare {
        return Some("bare");
    }
    if op != SyncOp::Fetch && w.detached {
        return Some("detached HEAD, no branch to sync");
    }
    None
}

/// The remote to push a branch that has no upstream to: `origin` when it
/// exists, else the only remote there is. More than one and no origin is a
/// choice we cannot make for the user.
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

/// The git command line for one worktree.
///
/// Only `push --set-upstream` needs the worktree to build it: a bare
/// `git push -u` has no upstream to read the remote and branch off of -- that
/// is the situation `-u` exists for -- so git rejects it. Naming them is what
/// the user would have typed, and the branch is per-worktree, which is why this
/// is built per-worktree rather than once.
pub(crate) fn sync_argv(w: &Worktree, args: &SyncArgs) -> Result<Vec<String>, String> {
    let mut argv: Vec<String> = vec![args.op.word().to_string()];
    argv.extend(args.flags.iter().cloned());

    if args.op == SyncOp::Push && args.flags.iter().any(|f| f == "--set-upstream") {
        let branch = w
            .branch
            .as_deref()
            .ok_or("no branch to set an upstream for")?;
        // An upstream already set is the one meant; -u then just re-points it
        // there, which is what a bare `git push -u` does.
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

/// Git's own "no upstream" errors point at a raw `git push -u origin <branch>`.
/// That works, but it bypasses this tool's own flag for the same thing, so
/// point back at that instead: `git-wt <n> push -u`.
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

/// Run one remote verb across the given worktrees.
///
/// Every worktree runs, whatever the ones before it did: a sweep that stops
/// halfway leaves half the worktrees synced and does not say which half. The
/// failures are collected and reported at the end, and the exit code is the
/// summary.
pub(crate) fn cmd_sync(trees: &[Worktree], idxs: &[usize], args: &SyncArgs) -> Result<(), String> {
    for (i, a) in idxs.iter().enumerate() {
        if idxs[i + 1..].contains(a) {
            return Err(format!("worktree #{} listed twice", a + 1));
        }
    }

    let word = args.op.word();
    let on = color_enabled(std::io::stderr().is_terminal());
    // A sweep is many commands, so it says where each failure happened as it
    // goes. A single target is one command: main prints its error, and printing
    // it here too would say it twice.
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
        // Building the command line can fail on its own (a worktree with no
        // remote to push to). That is this worktree's failure like any other:
        // a sweep owes the rest of the list their turn.
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

    // One worktree is not a sweep: git's own error is the whole story, and a
    // summary of a single line repeats it.
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

    fn sync_args(op: SyncOp, args: &[&str]) -> Result<SyncArgs, String> {
        let v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse_sync_args(op, &v)
    }


    #[test]
    fn sync_words_are_exact() {
        assert_eq!(SyncOp::from_word("fetch"), Some(SyncOp::Fetch));
        assert_eq!(SyncOp::from_word("pull"), Some(SyncOp::Pull));
        assert_eq!(SyncOp::from_word("push"), Some(SyncOp::Push));
        // An abbreviation would shadow a branch of the same name.
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
        // `--all` is ours, so it never reaches git as `fetch --all` (every remote).
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
        assert_eq!(sync_args(SyncOp::Pull, &["--rb"]).unwrap().flags, sync_args(SyncOp::Pull, &["--rebase"]).unwrap().flags);
        assert_eq!(sync_args(SyncOp::Pull, &["--nr"]).unwrap().flags, sync_args(SyncOp::Pull, &["--no-rebase"]).unwrap().flags);
        assert_eq!(sync_args(SyncOp::Pull, &["--as"]).unwrap().flags, sync_args(SyncOp::Pull, &["--autostash"]).unwrap().flags);
        assert_eq!(sync_args(SyncOp::Fetch, &["--nt"]).unwrap().flags, sync_args(SyncOp::Fetch, &["--no-tags"]).unwrap().flags);
        assert_eq!(sync_args(SyncOp::Push, &["--fl"]).unwrap().flags, sync_args(SyncOp::Push, &["--force-with-lease"]).unwrap().flags);
    }


    #[test]
    fn sync_flags_are_per_verb() {
        assert!(sync_args(SyncOp::Pull, &["--rebase"]).is_ok());
        assert!(sync_args(SyncOp::Push, &["--rebase"]).is_err());
        assert!(sync_args(SyncOp::Fetch, &["--rebase"]).is_err());
        assert!(sync_args(SyncOp::Push, &["--set-upstream"]).is_ok());
        assert!(sync_args(SyncOp::Pull, &["--set-upstream"]).is_err());
        // -p is prune for fetch/pull; push has no -p at all.
        assert!(sync_args(SyncOp::Push, &["-p"]).is_err());
    }


    #[test]
    fn sync_unknown_flag_is_not_a_passthrough() {
        let e = sync_args(SyncOp::Pull, &["--depth=1"]).unwrap_err();
        assert!(e.contains("unknown option '--depth=1' for pull"));
        // The error hands back the command that would work.
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
        // fetch --force only refreshes a ref that moved; it overwrites no remote.
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
        };
        let detached = Worktree {
            path: PathBuf::from("/code/myapp-x"),
            branch: None,
            detached: true,
            bare: false,
        };
        let normal = Worktree {
            path: PathBuf::from("/code/myapp"),
            branch: Some("main".into()),
            detached: false,
            bare: false,
        };
        assert_eq!(sync_skip(&bare, SyncOp::Fetch), Some("bare"));
        assert_eq!(sync_skip(&bare, SyncOp::Push), Some("bare"));
        // fetch only moves remote-tracking refs, so a detached HEAD is fine.
        assert_eq!(sync_skip(&detached, SyncOp::Fetch), None);
        assert!(sync_skip(&detached, SyncOp::Pull).is_some());
        assert!(sync_skip(&detached, SyncOp::Push).is_some());
        for op in [SyncOp::Fetch, SyncOp::Pull, SyncOp::Push] {
            assert_eq!(sync_skip(&normal, op), None);
        }
    }

}
