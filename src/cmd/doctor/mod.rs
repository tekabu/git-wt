pub(crate) mod args;

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::cmd::doctor::args::DoctorArgs;
use crate::git::{git_quiet, git_run};
use crate::ui::{color_enabled, paint, DIM, GREEN, RED, YELLOW};
use crate::worktree::{canon, default_worktrees_dir, label, leaf_of, sanitize, worktrees, Worktree};

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
    for w in trees.iter() {
        let issues = scan(w);
        if issues.is_empty() {
            continue;
        }
        total += issues.len();
        println!(
            "{}  {}  {}",
            paint(&w.id.to_string(), DIM, color),
            label(w),
            w.path.display()
        );
        for issue in &issues {
            println!("    {}", paint(&issue.summary, issue.severity, color));
        }
    }
    total
}

/// Directories that might hold an orphaned (unregistered) worktree: the
/// repo root's sibling directory (the original default, pre-consolidation),
/// `<root>/.worktrees/` (a since-retired consolidated default), and
/// `<repo>-worktrees` (the current default).
fn orphan_candidates(root: &Path) -> Vec<PathBuf> {
    let dirs = [
        root.parent().map(Path::to_path_buf),
        Some(root.join(".worktrees")),
        Some(default_worktrees_dir(root)),
    ];
    dirs.into_iter()
        .flatten()
        .filter_map(|dir| std::fs::read_dir(&dir).ok())
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p != root && p.join(".git").is_file())
        .collect()
}

fn repair(root: &Path, trees: &[Worktree]) -> Result<(), String> {
    let mut candidates: Vec<PathBuf> = trees.iter().filter(|w| !w.bare).map(|w| w.path.clone()).collect();
    for c in orphan_candidates(root) {
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

/// Move every worktree not already under the default `<repo>-worktrees`
/// directory there, via `git worktree move` (updates the admin link, unlike
/// a plain `mv`). The main worktree (`root` itself) is never a candidate.
fn migrate(root: &Path, trees: &[Worktree], color: bool) -> Result<(), String> {
    let target_dir = default_worktrees_dir(root);
    let root_c = canon(root);
    let target_c = canon(&target_dir);

    let mut moved = 0;
    let mut failed = 0;

    for w in trees {
        if w.bare {
            continue;
        }
        let wt_c = canon(&w.path);
        if wt_c == root_c || wt_c.starts_with(&target_c) {
            continue;
        }

        let leaf = leaf_of(&w.path);
        // `symlink_metadata` (unlike `exists`) also catches a broken symlink
        // sitting at the destination, which `exists` silently misses. A
        // free destination is found by suffixing `-2`, `-3`, ... rather than
        // skipping outright, since a collision here is between two worktrees
        // both being migrated (already-migrated ones were excluded above),
        // not one worktree already in place.
        let mut new_path = target_dir.join(&leaf);
        let mut n = 2;
        while new_path.symlink_metadata().is_ok() {
            new_path = target_dir.join(format!("{leaf}-{n}"));
            n += 1;
        }
        if n > 2 {
            eprintln!(
                "{} {} -- {leaf} taken, using {}",
                paint("rename", YELLOW, color),
                w.path.display(),
                leaf_of(&new_path)
            );
        }

        std::fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
        let old_s = w.path.to_string_lossy().to_string();
        let new_s = new_path.to_string_lossy().to_string();
        match git_run(root, &["worktree", "move", &old_s, &new_s]) {
            Ok(_) => {
                println!("{} {old_s} -> {new_s}", paint("moved", GREEN, color));
                moved += 1;
            }
            Err(e) => {
                eprintln!("{} {old_s}: {e}", paint("failed", RED, color));
                failed += 1;
            }
        }
    }

    println!();
    if moved == 0 && failed == 0 {
        println!(
            "{}",
            paint(
                &format!("nothing to migrate -- all worktrees already under {}", target_dir.display()),
                GREEN,
                color
            )
        );
        return Ok(());
    }
    println!("{moved} moved, {failed} failed");
    if failed > 0 {
        return Err(format!("{failed} worktree(s) failed to migrate (see above)"));
    }
    Ok(())
}

/// Rename every worktree folder (wherever it lives) to its branch alone, via
/// `git worktree move` -- the same admin-link-safe rename `migrate` uses,
/// just in place rather than relocating. The main worktree and any
/// detached/bare worktree (no branch to name it after) are never candidates.
fn rename(root: &Path, trees: &[Worktree], color: bool) -> Result<(), String> {
    let root_c = canon(root);

    let mut renamed = 0;
    let mut failed = 0;

    for w in trees {
        if w.bare {
            continue;
        }
        let Some(branch) = &w.branch else { continue };
        if canon(&w.path) == root_c {
            continue;
        }

        let wanted = sanitize(branch);
        if leaf_of(&w.path) == wanted {
            continue;
        }
        let parent = match w.path.parent() {
            Some(p) => p,
            None => continue,
        };

        let mut new_path = parent.join(&wanted);
        let mut n = 2;
        while new_path.symlink_metadata().is_ok() {
            new_path = parent.join(format!("{wanted}-{n}"));
            n += 1;
        }
        if n > 2 {
            eprintln!(
                "{} {} -- {wanted} taken, using {}",
                paint("rename", YELLOW, color),
                w.path.display(),
                leaf_of(&new_path)
            );
        }

        let old_s = w.path.to_string_lossy().to_string();
        let new_s = new_path.to_string_lossy().to_string();
        match git_run(root, &["worktree", "move", &old_s, &new_s]) {
            Ok(_) => {
                println!("{} {old_s} -> {new_s}", paint("renamed", GREEN, color));
                renamed += 1;
            }
            Err(e) => {
                eprintln!("{} {old_s}: {e}", paint("failed", RED, color));
                failed += 1;
            }
        }
    }

    println!();
    if renamed == 0 && failed == 0 {
        println!("{}", paint("nothing to rename -- every worktree already matches its branch", GREEN, color));
        return Ok(());
    }
    println!("{renamed} renamed, {failed} failed");
    if failed > 0 {
        return Err(format!("{failed} worktree(s) failed to rename (see above)"));
    }
    Ok(())
}

pub(crate) fn cmd_doctor(root: &Path, trees: &[Worktree], args: DoctorArgs) -> Result<(), String> {
    let color = color_enabled(std::io::stdout().is_terminal());

    if args.migrate.migrate || args.syncname.syncname {
        if args.migrate.migrate {
            migrate(root, trees, color)?;
        }
        if args.syncname.syncname {
            // Re-read from disk: a preceding `--migrate` moved paths out
            // from under the `trees` this function was called with.
            let fresh = worktrees(root)?;
            rename(root, &fresh, color)?;
        }
        return Ok(());
    }

    let total = print_report(trees, color);
    if total == 0 {
        println!("{}", paint("all worktrees healthy", GREEN, color));
        return Ok(());
    }

    if !args.repair.repair {
        println!();
        println!("{total} issue(s) found (see above)");
        println!("'git-wt doctor --repair' attempts to fix them");
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
            "a deleted (not moved) worktree needs 'git-wt <N> remove -f' \
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
            id: 1,
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

    fn git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?} failed: {out:?}");
    }

    /// A throwaway repo with one linked worktree at `<root>/../<root's
    /// leaf>-feat`, the pre-`.worktrees` default location. Returns
    /// `(root, sibling worktree path)`; the caller removes the whole tree.
    fn repo_with_sibling_worktree(name: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let root = base.join("repo");
        std::fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "--quiet", "--initial-branch=main"]);
        git(&root, &["config", "user.email", "t@test"]);
        git(&root, &["config", "user.name", "t"]);
        git(&root, &["commit", "--quiet", "--allow-empty", "-m", "init"]);

        let sibling = base.join("repo-feat");
        git(&root, &["worktree", "add", "--quiet", "-b", "feat", sibling.to_str().unwrap()]);

        (root, sibling)
    }

    #[test]
    fn migrate_moves_a_sibling_worktree_under_repo_worktrees() {
        let (root, sibling) = repo_with_sibling_worktree("git-wt-migrate-test");

        let trees = worktrees(&root).unwrap();
        migrate(&root, &trees, false).unwrap();

        let new_path = root.parent().unwrap().join("repo-worktrees").join("repo-feat");
        assert!(new_path.is_dir(), "expected {new_path:?} to exist");
        assert!(!sibling.exists(), "expected {sibling:?} to be gone");

        let fresh = worktrees(&root).unwrap();
        let paths: Vec<_> = fresh.iter().map(|w| w.path.clone()).collect();
        assert!(
            fresh.iter().any(|w| canon(&w.path) == canon(&new_path)),
            "git's own worktree list should reflect the move: {paths:?}"
        );

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn migrate_is_a_no_op_the_second_time() {
        let (root, _sibling) = repo_with_sibling_worktree("git-wt-migrate-idempotent-test");

        let trees = worktrees(&root).unwrap();
        migrate(&root, &trees, false).unwrap();

        let fresh = worktrees(&root).unwrap();
        // Second pass finds nothing left outside the target dir: no path
        // should change, and nothing should error.
        migrate(&root, &fresh, false).unwrap();
        let still = worktrees(&root).unwrap();
        assert_eq!(
            fresh.iter().map(|w| w.path.clone()).collect::<Vec<_>>(),
            still.iter().map(|w| w.path.clone()).collect::<Vec<_>>(),
        );

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn migrate_dedups_a_colliding_leaf_name() {
        let (root, sibling) = repo_with_sibling_worktree("git-wt-migrate-collision-test");

        // Occupy the destination with an unrelated directory first.
        let dest = root.parent().unwrap().join("repo-worktrees").join("repo-feat");
        std::fs::create_dir_all(&dest).unwrap();

        let trees = worktrees(&root).unwrap();
        migrate(&root, &trees, false).unwrap();

        // Moved anyway, under a disambiguated name rather than left behind.
        assert!(!sibling.exists(), "expected {sibling:?} to be gone");
        assert!(dest.is_dir(), "the unrelated directory must be left alone");
        let renamed = root.parent().unwrap().join("repo-worktrees").join("repo-feat-2");
        assert!(renamed.is_dir(), "expected {renamed:?} to exist");

        let fresh = worktrees(&root).unwrap();
        assert!(fresh.iter().any(|w| canon(&w.path) == canon(&renamed)));

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn migrate_errors_when_a_move_fails() {
        let (root, sibling) = repo_with_sibling_worktree("git-wt-migrate-failure-test");

        // Lock the worktree so `git worktree move` refuses it.
        git(&root, &["worktree", "lock", sibling.to_str().unwrap()]);

        let trees = worktrees(&root).unwrap();
        let err = migrate(&root, &trees, false).unwrap_err();
        assert!(err.contains("failed"), "{err}");
        assert!(sibling.is_dir(), "a locked worktree must stay put");

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn rename_fixes_a_repo_prefixed_leaf_in_place() {
        let (root, sibling) = repo_with_sibling_worktree("git-wt-rename-test");

        let trees = worktrees(&root).unwrap();
        rename(&root, &trees, false).unwrap();

        let new_path = sibling.parent().unwrap().join("feat");
        assert!(new_path.is_dir(), "expected {new_path:?} to exist");
        assert!(!sibling.exists(), "expected {sibling:?} to be gone");

        let fresh = worktrees(&root).unwrap();
        assert!(fresh.iter().any(|w| canon(&w.path) == canon(&new_path)));

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn rename_does_not_relocate_a_worktree() {
        let (root, sibling) = repo_with_sibling_worktree("git-wt-rename-no-move-test");

        let trees = worktrees(&root).unwrap();
        rename(&root, &trees, false).unwrap();

        let new_path = sibling.parent().unwrap().join("feat");
        assert_eq!(new_path.parent(), sibling.parent(), "rename must not change the worktree's directory");

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn rename_is_a_no_op_the_second_time() {
        let (root, _sibling) = repo_with_sibling_worktree("git-wt-rename-idempotent-test");

        let trees = worktrees(&root).unwrap();
        rename(&root, &trees, false).unwrap();

        let fresh = worktrees(&root).unwrap();
        rename(&root, &fresh, false).unwrap();
        let still = worktrees(&root).unwrap();
        assert_eq!(
            fresh.iter().map(|w| w.path.clone()).collect::<Vec<_>>(),
            still.iter().map(|w| w.path.clone()).collect::<Vec<_>>(),
        );

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn migrate_does_not_rename_an_already_consolidated_worktree() {
        let (root, sibling) = repo_with_sibling_worktree("git-wt-migrate-no-rename-test");

        let trees = worktrees(&root).unwrap();
        migrate(&root, &trees, false).unwrap();

        // Second migrate pass: the worktree is already under the target
        // dir, so a plain `--migrate` must leave its (still repo-prefixed)
        // name alone -- `--syncname` is the only thing that touches names.
        let fresh = worktrees(&root).unwrap();
        migrate(&root, &fresh, false).unwrap();
        let still_path = root.parent().unwrap().join("repo-worktrees").join("repo-feat");
        assert!(still_path.is_dir(), "expected {still_path:?} to still exist, untouched");

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
        let _ = sibling;
    }

    #[test]
    fn orphan_candidates_finds_dot_worktrees_entries() {
        let base = std::env::temp_dir().join(format!(
            "git-wt-orphan-candidates-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("repo");
        std::fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "--quiet", "--initial-branch=main"]);
        git(&root, &["config", "user.email", "t@test"]);
        git(&root, &["config", "user.name", "t"]);
        git(&root, &["commit", "--quiet", "--allow-empty", "-m", "init"]);

        // A worktree admin ".git" file dropped straight in `.worktrees/`
        // without ever being registered via `git worktree add`, standing
        // in for one `worktree add` created and something later un-registered
        // (e.g. `.git` file survived a manual directory move).
        let orphan = root.join(".worktrees").join("orphan");
        std::fs::create_dir_all(&orphan).unwrap();
        std::fs::write(orphan.join(".git"), "gitdir: /nowhere\n").unwrap();

        let found = orphan_candidates(&root);
        assert!(
            found.iter().any(|p| p == &orphan),
            "expected {orphan:?} in {found:?}"
        );

        std::fs::remove_dir_all(&base).unwrap();
    }
}
