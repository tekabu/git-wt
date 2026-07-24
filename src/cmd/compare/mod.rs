pub(crate) mod args;

use std::io::IsTerminal;
use std::path::Path;

use crate::cmd::compare::args::CompareArgs;
use crate::cmd::meld::{require_meld, temp_meld_dir};
use crate::git::{git_bytes, git_cmd, git_quiet};
use crate::ui::{color_enabled, paint, GREEN};

/// Compare one or more files in `cwd` against `branch` or `args.commit`.
///
/// `branch` comes from the global `-b/--branch`, reused here (rather than a
/// compare-local `-b`) since a subcommand can't redefine a short flag the top
/// level already claims globally; the caller has already checked it holds at
/// most one plain branch name.
pub(crate) fn cmd_compare(cwd: &Path, args: &CompareArgs, branch: Option<&str>) -> Result<(), String> {
    let files: Vec<String> = args.files.split(',').map(str::to_string).collect();
    if files.iter().any(|f| f.is_empty()) {
        return Err(format!(
            "bad file list '{}'; want comma-separated paths, e.g. 'a.rs,b.rs'",
            args.files
        ));
    }

    let r#ref = match (branch, &args.commit) {
        (Some(b), None) => b.to_string(),
        (None, Some(c)) => c.clone(),
        (Some(_), Some(_)) => {
            return Err("'-b/--branch' and '-c/--commit' are alternatives; use one or the other".into());
        }
        (None, None) => {
            return Err("compare needs a ref: '-b/--branch NAME' or '-c/--commit SHA'".into());
        }
    };

    if !git_quiet(cwd, &["rev-parse", "--verify", "-q", &format!("{ref}^{{commit}}")]) {
        return Err(format!("no such branch or commit '{ref}'"));
    }

    for f in &files {
        if !cwd.join(f).is_file() {
            return Err(format!("no such file '{f}' (relative to {})", cwd.display()));
        }
    }

    if !args.meld {
        let status = git_cmd(cwd, &[])
            .arg("diff")
            .arg(&r#ref)
            .arg("--")
            .args(&files)
            .status()
            .map_err(|e| format!("failed to run git: {e}"))?;
        if !status.success() {
            return Err("git diff exited with an error".into());
        }
        return Ok(());
    }

    require_meld()?;

    let tmp = temp_meld_dir()?;
    let extract = || -> Result<Vec<String>, String> {
        let mut meld_args = Vec::new();
        for f in &files {
            let content = git_bytes(cwd, &["show", &format!("{ref}:{f}")]).unwrap_or_default();
            let target = tmp.join(f);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("failed to create {parent:?}: {e}"))?;
            }
            std::fs::write(&target, content).map_err(|e| format!("failed to write {target:?}: {e}"))?;
            meld_args.push("--diff".to_string());
            meld_args.push(cwd.join(f).to_string_lossy().to_string());
            meld_args.push(target.to_string_lossy().to_string());
        }
        Ok(meld_args)
    };
    let meld_args = match extract() {
        Ok(a) => a,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e);
        }
    };

    let on = color_enabled(std::io::stderr().is_terminal());
    eprintln!("{} {} ↔ {}", paint("compare", GREEN, on), r#ref, files.join(", "));

    let status = std::process::Command::new("meld").args(&meld_args).status();
    let _ = std::fs::remove_dir_all(&tmp);
    let status = status.map_err(|e| format!("failed to run meld: {e}"))?;
    if !status.success() {
        return Err("meld exited with an error".into());
    }
    Ok(())
}
