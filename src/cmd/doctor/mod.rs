pub(crate) mod args;

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::cmd::doctor::args::DoctorArgs;
use crate::git::{git_quiet, git_run};
use crate::ui::{color_enabled, paint, DIM, GREEN, RED, YELLOW};
use crate::worktree::{label, worktrees, Worktree};

pub(crate) struct Issue {
    pub(crate) summary: String,
    pub(crate) severity: &'static str,
}

fn gitdir_link_target(path: &Path) -> Option<PathBuf> {
    let git_file = path.join(".git");
    if !git_file.is_file() {
        return None;
    }
    let content = std::fs::read_to_string(git_file).ok()?;
    let target = content.trim().strip_prefix("gitdir:")?.trim();
    Some(PathBuf::from(target))
}

pub(crate) fn scan(w: &Worktree) -> Vec<Issue> {
    let mut out = Vec::new();

    if let Some(reason) = &w.prunable {
        let why = if reason.is_empty() {
            "directory is gone or unreachable".to_string()
        } else {
            reason.clone()
        };
        out.push(Issue {
            summary: format!("prunable: {why}"),
            severity: RED,
        });
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
            _ if !git_quiet(&w.path,
                &["rev-parse", "-q", "--verify", "HEAD"],
            ) => {
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

fn sibling_candidates(root: &Path) -> Vec<PathBuf> {
    let Some(parent) = root.parent() else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(parent) else { return Vec::new() };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p != root && p.join(".git").is_file())
        .collect()
}

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
        let _ = git_run(root, &argv_ref);
    }
    let _ = git_run(root, &["worktree", "prune", "-v"]);
    Ok(())
}

pub(crate) fn cmd_doctor(root: &Path, trees: &[Worktree], args: DoctorArgs) -> Result<(), String> {
    let color = color_enabled(std::io::stdout().is_terminal());

    let total = print_report(trees, color);
    if total == 0 {
        println!("{}", paint("all worktrees healthy", GREEN, color));
        return Ok(());
    }

    if !args.repair {
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
