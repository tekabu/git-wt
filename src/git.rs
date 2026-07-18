use std::path::Path;
use std::process::{Command, Stdio};

// ---------------------------------------------------------------------------
// git plumbing
// ---------------------------------------------------------------------------

pub(crate) fn git_cmd(dir: &Path, args: &[&str]) -> Command {
    let mut c = Command::new("git");
    c.current_dir(dir).args(args);
    c
}

/// Run git, streaming its output through. Errors carry git's stderr.
pub(crate) fn git_run(dir: &Path, args: &[&str]) -> Result<(), String> {
    let out = git_cmd(dir, args)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    // git's own progress text belongs on stderr, not in our stdout contract.
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        eprintln!("{line}");
    }

    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Run git with the editor disabled. We capture git's output, so a spawned
/// editor would have no terminal and hang; instead git's default commit message
/// is taken as-is (`-m` is how a user overrides it).
pub(crate) fn git_run_no_editor(dir: &Path, args: &[&str]) -> Result<(), String> {
    let out = git_cmd(dir, args)
        .env("GIT_EDITOR", "true")
        .env("GIT_MERGE_AUTOEDIT", "no")
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        eprintln!("{line}");
    }

    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub(crate) fn git_stdout(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = git_cmd(dir, args)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Like `git_stdout` but preserves bytes (for binary files).
pub(crate) fn git_bytes(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = git_cmd(dir, args)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// True when git exits 0. Used for ref existence checks.
pub(crate) fn git_quiet(dir: &Path, args: &[&str]) -> bool {
    git_cmd(dir, args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Is `cmd` an executable file on PATH? Checked before spawning so a missing
/// tool is a clear error rather than an opaque NotFound from the OS.
pub(crate) fn on_path(cmd: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let p = dir.join(cmd);
        p.is_file() && is_executable(&p)
    })
}

#[cfg(unix)]
pub(crate) fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
pub(crate) fn is_executable(_p: &Path) -> bool {
    true
}
