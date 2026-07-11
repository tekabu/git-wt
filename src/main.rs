//! git-wt — create and manage git worktrees in sibling directories named
//! `<repo-folder>-<sanitized-branch>`.
//!
//! Installed on PATH as `git-wt`, so it is also reachable as `git wt`.
//!
//! Grammar is target-first for existing worktrees (`git-wt <N> <action>`) with
//! an explicit `add` verb for creation.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
git-wt — worktrees in sibling directories named <repo>-<branch>

USAGE:
    git-wt                       List worktrees, numbered from 1
    git-wt list [SEARCH] [--col ...] [--long|--short]
                                 List, optional fuzzy filter; --col picks/orders
                                 columns (1=id, 2=branch, 3=dir, 4=status,
                                 5=last-commit). --long shows all; --short is a
                                 one-line id+branch+status summary.
    git-wt <N>                   == git-wt <N> switch
    git-wt <N> switch            cd into worktree N (alias: cd)
    git-wt <N> path              Print worktree N's path only (alias: show)
    git-wt <N> remove [-y] [-f]  Remove worktree N
    git-wt add [BRANCH] [flags]  Create a worktree (picker when BRANCH omitted)
    git-wt version
    git-wt --help

    Aliases: ls = list, rm = remove, cd = switch, show = path.

ADD OPTIONS:
    -n, --name NAME       Suffix only -> leaf = <repo>-NAME
        --dirname DIR     Whole leaf, verbatim (sanitized); with '/' = a path
    -p, --parentdir DIR   Parent dir (default: primary worktree's parent)
        --from REF        Base ref for a NEW branch
                          (default: the branch of the worktree you run from)
        --stay            wrapper: do NOT cd into the new worktree

REMOVE OPTIONS:
    -y                    Skip the confirmation prompt
    -f, --force           Discard uncommitted/untracked changes

ADD:
    The worktree directory is a sibling of the repo root, named
    <repo-folder>-<branch>, with '/', ' ', ':' and '\\' collapsed to '-'.

        ~/code/myapp  +  feature/login  ->  ~/code/myapp-feature-login

    Branch resolution, in order:
      1. Local branch exists      -> check it out
      2. origin/<branch> exists   -> create a tracking branch from it
      3. Neither                  -> prompt, then create from --from (HEAD)

    With no BRANCH, a picker lists local branches: fzf when installed,
    otherwise a numbered prompt.

STDOUT:
    Only 'switch'/'path' (bare <N>), 'add', and 'remove' print a path, alone,
    on stdout, so a shell can cd into it or capture it. Status goes to stderr.

        cd \"$(git-wt 1 path)\"
        dir=\"$(git-wt add feature/login)\"

COLOR:
    Color and status/last-commit columns turn on only when stdout is a
    terminal, so 'git-wt list | cat' stays plain and parseable. Honors
    NO_COLOR (disable) and CLICOLOR_FORCE (force on).
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

    // Meta / no-args first — these don't all need a repo, and no-args = list.
    match args.first().map(String::as_str) {
        None => {
            let root = repo_root()?;
            return list_from_args(&root, &[]);
        }
        // A leading list flag with no `list` word: `git-wt --col 1,2`.
        Some("--col") | Some("-c") => {
            let root = repo_root()?;
            return list_from_args(&root, &args);
        }
        Some(s) if s.starts_with("--col=") => {
            let root = repo_root()?;
            return list_from_args(&root, &args);
        }
        Some("-h") | Some("--help") | Some("help") => {
            print!("{HELP}");
            return Ok(());
        }
        Some("-V") | Some("--version") | Some("version") => {
            println!("git-wt {VERSION}");
            return Ok(());
        }
        _ => {}
    }

    let first = &args[0];

    if first == "list" || first == "ls" {
        let root = repo_root()?;
        return list_from_args(&root, &args[1..]);
    }

    if first == "add" {
        let root = repo_root()?;
        return cmd_add(&root, &args[1..]);
    }

    // <N> <action> — the target-first grammar.
    if let Ok(n) = first.parse::<usize>() {
        let root = repo_root()?;
        return dispatch_target(&root, n, &args[1..]);
    }

    if first.starts_with('-') {
        return Err(format!("unknown option '{first}'\nTry 'git-wt --help'"));
    }

    Err(unknown_command_msg(first))
}

/// Message for a leading word that is neither a number nor a known verb.
/// Legacy verb-first forms get a migration hint; branch-like words get an
/// `add` suggestion.
fn unknown_command_msg(tok: &str) -> String {
    match tok {
        "show" => "unknown command 'show'; use 'git-wt 1 path'".into(),
        "remove" | "rm" => format!("unknown command '{tok}'; use 'git-wt 1 remove'"),
        _ if branch_like(tok) => format!("unknown command '{tok}'; did you mean 'add {tok}'?"),
        _ => format!("unknown command '{tok}'"),
    }
}

/// A word looks like a branch when it has a `/` or `-` and no whitespace.
fn branch_like(s: &str) -> bool {
    !s.chars().any(char::is_whitespace) && (s.contains('/') || s.contains('-'))
}

// ---------------------------------------------------------------------------
// Target dispatch: git-wt <N> [action]
// ---------------------------------------------------------------------------

fn dispatch_target(root: &Path, n: usize, rest: &[String]) -> Result<(), String> {
    let trees = worktrees(root)?;
    let idx = check_index(n, trees.len())?;

    let action = rest.first().map(String::as_str).unwrap_or("switch");
    match action {
        "switch" | "cd" | "path" | "show" => {
            if rest.len() > 1 {
                return Err("too many arguments\nTry 'git-wt --help'".into());
            }
            // The branch is status, so it goes to stderr; the path is the
            // stdout contract (`cd "$(git-wt 1 path)"` stays clean).
            eprintln!("{}", label(&trees[idx]));
            println!("{}", trees[idx].path.display());
            Ok(())
        }
        "remove" | "rm" => {
            let mut yes = false;
            let mut force = false;
            for a in &rest[1..] {
                match a.as_str() {
                    "-y" => yes = true,
                    "-f" | "--force" => force = true,
                    other => {
                        return Err(format!("unknown option '{other}' for remove"));
                    }
                }
            }
            cmd_remove(root, &trees, idx, yes, force)
        }
        "-n" | "--name" => Err("switch/path/remove take no --name".into()),
        other if other.starts_with('-') => {
            Err(format!("switch/path/remove take no {other}"))
        }
        other => Err(format!("unknown action '{other}' (switch, path, remove)")),
    }
}

/// Map a 1-based index to a 0-based one, or an error.
fn check_index(n: usize, len: usize) -> Result<usize, String> {
    if n == 0 {
        return Err("no worktree #0".into());
    }
    if n > len {
        return Err(format!(
            "no worktree #{n}; there are {len} (see 'git-wt list')"
        ));
    }
    Ok(n - 1)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Parse `list` arguments (an optional SEARCH plus `--col`) then list. Shared
/// by `list`/`ls`, the no-args default, and a bare leading `--col`.
fn list_from_args(root: &Path, args: &[String]) -> Result<(), String> {
    let mut search: Option<String> = None;
    let mut cols: Option<Vec<usize>> = None;
    let mut mode = ListMode::Normal;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--col" | "-c" => {
                let v = it.next().ok_or("--col needs columns, e.g. 1,2,3")?;
                cols = Some(parse_cols(v)?);
            }
            s if s.starts_with("--col=") => cols = Some(parse_cols(&s["--col=".len()..])?),
            "--long" | "-l" => mode = ListMode::Long,
            "--short" | "-s" => mode = ListMode::Short,
            s if s.starts_with('-') && s != "-" => {
                return Err(format!("unknown option '{s}'\nTry 'git-wt --help'"));
            }
            s => {
                if search.is_some() {
                    return Err("too many arguments\nTry 'git-wt --help'".into());
                }
                search = Some(s.to_string());
            }
        }
    }
    cmd_list(root, search.as_deref(), cols, mode)
}

/// Verbosity for `list`. Normal enriches to status + last-commit only on a
/// terminal; on a pipe it stays the plain id/branch/dir contract.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ListMode {
    Short,
    Normal,
    Long,
}

fn cmd_list(
    root: &Path,
    search: Option<&str>,
    cols: Option<Vec<usize>>,
    mode: ListMode,
) -> Result<(), String> {
    let trees = worktrees(root)?;

    // Keep the original 1-based index so `git-wt <N> ...` means the same tree
    // no matter what filter was applied.
    let rows: Vec<(usize, &Worktree)> = trees
        .iter()
        .enumerate()
        .filter(|(_, w)| match search {
            Some(s) => fuzzy_match(w, s),
            None => true,
        })
        .collect();

    if let Some(s) = search {
        if rows.is_empty() {
            return Err(format!("no worktree matches '{s}'"));
        }
    }

    let stdout_tty = std::io::stdout().is_terminal();
    let color = color_enabled(stdout_tty);
    let explicit = cols.is_some();

    // Columns to show, in order: 1=id, 2=branch, 3=dir, 4=status, 5=last.
    // Without --col: Short is a compact summary, Long shows everything, and
    // Normal enriches only on a TTY so a piped `git-wt list` keeps the plain
    // id/branch/dir contract.
    let cols = match (cols, mode) {
        (Some(c), _) => c,
        (None, ListMode::Short) => vec![1, 2, 4],
        (None, ListMode::Long) => vec![1, 2, 3, 4, 5],
        (None, ListMode::Normal) if stdout_tty => vec![1, 2, 3, 4, 5],
        (None, ListMode::Normal) => vec![1, 2, 3],
    };

    // Branch color needs status too, so fetch it whenever we color or show it.
    let need_status = color || cols.contains(&4);
    let need_last = cols.contains(&5);
    let header = !explicit && stdout_tty && mode != ListMode::Short;

    // Right-align the index to the widest possible so filtered output lines up.
    let numw = trees.len().to_string().len();

    // Per-row metadata, fetched once (read-only git calls).
    let meta: Vec<(Status, String)> = rows
        .iter()
        .map(|(_, w)| {
            let st = if need_status && !w.bare {
                worktree_status(&w.path)
            } else {
                Status::Unknown
            };
            let last = if need_last { last_commit(&w.path) } else { String::new() };
            (st, last)
        })
        .collect();

    // Plain (uncolored) cells drive column widths; color is applied at print
    // time so the ANSI escapes never skew alignment.
    let cells: Vec<Vec<String>> = rows
        .iter()
        .zip(&meta)
        .map(|((i, w), (st, last))| {
            cols.iter()
                .map(|c| match c {
                    1 => format!("{:>numw$}", i + 1, numw = numw),
                    2 => label(w),
                    3 => w.path.display().to_string(),
                    4 => status_text(*st).to_string(),
                    _ => last.clone(),
                })
                .collect()
        })
        .collect();

    let header_cells: Vec<String> = cols.iter().map(|c| col_header(*c).to_string()).collect();

    // Per-column width over the header and every data row.
    let mut widths = vec![0usize; cols.len()];
    for row in cells.iter().chain(header.then_some(&header_cells)) {
        for (k, cell) in row.iter().enumerate() {
            widths[k] = widths[k].max(cell.chars().count());
        }
    }

    if header {
        let line = render_row(&header_cells, &cols, &widths, Status::Unknown, false);
        println!("{}", paint(&line, DIM, color));
    }

    for (row, (st, _)) in cells.iter().zip(&meta) {
        let line = render_row(row, &cols, &widths, *st, color);
        println!("{line}");
    }
    Ok(())
}

/// Header label for a column id.
fn col_header(c: usize) -> &'static str {
    match c {
        1 => "#",
        2 => "branch",
        3 => "path",
        4 => "status",
        _ => "last",
    }
}

/// Join one row's cells with two-space gaps, padding all but the last column.
/// When `color`, the branch (col 2) and status (col 4) cells are tinted by
/// `st`. Padding is computed on the plain text, then color wraps it, so ANSI
/// never affects alignment.
fn render_row(
    row: &[String],
    cols: &[usize],
    widths: &[usize],
    st: Status,
    color: bool,
) -> String {
    let mut line = String::new();
    let last = row.len() - 1;
    for (k, cell) in row.iter().enumerate() {
        if k > 0 {
            line.push_str("  ");
        }
        let padded = if k == last {
            cell.clone()
        } else {
            format!("{:<w$}", cell, w = widths[k])
        };
        let code = status_color(st);
        let tinted = matches!(cols[k], 2 | 4) && color && !code.is_empty();
        if tinted {
            line.push_str(&paint(&padded, code, true));
        } else {
            line.push_str(&padded);
        }
    }
    line
}

/// Parse `--col` value like "1,2,4" into column ids.
/// 1=id, 2=branch, 3=dir, 4=status, 5=last-commit.
const COL_HELP: &str = "1=id, 2=branch, 3=dir, 4=status, 5=last";

fn parse_cols(s: &str) -> Result<Vec<usize>, String> {
    let mut v = Vec::new();
    for part in s.split(',') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let n: usize = p
            .parse()
            .map_err(|_| format!("bad column '{p}' (use {COL_HELP})"))?;
        if n < 1 || n > 5 {
            return Err(format!("no column {n} (use {COL_HELP})"));
        }
        v.push(n);
    }
    if v.is_empty() {
        return Err("--col needs columns, e.g. 1,2,3".into());
    }
    Ok(v)
}

// ---------------------------------------------------------------------------
// Color, status, and metadata (no dependencies; ANSI on a TTY only)
// ---------------------------------------------------------------------------

const RESET: &str = "\x1b[0m";
const GREEN: &str = "32";
const YELLOW: &str = "33";
const RED: &str = "31";
const DIM: &str = "2";

/// Whether to emit ANSI for a stream that is (or isn't) a terminal. Honors the
/// `NO_COLOR` (any value disables) and `CLICOLOR_FORCE` (nonzero forces on)
/// conventions; otherwise follows the stream's TTY-ness.
fn color_enabled(is_tty: bool) -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if let Some(v) = std::env::var_os("CLICOLOR_FORCE") {
        if !v.is_empty() && v != "0" {
            return true;
        }
    }
    is_tty
}

/// Wrap `s` in an ANSI SGR code when `on`, else return it unchanged. The code
/// is a bare parameter string like "32" or "2".
fn paint(s: &str, code: &str, on: bool) -> String {
    if on {
        format!("\x1b[{code}m{s}{RESET}")
    } else {
        s.to_string()
    }
}

/// Working-tree cleanliness of a worktree.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Status {
    Clean,
    Dirty,
    Untracked,
    /// Bare worktree, or git couldn't report (shown blank).
    Unknown,
}

/// Classify `git status --porcelain` output. Any `??` line means untracked;
/// other entries mean dirty; empty means clean.
fn classify_status(porcelain: &str) -> Status {
    if porcelain.trim().is_empty() {
        return Status::Clean;
    }
    if porcelain.lines().any(|l| l.starts_with("??")) {
        Status::Untracked
    } else {
        Status::Dirty
    }
}

/// Run `git status --porcelain` in the worktree and classify it.
fn worktree_status(path: &Path) -> Status {
    match git_stdout(path, &["status", "--porcelain"]) {
        Ok(s) => classify_status(&s),
        Err(_) => Status::Unknown,
    }
}

fn status_text(s: Status) -> &'static str {
    match s {
        Status::Clean => "clean",
        Status::Dirty => "dirty",
        Status::Untracked => "untracked",
        Status::Unknown => "",
    }
}

/// ANSI color for a status, or "" (no color) for Unknown.
fn status_color(s: Status) -> &'static str {
    match s {
        Status::Clean => GREEN,
        Status::Dirty => YELLOW,
        Status::Untracked => RED,
        Status::Unknown => "",
    }
}

/// Relative time of the worktree's last commit (e.g. "2 minutes ago"), or ""
/// when unavailable (bare / no commits).
fn last_commit(path: &Path) -> String {
    git_stdout(path, &["log", "-1", "--format=%ar"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Case-insensitive subsequence match over "<label> <path>".
fn fuzzy_match(w: &Worktree, needle: &str) -> bool {
    let hay = format!("{} {}", label(w), w.path.display()).to_lowercase();
    is_subseq(&hay, &needle.to_lowercase())
}

/// True when every char of `needle` appears in `hay`, in order.
fn is_subseq(hay: &str, needle: &str) -> bool {
    let mut chars = hay.chars();
    'outer: for nc in needle.chars() {
        for hc in chars.by_ref() {
            if hc == nc {
                continue 'outer;
            }
        }
        return false;
    }
    true
}

fn cmd_remove(
    root: &Path,
    trees: &[Worktree],
    idx: usize,
    yes: bool,
    force: bool,
) -> Result<(), String> {
    let wanted = &trees[idx];

    // The main worktree is the first entry git reports; a bare one is never
    // a checkout to remove.
    if idx == 0 || wanted.bare {
        return Err("refusing to remove the main worktree".into());
    }

    // Was the shell standing inside the tree we're about to remove? Capture it
    // before removal (canonicalize needs the dir to still exist). Only then does
    // a wrapper need to cd back to main; otherwise it should stay put.
    let inside = match std::env::current_dir() {
        Ok(cwd) => canon(&cwd).starts_with(canon(&wanted.path)),
        Err(_) => false,
    };

    let path_s = wanted.path.to_string_lossy().to_string();
    if !yes
        && !confirm(&format!(
            "Remove worktree '{}' at {path_s}? [y/N] ",
            label(wanted)
        ))?
    {
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
            format!("{e}\nhint: re-run with -f to discard them")
        } else {
            e
        }
    })?;

    git_run(root, &["worktree", "prune"])?;

    // The `remove` verb only detaches the worktree; the branch itself stays.
    let leaf = leaf_of(&wanted.path);
    let branch_note = match &wanted.branch {
        Some(b) => format!("branch {b} kept"),
        None => "detached".into(),
    };
    let on = std::io::stderr().is_terminal() && color_enabled(true);
    eprintln!("{} {leaf}  ({branch_note})", paint("Removed", GREEN, on));

    // Only when the shell was inside the removed tree does its cwd now dangle,
    // so print the main path for a wrapper to cd back. Removing some other tree
    // leaves you where you are — print nothing, so the wrapper stays put.
    if inside {
        if let Some(main) = trees.first() {
            println!("{}", main.path.display());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Create: git-wt add [BRANCH] [flags]
// ---------------------------------------------------------------------------

fn cmd_add(root: &Path, args: &[String]) -> Result<(), String> {
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
    let has_remote = git_quiet(
        root,
        &["show-ref", "--verify", &format!("refs/remotes/origin/{branch}")],
    );

    // --from only affects creating a NEW branch; if the branch already exists
    // it is silently overridden, so warn + confirm.
    if from.is_some() && (has_local || has_remote) {
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
    } else if has_remote {
        eprintln!("Tracking remote branch 'origin/{branch}'");
        argv.extend(["--track".into(), "-b".into(), branch.clone()]);
        argv.push(dir_s.clone());
        argv.push(format!("origin/{branch}"));
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
    let origin = if has_local {
        format!("branch {branch}")
    } else if has_remote {
        format!("branch {branch} tracking origin/{branch}")
    } else {
        format!("branch {branch} from {from_ref}")
    };
    let leaf = leaf_of(&dir);
    let on = std::io::stderr().is_terminal() && color_enabled(true);
    eprintln!("{} {leaf}  ({origin})", paint("Created", GREEN, on));

    // Print the new worktree path on stdout (alone) so scripts can capture it:
    // `dir=$(git-wt add feat/x)`. Status/progress went to stderr.
    println!("{dir_s}");
    Ok(())
}

/// Resolve the worktree directory for `add`. Returns `Ok(None)` when the user
/// declines a warn-and-confirm (an override), which the caller treats as an
/// abort rather than an error.
fn resolve_add_path(
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
fn pick_branch(root: &Path) -> Result<String, String> {
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
fn new_branch_prompt() -> Result<String, String> {
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
fn fzf_pick(root: &Path, items: &[&str]) -> Result<Option<String>, String> {
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
fn number_pick(items: &[(&str, &str)]) -> Result<String, String> {
    let color = std::io::stderr().is_terminal() && color_enabled(true);
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

// ---------------------------------------------------------------------------
// Prompt
// ---------------------------------------------------------------------------

/// Print a prompt to stderr and read a yes/no answer from stdin. Requires the
/// user to type and press Enter; empty or anything but y/yes is No. EOF / no
/// tty is No.
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

/// Single-quote a path for safe interpolation into an `sh -c` command line
/// (used to build fzf's --preview). Embedded quotes are escaped `'\''`.
fn sh_quote(p: &Path) -> String {
    format!("'{}'", p.to_string_lossy().replace('\'', "'\\''"))
}

/// Canonical path for comparison; falls back to the input when it can't be
/// resolved (e.g. it no longer exists), so equal paths still compare equal.
fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Last path component (directory leaf) as a display string, or the whole path
/// when it has none.
fn leaf_of(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| p.display().to_string())
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

/// The ref checked out in the current directory's worktree: the branch name,
/// or a short commit sha when detached. Falls back to "HEAD" if git can't say.
fn current_ref() -> String {
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(_) => return "HEAD".into(),
    };
    if let Ok(b) = git_stdout(&cwd, &["symbolic-ref", "--short", "-q", "HEAD"]) {
        let b = b.trim();
        if !b.is_empty() {
            return b.to_string();
        }
    }
    // Detached HEAD: use the short commit sha.
    if let Ok(sha) = git_stdout(&cwd, &["rev-parse", "--short", "HEAD"]) {
        let sha = sha.trim();
        if !sha.is_empty() {
            return sha.to_string();
        }
    }
    "HEAD".into()
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

    #[test]
    fn subseq_matches_in_order() {
        assert!(is_subseq("feature-login", "flogin"));
        assert!(is_subseq("feature-login", "feat"));
        assert!(!is_subseq("feature-login", "zzz"));
        assert!(!is_subseq("abc", "cba"));
    }

    #[test]
    fn branch_like_detection() {
        assert!(branch_like("feat/x"));
        assert!(branch_like("feat-x"));
        assert!(!branch_like("lsit"));
        assert!(!branch_like("foo bar"));
    }

    #[test]
    fn check_index_bounds() {
        assert_eq!(check_index(1, 3), Ok(0));
        assert_eq!(check_index(3, 3), Ok(2));
        assert_eq!(check_index(0, 3), Err("no worktree #0".into()));
        assert_eq!(
            check_index(4, 3),
            Err("no worktree #4; there are 3 (see 'git-wt list')".into())
        );
    }

    #[test]
    fn classify_status_reads_porcelain() {
        assert_eq!(classify_status(""), Status::Clean);
        assert_eq!(classify_status("   \n"), Status::Clean);
        assert_eq!(classify_status(" M src/main.rs"), Status::Dirty);
        assert_eq!(classify_status("?? new.txt"), Status::Untracked);
        // Untracked wins when both are present.
        assert_eq!(classify_status(" M a\n?? b"), Status::Untracked);
    }

    #[test]
    fn paint_wraps_only_when_on() {
        assert_eq!(paint("x", GREEN, false), "x");
        assert_eq!(paint("x", GREEN, true), "\x1b[32mx\x1b[0m");
    }

    #[test]
    fn parse_cols_accepts_status_and_last() {
        assert_eq!(parse_cols("1,4,5").unwrap(), vec![1, 4, 5]);
        assert!(parse_cols("6").is_err());
    }

    #[test]
    fn sh_quote_wraps_and_escapes() {
        assert_eq!(sh_quote(Path::new("/code/my app")), "'/code/my app'");
        assert_eq!(sh_quote(Path::new("/a'b")), "'/a'\\''b'");
    }

    #[test]
    fn leaf_of_returns_last_component() {
        assert_eq!(leaf_of(Path::new("/code/myapp-feat-x")), "myapp-feat-x");
        assert_eq!(leaf_of(Path::new("myapp")), "myapp");
    }

    #[test]
    fn render_row_pads_and_tints() {
        let cols = vec![1, 2];
        let row = vec!["1".to_string(), "main".to_string()];
        let widths = vec![1, 7];
        // No color: branch is left-padded to width, no ANSI.
        let plain = render_row(&row, &cols, &widths, Status::Clean, false);
        assert_eq!(plain, "1  main");
        // Color: branch cell tinted green (padding inside the escape).
        let tinted = render_row(&row, &cols, &widths, Status::Clean, true);
        assert_eq!(tinted, "1  \x1b[32mmain\x1b[0m");
    }

    #[test]
    fn unknown_command_messages() {
        assert_eq!(
            unknown_command_msg("show"),
            "unknown command 'show'; use 'git-wt 1 path'"
        );
        assert_eq!(
            unknown_command_msg("remove"),
            "unknown command 'remove'; use 'git-wt 1 remove'"
        );
        assert_eq!(
            unknown_command_msg("feat/x"),
            "unknown command 'feat/x'; did you mean 'add feat/x'?"
        );
        assert_eq!(unknown_command_msg("lsit"), "unknown command 'lsit'");
    }
}
