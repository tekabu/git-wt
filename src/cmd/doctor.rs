use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::git::{git_quiet, git_run};
use crate::ui::{color_enabled, paint, DIM, GREEN, RED, YELLOW};
use crate::worktree::{label, worktrees, Worktree};

/// One thing doctor found wrong with a worktree.
pub(crate) struct Issue {
    pub(crate) summary: String,
    /// RED for something broken, YELLOW for something merely worth knowing.
    pub(crate) severity: &'static str,
}

/// The admin dir a linked worktree's own `.git` file points back at, read
/// straight from that file -- `None` for a bare or main worktree, whose
/// `.git` is a real repository directory, not this back-pointer.
fn gitdir_link_target(path: &Path) -> Option<PathBuf> {
    let git_file = path.join(".git");
    if !git_file.is_file() {
        return None;
    }
    let content = std::fs::read_to_string(git_file).ok()?;
    let target = content.trim().strip_prefix("gitdir:")?.trim();
    Some(PathBuf::from(target))
}

/// Everything doctor checks about one worktree, read straight off what git's
/// own porcelain output and the filesystem already say -- no extra history
/// walk, so a repo with many worktrees stays fast.
///
/// 'prunable' is git's own verdict (the same field 'git worktree prune'
/// acts on) and catches this worktree's own directory being gone or
/// unreachable before the filesystem check below would even need to run.
/// The `.git`-file check catches the opposite direction: the directory is
/// still right there, but its back-pointer names an admin dir that no
/// longer exists -- which is exactly what a linked worktree looks like
/// after the *main* worktree gets moved or renamed, since git never updates
/// that pointer on its own. 'locked' is not broken -- it is why
/// 'remove'/'prune' refuse to touch the worktree -- so it is reported
/// separately and never suppresses the checks above it.
pub(crate) fn scan(w: &Worktree) -> Vec<Issue> {
    let mut out = Vec::new();

    if let Some(reason) = &w.prunable {
        let why = if reason.is_empty() {
            "directory is gone or unreachable".to_string()
        } else {
            reason.clone()
        };
        out.push(Issue { summary: format!("prunable: {why}"), severity: RED });
    } else if !w.path.exists() {
        out.push(Issue {
            summary: "directory not found on disk (moved or deleted)".into(),
            severity: RED,
        });
    } else if !w.bare {
        match gitdir_link_target(&w.path) {
            Some(target) if !target.exists() => {
                out.push(Issue {
                    summary: format!(
                        "'.git' points to a missing admin dir ({}) -- \
                         the main worktree was likely moved or renamed",
                        target.display()
                    ),
                    severity: RED,
                });
            }
            _ if !git_quiet(&w.path, &["rev-parse", "-q", "--verify", "HEAD"]) => {
                out.push(Issue {
                    summary: "HEAD unreadable (repository may be corrupt)".into(),
                    severity: RED,
                });
            }
            _ => {}
        }
    }

    if let Some(reason) = &w.locked {
        let why = if reason.is_empty() { "no reason given".to_string() } else { reason.clone() };
        out.push(Issue { summary: format!("locked: {why}"), severity: YELLOW });
    }

    out
}

fn print_report(trees: &[Worktree], color: bool) -> usize {
    let mut total = 0;
    for (i, w) in trees.iter().enumerate() {
        let issues = scan(w);
        if issues.is_empty() {
            continue;
        }
        total += issues.len();
        println!(
            "{}  {}  {}",
            paint(&(i + 1).to_string(), DIM, color),
            label(w),
            w.path.display()
        );
        for issue in &issues {
            println!("    {}", paint(&issue.summary, issue.severity, color));
        }
    }
    total
}

/// Every sibling of the repo root that looks like a worktree checkout: a
/// directory whose '.git' is a plain file (a linked worktree's marker, a
/// full repo's is a directory). 'add' puts every worktree it creates here,
/// as '<repo>-<branch>', so a worktree moved by hand (renamed, or dragged
/// to a new parent that still has one) usually still turns up in this list.
fn sibling_candidates(root: &Path) -> Vec<PathBuf> {
    let Some(parent) = root.parent() else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(parent) else { return Vec::new() };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p != root && p.join(".git").is_file())
        .collect()
}

/// Attempt to fix what `scan` found. 'git worktree repair' takes a list of
/// worktree paths and, for each, relinks it with this repo's admin dir
/// whenever the two already agree on which worktree the other is; anything
/// else it leaves untouched. So it is safe to hand it every candidate this
/// repo might mean:
///
///   - every worktree's own recorded path, unchanged -- the fix for the
///     common case, the *main* worktree having moved: each linked
///     worktree's `.git` file still names the old admin dir, and repair
///     rewrites it to the current one, using exactly the path
///     `worktree list` already had.
///   - every sibling of the repo root whose `.git` is a plain file -- the
///     fix for a *linked* worktree having moved, whose recorded path (now
///     missing) no longer names it.
///
/// Whatever neither relinks (a directory truly deleted, not moved) is what
/// 'prune' is for: it only removes entries git already marked prunable,
/// never a worktree it still considers live.
fn repair(root: &Path, trees: &[Worktree]) -> Result<(), String> {
    let mut candidates: Vec<PathBuf> = trees.iter().filter(|w| !w.bare).map(|w| w.path.clone()).collect();
    for c in sibling_candidates(root) {
        if !candidates.contains(&c) {
            candidates.push(c);
        }
    }
    if !candidates.is_empty() {
        let strs: Vec<String> = candidates.iter().map(|p| p.to_string_lossy().to_string()).collect();
        let mut argv = vec!["worktree".to_string(), "repair".to_string()];
        argv.extend(strs);
        let argv_ref: Vec<&str> = argv.iter().map(String::as_str).collect();
        // Best-effort: an irrelevant candidate can make git exit nonzero even
        // when every real worktree it named was fixed, so a failure here
        // does not stop the prune pass below.
        let _ = git_run(root, &argv_ref);
    }
    let _ = git_run(root, &["worktree", "prune", "-v"]);
    Ok(())
}

pub(crate) fn cmd_doctor(root: &Path, trees: &[Worktree], do_repair: bool) -> Result<(), String> {
    let color = color_enabled(std::io::stdout().is_terminal());

    let total = print_report(trees, color);
    if total == 0 {
        println!("{}", paint("all worktrees healthy", GREEN, color));
        return Ok(());
    }

    if !do_repair {
        println!();
        println!("{total} issue(s) found (see above)");
        println!("hint: 'git-wt doctor --repair' attempts to fix them");
        return Ok(());
    }

    println!();
    println!("repairing...");
    repair(root, trees)?;

    let fresh = worktrees(root)?;
    println!();
    let remaining = print_report(&fresh, color);
    if remaining == 0 {
        println!("{}", paint("all issues repaired", GREEN, color));
    } else {
        println!("{remaining} issue(s) remain -- not everything can be auto-repaired");
        println!(
            "hint: a deleted (not moved) worktree needs 'git-wt <N> remove -f' \
             or 'git worktree prune'"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn wt(path: &str) -> Worktree {
        Worktree {
            path: PathBuf::from(path),
            branch: Some("main".into()),
            detached: false,
            bare: false,
            locked: None,
            prunable: None,
        }
    }

    #[test]
    fn a_healthy_worktree_has_no_issues() {
        // Its own directory ("."), which always exists, and a HEAD 'git
        // rev-parse' can actually verify since it's this crate's own repo.
        let w = wt(".");
        assert!(scan(&w).is_empty());
    }

    #[test]
    fn a_missing_directory_is_reported() {
        let w = wt("/no/such/path/git-wt-doctor-test");
        let issues = scan(&w);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].summary.contains("not found"));
        assert_eq!(issues[0].severity, RED);
    }

    #[test]
    fn prunable_wins_over_the_bare_directory_check() {
        let mut w = wt("/no/such/path/git-wt-doctor-test");
        w.prunable = Some("gitdir file points to non-existent location".into());
        let issues = scan(&w);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].summary.starts_with("prunable:"));
    }

    #[test]
    fn a_locked_worktree_is_flagged_but_not_broken() {
        let mut w = wt(".");
        w.locked = Some("reviewing".into());
        let issues = scan(&w);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].summary, "locked: reviewing");
        assert_eq!(issues[0].severity, YELLOW);
    }
}
