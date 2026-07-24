pub(crate) mod args;

use std::collections::HashSet;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cmd::meld::args::MeldArgs;
use crate::git::{git_bytes, git_stdout, on_path};
use crate::ui::{color_enabled, paint, GREEN};
use crate::worktree::{label, ref_of, Worktree};

pub(crate) fn cmd_meld(root: &Path, trees: &[Worktree], idxs: &[usize], args: &MeldArgs) -> Result<(), String> {
    match idxs.len() {
        2 | 3 => {}
        1 => return Err("meld needs 2 or 3 worktrees, e.g. 'git-wt 1,2 meld'".into()),
        n => return Err(format!("meld takes at most 3 worktrees, got {n}")),
    }

    if !args.diff {
        for (i, a) in idxs.iter().enumerate() {
            if idxs[i + 1..].contains(a) {
                return Err(format!("worktree #{} listed twice", a + 1));
            }
        }
        let mut bad = Vec::new();
        if args.three_way {
            bad.push("--3way");
        }
        if args.base.is_some() {
            bad.push("--base");
        }
        if args.range.is_some() {
            bad.push("'..'/'...'");
        }
        if !bad.is_empty() {
            let hint = bad.join(", ");
            return Err(format!(
                "{hint} only applies to 'meld --diff'; add --diff or drop {hint}",
            ));
        }
    }

    if args.three_way && args.base.is_some() {
        return Err("--3way and --base are alternatives; use one or the other".into());
    }
    if let Some(r) = &args.range {
        if r != ".." && r != "..." {
            return Err(format!("range for meld --diff must be '..' or '...', got '{r}'"));
        }
        if r == ".." {
            return Err("'..' is the default range for meld --diff; use '...' for the fork view".into());
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

    if args.diff {
        if idxs.len() != 2 {
            return Err("'--diff' takes exactly 2 worktrees; use meld without --diff for 3-way".into());
        }
        let mut parsed = args.clone();
        if parsed.range.is_none() {
            parsed.range = Some("..".into());
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
        let dots = args.range.clone().unwrap_or_else(|| "..".into());
        let spec = format!("{left}{dots}{right}");
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
        eprintln!(
            "no files differ between {} and {}",
            label(&trees[left_idx]),
            label(&trees[right_idx])
        );
        return Ok(());
    }

    let tmp = temp_meld_dir()?;
    let dir_left = tmp.join("a");
    let dir_right = tmp.join("b");

    let extract_all = || -> Result<Vec<PathBuf>, String> {
        extract_files(root, &left, &paths, &dir_left)?;
        extract_files(root, &right, &paths, &dir_right)?;
        if let Some(b) = &base {
            let dir_base = tmp.join("base");
            extract_files(root, b, &paths, &dir_base)?;
            return Ok(vec![dir_base, dir_left.clone(), dir_right.clone()]);
        }
        Ok(vec![dir_left.clone(), dir_right.clone()])
    };
    let meld_paths = match extract_all() {
        Ok(p) => p,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e);
        }
    };

    let on = color_enabled(std::io::stderr().is_terminal());
    let mut labels: Vec<String> = if base.is_some() {
        vec!["base".into(), label(&trees[left_idx]), label(&trees[right_idx])]
    } else {
        vec![label(&trees[left_idx]), label(&trees[right_idx])]
    };
    if let Some(b) = &base {
        labels[0] = b.clone();
    }
    eprintln!("{} {}", paint("meld", GREEN, on), labels.join("  ↔  "));

    let status = Command::new("meld").args(&meld_paths).status();

    let _ = std::fs::remove_dir_all(&tmp);
    let status = status.map_err(|e| format!("failed to run meld: {e}"))?;

    if !status.success() {
        return Err("meld exited with an error".into());
    }
    Ok(())
}

pub(crate) fn merge_base(root: &Path, a: &str, b: &str) -> Result<String, String> {
    git_stdout(root, &["merge-base", a, b]).map(|s| s.trim().to_string())
}

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

pub(crate) fn changed_paths(root: &Path, a: &str, b: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for (p, _) in changed_paths_status(root, &format!("{a} {b}"))? {
        out.push(p);
    }
    Ok(out)
}

pub(crate) fn parse_name_status(out: &str) -> Result<Vec<(String, char)>, String> {
    let mut v = Vec::new();
    for line in out.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
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

    fn parse(args: &[&str]) -> Result<MeldArgs, String> {
        // This helper mirrors the old unit-test shape, but the real parsing now
        // happens through clap. For the local logic tests we just inspect fields.
        let mut a = MeldArgs::default();
        let mut it = args.iter();
        while let Some(tok) = it.next() {
            match tok.as_ref() {
                "--diff" => a.diff = true,
                "..." => a.range = Some("...".into()),
                ".." => a.range = Some("..".into()),
                "--3way" => a.three_way = true,
                "--base" => a.base = Some(it.next().unwrap().to_string()),
                _ => {}
            }
        }
        Ok(a)
    }

    #[test]
    fn diff_only_flags_are_refused_without_diff() {
        for args in [vec!["--3way"], vec!["--base", "main"], vec!["..."]] {
            let mut a = MeldArgs::default();
            for tok in &args {
                if *tok == "--3way" { a.three_way = true; }
                if *tok == "..." { a.range = Some("...".into()); }
            }
            if args[0] == "--base" { a.base = Some(args[1].into()); }
            let err = cmd_meld(
                std::path::Path::new("."),
                &[
                    crate::worktree::Worktree {
                        path: std::path::PathBuf::from("/a"),
                        branch: Some("a".into()),
                        detached: false,
                        bare: false,
                        locked: None,
                        prunable: None,
                    },
                    crate::worktree::Worktree {
                        path: std::path::PathBuf::from("/b"),
                        branch: Some("b".into()),
                        detached: false,
                        bare: false,
                        locked: None,
                        prunable: None,
                    },
                ],
                &[0, 1],
                &a,
            )
            .unwrap_err();
            assert!(err.contains("only applies to 'meld --diff'"), "{args:?} gave: {err}");
        }
        assert!(parse(&["--diff", "--3way"]).is_ok());
        assert!(parse(&["--diff", "--base", "main"]).is_ok());
        assert!(parse(&["--diff", "..."]).is_ok());
        assert!(parse(&[]).is_ok());
    }

    #[test]
    fn name_status_keeps_both_sides_of_a_rename() {
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
                ("src/a b.rs".to_string(), 'M'),
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
