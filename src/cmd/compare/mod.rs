pub(crate) mod args;

use std::io::IsTerminal;
use std::path::Path;

use crate::cmd::compare::args::CompareArgs;
use crate::cmd::meld::{require_meld, temp_meld_dir};
use crate::git::{git_bytes, git_cmd, git_quiet};
use crate::ui::{color_enabled, paint, GREEN};

/// Compare one or more files in `cwd` against `args.ref`.
///
/// The ref is compare's own `-r/--ref`, not the global `-b/--branch`: it is a
/// single rev, and one that need not be a branch at all, so it shares nothing
/// with the worktree list `-b` names everywhere else. The caller rejects a
/// global `-b` here rather than letting it pass unused.
pub(crate) fn cmd_compare(cwd: &Path, args: &CompareArgs) -> Result<(), String> {
    let files: Vec<String> = args.files.split(',').map(str::to_string).collect();
    if files.iter().any(|f| f.is_empty()) {
        return Err(format!(
            "bad file list '{}'; want comma-separated paths, e.g. 'a.rs,b.rs'",
            args.files
        ));
    }

    let r#ref = args.r#ref.as_str();
    if !git_quiet(cwd, &["rev-parse", "--verify", "-q", &format!("{ref}^{{commit}}")]) {
        return Err(format!("no such ref '{ref}'"));
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
