pub(crate) mod args;

use std::io::IsTerminal;
use std::path::Path;

use crate::cmd::list::ListMode;
use crate::git::{git_cmd, git_stdout};
use crate::ui::{color_enabled, paint, GREEN};
use crate::worktree::{ref_of, Worktree};

pub(crate) fn cmd_merged_others(
    root: &Path,
    trees: &[Worktree],
    idx: usize,
    show_path: bool,
) -> Result<(), String> {
    let dest = ref_of(&trees[idx])?;
    crate::cmd::list::cmd_list_impl(
        root,
        None,
        None,
        ListMode::Normal,
        show_path,
        false,
        Some(&dest),
        false,
    )
}

pub(crate) fn merged_text(root: &Path, w: &Worktree, here: &str) -> String {
    let Some(src) = w.branch.as_deref() else {
        return "-".into();
    };
    if src == here {
        return "self".into();
    }
    merged_status_text(root, src, here)
}

pub(crate) fn merged_status_text(root: &Path, src: &str, dest: &str) -> String {
    match git_cmd(root, &["merge-base", "--is-ancestor", src, dest]).output() {
        Ok(out) => match out.status.code() {
            Some(0) => "merged".into(),
            Some(1) => match ahead_count(root, src, dest) {
                Ok(n) => format!("ahead {n}"),
                Err(_) => "ahead".into(),
            },
            _ => "-".into(),
        },
        Err(_) => "-".into(),
    }
}

pub(crate) fn merged_text_at(root: &Path, w: &Worktree, dest: &str) -> (String, String) {
    let Some(src) = w.branch.as_deref() else {
        return ("-".into(), "-".into());
    };
    if src == dest {
        return ("self".into(), "-".into());
    }
    let status = merged_status_text(root, src, dest);
    let at = if status == "merged" {
        last_merge_date(root, src, dest)
    } else {
        "-".into()
    };
    (status, at)
}

pub(crate) fn last_merge_date(root: &Path, src: &str, dest: &str) -> String {
    git_stdout(
        root,
        &[
            "log",
            "-1",
            "--ancestry-path",
            "--merges",
            "--format=%ar",
            &format!("{src}..{dest}"),
        ],
    )
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "-".into())
}

pub(crate) fn cmd_merged(dir: &Path, src: &str, dest: &str) -> Result<(), String> {
    let out = git_cmd(dir, &["merge-base", "--is-ancestor", src, dest])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    let color = std::io::stderr().is_terminal() && color_enabled(true);

    match out.status.code() {
        Some(0) => {
            println!("{} {src} is already in {dest}", paint("Merged", GREEN, color));
            Ok(())
        }
        Some(1) => {
            let count_msg = match ahead_count(dir, src, dest) {
                Ok(n) => format!("ahead {n}"),
                Err(_) => "ahead".to_string(),
            };
            Err(format!("Ahead {src} is NOT in {dest} ({count_msg})"))
        }
        _ => {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(err.trim().to_string())
        }
    }
}

pub(crate) fn ahead_count(dir: &Path, src: &str, dest: &str) -> Result<usize, String> {
    let s = git_stdout(dir, &["rev-list", "--count", &format!("{dest}..{src}")])?;
    s.trim()
        .parse()
        .map_err(|e| format!("could not parse ahead count: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &std::path::Path, args: &[&str]) {
        let mut c = std::process::Command::new("git");
        c.current_dir(dir).args(args);
        let out = c.output().unwrap();
        assert!(out.status.success(), "git {:?} failed: {:?}", args, out);
    }

    #[test]
    fn merged_reports_ancestor_and_non_ancestor() {
        let tmp = std::env::temp_dir().join(format!("git-wt-merged-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"]);
        git(&tmp, &["config", "user.email", "t@test"]);
        git(&tmp, &["config", "user.name", "t"]);
        std::fs::write(tmp.join("x.txt"), "init").unwrap();
        git(&tmp, &["add", "x.txt"]);
        git(&tmp, &["commit", "--quiet", "-m", "init"]);
        git(&tmp, &["branch", "feat"]);
        git(&tmp, &["checkout", "--quiet", "feat"]);
        std::fs::write(tmp.join("y.txt"), "a").unwrap();
        git(&tmp, &["add", "y.txt"]);
        git(&tmp, &["commit", "--quiet", "-m", "add"]);

        assert!(cmd_merged(&tmp, "main", "feat").is_ok());
        let err = cmd_merged(&tmp, "feat", "main").unwrap_err();
        assert!(err.contains("Ahead feat is NOT in main"), "{err}");
        assert!(err.contains("ahead 1"), "{err}");
        let err = cmd_merged(&tmp, "no-such-ref", "main").unwrap_err();
        assert!(err.contains("no-such-ref") || err.contains("Not a valid object"), "{err}");

        std::fs::remove_dir_all(&tmp).ok();
    }
}
