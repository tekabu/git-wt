use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::git::{git_quiet, git_run, git_stdout};
use crate::ui::{color_enabled, confirm, paint, DIM, GREEN};
use crate::worktree::{current_ref, leaf_of, sanitize, sh_quote, worktrees};

// ---------------------------------------------------------------------------
// Create: git-wt add [BRANCH] [flags]
// ---------------------------------------------------------------------------

pub(crate) fn cmd_add(root: &Path, args: &[String]) -> Result<(), String> {
    let mut branch: Option<String> = None;
    let mut name: Option<String> = None;
    let mut dirname: Option<String> = None;
    let mut parentdir: Option<String> = None;
    let mut from: Option<String> = None;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-n" | "--name" => {
                name = Some(it.next().ok_or("--name needs a name")?.clone());
            }
            "--dirname" => {
                dirname = Some(it.next().ok_or("--dirname needs a directory")?.clone());
            }
            "-p" | "--parentdir" => {
                parentdir = Some(it.next().ok_or("--parentdir needs a directory")?.clone());
            }
            "--from" => {
                from = Some(it.next().ok_or("--from needs a ref")?.clone());
            }
            s if s.starts_with("--name=") => name = Some(s["--name=".len()..].to_string()),
            s if s.starts_with("--dirname=") => {
                dirname = Some(s["--dirname=".len()..].to_string())
            }
            s if s.starts_with("--parentdir=") => {
                parentdir = Some(s["--parentdir=".len()..].to_string())
            }
            s if s.starts_with("--from=") => from = Some(s["--from=".len()..].to_string()),
            // A hint for the `wt` wrapper (stay put instead of cd'ing into the
            // new worktree). The binary never cd's, so it just accepts it.
            "--stay" => {}
            s if s.starts_with('-') && s != "-" => {
                return Err(format!("unknown option '{s}'\nTry 'git-wt --help'"));
            }
            s => {
                if branch.is_some() {
                    return Err("too many arguments\nTry 'git-wt --help'".into());
                }
                branch = Some(s.to_string());
            }
        }
    }

    if name.is_some() && dirname.is_some() {
        return Err("--name and --dirname conflict".into());
    }
    if let Some(n) = &name {
        if n.is_empty() {
            return Err("--name cannot be empty".into());
        }
    }
    if let Some(d) = &dirname {
        if d.is_empty() {
            return Err("--dirname cannot be empty".into());
        }
    }

    // No branch -> interactive picker over local branches.
    let branch = match branch {
        Some(b) => b,
        None => pick_branch(root)?,
    };

    let dir = match resolve_add_path(
        root,
        &branch,
        name.as_deref(),
        dirname.as_deref(),
        parentdir.as_deref(),
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

    // Refuse to point a new worktree at a branch already checked out; git
    // shares one ref between worktrees, so the two HEADs would drift.
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

    // --from only affects creating a NEW branch; if the branch already exists
    // it is silently overridden, so warn + confirm.
    if from.is_some() && (has_local || remote.is_some()) {
        if !confirm(&format!(
            "branch '{branch}' already exists; --from ignored. Continue? [y/N] "
        ))? {
            eprintln!("Aborted.");
            return Ok(());
        }
    }

    // Default base for a NEW branch is the ref checked out where the user is
    // standing (the current worktree), not the primary's HEAD. `--from` wins.
    let default_from = current_ref();
    let from_ref = from.as_deref().unwrap_or(&default_from);
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

    // One-line summary on stderr (never stdout) so interactive users get
    // context without polluting the captured path.
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

    // Print the new worktree path on stdout (alone) so scripts can capture it:
    // `dir=$(git-wt add feat/x)`. Status/progress went to stderr.
    println!("{dir_s}");
    Ok(())
}

/// Find a remote whose tracking ref `<remote>/<branch>` exists, so `add`
/// works with any remote name (not just `origin`). Prefers `origin`; otherwise
/// the first configured remote that has the branch.
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

/// Resolve the worktree directory for `add`. Returns `Ok(None)` when the user
/// declines a warn-and-confirm (an override), which the caller treats as an
/// abort rather than an error.
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

    // --dirname with a '/' is a path: sanitize skipped, -p ignored.
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

// ---------------------------------------------------------------------------
// Branch picker (no BRANCH given to `add`)
// ---------------------------------------------------------------------------

/// Choose a local branch interactively. Prefers fzf's search filter; falls
/// back to a numbered prompt when fzf is not installed.
///
/// Branches already checked out in a worktree can't be added again, so they are
/// dropped from the selectable list and shown separately, for reference.
pub(crate) fn pick_branch(root: &Path) -> Result<String, String> {
    // Sort recently-committed branches to the top so the picker surfaces what
    // you're likely reaching for. Fetch each branch's relative age in the same
    // call (tab-delimited) so the numbered picker needs no per-branch git log.
    let out = git_stdout(
        root,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname:short)%09%(committerdate:relative)",
            "refs/heads",
        ],
    )?;
    // Each line is "<branch>\t<age>"; a missing tab leaves the age empty.
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
        // Separator between the reference list and the selectable choices.
        eprintln!("{}", "─".repeat(48));
    }

    if selectable.is_empty() {
        // Every local branch is checked out; rather than dead-end, offer to
        // create a new branch by name (cmd_add then confirms the base ref).
        return new_branch_prompt();
    }

    let names: Vec<&str> = selectable.iter().map(|(b, _)| *b).collect();
    if let Some(sel) = fzf_pick(root, &names)? {
        return Ok(sel);
    }
    number_pick(&selectable)
}

/// Empty-state fallback: no branch is available to check out, so read a new
/// branch name to create. Empty input / EOF cancels.
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

/// Run fzf over `items`. Returns Ok(None) when fzf is not on PATH so the caller
/// can fall back; an empty/aborted selection is an error.
pub(crate) fn fzf_pick(root: &Path, items: &[&str]) -> Result<Option<String>, String> {
    // Preview the highlighted branch's last commit. fzf shell-quotes {} before
    // substitution, and root is quoted here, so both are safe in `sh -c`.
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
        Err(_) => return Ok(None), // fzf not available
    };

    child
        .stdin
        .as_mut()
        .ok_or("fzf: no stdin")?
        .write_all(items.join("\n").as_bytes())
        .map_err(|e| e.to_string())?;

    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("no branch selected".into()); // ESC / Ctrl-C in fzf
    }
    let sel = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if sel.is_empty() {
        return Err("no branch selected".into());
    }
    Ok(Some(sel))
}

/// Numbered fallback picker; reads a number from stdin. Each branch is shown
/// with the relative age of its last commit (dimmed on a terminal) for context.
/// `items` are `(branch, age)` pairs already gathered by `pick_branch`.
pub(crate) fn number_pick(items: &[(&str, &str)]) -> Result<String, String> {
    let color = color_enabled(std::io::stderr().is_terminal());
    eprintln!("Available branches (most recent first):");
    let w = items.len().to_string().len();
    let bw = items.iter().map(|(b, _)| b.chars().count()).max().unwrap_or(0);
    for (i, (b, age)) in items.iter().enumerate() {
        let meta = paint(age, DIM, color && !age.is_empty());
        eprintln!("  {:>w$}  {:<bw$}  {}", i + 1, b, meta, w = w, bw = bw);
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
        let p =
            resolve_add_path(Path::new("/code/myapp"), "feat/x", None, Some("/tmp/scratch"), None)
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
