use std::collections::HashSet;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::git::{git_bytes, git_stdout, on_path};
use crate::ui::{color_enabled, paint, GREEN};
use crate::worktree::{label, ref_of, Worktree};

// ---------------------------------------------------------------------------
// Meld: git-wt <N>,<N>[,<N>] meld
// ---------------------------------------------------------------------------

/// Open meld on 2-3 worktree directories, in the order given, and wait for it.
/// meld itself is the arbiter of 2-way vs 3-way, so we only pass the paths.
/// Arguments for `meld --diff`.
#[derive(Debug, Default)]
pub(crate) struct MeldArgs {
    /// Filter to only differing files, extracted into temp dirs.
    pub(crate) diff: bool,
    /// Range for `--diff`: None means A..B, Some("...") means A...B.
    pub(crate) range: Option<String>,
    /// Three-way: include the merge-base of A and B as the middle pane.
    pub(crate) three_way: bool,
    /// Explicit base ref (branch, commit, or worktree number).
    pub(crate) base: Option<String>,
}

pub(crate) fn parse_meld_args(args: &[String]) -> Result<MeldArgs, String> {
    let mut out = MeldArgs::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--diff" => out.diff = true,
            "..." => out.range = Some("...".into()),
            ".." => {
                return Err("'..' is the default range for meld --diff; use '...' for the fork view"
                    .into())
            }
            "--3way" => out.three_way = true,
            "--base" => {
                out.base = Some(it.next().ok_or("--base needs a ref")?.clone());
            }
            s if s.starts_with('-') && s != "-" => {
                return Err(format!("unknown option '{s}' for meld\nTry 'git-wt --help'"));
            }
            other => {
                return Err(format!(
                    "meld takes no positional arguments, got '{other}'\nTry 'git-wt --help'"
                ));
            }
        }
    }
    if out.three_way && out.base.is_some() {
        return Err("--3way and --base are alternatives; use one or the other".into());
    }
    Ok(out)
}

pub(crate) fn cmd_meld(
    root: &Path,
    trees: &[Worktree],
    idxs: &[usize],
    args: &[String],
) -> Result<(), String> {
    let parsed = parse_meld_args(args)?;

    match idxs.len() {
        2 | 3 => {}
        1 => return Err("meld needs 2 or 3 worktrees, e.g. 'git-wt 1,2 meld'".into()),
        n => return Err(format!("meld takes at most 3 worktrees, got {n}")),
    }

    // --diff compares refs, so a self-comparison is a clean no-op rather than an
    // error. For full-directory meld, showing a directory against itself is never
    // meant, so the duplicate check stays there.
    if !parsed.diff {
        for (i, a) in idxs.iter().enumerate() {
            if idxs[i + 1..].contains(a) {
                return Err(format!("worktree #{} listed twice", a + 1));
            }
        }
    }

    if !on_path("meld") {
        return Err(
            "meld is not installed (or not on PATH)\n\
             hint: macOS 'brew install --cask meld', Debian/Ubuntu 'apt install meld', \
             Fedora 'dnf install meld'"
                .into(),
        );
    }

    if parsed.diff {
        if idxs.len() != 2 {
            return Err("'--diff' takes exactly 2 worktrees; use meld without --diff for 3-way"
                .into());
        }
        return cmd_meld_filtered(root, trees, idxs[0], idxs[1], &parsed);
    }

    let paths: Vec<&Path> = idxs.iter().map(|&i| trees[i].path.as_path()).collect();
    let on = color_enabled(std::io::stderr().is_terminal());
    let names: Vec<String> = idxs.iter().map(|&i| label(&trees[i])).collect();
    eprintln!("{} {}", paint("meld", GREEN, on), names.join("  ↔  "));

    let status = Command::new("meld")
        .args(&paths)
        .status()
        .map_err(|e| format!("failed to run meld: {e}"))?;
    if !status.success() {
        return Err("meld exited with an error".into());
    }
    Ok(())
}

/// Meld only the files that differ between two refs, extracted into temp dirs.
/// Supports 2-way and 3-way (with auto or explicit base).
pub(crate) fn cmd_meld_filtered(
    root: &Path,
    trees: &[Worktree],
    left_idx: usize,
    right_idx: usize,
    args: &MeldArgs,
) -> Result<(), String> {
    let left = ref_of(&trees[left_idx])?;
    let right = ref_of(&trees[right_idx])?;

    let base = if args.three_way {
        Some(merge_base(root, &left, &right)?)
    } else {
        args.base.clone()
    };

    let mut paths = if let Some(b) = &base {
        let mut set = changed_paths(root, b, &left)?;
        set.extend(changed_paths(root, b, &right)?);
        set
    } else {
        let spec = match &args.range {
            Some(dots) => format!("{left}{dots}{right}"),
            None => format!("{left} {right}"),
        };
        changed_paths_status(root, &spec)?
            .into_iter()
            .map(|(p, _)| p)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect()
    };
    paths.sort();
    paths.dedup();

    if paths.is_empty() {
        eprintln!("no files differ between {} and {}", label(&trees[left_idx]), label(&trees[right_idx]));
        return Ok(());
    }

    let tmp = temp_meld_dir()?;
    let dir_left = tmp.join("a");
    let dir_right = tmp.join("b");
    extract_files(root, &left, &paths, &dir_left)?;
    extract_files(root, &right, &paths, &dir_right)?;

    let mut meld_paths: Vec<PathBuf> = vec![dir_left.clone(), dir_right.clone()];

    if let Some(b) = &base {
        let dir_base = tmp.join("base");
        extract_files(root, b, &paths, &dir_base)?;
        // Order: base, left, right (BASE in middle pane).
        meld_paths = vec![dir_base, dir_left, dir_right];
    }

    let on = color_enabled(std::io::stderr().is_terminal());
    let mut labels: Vec<String> = if base.is_some() {
        vec!["base".into(), label(&trees[left_idx]), label(&trees[right_idx])]
    } else {
        vec![label(&trees[left_idx]), label(&trees[right_idx])]
    };
    if let Some(b) = &base {
        // Replace generic "base" label with the actual ref when explicit or auto.
        labels[0] = b.clone();
    }
    eprintln!("{} {}", paint("meld", GREEN, on), labels.join("  ↔  "));

    let status = Command::new("meld")
        .args(&meld_paths)
        .status()
        .map_err(|e| format!("failed to run meld: {e}"))?;

    let _ = std::fs::remove_dir_all(&tmp);

    if !status.success() {
        return Err("meld exited with an error".into());
    }
    Ok(())
}

/// Merge base of two refs.
pub(crate) fn merge_base(root: &Path, a: &str, b: &str) -> Result<String, String> {
    git_stdout(root, &["merge-base", a, b]).map(|s| s.trim().to_string())
}

/// Run `git diff --name-status` and return the paths with their status.
/// `spec` is either "A B" or "A...B".
pub(crate) fn changed_paths_status(root: &Path, spec: &str) -> Result<Vec<(String, char)>, String> {
    let parts: Vec<&str> = spec.split_whitespace().collect();
    let out = if parts.len() == 1 && parts[0].contains("...") {
        git_stdout(root, &["diff", "--name-status", parts[0]])?
    } else {
        let mut argv = vec!["diff", "--name-status"];
        argv.extend(parts);
        git_stdout(root, &argv)?
    };
    parse_name_status(&out)
}

/// Convenience wrapper returning only the distinct paths from a two-ref spec.
pub(crate) fn changed_paths(root: &Path, a: &str, b: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for (p, _) in changed_paths_status(root, &format!("{a} {b}"))? {
        out.push(p);
    }
    Ok(out)
}

/// Parse `git diff --name-status` output into (path, status) pairs.
///
/// A rename or copy prints three tab-separated fields -- `R100`, the old path,
/// then the new one -- and both paths are kept. The old path is what the left
/// ref has and the new path is what the right ref has, so keeping only one of
/// them would drop that file from one side of the comparison entirely.
pub(crate) fn parse_name_status(out: &str) -> Result<Vec<(String, char)>, String> {
    let mut v = Vec::new();
    for line in out.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        // `M\tpath`, or `R100\told\tnew` for a rename. A line with no tab at all
        // is read the way this always read one: status letter, then the path.
        let (code, rest) = match line.split_once('\t') {
            Some(pair) => pair,
            None => line.split_at(1),
        };
        let status = code.trim().chars().next().unwrap_or('?');
        let mut found = 0;
        for p in rest.split('\t') {
            let p = p.trim_start();
            if p.is_empty() {
                continue;
            }
            v.push((p.to_string(), status));
            found += 1;
        }
        if found == 0 {
            return Err(format!("malformed --name-status line: '{line}'"));
        }
    }
    Ok(v)
}

/// Create a unique temp directory for this meld invocation.
pub(crate) fn temp_meld_dir() -> Result<PathBuf, String> {
    let pid = std::process::id();
    let base = std::env::temp_dir();
    for n in 0..1000 {
        let candidate = base.join(format!("git-wt-meld-{pid}-{n}"));
        if std::fs::create_dir(&candidate).is_ok() {
            return Ok(candidate);
        }
    }
    Err("could not create a temporary directory for meld".into())
}

/// Extract the given paths from a ref into a directory. Paths that do not exist
/// in the ref are silently skipped, which naturally represents adds and deletes.
pub(crate) fn extract_files(root: &Path, r#ref: &str, paths: &[String], dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("failed to create temp dir: {e}"))?;
    for p in paths {
        let Ok(content) = git_bytes(root, &["show", &format!("{}:{}", r#ref, p)]) else {
            continue;
        };
        let target = dir.join(p);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("failed to create {parent:?}: {e}"))?;
        }
        std::fs::write(&target, content)
            .map_err(|e| format!("failed to write {target:?}: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_status_keeps_both_sides_of_a_rename() {
        // What `git diff --name-status` actually prints for a rename: a
        // similarity score glued to the status letter, then old and new paths.
        let out = "M\tkeep.txt\nR075\told.txt\tnew.txt\n";
        let v = parse_name_status(out).unwrap();
        assert_eq!(
            v,
            vec![
                ("keep.txt".to_string(), 'M'),
                ("old.txt".to_string(), 'R'),
                ("new.txt".to_string(), 'R'),
            ]
        );
        // The score is part of the status field, never part of a path -- the
        // bug this guards against extracted "075\told.txt\tnew.txt" as one path
        // and then silently dropped the file from both meld panes.
        assert!(v.iter().all(|(p, _)| !p.contains('\t') && !p.starts_with('0')));
    }

    #[test]
    fn name_status_reads_the_ordinary_statuses() {
        let out = "A\tadded.rs\nD\tgone.rs\nM\tsrc/a b.rs\nC100\tfrom.rs\tto.rs\n";
        let v = parse_name_status(out).unwrap();
        assert_eq!(
            v,
            vec![
                ("added.rs".to_string(), 'A'),
                ("gone.rs".to_string(), 'D'),
                // A space in a path is not a field separator; only tabs are.
                ("src/a b.rs".to_string(), 'M'),
                // A copy names both too: the source is in each ref, the
                // destination only in the newer one.
                ("from.rs".to_string(), 'C'),
                ("to.rs".to_string(), 'C'),
            ]
        );
    }

    #[test]
    fn name_status_skips_blanks_and_rejects_a_pathless_line() {
        assert!(parse_name_status("\n\n").unwrap().is_empty());
        assert!(parse_name_status("M\t\n").is_err());
        assert!(parse_name_status("M").is_err());
    }
}
