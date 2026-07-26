pub(crate) mod args;

use std::io::IsTerminal;
use std::path::Path;

use crate::cmd::compare::args::CompareArgs;
use crate::cmd::meld::{require_meld, temp_meld_dir};
use crate::git::{git_bytes, git_cmd, git_quiet};
use crate::ui::{color_enabled, paint, GREEN};

/// Compare one or more files in `cwd` against `args.ref`.
///
/// The ref is `-x/--reference` (alias `--ref`), `ExtraRef` -- a branch,
/// worktree number, or commit sha, not the global `-b/--branch` worktree
/// list. `ExtraRef` is list-shaped and repeatable elsewhere, but `compare`
/// only ever diffs against one thing, so "exactly one" is checked here
/// rather than left to clap's `required`. `files` is the shared `PathFilter`
/// (`-p/--path`), likewise not `required` -- "at least one" is checked here.
pub(crate) fn cmd_compare(cwd: &Path, args: &CompareArgs) -> Result<(), String> {
    let files = &args.files.filename;
    if files.is_empty() {
        return Err("compare needs at least one file: '-p/--path <FILE_LIST>'".into());
    }
    if files.iter().any(|f| f.trim().is_empty()) {
        return Err(format!(
            "bad file list '{}'; want paths, e.g. 'a.rs,b.rs'",
            files.join(",")
        ));
    }

    let r#ref = match args.r#ref.extra_ref.as_slice() {
        [r] => r.as_str(),
        [] => return Err("compare needs a ref: '-x/--reference <REF>' (or '--ref')".into()),
        refs => {
            return Err(format!(
                "compare takes exactly one ref, got {}: '{}'",
                refs.len(),
                refs.join(", ")
            ))
        }
    };
    if !git_quiet(cwd, &["rev-parse", "--verify", "-q", &format!("{ref}^{{commit}}")]) {
        return Err(format!("no such ref '{ref}'"));
    }

    for f in files {
        if !cwd.join(f).is_file() {
            return Err(format!("no such file '{f}' (relative to {})", cwd.display()));
        }
    }

    if !args.meld.meld {
        let status = git_cmd(cwd, &[])
            .arg("diff")
            .arg(r#ref)
            .arg("--")
            .args(files)
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
        for f in files {
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
