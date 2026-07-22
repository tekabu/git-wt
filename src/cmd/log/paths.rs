use std::path::{Path, PathBuf};

use crate::git::git_quiet;
use crate::worktree::Worktree;

/// Collapse `.`/`..` components lexically, without touching the filesystem.
///
/// `canonicalize` needs the path to exist, which a file this table is asked
/// about may not: "deleted on main, alive on feat/x" is exactly the case
/// `log` exists for, and the deleted copy is nowhere on disk to canonicalize.
/// A lexical normalize is enough to compare against a worktree root, which is
/// itself canonicalized (worktree roots do exist).
fn normalize_lexically(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        use std::path::Component::*;
        match comp {
            CurDir => {}
            ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// A path as git wants it: forward slashes, whatever the OS. A path with no
/// components at all is the worktree root itself -- `.`, not an empty string,
/// which git reads as a pathspec matching nothing rather than everything.
fn to_pathspec(p: &Path) -> String {
    let s = p
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if s.is_empty() {
        ".".to_string()
    } else {
        s
    }
}

/// Does `candidate` (repo-relative, forward slashes) name a real path in
/// `root`'s `HEAD` tree? Used only for the third reading below, where the
/// path resolved under no worktree root but may still be a real one, typed
/// from a subdirectory rather than through it.
fn exists_in_tree(root: &Path, candidate: &str) -> bool {
    !candidate.is_empty() && git_quiet(root, &["cat-file", "-e", &format!("HEAD:{candidate}")])
}

/// Turn whatever shape `input` arrived in -- absolute, relative, or already
/// repo-relative -- into one repo-relative pathspec, the shape `log` applies
/// to every listed branch alike.
///
/// Every worktree is a full checkout of the same repository, so any worktree's
/// copy of `src/ui.rs` names the same file in history; the worktree a path
/// resolves under need not be one of the branches actually listed. Order
/// matters only in that the primary root is tried like any other worktree
/// (it may not appear in `trees` under an old git, and is always a valid
/// checkout).
pub(crate) fn resolve_pathspec(
    root: &Path,
    trees: &[Worktree],
    cwd: &Path,
    input: &str,
) -> Result<String, String> {
    let abs = if Path::new(input).is_absolute() {
        PathBuf::from(input)
    } else {
        cwd.join(input)
    };
    let abs = normalize_lexically(&abs);

    for base in trees.iter().map(|w| w.path.as_path()).chain(std::iter::once(root)) {
        let base = normalize_lexically(base);
        if let Ok(rel) = abs.strip_prefix(&base) {
            return Ok(to_pathspec(rel));
        }
    }

    // Repo-relative already, typed from anywhere: used verbatim when it
    // resolves under no worktree root but does exist in the tree -- e.g. a
    // path typed relative to the repo root while standing somewhere that
    // is not, itself, one of its worktrees.
    if !Path::new(input).is_absolute() {
        let candidate = input.trim_start_matches("./");
        if exists_in_tree(root, candidate) {
            return Ok(candidate.to_string());
        }
    }

    Err(format!(
        "'{input}' is outside the repository\n\
         hint: paths are resolved against the worktree they sit in"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wt(path: &str) -> Worktree {
        Worktree {
            path: PathBuf::from(path),
            branch: Some("main".into()),
            detached: false,
            bare: false,
        }
    }

    #[test]
    fn an_absolute_path_strips_the_worktree_it_sits_under() {
        let trees = [wt("/repo/main"), wt("/repo/feat-x")];
        let root = Path::new("/repo/main");
        let cwd = Path::new("/repo/main");
        // Under the primary root.
        assert_eq!(
            resolve_pathspec(root, &trees, cwd, "/repo/main/src/ui.rs").unwrap(),
            "src/ui.rs"
        );
        // Under a *different* worktree than any listed as a target -- still
        // fine, since a worktree's copy names the same file in history.
        assert_eq!(
            resolve_pathspec(root, &trees, cwd, "/repo/feat-x/src/ui.rs").unwrap(),
            "src/ui.rs"
        );
    }

    #[test]
    fn a_relative_path_resolves_against_cwd_first() {
        let trees = [wt("/repo/main")];
        let root = Path::new("/repo/main");
        let cwd = Path::new("/repo/main/src/cmd");
        assert_eq!(
            resolve_pathspec(root, &trees, cwd, "render.rs").unwrap(),
            "src/cmd/render.rs"
        );
        // '..' walks lexically, no filesystem touched -- the file need not
        // exist for a deleted-on-this-branch path to still resolve.
        assert_eq!(
            resolve_pathspec(root, &trees, cwd, "../other/render.rs").unwrap(),
            "src/other/render.rs"
        );
    }

    #[test]
    fn a_path_outside_every_worktree_is_an_error_naming_both_readings() {
        let trees = [wt("/repo/main")];
        let root = Path::new("/repo/main");
        let cwd = Path::new("/repo/main");
        let err = resolve_pathspec(root, &trees, cwd, "/etc/hosts").unwrap_err();
        assert!(err.contains("'/etc/hosts' is outside the repository"), "{err}");
        assert!(err.contains("hint: paths are resolved"), "{err}");
    }

    #[test]
    fn a_path_missing_on_disk_still_resolves_lexically() {
        // The whole point: a file deleted on this branch and alive on another
        // is not on disk here at all, so canonicalize would fail it outright.
        let trees = [wt("/repo/main")];
        let root = Path::new("/repo/main");
        let cwd = Path::new("/repo/main");
        assert_eq!(
            resolve_pathspec(root, &trees, cwd, "src/long-gone.rs").unwrap(),
            "src/long-gone.rs"
        );
    }

    /// The third reading, against a real repo: a repo-relative path typed from
    /// a cwd that is not itself under any worktree root at all -- so it
    /// resolves under none of them, but names a real path in the tree.
    #[test]
    fn a_repo_relative_path_resolves_verbatim_when_no_worktree_root_claims_it() {
        let tmp = std::env::temp_dir().join(format!("git-wt-pathspec-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("src")).unwrap();
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .current_dir(&tmp)
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?} failed: {out:?}");
        };
        git(&["init", "--quiet", "--initial-branch=main"]);
        git(&["config", "user.email", "t@test"]);
        git(&["config", "user.name", "t"]);
        std::fs::write(tmp.join("src/ui.rs"), "x").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "--quiet", "-m", "add"]);

        // cwd is /tmp, which is under no worktree root; the primary root is
        // canon(tmp), and the trees list is empty (no other worktrees).
        let cwd = std::env::temp_dir();
        assert_eq!(
            resolve_pathspec(&tmp, &[], &cwd, "src/ui.rs").unwrap(),
            "src/ui.rs"
        );
        // A path that resolves under no worktree root and names nothing in
        // the tree either is the outside-repo error, not a silent guess.
        let err = resolve_pathspec(&tmp, &[], &cwd, "src/nope.rs").unwrap_err();
        assert!(err.contains("is outside the repository"), "{err}");

        std::fs::remove_dir_all(&tmp).ok();
    }
}
