use std::path::{Path, PathBuf};

use crate::git::git_stdout;
use crate::ui::{GREEN, RED, YELLOW};

/// A worktree as reported by `git worktree list --porcelain`.
pub(crate) struct Worktree {
    pub(crate) path: PathBuf,
    /// Short branch name, or None when detached/bare.
    pub(crate) branch: Option<String>,
    pub(crate) detached: bool,
    pub(crate) bare: bool,
}

// ---------------------------------------------------------------------------
// Color, status, and metadata (no dependencies; ANSI on a TTY only)
// ---------------------------------------------------------------------------

/// Working-tree cleanliness of a worktree.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Status {
    Clean,
    Dirty,
    Untracked,
    /// Bare worktree, or git couldn't report (shown blank).
    Unknown,
}

/// Classify `git status --porcelain` output. Any `??` line means untracked;
/// other entries mean dirty; empty means clean.
pub(crate) fn classify_status(porcelain: &str) -> Status {
    if porcelain.trim().is_empty() {
        return Status::Clean;
    }
    if porcelain.lines().any(|l| l.starts_with("??")) {
        Status::Untracked
    } else {
        Status::Dirty
    }
}

/// Run `git status --porcelain` in the worktree and classify it.
pub(crate) fn worktree_status(path: &Path) -> Status {
    match git_stdout(path, &["status", "--porcelain"]) {
        Ok(s) => classify_status(&s),
        Err(_) => Status::Unknown,
    }
}

pub(crate) fn status_text(s: Status) -> &'static str {
    match s {
        Status::Clean => "clean",
        Status::Dirty => "dirty",
        Status::Untracked => "untracked",
        Status::Unknown => "",
    }
}

/// ANSI color for a status, or "" (no color) for Unknown.
pub(crate) fn status_color(s: Status) -> &'static str {
    match s {
        Status::Clean => GREEN,
        Status::Dirty => YELLOW,
        Status::Untracked => RED,
        Status::Unknown => "",
    }
}

/// Does the worktree have uncommitted tracked changes or untracked files?
/// Unknown (bare, or git failed) counts as not dirty: no warning beats a wrong
/// one. Porcelain stays interpreted in exactly one place, `classify_status`.
pub(crate) fn is_dirty(dir: &Path) -> bool {
    matches!(worktree_status(dir), Status::Dirty | Status::Untracked)
}

/// The committed state a worktree points at. A branch name reads better in
/// diff headers than a sha, so prefer it; detached/bare use the short sha.
pub(crate) fn ref_of(w: &Worktree) -> Result<String, String> {
    if let Some(b) = &w.branch {
        return Ok(b.clone());
    }
    let sha = git_stdout(&w.path,
        &["rev-parse", "--short", "HEAD"],
    )
    .map_err(|e| format!("worktree {} has no HEAD: {e}", w.path.display()))?;
    Ok(sha.trim().to_string())
}

/// The worktree the shell is standing in, if any.
///
/// The deepest match wins: `add --dirname` can put one worktree inside
/// another's tree, and the innermost is the one you are actually in.
pub(crate) fn here_index(trees: &[Worktree]) -> Option<usize> {
    let cwd = canon(&std::env::current_dir().ok()?);
    trees
        .iter()
        .enumerate()
        .filter(|(_, w)| cwd.starts_with(canon(&w.path)))
        .max_by_key(|(_, w)| canon(&w.path).components().count())
        .map(|(i, _)| i)
}

// ---------------------------------------------------------------------------
// Paths and naming
// ---------------------------------------------------------------------------

/// Collapse path-hostile characters to single dashes; trim leading/trailing.
pub(crate) fn sanitize(branch: &str) -> String {
    let mut out = String::with_capacity(branch.len());
    let mut last_dash = false;
    for c in branch.chars() {
        let c = if matches!(c, '/' | ' ' | ':' | '\\') { '-' } else { c };
        if c == '-' {
            if !last_dash {
                out.push('-');
            }
            last_dash = true;
        } else {
            out.push(c);
            last_dash = false;
        }
    }
    out.trim_matches('-').to_string()
}

/// Single-quote a path for safe interpolation into an `sh -c` command line
/// (used to build fzf's --preview). Embedded quotes are escaped `'\''`.
pub(crate) fn sh_quote(p: &Path) -> String {
    format!("'{}'", p.to_string_lossy().replace('\'', "'\\''"))
}

/// Canonical path for comparison; falls back to the input when it can't be
/// resolved (e.g. it no longer exists), so equal paths still compare equal.
pub(crate) fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Last path component (directory leaf) as a display string, or the whole path
/// when it has none.
pub(crate) fn leaf_of(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| p.display().to_string())
}

pub(crate) fn label(w: &Worktree) -> String {
    if w.bare {
        "(bare)".into()
    } else if w.detached {
        "(detached)".into()
    } else {
        w.branch.clone().unwrap_or_else(|| "(unknown)".into())
    }
}

// ---------------------------------------------------------------------------
// git plumbing
// ---------------------------------------------------------------------------

/// Absolute path to the main worktree root, even when invoked from a
/// subdirectory or from inside a linked worktree.
pub(crate) fn repo_root() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let common = git_stdout(&cwd,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .map_err(|_| "not inside a git repository".to_string())?;

    let common = PathBuf::from(common.trim());
    // `.../repo/.git` -> `.../repo`; a bare repo has no `.git` component.
    let root = if common.file_name().map(|n| n == ".git").unwrap_or(false) {
        common.parent().ok_or("malformed git dir")?.to_path_buf()
    } else {
        common
    };
    Ok(root)
}

/// The ref checked out in the current directory's worktree: the branch name,
/// or a short commit sha when detached. Falls back to "HEAD" if git can't say.
pub(crate) fn current_ref() -> String {
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(_) => return "HEAD".into(),
    };
    if let Ok(b) = git_stdout(&cwd,
        &["symbolic-ref", "--short", "-q", "HEAD"],
    ) {
        let b = b.trim();
        if !b.is_empty() {
            return b.to_string();
        }
    }
    // Detached HEAD: use the short commit sha.
    if let Ok(sha) = git_stdout(&cwd,
        &["rev-parse", "--short", "HEAD"],
    ) {
        let sha = sha.trim();
        if !sha.is_empty() {
            return sha.to_string();
        }
    }
    "HEAD".into()
}

pub(crate) fn worktrees(root: &Path) -> Result<Vec<Worktree>, String> {
    let out = git_stdout(root, &["worktree", "list", "--porcelain"])?;
    let mut trees = Vec::new();
    let mut cur: Option<Worktree> = None;

    for line in out.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            if let Some(w) = cur.take() {
                trees.push(w);
            }
            cur = Some(Worktree {
                path: PathBuf::from(p),
                branch: None,
                detached: false,
                bare: false,
            });
        } else if let Some(w) = cur.as_mut() {
            if let Some(b) = line.strip_prefix("branch ") {
                w.branch = Some(b.strip_prefix("refs/heads/").unwrap_or(b).to_string());
            } else if line == "detached" {
                w.detached = true;
            } else if line == "bare" {
                w.bare = true;
            }
        }
    }
    if let Some(w) = cur {
        trees.push(w);
    }
    Ok(trees)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_collapses_separators() {
        assert_eq!(sanitize("feature/login"), "feature-login");
        assert_eq!(sanitize("a/b/c/d"), "a-b-c-d");
        assert_eq!(sanitize("feat//x"), "feat-x");
        assert_eq!(sanitize("has space"), "has-space");
        assert_eq!(sanitize("/leading/"), "leading");
        assert_eq!(sanitize("release-3.2.1"), "release-3.2.1");
    }


    #[test]
    fn classify_status_reads_porcelain() {
        assert_eq!(classify_status(""), Status::Clean);
        assert_eq!(classify_status("   \n"), Status::Clean);
        assert_eq!(classify_status(" M src/main.rs"), Status::Dirty);
        assert_eq!(classify_status("?? new.txt"), Status::Untracked);
        // Untracked wins when both are present.
        assert_eq!(classify_status(" M a\n?? b"), Status::Untracked);
    }


    #[test]
    fn sh_quote_wraps_and_escapes() {
        assert_eq!(sh_quote(Path::new("/code/my app")), "'/code/my app'");
        assert_eq!(sh_quote(Path::new("/a'b")), "'/a'\\''b'");
    }


    #[test]
    fn leaf_of_returns_last_component() {
        assert_eq!(leaf_of(Path::new("/code/myapp-feat-x")), "myapp-feat-x");
        assert_eq!(leaf_of(Path::new("myapp")), "myapp");
    }

}
