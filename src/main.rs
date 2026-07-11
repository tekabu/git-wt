//! git-wt — create git worktrees in a sibling directory named
//! `<repo-folder>-<sanitized-branch>`.
//!
//! Installed on PATH as `git-wt`, so it is also reachable as `git wt`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
git-wt — worktrees in sibling directories named <repo>-<branch>

USAGE:
    git-wt <branch> [base-ref]   Create a worktree for <branch>
    git-wt                       Pick a branch interactively (fzf, else a list)
    git-wt list                  List worktrees, numbered from 1
    git-wt show <N>              Print worktree #N's path (for cd)
    git-wt remove <N|branch>     Remove a worktree; N is from 'list'
    git-wt --help
    git-wt --version

    Aliases: ls = list, rm = remove, go/cd = show.

OPTIONS:
    -n, --name <dir>   Create: override the worktree directory name
    -s, --show         Create: print the new worktree path on stdout (for cd)
    -f, --force        Remove: discard uncommitted changes
    -h, --help         Show this help
    -V, --version      Show version

CREATE:
    The worktree directory is a sibling of the repo root, named
    <repo-folder>-<branch>, with '/', ' ', ':' and '\\' collapsed to '-'.
    --name overrides it.

        ~/code/myapp  +  feature/login  ->  ~/code/myapp-feature-login

    Branch resolution, in order:
      1. Local branch exists      -> check it out
      2. origin/<branch> exists   -> create a tracking branch from it
      3. Neither                  -> prompt, then create from [base-ref] (HEAD)

    Refuses to create when the branch is already checked out in another
    worktree, or when the target directory already exists.

    With no <branch>, a picker lists local branches with a search filter:
    fzf when installed, otherwise a numbered prompt.

REMOVE:
    Target is a number from 'git-wt list', a branch name (only when a worktree
    has it), or a path. Prompts before removing; --force discards uncommitted
    changes. On success prints the main worktree path, so a wrapper can cd back
    to it in case you were standing inside the tree just removed.

STDOUT:
    Only 'show <N>', 'create --show', and 'remove' print a path, alone, on
    stdout, so a shell can cd into it. Status text goes to stderr.

        cd \"$(git-wt show 2)\"
        cd \"$(git-wt feature/login --show)\"
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

    let mut show_path = false;
    let mut force = false;
    let mut name: Option<String> = None;
    let mut list = false;
    let mut show_mode = false;
    let mut remove_mode = false;
    let mut remove_flag_target: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();

    // A leading bare word may be a subcommand; anything else is a branch to
    // create (the default action).
    let mut rest: &[String] = &args;
    if let Some(first) = args.first() {
        match first.as_str() {
            "list" | "ls" => {
                list = true;
                rest = &args[1..];
            }
            "show" | "go" | "cd" => {
                show_mode = true;
                rest = &args[1..];
            }
            "remove" | "rm" => {
                remove_mode = true;
                rest = &args[1..];
            }
            _ => {}
        }
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
            "-s" | "--show" => show_path = true,
            "-f" | "--force" => force = true,
            "-n" | "--name" => {
                let t = iter.next().ok_or("--name needs a directory name")?;
                name = Some(t.clone());
            }
            "-r" | "--remove" => {
                let t = iter.next().ok_or("--remove needs a number, branch, or path")?;
                remove_flag_target = Some(t.clone());
            }
            s if s.starts_with("--name=") => name = Some(s["--name=".len()..].to_string()),
            s if s.starts_with("--remove=") => {
                remove_flag_target = Some(s["--remove=".len()..].to_string())
            }
            s if s.starts_with('-') && s != "-" => {
                return Err(format!("unknown option: {s}\nTry 'git-wt --help'"));
            }
            s => positional.push(s.to_string()),
        }
    }

    if remove_flag_target.is_some() {
        remove_mode = true;
    }

    // Every mode below needs to be inside a repo.
    let root = repo_root()?;

    if list {
        if name.is_some() || show_path || force {
            return Err("list takes no --name/--show/--force".into());
        }
        return cmd_list(&root);
    }

    if show_mode {
        if name.is_some() || show_path || force {
            return Err("show takes no --name/--show/--force".into());
        }
        let n = single(&positional, "show needs a worktree number (see 'git-wt list')")?;
        let n: usize = n
            .parse()
            .map_err(|_| format!("'{n}' is not a worktree number; see 'git-wt list'"))?;
        return cmd_show(&root, n);
    }

    if remove_mode {
        if name.is_some() || show_path {
            return Err("remove takes no --name/--show".into());
        }
        let target = match &remove_flag_target {
            Some(t) => t.clone(),
            None => single(&positional, "remove needs a number, branch, or path")?.clone(),
        };
        return cmd_remove(&root, &target, force);
    }

    // CREATE (default).
    if force {
        return Err("create takes no --force".into());
    }
    if positional.len() > 2 {
        return Err("too many arguments\nTry 'git-wt --help'".into());
    }
    let branch = match positional.first() {
        Some(b) => b.clone(),
        None => pick_branch(&root)?, // interactive picker
    };
    let base = positional.get(1).map(String::as_str).unwrap_or("HEAD");
    cmd_create(&root, &branch, base, name.as_deref(), show_path)
}

/// Exactly one positional, or an error.
fn single<'a>(positional: &'a [String], empty_msg: &str) -> Result<&'a String, String> {
    match positional.len() {
        0 => Err(empty_msg.into()),
        1 => Ok(&positional[0]),
        _ => Err("too many arguments\nTry 'git-wt --help'".into()),
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn cmd_create(
    root: &Path,
    branch: &str,
    base: &str,
    name: Option<&str>,
    show: bool,
) -> Result<(), String> {
    let dir = match name {
        Some(n) => custom_path(root, n)?,
        None => worktree_path(root, branch)?,
    };

    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }

    // Refuse to point a new worktree at a branch that is already checked out;
    // git shares one ref between worktrees, so the two HEADs would drift.
    if let Some(w) = worktrees(root)?
        .into_iter()
        .find(|w| w.branch.as_deref() == Some(branch))
    {
        return Err(format!(
            "branch '{branch}' is already checked out at {}",
            w.path.display()
        ));
    }

    let has_local = git_quiet(root, &["show-ref", "--verify", &format!("refs/heads/{branch}")]);
    let has_remote = git_quiet(
        root,
        &["show-ref", "--verify", &format!("refs/remotes/origin/{branch}")],
    );

    let dir_s = dir.to_string_lossy().to_string();
    let mut argv: Vec<String> = vec!["worktree".into(), "add".into()];

    if has_local {
        eprintln!("Checking out existing local branch '{branch}'");
        argv.push(dir_s.clone());
        argv.push(branch.into());
    } else if has_remote {
        eprintln!("Tracking remote branch 'origin/{branch}'");
        argv.extend(["--track".into(), "-b".into(), branch.into()]);
        argv.push(dir_s.clone());
        argv.push(format!("origin/{branch}"));
    } else {
        if !confirm(&format!(
            "Branch '{branch}' does not exist. Create it from '{base}'? [y/N] "
        ))? {
            eprintln!("Aborted.");
            return Ok(());
        }
        eprintln!("Creating new branch '{branch}' from '{base}'");
        argv.extend(["-b".into(), branch.into()]);
        argv.push(dir_s.clone());
        argv.push(base.into());
    }

    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    git_run(root, &refs)?;

    // With --show, print the path (alone) on stdout so a wrapper can cd to it.
    if show {
        println!("{dir_s}");
    } else {
        eprintln!("{dir_s}");
    }
    Ok(())
}

fn cmd_list(root: &Path) -> Result<(), String> {
    let trees = worktrees(root)?;

    let width = trees
        .iter()
        .map(|w| label(w).chars().count())
        .max()
        .unwrap_or(0);
    // Right-align the 1-based index to the widest number.
    let numw = trees.len().to_string().len();

    for (i, w) in trees.iter().enumerate() {
        println!(
            "{:>numw$}  {:<width$}  {}",
            i + 1,
            label(w),
            w.path.display(),
            numw = numw,
            width = width,
        );
    }
    Ok(())
}

/// `show <N>`: print the Nth worktree's path (1-based) on stdout for `cd`.
fn cmd_show(root: &Path, n: usize) -> Result<(), String> {
    let trees = worktrees(root)?;
    if n == 0 || n > trees.len() {
        return Err(format!(
            "no worktree #{n}; there are {} (see 'git-wt list')",
            trees.len()
        ));
    }
    println!("{}", trees[n - 1].path.display());
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
    if !confirm(&format!(
        "Remove worktree '{}' at {path_s}? [y/N] ",
        label(wanted)
    ))? {
        eprintln!("Aborted.");
        return Ok(());
    }

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

    // Print the main worktree path so a wrapper can cd back to it — you may
    // have been standing inside the tree just removed.
    if let Some(main) = trees.first() {
        println!("{}", main.path.display());
    }
    Ok(())
}

/// Resolve a remove target: a 1-based number from `list`, an exact path, a
/// branch name (only when a worktree has it), or a generated directory name.
fn resolve_target<'a>(
    root: &Path,
    trees: &'a [Worktree],
    target: &str,
) -> Result<&'a Worktree, String> {
    // A plain number selects by position in `git-wt list`.
    if let Ok(n) = target.parse::<usize>() {
        if n == 0 || n > trees.len() {
            return Err(format!(
                "no worktree #{n}; there are {} (see 'git-wt list')",
                trees.len()
            ));
        }
        return Ok(&trees[n - 1]);
    }

    // A path is unambiguous, so it wins over a branch name.
    let as_path = canon(Path::new(target));
    if let Some(w) = trees.iter().find(|w| canon(&w.path) == as_path) {
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
        msg.push_str("hint: pass the number from 'git-wt list' or a path");
        return Err(msg);
    }
    if let Some(w) = by_branch.first() {
        return Ok(w);
    }

    if let Ok(generated) = worktree_path(root, target) {
        if let Some(w) = trees.iter().find(|w| canon(&w.path) == canon(&generated)) {
            return Ok(w);
        }
    }
    if let Ok(named) = custom_path(root, target) {
        if let Some(w) = trees.iter().find(|w| canon(&w.path) == canon(&named)) {
            return Ok(w);
        }
    }

    Err(format!("no worktree for '{target}' (see 'git-wt list')"))
}

// ---------------------------------------------------------------------------
// Branch picker (no <branch> given)
// ---------------------------------------------------------------------------

/// Choose a local branch interactively. Prefers fzf's search filter; falls
/// back to a numbered prompt when fzf is not installed.
fn pick_branch(root: &Path) -> Result<String, String> {
    let out = git_stdout(root, &["for-each-ref", "--format=%(refname:short)", "refs/heads"])?;
    let branches: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
    if branches.is_empty() {
        return Err("no local branches to choose from".into());
    }

    if let Some(sel) = fzf_pick(&branches)? {
        return Ok(sel);
    }
    number_pick(&branches)
}

/// Run fzf over `items`. Returns Ok(None) when fzf is not on PATH so the caller
/// can fall back; an empty/aborted selection is an error.
fn fzf_pick(items: &[&str]) -> Result<Option<String>, String> {
    let mut child = match Command::new("fzf")
        .args(["--prompt", "branch> ", "--height", "40%", "--reverse"])
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

/// Numbered fallback picker; reads a number from stdin.
fn number_pick(items: &[&str]) -> Result<String, String> {
    let w = items.len().to_string().len();
    for (i, b) in items.iter().enumerate() {
        eprintln!("{:>w$}  {}", i + 1, b, w = w);
    }
    eprint!("Select a branch number (Enter to cancel): ");
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
    Ok(items[n - 1].to_string())
}

// ---------------------------------------------------------------------------
// Prompt
// ---------------------------------------------------------------------------

/// Print a prompt to stderr and read a yes/no answer from stdin. Requires the
/// user to type and press Enter; empty or anything but y/yes is No.
fn confirm(prompt: &str) -> Result<bool, String> {
    eprint!("{prompt}");
    std::io::stderr().flush().ok();
    let mut line = String::new();
    let n = std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Ok(false); // EOF / no tty -> treat as No
    }
    let a = line.trim().to_ascii_lowercase();
    Ok(a == "y" || a == "yes")
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

/// Canonical path for comparison; falls back to the input when it can't be
/// resolved (e.g. it doesn't exist), so equal paths still compare equal.
fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
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
