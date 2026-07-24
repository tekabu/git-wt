pub(crate) mod args;

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::cmd::add::args::AddArgs;
use crate::git::{git_quiet, git_run, git_stdout};
use crate::ui::{color_enabled, confirm, paint, DIM, GREEN};
use crate::worktree::{current_ref, leaf_of, sanitize, sh_quote, worktrees};

/// Create a new worktree in a sibling directory.
pub(crate) fn cmd_add(root: &Path, args: AddArgs) -> Result<(), String> {
    if args.name.is_some() && args.dirname.is_some() {
        return Err("--name and --dirname conflict".into());
    }
    if let Some(n) = &args.name {
        if n.is_empty() {
            return Err("--name cannot be empty".into());
        }
    }
    if let Some(d) = &args.dirname {
        if d.is_empty() {
            return Err("--dirname cannot be empty".into());
        }
    }

    let branch = match args.branch_name {
        Some(b) => b,
        None => pick_branch(root)?,
    };

    let dir = match resolve_add_path(
        root,
        &branch,
        args.name.as_deref(),
        args.dirname.as_deref(),
        args.parentdir.as_deref(),
    )? {
        Some(d) => d,
        None => {
            eprintln!("Aborted.");
            return Ok(());
        }
    };

    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }

    if let Some(w) = worktrees(root)?
        .into_iter()
        .find(|w| w.branch.as_deref() == Some(branch.as_str()))
    {
        return Err(format!(
            "branch '{branch}' already checked out at {}",
            w.path.display()
        ));
    }

    let has_local = git_quiet(root, &["show-ref", "--verify", &format!("refs/heads/{branch}")]);
    let remote = find_remote_branch(root, &branch);

    if args.from.is_some()
        && (has_local || remote.is_some())
        && !confirm(&format!(
            "branch '{branch}' already exists; --from ignored. Continue? [y/N] "
        ))?
    {
        eprintln!("Aborted.");
        return Ok(());
    }

    let default_from = current_ref();
    let from_ref = args.from.as_deref().unwrap_or(&default_from);
    let dir_s = dir.to_string_lossy().to_string();
    let mut argv: Vec<String> = vec!["worktree".into(), "add".into()];

    if has_local {
        eprintln!("Checking out existing local branch '{branch}'");
        argv.push(dir_s.clone());
        argv.push(branch.clone());
    } else if let Some(r) = &remote {
        eprintln!("Tracking remote branch '{r}/{branch}'");
        argv.extend(["--track".into(), "-b".into(), branch.clone()]);
        argv.push(dir_s.clone());
        argv.push(format!("{r}/{branch}"));
    } else {
        if !confirm(&format!(
            "Branch '{branch}' does not exist. Create it from '{from_ref}'? [y/N] "
        ))? {
            eprintln!("Aborted.");
            return Ok(());
        }
        eprintln!("Creating new branch '{branch}' from '{from_ref}'");
        argv.extend(["-b".into(), branch.clone()]);
        argv.push(dir_s.clone());
        argv.push(from_ref.into());
    }

    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    git_run(root, &refs)?;

    let summary = if has_local {
        format!("branch {branch}")
    } else if let Some(r) = &remote {
        format!("branch {branch} tracking {r}/{branch}")
    } else {
        format!("branch {branch} from {from_ref}")
    };
    let leaf = leaf_of(&dir);
    let on = color_enabled(std::io::stderr().is_terminal());
    eprintln!("{} {leaf}  ({summary})", paint("Created", GREEN, on));

    println!("{dir_s}");
    Ok(())
}

/// Find a remote whose tracking ref `<remote>/<branch>` exists.
pub(crate) fn find_remote_branch(root: &Path, branch: &str) -> Option<String> {
    let has = |r: &str| {
        git_quiet(
            root,
            &["show-ref", "--verify", &format!("refs/remotes/{r}/{branch}")],
        )
    };
    if has("origin") {
        return Some("origin".into());
    }
    git_stdout(root, &["remote"])
        .ok()?
        .lines()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .find(|r| has(r))
        .map(String::from)
}

/// Resolve the worktree directory for `add`.
pub(crate) fn resolve_add_path(
    root: &Path,
    branch: &str,
    name: Option<&str>,
    dirname: Option<&str>,
    parentdir: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    let repo = root
        .file_name()
        .ok_or("cannot determine repo folder name")?
        .to_string_lossy()
        .to_string();
    let default_parent = root.parent().ok_or("repo root has no parent directory")?;

    if let Some(d) = dirname {
        if d.contains('/') {
            if parentdir.is_some()
                && !confirm(
                    "--parentdir ignored because --dirname is a path. Continue? [y/N] ",
                )?
            {
                return Ok(None);
            }
            let p = Path::new(d);
            if p.is_absolute() {
                return Ok(Some(p.to_path_buf()));
            }
            return Ok(Some(default_parent.join(p)));
        }
    }

    let parent = match parentdir {
        Some(p) => PathBuf::from(p),
        None => default_parent.to_path_buf(),
    };
    let leaf = match (name, dirname) {
        (Some(n), _) => format!("{repo}-{}", sanitize(n)),
        (_, Some(d)) => sanitize(d),
        _ => format!("{repo}-{}", sanitize(branch)),
    };
    Ok(Some(parent.join(leaf)))
}

/// Choose a local branch interactively.
pub(crate) fn pick_branch(root: &Path) -> Result<String, String> {
    let out = git_stdout(
        root,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname:short)%09%(committerdate:relative)",
            "refs/heads",
        ],
    )?;
    let branches: Vec<(&str, &str)> = out
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.split_once('\t').unwrap_or((l, "")))
        .collect();
    if branches.is_empty() {
        return Err("no local branches to choose from".into());
    }

    let trees = worktrees(root)?;
    let mut selectable: Vec<(&str, &str)> = Vec::new();
    let mut checked_out: Vec<(&str, &Path)> = Vec::new();
    for (b, age) in &branches {
        match trees.iter().find(|w| w.branch.as_deref() == Some(*b)) {
            Some(w) => checked_out.push((*b, w.path.as_path())),
            None => selectable.push((*b, *age)),
        }
    }

    if !checked_out.is_empty() {
        eprintln!("Already checked out (not selectable):");
        let w = checked_out
            .iter()
            .map(|(b, _)| b.chars().count())
            .max()
            .unwrap_or(0);
        for (b, p) in &checked_out {
            eprintln!("  {:<w$}  {}", b, p.display(), w = w);
        }
        eprintln!("{}", "─".repeat(48));
    }

    if selectable.is_empty() {
        return new_branch_prompt();
    }

    let names: Vec<&str> = selectable.iter().map(|(b, _)| *b).collect();
    if let Some(sel) = fzf_pick(root, &names)? {
        return Ok(sel);
    }
    number_pick(&selectable)
}

/// Empty-state fallback: read a new branch name to create.
pub(crate) fn new_branch_prompt() -> Result<String, String> {
    eprintln!("All local branches are already checked out.");
    eprint!("Enter a new branch name to create (Enter to cancel): ");
    std::io::stderr().flush().ok();
    let mut line = String::new();
    let n = std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("no branch selected".into());
    }
    let name = line.trim();
    if name.is_empty() {
        return Err("no branch selected".into());
    }
    Ok(name.to_string())
}

/// Run fzf over `items`.
pub(crate) fn fzf_pick(root: &Path, items: &[&str]) -> Result<Option<String>, String> {
    let preview = format!(
        "git -C {} log -1 --format='%h  %s%n%an · %ar' {{}} --",
        sh_quote(root)
    );
    let mut child = match Command::new("fzf")
        .args([
            "--prompt",
            "branch> ",
            "--height",
            "40%",
            "--reverse",
            "--preview",
            &preview,
            "--preview-window",
            "down,3,wrap",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };

    child
        .stdin
        .as_mut()
        .ok_or("fzf: no stdin")?
        .write_all(items.join("\n").as_bytes())
        .map_err(|e| e.to_string())?;

    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("no branch selected".into());
    }
    let sel = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if sel.is_empty() {
        return Err("no branch selected".into());
    }
    Ok(Some(sel))
}

/// Numbered fallback picker.
pub(crate) fn number_pick(items: &[(&str, &str)]) -> Result<String, String> {
    let color = color_enabled(std::io::stderr().is_terminal());
    eprintln!("Available branches (most recent first):");
    let w = items.len().to_string().len();
    let bw = items.iter().map(|(b, _)| b.chars().count()).max().unwrap_or(0);
    for (i, (b, age)) in items.iter().enumerate() {
        let meta = paint(age, DIM, color && !age.is_empty());
        eprintln!(
            "  {:>w$}  {:<bw$}  {}",
            i + 1,
            b,
            meta,
            w = w,
            bw = bw
        );
    }
    eprint!("Select a branch [1-{}], or Enter to cancel: ", items.len());
    std::io::stderr().flush().ok();

    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let t = line.trim();
    if t.is_empty() {
        return Err("no branch selected".into());
    }
    let n: usize = t.parse().map_err(|_| format!("'{t}' is not a number"))?;
    if n == 0 || n > items.len() {
        return Err(format!("no branch #{n}"));
    }
    Ok(items[n - 1].0.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_path_default_is_sibling() {
        let p = resolve_add_path(Path::new("/code/myapp"), "feat/x", None, None, None)
            .unwrap()
            .unwrap();
        assert_eq!(p, PathBuf::from("/code/myapp-feat-x"));
    }

    #[test]
    fn add_path_name_is_suffix() {
        let p = resolve_add_path(Path::new("/code/myapp"), "feat/x", Some("test"), None, None)
            .unwrap()
            .unwrap();
        assert_eq!(p, PathBuf::from("/code/myapp-test"));
    }

    #[test]
    fn add_path_dirname_is_whole_leaf() {
        let p = resolve_add_path(Path::new("/code/myapp"), "feat/x", None, Some("test"), None)
            .unwrap()
            .unwrap();
        assert_eq!(p, PathBuf::from("/code/test"));
    }

    #[test]
    fn add_path_parentdir_overrides() {
        let p = resolve_add_path(Path::new("/code/myapp"), "feat/x", None, None, Some("/work"))
            .unwrap()
            .unwrap();
        assert_eq!(p, PathBuf::from("/work/myapp-feat-x"));
    }

    #[test]
    fn add_path_dirname_absolute_is_verbatim() {
        let p = resolve_add_path(
            Path::new("/code/myapp"),
            "feat/x",
            None,
            Some("/tmp/scratch"),
            None,
        )
        .unwrap()
        .unwrap();
        assert_eq!(p, PathBuf::from("/tmp/scratch"));
    }

    #[test]
    fn add_path_dirname_relative_path_is_parent_relative() {
        let p = resolve_add_path(Path::new("/code/myapp"), "feat/x", None, Some("sub/test"), None)
            .unwrap()
            .unwrap();
        assert_eq!(p, PathBuf::from("/code/sub/test"));
    }
}
