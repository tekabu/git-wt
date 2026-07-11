//! git-wt — create git worktrees in a sibling directory named
//! `<repo-folder>-<sanitized-branch>`.
//!
//! Installed on PATH as `git-wt`, so it is also reachable as `git wt`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
git-wt — worktrees in sibling directories named <repo>-<branch>

USAGE:
    git-wt <branch> [base-ref]     Create a worktree for <branch>
    git-wt create <branch> [base]  Create, explicit form (branch may be 'list' etc.)
    git-wt list                    List worktrees of this repo
    git-wt remove <target>         Remove a worktree (by branch or path)
    git-wt --help
    git-wt --version

    Subcommands list/remove have flag equivalents --list/--remove.
    Aliases: ls = list, rm = remove.

OPTIONS:
    -l, --list             List worktrees, with branch and path
    -r, --remove <target>  Remove the worktree for a branch name or path
    -n, --name <dir>       Override the worktree directory name
    -d, --detach           Check out detached, not on the branch
    -f, --force            Create: allow a branch already checked out elsewhere
                           Remove: discard uncommitted changes
    -h, --help             Show this help
    -V, --version          Show version

CREATE:
    The worktree directory is a sibling of the repo root, named
    <repo-folder>-<branch>, with '/', ' ' and ':' collapsed to '-'.

        ~/code/myapp  +  feature/login  ->  ~/code/myapp-feature-login

    Branch resolution, in order:
      1. Local branch exists          -> check it out
      2. origin/<branch> exists       -> create a tracking branch from it
      3. Neither                      -> create it from [base-ref] (default HEAD)

    --name overrides the directory. A bare name is still a sibling of the
    repo root; a name containing '/' is used as a path as given.

        git-wt feature/login --name myapp-review
        git-wt feature/login --name /tmp/scratch

SAME BRANCH, SECOND FOLDER:
    git refuses to check out one branch in two worktrees, because both would
    share a single ref: committing in one leaves the other's HEAD stale.

    Prefer --detach for a read-only second copy. HEAD sits at the same commit
    but on no branch, so nothing is shared and nothing can drift:

        git-wt feature/login --name myapp-review --detach

    Use --force only if you truly want two live checkouts of one branch, and
    expect to commit from just one of them:

        git-wt feature/login --name myapp-hotfix --force

REMOVE:
    Removes the worktree directory and prunes git's admin data.
    The branch itself is left alone. Refuses to remove a worktree with
    uncommitted changes unless --force is given. When one branch has several
    worktrees the branch name is ambiguous, so pass a path instead.

EXIT:
    On success, create prints the worktree path, alone, on stdout, so a
    shell wrapper can cd into it. Status text goes to stderr.

        wt() { local d; d=\"$(git-wt \"$@\")\" || return; cd \"$d\"; }
";

fn main() {
    let code = match run() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    };
    std::process::exit(code);
}

/// A worktree as reported by `git worktree list --porcelain`.
struct Worktree {
    path: PathBuf,
    /// Short branch name, or None when detached/bare.
    branch: Option<String>,
    detached: bool,
    bare: bool,
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() {
        print!("{HELP}");
        return Ok(());
    }

    let mut force = false;
    let mut detach = false;
    let mut name: Option<String> = None;
    let mut remove_target: Option<String> = None;
    let mut list = false;
    let mut remove_mode = false;
    let mut positional: Vec<String> = Vec::new();

    // A leading bare word may be a subcommand. `create` is the escape hatch
    // for a branch whose name collides with a subcommand ('git-wt create list').
    let mut rest = &args[..];
    match args[0].as_str() {
        "list" | "ls" => {
            list = true;
            rest = &args[1..];
        }
        "remove" | "rm" => {
            remove_mode = true;
            rest = &args[1..];
        }
        "create" => {
            rest = &args[1..];
        }
        _ => {}
    }

    let mut iter = rest.iter().peekable();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(());
            }
            "-V" | "--version" => {
                println!("git-wt {VERSION}");
                return Ok(());
            }
            "-l" | "--list" => list = true,
            "-f" | "--force" => force = true,
            "-d" | "--detach" => detach = true,
            "-r" | "--remove" => {
                let t = iter.next().ok_or("--remove needs a branch name or path")?;
                remove_target = Some(t.clone());
            }
            "-n" | "--name" => {
                let t = iter.next().ok_or("--name needs a directory name")?;
                name = Some(t.clone());
            }
            s if s.starts_with("--remove=") => {
                remove_target = Some(s["--remove=".len()..].to_string());
            }
            s if s.starts_with("--name=") => {
                name = Some(s["--name=".len()..].to_string());
            }
            s if s.starts_with('-') && s != "-" => {
                return Err(format!("unknown option: {s}\nTry 'git-wt --help'"));
            }
            s => positional.push(s.to_string()),
        }
    }

    // `remove <target>` takes its target as a positional.
    if remove_mode {
        match positional.len() {
            0 => return Err("remove needs a branch name or path".into()),
            1 => remove_target = Some(positional.remove(0)),
            _ => return Err("too many arguments\nTry 'git-wt --help'".into()),
        }
    }

    if list && remove_target.is_some() {
        return Err("list and remove are mutually exclusive".into());
    }

    // Every mode below needs to be inside a repo.
    let root = repo_root()?;

    if list {
        if name.is_some() || detach {
            return Err("--list takes no --name/--detach".into());
        }
        return cmd_list(&root);
    }
    if let Some(target) = remove_target {
        if name.is_some() || detach {
            return Err("--remove takes no --name/--detach".into());
        }
        return cmd_remove(&root, &target, force);
    }

    let opts = CreateOpts {
        name,
        detach,
        force,
    };
    match positional.len() {
        0 => Err("no branch given\nTry 'git-wt --help'".into()),
        1 => cmd_create(&root, &positional[0], "HEAD", &opts),
        2 => cmd_create(&root, &positional[0], &positional[1], &opts),
        _ => Err("too many arguments\nTry 'git-wt --help'".into()),
    }
}

struct CreateOpts {
    name: Option<String>,
    detach: bool,
    force: bool,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn cmd_create(root: &Path, branch: &str, base: &str, opts: &CreateOpts) -> Result<(), String> {
    let dir = match &opts.name {
        Some(n) => custom_path(root, n)?,
        None => worktree_path(root, branch)?,
    };

    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }

    let has_local = git_quiet(root, &["show-ref", "--verify", &format!("refs/heads/{branch}")]);
    let has_remote = git_quiet(
        root,
        &["show-ref", "--verify", &format!("refs/remotes/origin/{branch}")],
    );

    // git would reject this itself, but with a worse message than we can give.
    if !opts.detach && !opts.force {
        if let Some(w) = worktrees(root)?
            .into_iter()
            .find(|w| w.branch.as_deref() == Some(branch))
        {
            return Err(format!(
                "branch '{branch}' is already checked out at {}\n\
                 hint: --detach for an independent second copy at the same commit\n\
                 hint: --force to check the same branch out twice (shared ref, HEAD can drift)",
                w.path.display()
            ));
        }
    }

    let dir_s = dir.to_string_lossy().to_string();
    let mut argv: Vec<String> = vec!["worktree".into(), "add".into()];
    if opts.force {
        argv.push("--force".into());
    }

    if opts.detach {
        // Detached: resolve to a commit, never create or occupy a branch.
        let rev = if has_local {
            branch.to_string()
        } else if has_remote {
            format!("origin/{branch}")
        } else {
            base.to_string()
        };
        if !git_quiet(root, &["rev-parse", "--verify", "--quiet", &rev]) {
            return Err(format!("cannot resolve '{rev}' to a commit"));
        }
        eprintln!("Detached checkout at '{rev}'");
        argv.push("--detach".into());
        argv.push(dir_s.clone());
        argv.push(rev);
    } else if has_local {
        eprintln!("Checking out existing local branch '{branch}'");
        argv.push(dir_s.clone());
        argv.push(branch.into());
    } else if has_remote {
        eprintln!("Tracking remote branch 'origin/{branch}'");
        argv.extend(["--track".into(), "-b".into(), branch.into()]);
        argv.push(dir_s.clone());
        argv.push(format!("origin/{branch}"));
    } else {
        eprintln!("Creating new branch '{branch}' from '{base}'");
        argv.extend(["-b".into(), branch.into()]);
        argv.push(dir_s.clone());
        argv.push(base.into());
    }

    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    git_run(root, &refs)?;

    // Path goes to stdout, alone, so wrappers can capture it.
    println!("{dir_s}");
    Ok(())
}

fn cmd_list(root: &Path) -> Result<(), String> {
    let trees = worktrees(root)?;

    let width = trees
        .iter()
        .map(|w| label(w).chars().count())
        .max()
        .unwrap_or(0);

    for w in &trees {
        println!("{:<width$}  {}", label(w), w.path.display(), width = width);
    }
    Ok(())
}

fn cmd_remove(root: &Path, target: &str, force: bool) -> Result<(), String> {
    let trees = worktrees(root)?;
    let wanted = resolve_target(root, &trees, target)?;

    if wanted.bare {
        return Err("refusing to remove the bare/main worktree".into());
    }
    // The main worktree is the first entry git reports.
    if trees.first().map(|w| &w.path) == Some(&wanted.path) {
        return Err(format!(
            "{} is the main worktree, not a linked one",
            wanted.path.display()
        ));
    }

    let path_s = wanted.path.to_string_lossy().to_string();
    let mut argv = vec!["worktree", "remove"];
    if force {
        argv.push("--force");
    }
    argv.push(&path_s);

    git_run(root, &argv).map_err(|e| {
        if !force && e.contains("contains modified or untracked files") {
            format!("{e}\nhint: re-run with --force to discard them")
        } else {
            e
        }
    })?;

    git_run(root, &["worktree", "prune"])?;
    eprintln!("Removed {path_s}");
    Ok(())
}

/// Match `target` against a worktree by exact path, branch name, or the
/// directory name this tool would have generated for that branch.
fn resolve_target<'a>(
    root: &Path,
    trees: &'a [Worktree],
    target: &str,
) -> Result<&'a Worktree, String> {
    // A path is unambiguous, so it wins over a branch name.
    let as_path = std::fs::canonicalize(target).unwrap_or_else(|_| PathBuf::from(target));
    if let Some(w) = trees.iter().find(|w| w.path == as_path) {
        return Ok(w);
    }

    let by_branch: Vec<&Worktree> = trees
        .iter()
        .filter(|w| w.branch.as_deref() == Some(target))
        .collect();
    if by_branch.len() > 1 {
        let mut msg = format!("branch '{target}' has {} worktrees:\n", by_branch.len());
        for w in &by_branch {
            msg.push_str(&format!("  {}\n", w.path.display()));
        }
        msg.push_str("hint: pass the path instead of the branch name");
        return Err(msg);
    }
    if let Some(w) = by_branch.first() {
        return Ok(w);
    }

    if let Ok(generated) = worktree_path(root, target) {
        if let Some(w) = trees.iter().find(|w| w.path == generated) {
            return Ok(w);
        }
    }
    if let Ok(named) = custom_path(root, target) {
        if let Some(w) = trees.iter().find(|w| w.path == named) {
            return Ok(w);
        }
    }

    Err(format!(
        "no worktree matches '{target}'\nTry 'git-wt --list'"
    ))
}

// ---------------------------------------------------------------------------
// Paths and naming
// ---------------------------------------------------------------------------

/// `<parent>/<repo-folder>-<sanitized-branch>`
fn worktree_path(root: &Path, branch: &str) -> Result<PathBuf, String> {
    let name = root
        .file_name()
        .ok_or("cannot determine repo folder name")?
        .to_string_lossy();
    let parent = root.parent().ok_or("repo root has no parent directory")?;
    Ok(parent.join(format!("{name}-{}", sanitize(branch))))
}

/// `--name` target. A bare name is a sibling of the repo root; anything
/// containing a separator is taken as a path, relative to the cwd.
fn custom_path(root: &Path, name: &str) -> Result<PathBuf, String> {
    if name.is_empty() {
        return Err("--name cannot be empty".into());
    }
    let p = Path::new(name);
    if name.contains('/') {
        if p.is_absolute() {
            return Ok(p.to_path_buf());
        }
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        return Ok(cwd.join(p));
    }
    let parent = root.parent().ok_or("repo root has no parent directory")?;
    Ok(parent.join(name))
}

/// Collapse path-hostile characters to single dashes; trim leading/trailing.
fn sanitize(branch: &str) -> String {
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

fn label(w: &Worktree) -> String {
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
fn repo_root() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let common = git_stdout(&cwd, &["rev-parse", "--path-format=absolute", "--git-common-dir"])
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

fn worktrees(root: &Path) -> Result<Vec<Worktree>, String> {
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

fn git_cmd(dir: &Path, args: &[&str]) -> Command {
    let mut c = Command::new("git");
    c.current_dir(dir).args(args);
    c
}

/// Run git, streaming its output through. Errors carry git's stderr.
fn git_run(dir: &Path, args: &[&str]) -> Result<(), String> {
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

fn git_stdout(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = git_cmd(dir, args)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// True when git exits 0. Used for ref existence checks.
fn git_quiet(dir: &Path, args: &[&str]) -> bool {
    git_cmd(dir, args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
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
    fn worktree_path_is_sibling() {
        let p = worktree_path(Path::new("/code/myapp"), "feature/login").unwrap();
        assert_eq!(p, PathBuf::from("/code/myapp-feature-login"));
    }

    #[test]
    fn custom_name_bare_is_sibling() {
        let p = custom_path(Path::new("/code/myapp"), "myapp-review").unwrap();
        assert_eq!(p, PathBuf::from("/code/myapp-review"));
    }

    #[test]
    fn custom_name_absolute_is_verbatim() {
        let p = custom_path(Path::new("/code/myapp"), "/tmp/scratch").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/scratch"));
    }
}
