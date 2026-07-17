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
                                 5=last-commit, 6=merged). --long shows id/branch/dir/
                                 status/last; --short is a one-line id+branch+status summary.
    git-wt <N>                   == git-wt <N> switch
    git-wt <N> switch            cd into worktree N (alias: cd)
    git-wt <N> path              Print worktree N's path only (alias: show)
    git-wt <N> remove [-y] [-f]  Remove worktree N
    git-wt <N>,<M> merge         Merge M into N
    git-wt <N> merge <BRANCH>    Merge BRANCH into worktree N
    git-wt <N> merge continue|abort
    git-wt <N>,<M> merged        Is M's branch already in N's branch?
    git-wt <N> merged <BRANCH>   Is BRANCH already in worktree N's branch?
    git-wt <N> merged            Is N's branch already in the current branch?
    git-wt <N>,<M> diff [flags]  Diff worktree N against worktree M
    git-wt <N>,<N>[,<N>] meld    Diff 2-3 worktrees side by side in meld
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

DIFF OPTIONS:
    live                  Compare the files on disk, not the commits
    hunks                 Print each file's changed line numbers
    ...                   Range: only what M added since it forked from N (default)
    ..                    Range: everything that differs between the two tips
        --name-only       File names only
        --name-status     File names with A/M/D
        --stat            File names with a churn summary
    -- PATH...            Limit to these paths

DIFF:
    Diffs the two worktrees' committed state (their branches), through git's
    own pager, so uncommitted work does not show up; diff warns when either
    side is dirty and points at 'live'.

        git-wt 1,2 diff              -> git diff <branch 1>...<branch 2>
        git-wt 1,2 diff ..           -> git diff <branch 1>..<branch 2>
        git-wt 1,2 diff --stat
        git-wt 1,2 diff -- src/

    The default range is '...', so '1,2 diff' shows exactly what '1,2 merge'
    would bring in: M's own commits since the fork, and nothing of N's. '..'
    compares the two tips instead, which also reports N's commits, inverted,
    as if M had removed them.

    Any other git flag is an error, not a passthrough: run git yourself,
    'git diff <A>...<B> <flag>'. The error prints that command for you.

DIFF LIVE:
    'live' compares the literal bytes in the two directories, so uncommitted
    work shows up -- including the case no ref diff can ever answer, two
    worktrees sitting on the same commit. Only paths git would list are
    considered, so .gitignore is honored and build output stays out.

        git-wt 1,2 diff live         # literal files on disk
        git-wt 1,2 diff live hunks   # + changed line numbers
        git-wt 1,2 diff --live       # dashes optional, same thing

    'live' takes no range: '..'/'...' compare commits, which is the opposite
    question. --name-only/--name-status/--stat/-- PATH... all still apply.
    'hunks' works without 'live' too; its line numbers are the '+' side (M).

MELD:
    Opens meld on the worktree directories, in the order you list them, and
    waits until you close it. Requires meld on PATH.

        git-wt 1,3 meld      -> meld <dir 1> <dir 3>
        git-wt 2,1,3 meld    -> meld <dir 2> <dir 1> <dir 3>  (3-way)

MERGE WORDS:            (each takes an optional '--': 'abort' == '--abort')
    -c, continue          Conclude a conflicted merge
    -a, abort             Undo a conflicted merge
    -o, ours              On a conflicting hunk, keep worktree N's side
    -t, theirs            On a conflicting hunk, take the source's side
    -d, dry-run           Report whether it would merge; change nothing

MERGE OPTIONS:
    -m, --message MSG     Merge commit message
        --no-ff           Always create a merge commit
        --ff-only         Refuse anything but a fast-forward
        --squash          Stage the merge without committing
    -f, --force           Merge even when worktree N has uncommitted changes

MERGE:
    The merge runs inside worktree N, so N's branch is the one that moves:

        git-wt 1,2 merge            # worktree 2's branch -> worktree 1's branch
        git-wt 1 merge feat/x       # a branch name works too
        git-wt 1,2 merge dry-run    # would it conflict? nothing is touched
        git-wt 1,2 merge theirs     # let 2 win every collision

    The list reads dest-first, so '1,2 merge' merges 2 into 1. It takes
    exactly two worktrees -- unlike meld, which diffs 2-3 -- because a
    merge has one destination and one source. The list already names the
    source, so it cannot be combined with 'continue'/'abort'; those take a
    single target, 'git-wt 1 merge continue' (or 'git-wt 1 merge abort').

    A number that names a worktree wins over a branch of the same name, and
    the words above win over a branch of the same name: to merge a branch
    called 'theirs', spell it 'heads/theirs'.

    On conflict, git-wt exits nonzero and lists the conflicted files; fix
    them in worktree N, then run 'git-wt N merge continue' (or abort).
    Merge commits never open an editor: without -m, git's default message is
    taken as-is.

    'ours'/'theirs' are git's -X strategy options, so they settle only the
    hunks that actually collide -- the rest of both sides still merges. They
    are applied while the merge is computed, so they cannot join a merge that
    has already stopped: git-wt offers to abort and redo it instead.

ADD:
    The worktree directory is a sibling of the repo root, named
    <repo-folder>-<branch>, with '/', ' ', ':' and '\\' collapsed to '-'.

        ~/code/myapp  +  feature/login  ->  ~/code/myapp-feature-login

    Branch resolution, in order:
      1. Local branch exists      -> check it out
      2. <remote>/<branch> exists -> create a tracking branch from it
                                     (prefers origin, else first remote match)
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

    // <N>,<N>[,<N>] <action> — the multi-target grammar (meld).
    if let Some(ns) = parse_target_list(first)? {
        let root = repo_root()?;
        return dispatch_targets(&root, &ns, &args[1..]);
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
        "merge" => "unknown command 'merge'; use 'git-wt 1,2 merge'".into(),
        "merged" => "unknown command 'merged'; use 'git-wt 1 merged' or 'git-wt 1,2 merged'".into(),
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
                        return Err(format!("unexpected argument '{other}' for remove"));
                    }
                }
            }
            cmd_remove(root, &trees, idx, yes, force)
        }
        // `1 diff 2` was the old grammar; point at the list form meld already
        // uses. `merge` keeps the single-target form for branch sources and for
        // `continue`/`abort`; only a worktree-number source now uses the list.
        // A source equal to the destination is left to `cmd_merge`, which gives
        // the clearer "already checked out" error and preserves the documented
        // worktree-wins rule for digit branch names.
        "diff" => Err(format!(
            "diff takes a worktree list: 'git-wt {n},<M> diff'"
        )),
        "merge" => {
            let args = parse_merge_args(&rest[1..])?;
            if let MergeOp::Start(src) = &args.op {
                if let Ok(m) = src.parse::<usize>() {
                    if m != n && (1..=trees.len()).contains(&m) {
                        return Err(format!(
                            "merge takes a worktree list: 'git-wt {n},{m} merge' \
                             (or use 'heads/{m}' for a branch of the same name)"
                        ));
                    }
                }
            }
            cmd_merge(root, &trees, idx, &args)
        }
        "merged" => {
            let args = &rest[1..];
            if args.len() > 1 {
                return Err("too many arguments\nTry 'git-wt --help'".into());
            }
            // A worktree-number source uses the list form, as merge and diff do;
            // the single form stays for a branch source, which a list of numbers
            // cannot name. A source equal to the destination falls through to the
            // self-check below for its clearer "already checked out" error.
            if let Some(src) = args.first() {
                if let Ok(m) = src.parse::<usize>() {
                    if m != n && (1..=trees.len()).contains(&m) {
                        return Err(format!(
                            "merged takes a worktree list: 'git-wt {n},{m} merged' \
                             (or use 'heads/{m}' for a branch of the same name)"
                        ));
                    }
                }
            }
            let has_explicit_source = !args.is_empty();
            let src = if has_explicit_source {
                // "git-wt N merged BRANCH" reads dest-first, like merge.
                resolve_merge_source(root, &trees, &args[0])?
            } else {
                // "git-wt N merged" asks whether N's branch is already in the
                // branch we are standing in now.
                ref_of(&trees[idx])?
            };
            let dest = if has_explicit_source {
                ref_of(&trees[idx])?
            } else {
                current_ref()
            };
            // Reject the explicit self-check (1 merged 1) the same way merge
            // does; the bare form (1 merged) intentionally asks about itself.
            if has_explicit_source && src == dest {
                return Err(format!("'{src}' is already checked out in worktree {}", idx + 1));
            }
            cmd_merged(root, &src, &dest)
        }
        // A single target can't be melded, but say so in meld's own terms.
        "meld" => cmd_meld(&trees, &[idx]),
        // An option in the action slot is never right, whatever the option is:
        // each action carries its own, after its own verb.
        other if other.starts_with('-') => Err(format!(
            "'{other}' is an option, not an action; options follow the action, \
             e.g. 'git-wt {n} remove -f' or 'git-wt {n},2 diff --stat'"
        )),
        other => Err(format!(
            "unknown action '{other}' (switch, path, remove, diff, merge, meld, merged)"
        )),
    }
}

/// Recognize a comma-separated target list like `1,2,3`. Returns Ok(None) when
/// the token is not one at all (so the caller keeps looking), and an error when
/// it clearly meant to be one but is malformed (`1,,2`, `1,x`).
fn parse_target_list(tok: &str) -> Result<Option<Vec<usize>>, String> {
    if !tok.contains(',') {
        return Ok(None);
    }
    let mut out = Vec::new();
    for part in tok.split(',') {
        let n: usize = part
            .parse()
            .map_err(|_| format!("bad worktree list '{tok}'; want numbers, e.g. '1,2'"))?;
        out.push(n);
    }
    Ok(Some(out))
}

fn dispatch_targets(root: &Path, ns: &[usize], rest: &[String]) -> Result<(), String> {
    let trees = worktrees(root)?;
    let mut idxs = Vec::new();
    for &n in ns {
        idxs.push(check_index(n, trees.len())?);
    }

    match rest.first().map(String::as_str) {
        Some("meld") => {
            if rest.len() > 1 {
                return Err("meld takes no options\nTry 'git-wt --help'".into());
            }
            cmd_meld(&trees, &idxs)
        }
        Some("diff") => cmd_diff(root, &trees, &idxs, &rest[1..]),
        // `1,2 merge`: the list reads dest-first, so 2 merges into 1.
        Some("merge") => {
            // The list already names the source, so a resume word contradicts
            // it: there is nothing for `continue` to take a source from.
            // Check this before the count so an over-long list with `continue`
            // gets the more useful resume-word message.
            if let Some(word) = rest[1..].iter().find_map(|a| resume_word(a)) {
                return Err(format!(
                    "'{word}' takes no source, so a worktree list has nothing to name\n\
                     hint: 'git-wt {n} merge {word}'",
                    n = ns[0]
                ));
            }
            if idxs.len() != 2 {
                return Err(format!(
                    "merge takes exactly two worktrees, not {}: 'git-wt <N>,<M> merge' \
                     merges M into N",
                    idxs.len()
                ));
            }
            // Hand the source to the single-target parser as the positional it
            // already understands, so both spellings share one code path.
            let mut argv = vec![ns[1].to_string()];
            argv.extend_from_slice(&rest[1..]);
            let args = parse_merge_args(&argv)?;
            cmd_merge(root, &trees, idxs[0], &args)
        }
        // `1,2 merged` == "is 2 already in 1?" — same dest-first reading as merge.
        Some("merged") => {
            if idxs.len() != 2 {
                return Err(format!(
                    "merged takes exactly two worktrees, not {}: 'git-wt <N>,<M> merged' \
                     asks whether M is already in N",
                    idxs.len()
                ));
            }
            if rest.len() > 1 {
                return Err("merged takes no arguments\nTry 'git-wt --help'".into());
            }
            if idxs[0] == idxs[1] {
                return Err(format!("worktree #{} listed twice", idxs[0] + 1));
            }
            let dest = ref_of(&trees[idxs[0]])?;
            let src = ref_of(&trees[idxs[1]])?;
            cmd_merged(root, &src, &dest)
        }
        // A list only makes sense for actions that take more than one worktree.
        Some(other) => Err(format!(
            "'{other}' takes a single worktree; only 'diff', 'meld', 'merge' and 'merged' take a list"
        )),
        None => Err("a worktree list needs an action, e.g. 'git-wt 1,2 diff'".into()),
    }
}

/// The resume word a token spells, in any of its accepted forms, or None.
///
/// Only `continue`/`abort` qualify: they act on a merge that already exists, so
/// they name no source and a worktree list has nothing to hand them. The other
/// keywords — `ours`, `theirs`, `dry-run` — all describe a merge that is about
/// to start, so they combine with a list perfectly well.
fn resume_word(tok: &str) -> Option<&'static str> {
    match tok {
        "continue" | "--continue" | "-c" => Some("continue"),
        "abort" | "--abort" | "-a" => Some("abort"),
        _ => None,
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
    let need_merged = cols.contains(&6);
    let header = !explicit && stdout_tty && mode != ListMode::Short;

    // Right-align the index to the widest possible so filtered output lines up.
    let numw = trees.len().to_string().len();

    // The branch we are standing in; column 6 asks whether each row's branch is
    // already contained in it.
    let here = if need_merged { current_ref() } else { String::new() };

    // Per-row metadata, fetched once (read-only git calls).
    let meta: Vec<(Status, String, String)> = rows
        .iter()
        .map(|(_, w)| {
            let st = if need_status && !w.bare {
                worktree_status(&w.path)
            } else {
                Status::Unknown
            };
            let last = if need_last { last_commit(&w.path) } else { String::new() };
            let merged = if need_merged {
                merged_text(root, w, &here)
            } else {
                String::new()
            };
            (st, last, merged)
        })
        .collect();

    // Plain (uncolored) cells drive column widths; color is applied at print
    // time so the ANSI escapes never skew alignment.
    let cells: Vec<Vec<String>> = rows
        .iter()
        .zip(&meta)
        .map(|((i, w), (st, last, merged))| {
            cols.iter()
                .map(|c| match c {
                    1 => format!("{:>numw$}", i + 1, numw = numw),
                    2 => label(w),
                    3 => w.path.display().to_string(),
                    4 => status_text(*st).to_string(),
                    5 => last.clone(),
                    _ => merged.clone(),
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

    for (row, (st, _, _)) in cells.iter().zip(&meta) {
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
        5 => "last",
        _ => "merged",
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
/// 1=id, 2=branch, 3=dir, 4=status, 5=last-commit, 6=merged.
const COL_HELP: &str = "1=id, 2=branch, 3=dir, 4=status, 5=last, 6=merged";

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
        if n < 1 || n > 6 {
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

/// Short text for the "merged" column: whether `w`'s branch is already in the
/// branch we are standing in (`here`). `-` for bare worktrees or failures.
fn merged_text(root: &Path, w: &Worktree, here: &str) -> String {
    let Some(src) = w.branch.as_deref() else {
        return "-".into();
    };
    match git_cmd(root, &["merge-base", "--is-ancestor", src, here])
        .output()
    {
        Ok(out) => match out.status.code() {
            Some(0) => "merged".into(),
            Some(1) => match ahead_count(root, src, here) {
                Ok(n) => format!("ahead {n}"),
                Err(_) => "ahead".into(),
            },
            _ => "-".into(),
        },
        Err(_) => "-".into(),
    }
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
    let on = color_enabled(std::io::stderr().is_terminal());
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
// Merge: git-wt <N>,<M> merge | --continue | --abort
// ---------------------------------------------------------------------------

/// What a `merge` invocation asks for. `continue`/`abort` resume or undo a
/// merge that is already in progress, so they carry no source and no options.
#[derive(Debug, PartialEq, Eq)]
enum MergeOp {
    Start(String),
    Continue,
    Abort,
}

/// Which side wins a hunk that both branches touched. Maps to git's
/// `-X ours` / `-X theirs`, never `-s ours`: the strategy *option* picks a side
/// only where hunks actually collide, while the whole-tree *strategy* would drop
/// the source's changes entirely and still record a merge — silent data loss.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Ours,
    Theirs,
}

impl Side {
    /// The word the user typed, for echoing back in messages.
    fn word(self) -> &'static str {
        match self {
            Side::Ours => "ours",
            Side::Theirs => "theirs",
        }
    }

    /// git's `-X` argument.
    fn strategy_option(self) -> &'static str {
        self.word()
    }
}

#[derive(Debug, PartialEq, Eq)]
struct MergeArgs {
    op: MergeOp,
    message: Option<String>,
    no_ff: bool,
    ff_only: bool,
    squash: bool,
    force: bool,
    side: Option<Side>,
    dry_run: bool,
}

/// Parse the words after a `merge` verb, in either target form.
///
/// The list form hands its source in as a positional, so both spellings land
/// here with the same shape. A worktree-number source is only legal via the
/// list — `dispatch_target` rejects `git-wt <N> merge <M>` before this parser's
/// result is used — but a branch source and the resume words (`continue`,
/// `abort`, whose source is implicit in the in-progress merge) still arrive
/// from the single-target form.
///
/// The verb-ish words — `continue`, `abort`, `ours`, `theirs`, `dry-run` — each
/// take an optional `--` plus a short form, so `abort`, `--abort` and `-a` are
/// one thing: they read as words in this grammar, and the dashes are noise. The
/// flags that mirror git's own spelling (`--no-ff`, `--squash`, ...) keep their
/// dashes so muscle memory carries over.
///
/// Keywords are matched ahead of the positional source, so a branch actually
/// named `ours` or `abort` must be spelled `heads/ours` to be merged.
fn parse_merge_args(args: &[String]) -> Result<MergeArgs, String> {
    let mut source: Option<String> = None;
    let mut op: Option<MergeOp> = None;
    let mut message = None;
    let mut side: Option<Side> = None;
    let (mut no_ff, mut ff_only, mut squash, mut force, mut dry_run) =
        (false, false, false, false, false);

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "continue" | "--continue" | "-c" => set_merge_op(&mut op, MergeOp::Continue)?,
            "abort" | "--abort" | "-a" => set_merge_op(&mut op, MergeOp::Abort)?,
            "ours" | "--ours" | "-o" => set_side(&mut side, Side::Ours)?,
            "theirs" | "--theirs" | "-t" => set_side(&mut side, Side::Theirs)?,
            "dry-run" | "--dry-run" | "-d" => dry_run = true,
            "-m" | "--message" => {
                message = Some(it.next().ok_or("--message needs a message")?.clone());
            }
            s if s.starts_with("--message=") => {
                message = Some(s["--message=".len()..].to_string())
            }
            "--no-ff" => no_ff = true,
            "--ff-only" => ff_only = true,
            "--squash" => squash = true,
            "-f" | "--force" => force = true,
            s if s.starts_with('-') && s != "-" => {
                return Err(format!("unknown option '{s}' for merge\nTry 'git-wt --help'"));
            }
            s => {
                if source.is_some() {
                    return Err("too many arguments\nTry 'git-wt --help'".into());
                }
                source = Some(s.to_string());
            }
        }
    }

    if no_ff && ff_only {
        return Err("--no-ff and --ff-only conflict".into());
    }
    if squash && no_ff {
        return Err("--squash and --no-ff conflict".into());
    }

    // A resume takes no source and no merge options: those were settled when
    // the merge started, so silently accepting them would be a lie.
    if let Some(op) = op {
        let word = if op == MergeOp::Continue { "continue" } else { "abort" };
        if let Some(s) = source {
            return Err(format!("{word} takes no argument (got '{s}')"));
        }
        // The tempting one: `merge theirs continue` reads as "finish this
        // conflict by taking theirs", but a side is chosen when the merge is
        // computed. Name the real path instead of ignoring the word.
        if let Some(sd) = side {
            return Err(format!(
                "{word} takes no merge options\n\
                 hint: '{w}' is applied when a merge starts, so it cannot join one already stopped\n\
                 hint: 'git-wt <N> merge abort', then re-run the merge with '{w}'",
                w = sd.word()
            ));
        }
        let mut bad = start_only_flags(message.as_ref(), no_ff, ff_only, squash, force);
        if dry_run {
            bad.push("dry-run");
        }
        if !bad.is_empty() {
            return Err(format!("{word} takes no merge options (got {})", bad.join(", ")));
        }
        return Ok(MergeArgs {
            op,
            message: None,
            no_ff: false,
            ff_only: false,
            squash: false,
            force: false,
            side: None,
            dry_run: false,
        });
    }

    // A dry run computes the merge in memory and writes nothing, so the flags
    // that shape a real merge commit have nothing to act on.
    if dry_run {
        let bad = start_only_flags(message.as_ref(), no_ff, ff_only, squash, force);
        if !bad.is_empty() {
            return Err(format!("dry-run takes no merge options (got {})", bad.join(", ")));
        }
    }

    let source = source.ok_or(
        "merge needs a source: 'git-wt <N>,<M> merge' \
         (or 'git-wt <N> merge <BRANCH>', or continue/abort)",
    )?;
    Ok(MergeArgs { op: MergeOp::Start(source), message, no_ff, ff_only, squash, force, side, dry_run })
}

/// Record `ours`/`theirs`. They are opposite answers to one question, so asking
/// for both is a mistake worth naming; asking twice for the same side is not.
fn set_side(slot: &mut Option<Side>, side: Side) -> Result<(), String> {
    match slot {
        Some(s) if *s == side => Ok(()),
        Some(_) => Err("ours and theirs conflict".into()),
        None => {
            *slot = Some(side);
            Ok(())
        }
    }
}

/// Record `continue`/`abort`. Like `set_side`, they are opposite answers to one
/// question, so asking for both is a mistake; asking twice for the same one is
/// only redundant.
fn set_merge_op(slot: &mut Option<MergeOp>, op: MergeOp) -> Result<(), String> {
    match slot {
        Some(cur) if *cur == op => Ok(()),
        Some(_) => Err("continue and abort conflict".into()),
        None => {
            *slot = Some(op);
            Ok(())
        }
    }
}

/// The flags that only mean something when a real merge runs, as the user would
/// type them. Some shape the resulting commit (`-m`, `--no-ff`, `--squash`),
/// others gate whether it may run at all (`--ff-only`, `-f`) — either way they
/// need a merge that is about to start, which is exactly what `continue`/
/// `abort` (one already stopped) and `dry-run` (none at all) do not have.
///
/// Shared so all three can name what they are rejecting rather than just
/// saying "no merge options".
fn start_only_flags(
    message: Option<&String>,
    no_ff: bool,
    ff_only: bool,
    squash: bool,
    force: bool,
) -> Vec<&'static str> {
    let mut v = Vec::new();
    if message.is_some() {
        v.push("-m");
    }
    if no_ff {
        v.push("--no-ff");
    }
    if ff_only {
        v.push("--ff-only");
    }
    if squash {
        v.push("--squash");
    }
    if force {
        v.push("-f");
    }
    v
}

fn cmd_merge(
    root: &Path,
    trees: &[Worktree],
    idx: usize,
    args: &MergeArgs,
) -> Result<(), String> {
    let dest = &trees[idx];
    if dest.bare {
        return Err("cannot merge into a bare worktree".into());
    }
    let dir = dest.path.as_path();
    let in_progress = git_quiet(dir, &["rev-parse", "--verify", "-q", "MERGE_HEAD"]);
    let color = std::io::stderr().is_terminal() && color_enabled(true);

    match &args.op {
        MergeOp::Abort => {
            if !in_progress {
                return Err(format!("no merge in progress in {}", dir.display()));
            }
            git_run(dir, &["merge", "--abort"])?;
            eprintln!("{} merge in {}", paint("Aborted", GREEN, color), leaf_of(dir));
            return Ok(());
        }
        MergeOp::Continue => {
            if !in_progress {
                return Err(format!("no merge in progress in {}", dir.display()));
            }
            // Unresolved paths make `git merge --continue` fail with a terse
            // message; naming the files is what the user actually needs.
            let stuck = conflicted_files(dir);
            if !stuck.is_empty() {
                return Err(conflict_msg(dir, &stuck, idx));
            }
            git_run_no_editor(dir, &["merge", "--continue"])?;
            eprintln!("{} merge in {}", paint("Completed", GREEN, color), leaf_of(dir));
            return Ok(());
        }
        MergeOp::Start(_) => {}
    }

    let MergeOp::Start(source) = &args.op else { unreachable!() };
    let src_branch = resolve_merge_source(root, trees, source)?;

    if dest.branch.as_deref() == Some(src_branch.as_str()) {
        return Err(format!("'{src_branch}' is already checked out in worktree {}", idx + 1));
    }

    // A dry run only reads, so it answers before any of the guards below: it
    // never prompts, never writes, and is happy to report on a merge that is
    // currently stopped.
    if args.dry_run {
        return merge_dry_run(dir, &src_branch, &label(dest), color);
    }

    if in_progress {
        // `ours`/`theirs` are applied while the merge is computed, so they can't
        // join one that has already stopped. Redoing it from a clean state is
        // the only way to honor the word — and it costs whatever resolution has
        // been done in that tree, so it is the user's call, not ours.
        let Some(sd) = args.side else {
            return Err(format!(
                "a merge is already in progress in {}\n\
                 hint: 'git-wt {n} merge continue' or 'git-wt {n} merge abort'",
                dir.display(),
                n = idx + 1
            ));
        };
        eprintln!(
            "A merge is already in progress in {}, and '{}' only applies when a merge starts.",
            dir.display(),
            sd.word()
        );
        // Mid-merge, tracked changes are the half-resolved conflict itself —
        // but a merge started with -f over a dirty tree buried the user's own
        // edits in there too, and `merge --abort` unwinds the lot. Say so only
        // when there is actually something to lose.
        let at_risk = git_stdout(dir, &["status", "--porcelain"])
            .map(|p| has_tracked_changes(&p))
            .unwrap_or(true);
        let cost = if at_risk {
            "Uncommitted changes in that tree, including any conflict resolution \
             already done, are discarded"
        } else {
            "Any conflict resolution already done there is discarded"
        };
        if !confirm(&format!(
            "Abort it and re-merge '{src_branch}' with '{}'? {cost}. [y/N] ",
            sd.word()
        ))? {
            eprintln!("Aborted.");
            return Ok(());
        }
        git_run(dir, &["merge", "--abort"])?;
        eprintln!("{} the previous merge", paint("Abandoned", GREEN, color));
    }

    // A merge into uncommitted work can end with the user's own edits tangled in
    // conflict markers, so it takes an explicit --force. A status git can't read
    // is not a green light, so the error propagates rather than merging blind.
    if !args.force {
        let porcelain = git_stdout(dir, &["status", "--porcelain"])?;
        if has_tracked_changes(&porcelain) {
            return Err(format!(
                "worktree {} has uncommitted changes\nhint: commit or stash them, or re-run with -f",
                idx + 1
            ));
        }
    }

    let mut argv: Vec<String> = vec!["merge".into()];
    if args.no_ff {
        argv.push("--no-ff".into());
    }
    if args.ff_only {
        argv.push("--ff-only".into());
    }
    if args.squash {
        argv.push("--squash".into());
    }
    if let Some(sd) = args.side {
        argv.extend(["-X".into(), sd.strategy_option().into()]);
    }
    if let Some(m) = &args.message {
        argv.extend(["-m".into(), m.clone()]);
    }
    argv.push(src_branch.clone());

    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    if let Err(e) = git_run_no_editor(dir, &refs) {
        let stuck = conflicted_files(dir);
        if stuck.is_empty() {
            return Err(e);
        }
        return Err(conflict_msg(dir, &stuck, idx));
    }

    let into = label(dest);
    let what = if args.squash { "Squashed" } else { "Merged" };
    // Name the side when one was forced: it silently decided every collision,
    // so it belongs in the record of what just happened.
    let how = match args.side {
        Some(sd) => format!("{}, {} won conflicts", leaf_of(dir), sd.word()),
        None => leaf_of(dir),
    };
    eprintln!("{} {src_branch} into {into}  ({how})", paint(what, GREEN, color));
    if args.squash {
        eprintln!("hint: the merge is staged but not committed");
    }
    Ok(())
}

/// Report whether `src` would merge into the worktree's HEAD, touching nothing.
///
/// `git merge-tree --write-tree` does the whole job: it resolves the merge into
/// a tree object and exits 1 when a path conflicts, with no index, no checkout
/// and nothing to clean up afterwards. It needs git 2.38+, which is checked by
/// running it rather than by parsing `git --version`.
fn merge_dry_run(dir: &Path, src: &str, into: &str, color: bool) -> Result<(), String> {
    let out = git_cmd(dir, &["merge-tree", "--write-tree", "--name-only", "HEAD", src])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    match out.status.code() {
        Some(0) => {
            eprintln!(
                "{} {src} merges into {into} cleanly",
                paint("Clean", GREEN, color)
            );
            Ok(())
        }
        // Conflicts. stdout is the resulting tree's oid, then the paths that
        // collided. Exiting nonzero keeps `if git-wt 1,2 merge dry-run; then`
        // meaningful, and mirrors what a real merge does on a conflict.
        Some(1) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let files: Vec<String> = stdout
                .lines()
                .skip(1)
                .filter(|l| !l.trim().is_empty())
                .map(str::to_string)
                .collect();
            let mut m = format!("{src} does NOT merge into {into} cleanly\n");
            for f in &files {
                m.push_str(&format!("  {f}\n"));
            }
            m.push_str("hint: nothing was changed — this was a dry run\n");
            m.push_str("hint: 'ours' or 'theirs' would settle these automatically");
            Err(m)
        }
        _ => {
            let err = String::from_utf8_lossy(&out.stderr);
            // Older git has merge-tree, but not --write-tree.
            if err.contains("unknown option") || err.contains("usage:") {
                return Err("dry-run needs git 2.38 or newer (git merge-tree --write-tree)".into());
            }
            Err(err.trim().to_string())
        }
    }
}

// ---------------------------------------------------------------------------
// Merged: git-wt <N> merged [<M|BRANCH>] | git-wt <N>,<M> merged
// ---------------------------------------------------------------------------

/// Report whether `src` is already an ancestor of `dest`.
///
/// `git merge-base --is-ancestor` exits 0 when src is contained in dest, 1 when
/// it is not, and anything else is a real error. This is the same exit-code
/// contract `merge_dry_run` already uses, so `if git-wt 1 merged; then ...` works.
fn cmd_merged(dir: &Path, src: &str, dest: &str) -> Result<(), String> {
    let out = git_cmd(dir, &["merge-base", "--is-ancestor", src, dest])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    let color = std::io::stderr().is_terminal() && color_enabled(true);

    match out.status.code() {
        Some(0) => {
            eprintln!(
                "{} {src} is already in {dest}",
                paint("Merged", GREEN, color)
            );
            Ok(())
        }
        Some(1) => {
            let count_msg = match ahead_count(dir, src, dest) {
                Ok(n) => format!("ahead {n}"),
                Err(_) => "ahead".to_string(),
            };
            Err(format!("Ahead {src} is NOT in {dest} ({count_msg})"))
        }
        _ => {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(err.trim().to_string())
        }
    }
}

/// Number of commits in `src` that are not in `dest` (`dest..src`).
fn ahead_count(dir: &Path, src: &str, dest: &str) -> Result<usize, String> {
    let s = git_stdout(dir, &["rev-list", "--count", &format!("{dest}..{src}")])?;
    s.trim()
        .parse()
        .map_err(|e| format!("could not parse ahead count: {e}"))
}

/// Resolve a merge source word to a branch name. A number that names a
/// worktree wins over a same-named branch: numbers are this tool's grammar.
fn resolve_merge_source(
    root: &Path,
    trees: &[Worktree],
    source: &str,
) -> Result<String, String> {
    if let Ok(n) = source.parse::<usize>() {
        if n >= 1 && n <= trees.len() {
            let w = &trees[n - 1];
            return w.branch.clone().ok_or_else(|| {
                format!("worktree {n} is {} — no branch to merge", label(w))
            });
        }
    }
    if git_quiet(root, &["rev-parse", "--verify", "-q", &format!("{source}^{{commit}}")]) {
        return Ok(source.to_string());
    }
    Err(format!(
        "no worktree or branch '{source}' (see 'git-wt list')"
    ))
}

/// Whether `git status --porcelain` reports changes to *tracked* files, staged
/// or not. Untracked files don't count: a merge that would overwrite one makes
/// git refuse on its own, so they are no reason to demand `-f`. Deliberately
/// not `classify_status`, which collapses a tree holding both kinds to
/// `Untracked` and would wave those tracked edits through.
fn has_tracked_changes(porcelain: &str) -> bool {
    porcelain
        .lines()
        .any(|l| !l.trim().is_empty() && !l.starts_with("??"))
}

/// Paths with unresolved conflicts in a worktree, one per line.
fn conflicted_files(dir: &Path) -> Vec<String> {
    git_stdout(dir, &["diff", "--name-only", "--diff-filter=U"])
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// The message shown when a merge stops on conflicts: where it stopped, what
/// is conflicted, and the two ways out.
fn conflict_msg(dir: &Path, files: &[String], idx: usize) -> String {
    let mut m = format!("merge conflict in {}\n", dir.display());
    for f in files {
        m.push_str(&format!("  {f}\n"));
    }
    let n = idx + 1;
    m.push_str(&format!(
        "hint: resolve them there, 'git add' each, then 'git-wt {n} merge continue'\n\
         hint: or undo the merge with 'git-wt {n} merge abort'\n\
         hint: or redo it letting one side win: 'git-wt {n} merge abort', then \
         'git-wt {n},<M> merge theirs'"
    ));
    m
}

// ---------------------------------------------------------------------------
// Diff: git-wt <N>,<M> diff [..|...] [flags] [-- PATH...]
// ---------------------------------------------------------------------------

/// The committed state a worktree points at. A branch name reads better in
/// diff headers than a sha, so prefer it; detached/bare use the short sha.
fn ref_of(w: &Worktree) -> Result<String, String> {
    if let Some(b) = &w.branch {
        return Ok(b.clone());
    }
    let sha = git_stdout(&w.path, &["rev-parse", "--short", "HEAD"])
        .map_err(|e| format!("worktree {} has no HEAD: {e}", w.path.display()))?;
    Ok(sha.trim().to_string())
}

/// Diff two worktrees, as `git diff <ref1><dots><ref2>`.
///
/// Refs, not directories: a directory diff would drag in build output and
/// everything else .gitignore exists to hide. That also means uncommitted work
/// is invisible here, so warn when either side is dirty and point at meld.
fn cmd_diff(root: &Path, trees: &[Worktree], idxs: &[usize], rest: &[String]) -> Result<(), String> {
    let (idx, other) = match idxs {
        [a, b] => (*a, *b),
        _ => {
            return Err(format!(
                "diff takes exactly two worktrees, got {}\nhint: 'git-wt 1,2,3 meld' compares three",
                idxs.len()
            ));
        }
    };
    if other == idx {
        return Err(format!(
            "worktree #{} against itself is always empty",
            idx + 1
        ));
    }

    let a = ref_of(&trees[idx])?;
    let b = ref_of(&trees[other])?;

    // rather than becoming a flag with a new name to learn. `live`/`hunks` are
    // bare words for the same reason `..` is: they read as part of the sentence.
    // A pathspec can never be mistaken for one, since pathspecs follow `--`.
    // Settled before the main pass, so the unknown-argument hint below is right
    // whatever the word order: '1,2 diff -w live' must not be told to go run a
    // ref diff. Stops at `--`, where a *pathspec* named 'live' could begin.
    let live = rest
        .iter()
        .take_while(|a| a.as_str() != "--")
        .any(|a| a == "live" || a == "--live");

    let mut dots: Option<&str> = None;
    let mut hunks = false;
    let mut listing: Option<String> = None;
    let mut paths: Vec<String> = Vec::new();
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            ".." => dots = Some(".."),
            "..." => dots = Some("..."),
            // Already counted by the pre-scan.
            "live" | "--live" => {}
            "hunks" | "--hunks" => hunks = true,
            // Everything past `--` is a pathspec; git validates it, not us.
            "--" => {
                paths.extend(it.cloned());
                break;
            }
            "--name-only" | "--name-status" | "--stat" => listing = Some(arg.clone()),
            unknown => {
                // Under `live` there is no single git command to hand off to --
                // that is the whole reason `live` exists -- so pointing at one
                // would contradict the mode the user is already in.
                let hint = if live {
                    "hint: live has no git equivalent to defer to; \
                     'git diff --no-index <dir A>/<file> <dir B>/<file>' is the \
                     closest, one file at a time"
                        .to_string()
                } else {
                    let d = dots.unwrap_or("...");
                    format!(
                        "hint: for any other git flag, run git itself: \
                         git diff {a}{d}{b} {unknown}"
                    )
                };
                return Err(format!(
                    "unexpected argument '{unknown}' for diff\n\
                     diff takes live, hunks, .., ..., --name-only, --name-status, \
                     --stat, -- PATH...\n\
                     {hint}"
                ));
            }
        }
    }

    // A range is a statement about refs. `live` never looks at a ref, so the
    // two cannot both be honored -- silently dropping one would be worse.
    if live {
        if let Some(d) = dots {
            return Err(format!(
                "'live' and '{d}' cannot combine: a range compares commits, \
                 live compares the files on disk\n\
                 hint: drop '{d}' for live contents, or drop 'live' for the range"
            ));
        }
    }
    if let (true, Some(l)) = (hunks, listing.as_deref()) {
        return Err(format!(
            "'hunks' and '{l}' cannot combine: hunks prints line numbers per file, \
             {l} prints a listing"
        ));
    }

    let on_err = color_enabled(std::io::stderr().is_terminal());
    // `live` is the answer to the dirty warning, so it does not get warned at.
    if !live {
        for &i in &[idx, other] {
            if is_dirty(&trees[i].path) {
                eprintln!(
                    "{} #{} {} has uncommitted changes; this diff is committed state only \
                     (try 'git-wt {},{} diff live')",
                    paint("warning:", YELLOW, on_err),
                    i + 1,
                    label(&trees[i]),
                    idx + 1,
                    other + 1
                );
            }
        }
    }

    // '...' by default so a bare '1,2 diff' previews '1,2 merge': the range
    // holds M's commits since the fork and nothing of N's, which is what the
    // merge brings in. '..' answers a different question -- tip vs tip -- and
    // reports N's own commits as deletions, which reads as a huge phantom diff
    // on branches that have diverged at all.
    let dots = dots.unwrap_or("...");
    if live {
        let files = live_diff(
            root,
            &trees[idx].path,
            &trees[other].path,
            &paths,
            // --name-only/--name-status answer "which files", which the byte
            // compare already knows -- no per-file git process needed. Every
            // other view prints counts, which only the patch can supply.
            !matches!(listing.as_deref(), Some("--name-only") | Some("--name-status")),
        )?;
        let head = format!("diff {a} ↔ {b}   live — literal contents, .gitignore honored");
        return render(&files, &head, listing.as_deref(), hunks);
    }
    if hunks {
        let files = ref_diff(root, &format!("{a}{dots}{b}"), &paths)?;
        let head = format!("diff {a} ↔ {b}   {a}{dots}{b} — committed state");
        return render(&files, &head, None, true);
    }

    let mut argv: Vec<String> = Vec::new();
    if let Some(l) = &listing {
        argv.push(l.clone());
    }
    if !paths.is_empty() {
        argv.push("--".into());
        argv.extend(paths);
    }

    // Inherit stdio so git's own pager and color logic apply, exactly as a
    // hand-typed `git diff` would.
    let status = git_cmd(root, &[])
        .arg("diff")
        .arg(format!("{a}{dots}{b}"))
        .args(&argv)
        .status()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !status.success() {
        return Err("git diff exited with an error".into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// live: compare worktrees by file content instead of by commit
// ---------------------------------------------------------------------------

/// One hunk, reduced to what the `hunks` view prints: where it lands on the
/// `+` side, and what kind of change it is.
struct Hunk {
    line: usize,
    kind: &'static str,
    count: usize,
}

/// One differing path. `status` is A/M/D from the union of both sides, so a
/// file that is untracked-and-new on the `+` side is genuinely an add.
struct FileDiff {
    path: String,
    status: char,
    plus: usize,
    minus: usize,
    binary: bool,
    hunks: Vec<Hunk>,
}

/// Paths worth considering in a worktree: tracked, plus untracked that
/// `.gitignore` does not hide. Only git knows this set -- `diff -rq` would
/// drown in `target/`. `-z` because a path may contain anything but NUL.
fn live_files(dir: &Path, paths: &[String]) -> Result<Vec<String>, String> {
    let mut args: Vec<&str> = vec!["ls-files", "-z", "--cached", "--others", "--exclude-standard"];
    if !paths.is_empty() {
        args.push("--");
        args.extend(paths.iter().map(String::as_str));
    }
    let out = git_stdout(dir, &args)?;
    Ok(out
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}

/// Byte-for-byte equality. Length first, so unequal files usually cost one
/// `stat` each. An unreadable file counts as differing: better to show it and
/// let the diff report why than to silently call it unchanged.
fn same_bytes(a: &Path, b: &Path) -> bool {
    match (a.metadata(), b.metadata()) {
        (Ok(ma), Ok(mb)) if ma.len() != mb.len() => return false,
        (Ok(_), Ok(_)) => {}
        _ => return false,
    }
    match (std::fs::read(a), std::fs::read(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// The union of both worktrees' candidate paths, filtered to those that
/// actually differ on disk. With `content`, each survivor also gets a
/// `git diff --no-index` run for its counts and hunks.
fn live_diff(
    root: &Path,
    a_dir: &Path,
    b_dir: &Path,
    paths: &[String],
    content: bool,
) -> Result<Vec<FileDiff>, String> {
    let mut union: Vec<String> = live_files(a_dir, paths)?;
    union.extend(live_files(b_dir, paths)?);
    union.sort();
    union.dedup();

    let mut out = Vec::new();
    for p in union {
        let pa = a_dir.join(&p);
        let pb = b_dir.join(&p);
        // `--cached` lists index entries, so a path can be listed on a side
        // where the file is gone. Absent from both is nothing to report.
        let (ea, eb) = (pa.is_file(), pb.is_file());
        let status = match (ea, eb) {
            (false, false) => continue,
            (false, true) => 'A',
            (true, false) => 'D',
            (true, true) => {
                if same_bytes(&pa, &pb) {
                    continue;
                }
                'M'
            }
        };
        let mut fd = FileDiff {
            path: p,
            status,
            plus: 0,
            minus: 0,
            binary: false,
            hunks: Vec::new(),
        };
        if content {
            // Substituting /dev/null for the missing side turns a one-sided
            // file into real hunks instead of an error.
            let null = PathBuf::from("/dev/null");
            let text = no_index_diff(
                root,
                if ea { &pa } else { &null },
                if eb { &pb } else { &null },
                &fd.path,
            )?;
            parse_patch_into(&text, &mut fd);
        }
        out.push(fd);
    }
    Ok(out)
}

/// `git diff --no-index` on two literal paths: git ignoring that it is git.
/// It exits 1 to mean "they differ", which is the expected case here, so only
/// a code above 1 is a real failure. `show` names the path for errors, since
/// `a`/`b` may be absolute, or /dev/null for a one-sided file.
///
/// `root` is only the process's cwd; `--no-index` resolves the two paths
/// itself and never consults a repo, so any existing directory would do.
fn no_index_diff(root: &Path, a: &Path, b: &Path, show: &str) -> Result<String, String> {
    let out = git_cmd(root, &["diff", "--no-index", "-U0", "--no-color"])
        .arg(a)
        .arg(b)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    match out.status.code() {
        Some(0) | Some(1) => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
        _ => Err(format!(
            "git diff --no-index failed on '{show}': {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
    }
}

/// The `hunks` view over a ref diff. Line numbers are just as useful against
/// commits, so `hunks` does not require `live`.
fn ref_diff(root: &Path, range: &str, paths: &[String]) -> Result<Vec<FileDiff>, String> {
    // A rename reported as one entry would have no single `+` side to number;
    // --no-renames splits it back into the add and the delete `live` would
    // have seen anyway, so the two views agree.
    let mut args: Vec<&str> = vec!["diff", "-U0", "--no-color", "--no-renames", range];
    if !paths.is_empty() {
        args.push("--");
        args.extend(paths.iter().map(String::as_str));
    }
    let text = git_stdout(root, &args)?;
    Ok(split_patch(&text))
}

/// Split a multi-file patch on its `diff --git` headers. The path comes from
/// the `+++ b/` line, falling back to `--- a/` for a deletion, where the `+`
/// side is /dev/null.
fn split_patch(text: &str) -> Vec<FileDiff> {
    let mut out: Vec<FileDiff> = Vec::new();
    let mut cur: Option<FileDiff> = None;
    let mut in_hunks = false;
    for line in text.lines() {
        if line.starts_with("diff --git ") {
            if let Some(f) = cur.take() {
                out.push(f);
            }
            cur = Some(FileDiff {
                path: String::new(),
                status: 'M',
                plus: 0,
                minus: 0,
                binary: false,
                hunks: Vec::new(),
            });
            in_hunks = false;
            continue;
        }
        let Some(f) = cur.as_mut() else { continue };
        if !in_hunks {
            if let Some(p) = line.strip_prefix("--- ") {
                if p == "/dev/null" {
                    f.status = 'A';
                } else if f.path.is_empty() {
                    f.path = p.strip_prefix("a/").unwrap_or(p).to_string();
                }
                continue;
            }
            if let Some(p) = line.strip_prefix("+++ ") {
                if p == "/dev/null" {
                    f.status = 'D';
                } else {
                    f.path = p.strip_prefix("b/").unwrap_or(p).to_string();
                }
                continue;
            }
        }
        if line.starts_with("@@") {
            in_hunks = true;
        }
        eat_patch_line(line, f);
    }
    if let Some(f) = cur.take() {
        out.push(f);
    }
    out
}

/// Fold one file's `-U0` patch into `fd`'s counts and hunks.
fn parse_patch_into(text: &str, fd: &mut FileDiff) {
    let mut in_hunks = false;
    for line in text.lines() {
        if line.starts_with("@@") {
            in_hunks = true;
        }
        // The `---`/`+++` headers are +/- lines to a naive counter; skipping
        // everything before the first `@@` keeps them out of the totals.
        if !in_hunks && !line.starts_with("Binary files ") {
            continue;
        }
        eat_patch_line(line, fd);
    }
}

fn eat_patch_line(line: &str, fd: &mut FileDiff) {
    if line.starts_with("Binary files ") {
        fd.binary = true;
        return;
    }
    if line.starts_with("@@") {
        if let Some(h) = parse_hunk_header(line) {
            fd.hunks.push(h);
        }
        return;
    }
    if line.starts_with('+') {
        fd.plus += 1;
    } else if line.starts_with('-') {
        fd.minus += 1;
    }
}

/// `@@ -oldStart,oldCount +newStart,newCount @@`. Two traps live here: an
/// omitted count means 1, and a zero count is not an edit -- `old == 0` is a
/// pure insertion, `new == 0` a pure deletion. Labeling off the new-side
/// number alone would report every deletion as `+0`.
fn parse_hunk_header(line: &str) -> Option<Hunk> {
    let mut it = line.split_whitespace();
    it.next()?; // @@
    let (_, old_count) = parse_range(it.next()?)?;
    let (new_start, new_count) = parse_range(it.next()?)?;
    let (kind, count) = match (old_count, new_count) {
        (0, n) => ("added", n),
        (o, 0) => ("deleted", o),
        (_, n) => ("modified", n),
    };
    Some(Hunk {
        line: new_start,
        kind,
        count,
    })
}

/// `-119,3` / `+119` -> (start, count). No comma means a count of 1.
fn parse_range(tok: &str) -> Option<(usize, usize)> {
    let body = tok.strip_prefix('-').or_else(|| tok.strip_prefix('+'))?;
    match body.split_once(',') {
        Some((s, c)) => Some((s.parse().ok()?, c.parse().ok()?)),
        None => Some((body.parse().ok()?, 1)),
    }
}

// ---------------------------------------------------------------------------
// live: output
// ---------------------------------------------------------------------------

fn status_paint(s: char) -> &'static str {
    match s {
        'A' => GREEN,
        'D' => RED,
        _ => YELLOW,
    }
}

fn render(
    files: &[FileDiff],
    head: &str,
    listing: Option<&str>,
    hunks: bool,
) -> Result<(), String> {
    let on = color_enabled(std::io::stdout().is_terminal());

    // Silence is the right answer for "nothing differs", but on stdout it is
    // indistinguishable from the empty ref diff `live` exists to fix. Say so
    // on stderr, where it cannot corrupt a pipe -- from every view, so that
    // "no output" never means two different things depending on the flags.
    if files.is_empty() {
        eprintln!("no differences");
        return Ok(());
    }

    match listing {
        Some("--name-only") => {
            for f in files {
                println!("{}", f.path);
            }
            return Ok(());
        }
        Some("--name-status") => {
            for f in files {
                println!("{}\t{}", f.status, f.path);
            }
            return Ok(());
        }
        Some("--stat") => return render_stat(files, on),
        _ => {}
    }

    println!("{}\n", paint(head, DIM, on));
    let w = files.iter().map(|f| f.path.len()).max().unwrap_or(0);
    let pw = files
        .iter()
        .map(|f| format!("+{}", f.plus).len())
        .max()
        .unwrap_or(1);
    for f in files {
        let counts = if f.binary {
            "binary".to_string()
        } else {
            format!(
                "{:<pw$} {}",
                paint(&format!("+{}", f.plus), GREEN, on),
                paint(&format!("−{}", f.minus), RED, on),
                // `{:<n}` pads to a byte count, and paint() added bytes that
                // occupy no columns: "\x1b[" + GREEN + "m" ... RESET. Hence
                // +3 -- the two bytes of "\x1b[" plus the "m".
                pw = pw + if on { GREEN.len() + RESET.len() + 3 } else { 0 }
            )
        };
        println!(
            "{} {:<w$}  {}",
            paint(&f.status.to_string(), status_paint(f.status), on),
            f.path,
            counts,
            w = w
        );
        if hunks {
            // Right-align to this file's widest line number so the numbers
            // form a column, without padding every file out to a fixed width.
            let lw = f
                .hunks
                .iter()
                .map(|h| h.line.to_string().len())
                .max()
                .unwrap_or(1);
            for h in &f.hunks {
                println!("      {:>lw$}  {} {}", h.line, h.kind, h.count, lw = lw);
            }
        }
    }
    println!("\n{}", paint(&summary(files), DIM, on));
    Ok(())
}

/// `git diff --stat`'s shape: a churn bar per file, scaled so the widest row
/// fits, then the same summary line.
fn render_stat(files: &[FileDiff], on: bool) -> Result<(), String> {
    const BAR: usize = 40;
    let w = files.iter().map(|f| f.path.len()).max().unwrap_or(0);
    let max = files
        .iter()
        .map(|f| f.plus + f.minus)
        .max()
        .unwrap_or(0)
        .max(1);
    let nw = files
        .iter()
        .map(|f| (f.plus + f.minus).to_string().len())
        .max()
        .unwrap_or(1);
    for f in files {
        if f.binary {
            println!(" {:<w$} | Bin", f.path, w = w);
            continue;
        }
        let total = f.plus + f.minus;
        // Scale only when the widest row would overflow, so small diffs show
        // their exact churn one character per line, as git does.
        let cell = |n: usize| -> usize {
            if max <= BAR {
                n
            } else if n == 0 {
                0
            } else {
                (n * BAR / max).max(1)
            }
        };
        // An empty run must stay empty: painting "" would emit a colour code
        // wrapping nothing, which is invisible but real bytes on the pipe.
        let run = |n: usize, ch: &str, col: &str| match n {
            0 => String::new(),
            _ => paint(&ch.repeat(n), col, on),
        };
        let bar = format!(
            "{}{}",
            run(cell(f.plus), "+", GREEN),
            run(cell(f.minus), "-", RED)
        );
        println!(" {:<w$} | {:>nw$} {}", f.path, total, bar, w = w, nw = nw);
    }
    println!("{}", paint(&summary(files), DIM, on));
    Ok(())
}

/// git's own phrasing, singulars and all, so the line reads the same as the
/// `--stat` a user would get once the work is committed.
fn summary(files: &[FileDiff]) -> String {
    let p: usize = files.iter().map(|f| f.plus).sum();
    let m: usize = files.iter().map(|f| f.minus).sum();
    let mut s = format!(
        "{} file{} changed",
        files.len(),
        if files.len() == 1 { "" } else { "s" }
    );
    if p > 0 {
        s += &format!(", {p} insertion{}(+)", if p == 1 { "" } else { "s" });
    }
    if m > 0 {
        s += &format!(", {m} deletion{}(-)", if m == 1 { "" } else { "s" });
    }
    s
}

/// Does the worktree have uncommitted tracked changes or untracked files?
/// Unknown (bare, or git failed) counts as not dirty: no warning beats a wrong
/// one. Porcelain stays interpreted in exactly one place, `classify_status`.
fn is_dirty(dir: &Path) -> bool {
    matches!(worktree_status(dir), Status::Dirty | Status::Untracked)
}

// ---------------------------------------------------------------------------
// Meld: git-wt <N>,<N>[,<N>] meld
// ---------------------------------------------------------------------------

/// Open meld on 2-3 worktree directories, in the order given, and wait for it.
/// meld itself is the arbiter of 2-way vs 3-way, so we only pass the paths.
fn cmd_meld(trees: &[Worktree], idxs: &[usize]) -> Result<(), String> {
    match idxs.len() {
        2 | 3 => {}
        1 => return Err("meld needs 2 or 3 worktrees, e.g. 'git-wt 1,2 meld'".into()),
        n => return Err(format!("meld takes at most 3 worktrees, got {n}")),
    }

    // meld would silently show a directory against itself; that is never meant.
    for (i, a) in idxs.iter().enumerate() {
        if idxs[i + 1..].contains(a) {
            return Err(format!("worktree #{} listed twice", a + 1));
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

/// Is `cmd` an executable file on PATH? Checked before spawning so a missing
/// tool is a clear error rather than an opaque NotFound from the OS.
fn on_path(cmd: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let p = dir.join(cmd);
        p.is_file() && is_executable(&p)
    })
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(_p: &Path) -> bool {
    true
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
fn find_remote_branch(root: &Path, branch: &str) -> Option<String> {
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

/// Run git with the editor disabled. We capture git's output, so a spawned
/// editor would have no terminal and hang; instead git's default commit message
/// is taken as-is (`-m` is how a user overrides it).
fn git_run_no_editor(dir: &Path, args: &[&str]) -> Result<(), String> {
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
    fn parse_cols_accepts_status_last_and_merged() {
        assert_eq!(parse_cols("1,4,5").unwrap(), vec![1, 4, 5]);
        assert_eq!(parse_cols("1,2,6").unwrap(), vec![1, 2, 6]);
        assert!(parse_cols("7").is_err());
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

    fn merge_args(args: &[&str]) -> Result<MergeArgs, String> {
        let v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse_merge_args(&v)
    }

    #[test]
    fn tracked_changes_ignore_untracked_only() {
        assert!(!has_tracked_changes(""));
        assert!(!has_tracked_changes("?? new.txt"));
        assert!(!has_tracked_changes("?? a\n?? b"));
        assert!(has_tracked_changes(" M src/main.rs"));
        assert!(has_tracked_changes("A  staged.rs"));
        // The case classify_status collapses to Untracked: tracked edits are
        // still present, so a merge here needs -f.
        assert!(has_tracked_changes("?? new.txt\n M src/main.rs"));
        assert!(has_tracked_changes(" M src/main.rs\n?? new.txt"));
        assert_eq!(classify_status(" M a\n?? b"), Status::Untracked); // why not classify_status
    }

    #[test]
    fn merge_parses_source_and_options() {
        let a = merge_args(&["2"]).unwrap();
        assert_eq!(a.op, MergeOp::Start("2".into()));
        assert!(!a.no_ff && !a.squash && !a.force && a.message.is_none());

        let a = merge_args(&["feat/x", "--no-ff", "-m", "sync", "-f"]).unwrap();
        assert_eq!(a.op, MergeOp::Start("feat/x".into()));
        assert!(a.no_ff && a.force);
        assert_eq!(a.message.as_deref(), Some("sync"));

        assert_eq!(merge_args(&["2", "--message=hi"]).unwrap().message.as_deref(), Some("hi"));
    }

    #[test]
    fn merge_accepts_bare_and_dashed_resume_words() {
        assert_eq!(merge_args(&["continue"]).unwrap().op, MergeOp::Continue);
        assert_eq!(merge_args(&["--continue"]).unwrap().op, MergeOp::Continue);
        assert_eq!(merge_args(&["abort"]).unwrap().op, MergeOp::Abort);
        assert_eq!(merge_args(&["--abort"]).unwrap().op, MergeOp::Abort);
    }

    /// Every keyword means the same thing bare, dashed, or short.
    #[test]
    fn merge_words_take_optional_dashes_and_shorts() {
        for (bare, dashed, short) in [
            ("continue", "--continue", "-c"),
            ("abort", "--abort", "-a"),
        ] {
            let want = merge_args(&[bare]).unwrap().op;
            assert_eq!(merge_args(&[dashed]).unwrap().op, want, "{dashed}");
            assert_eq!(merge_args(&[short]).unwrap().op, want, "{short}");
        }
        for (bare, dashed, short, want) in [
            ("ours", "--ours", "-o", Side::Ours),
            ("theirs", "--theirs", "-t", Side::Theirs),
        ] {
            for w in [bare, dashed, short] {
                assert_eq!(merge_args(&["2", w]).unwrap().side, Some(want), "{w}");
            }
        }
        for w in ["dry-run", "--dry-run", "-d"] {
            assert!(merge_args(&["2", w]).unwrap().dry_run, "{w}");
        }
    }

    #[test]
    fn merge_side_maps_to_strategy_option() {
        // -X ours / -X theirs, never -s ours: the whole-tree strategy would
        // drop the source's changes and still record a merge.
        assert_eq!(Side::Ours.strategy_option(), "ours");
        assert_eq!(Side::Theirs.strategy_option(), "theirs");
    }

    #[test]
    fn merge_rejects_both_ops_but_allows_repeats() {
        let e = merge_args(&["continue", "abort"]).unwrap_err();
        assert_eq!(e, "continue and abort conflict");
        assert!(merge_args(&["-c", "--abort"]).is_err());
        // Saying the same word twice is redundant, not wrong — same rule as
        // ours/theirs.
        assert_eq!(merge_args(&["continue", "-c"]).unwrap().op, MergeOp::Continue);
    }

    #[test]
    fn merge_rejections_name_the_offending_flag() {
        let e = merge_args(&["abort", "-m", "x", "--squash"]).unwrap_err();
        assert!(e.contains("got -m, --squash"), "{e}");
        let e = merge_args(&["2", "dry-run", "--no-ff", "-f"]).unwrap_err();
        assert!(e.contains("got --no-ff, -f"), "{e}");
    }

    #[test]
    fn merge_rejects_both_sides_but_allows_repeats() {
        assert!(merge_args(&["2", "ours", "theirs"]).is_err());
        assert!(merge_args(&["2", "-o", "--theirs"]).is_err());
        // Saying the same side twice is redundant, not wrong.
        assert_eq!(merge_args(&["2", "ours", "-o"]).unwrap().side, Some(Side::Ours));
    }

    #[test]
    fn merge_resume_rejects_a_side_with_a_pointed_hint() {
        // 'theirs continue' reads as "finish this by taking theirs", which git
        // cannot do — the error has to say so rather than ignore the word.
        let e = merge_args(&["theirs", "continue"]).unwrap_err();
        assert!(e.contains("applied when a merge starts"), "{e}");
        assert!(e.contains("merge abort"), "{e}");
    }

    #[test]
    fn merge_dry_run_rejects_start_only_flags() {
        assert!(merge_args(&["2", "dry-run", "--no-ff"]).is_err());
        assert!(merge_args(&["2", "dry-run", "-m", "x"]).is_err());
        assert!(merge_args(&["2", "dry-run", "-f"]).is_err());
        // --ff-only gates the merge rather than shaping its commit, but a dry
        // run has no merge to gate: merge-tree resolves in memory and never
        // fast-forwards anything, so honoring it is impossible.
        let e = merge_args(&["2", "dry-run", "--ff-only"]).unwrap_err();
        assert!(e.contains("got --ff-only"), "{e}");
        // A side is fine: it changes what the dry run would report.
        assert!(merge_args(&["2", "dry-run", "theirs"]).is_ok());
    }

    #[test]
    fn merge_rejects_bad_combinations() {
        assert!(merge_args(&[]).is_err()); // no source
        assert!(merge_args(&["--continue", "2"]).is_err()); // resume takes no source
        assert!(merge_args(&["--continue", "--no-ff"]).is_err()); // nor options
        assert!(merge_args(&["--continue", "--abort"]).is_err());
        assert!(merge_args(&["2", "--no-ff", "--ff-only"]).is_err());
        assert!(merge_args(&["2", "--squash", "--no-ff"]).is_err());
        assert!(merge_args(&["2", "3"]).is_err()); // too many
        assert!(merge_args(&["2", "--rebase"]).is_err()); // unknown option
        assert!(merge_args(&["-m"]).is_err()); // -m needs a value
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
            unknown_command_msg("merge"),
            "unknown command 'merge'; use 'git-wt 1,2 merge'"
        );
        assert_eq!(
            unknown_command_msg("feat/x"),
            "unknown command 'feat/x'; did you mean 'add feat/x'?"
        );
        assert_eq!(
            unknown_command_msg("merged"),
            "unknown command 'merged'; use 'git-wt 1 merged' or 'git-wt 1,2 merged'"
        );
        assert_eq!(unknown_command_msg("lsit"), "unknown command 'lsit'");
    }

    fn hunk(line: &str) -> (usize, &'static str, usize) {
        let h = parse_hunk_header(line).expect("header should parse");
        (h.line, h.kind, h.count)
    }

    #[test]
    fn omitted_hunk_count_means_one() {
        // '@@ -119 +119 @@' is a one-line change, not a malformed header.
        assert_eq!(hunk("@@ -119 +119 @@"), (119, "modified", 1));
        assert_eq!(parse_range("-119"), Some((119, 1)));
        assert_eq!(parse_range("+42,7"), Some((42, 7)));
    }

    #[test]
    fn zero_hunk_count_is_not_an_edit() {
        // A zero side is a pure insert/delete. Labeling off the new-side
        // number alone would report every deletion as '+0' additions.
        assert_eq!(hunk("@@ -0,0 +290,2 @@"), (290, "added", 2));
        assert_eq!(hunk("@@ -5,3 +4,0 @@"), (4, "deleted", 3));
        assert_eq!(hunk("@@ -119,3 +119,5 @@ fn x() {"), (119, "modified", 5));
    }

    #[test]
    fn patch_counts_skip_the_file_headers() {
        // '--- a/x' / '+++ b/x' are +/- lines to a naive counter.
        let patch = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1,2 @@\n-old\n+new\n+extra\n";
        let mut fd = FileDiff {
            path: "x".into(),
            status: 'M',
            plus: 0,
            minus: 0,
            binary: false,
            hunks: Vec::new(),
        };
        parse_patch_into(patch, &mut fd);
        assert_eq!((fd.plus, fd.minus), (2, 1));
        assert_eq!(fd.hunks.len(), 1);
    }

    #[test]
    fn patch_splits_by_file_and_reads_status_from_dev_null() {
        let patch = "\
diff --git a/add.txt b/add.txt
--- /dev/null
+++ b/add.txt
@@ -0,0 +1 @@
+hi
diff --git a/gone.txt b/gone.txt
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
";
        let files = split_patch(patch);
        assert_eq!(files.len(), 2);
        assert_eq!((files[0].path.as_str(), files[0].status), ("add.txt", 'A'));
        assert_eq!((files[0].plus, files[0].minus), (1, 0));
        assert_eq!((files[1].path.as_str(), files[1].status), ("gone.txt", 'D'));
        assert_eq!((files[1].plus, files[1].minus), (0, 1));
    }

    #[test]
    fn binary_patch_reports_no_counts() {
        let mut fd = FileDiff {
            path: "i.png".into(),
            status: 'M',
            plus: 0,
            minus: 0,
            binary: false,
            hunks: Vec::new(),
        };
        parse_patch_into("Binary files a/i.png and b/i.png differ\n", &mut fd);
        assert!(fd.binary);
        assert_eq!((fd.plus, fd.minus), (0, 0));
    }

    #[test]
    fn summary_matches_gits_phrasing() {
        let f = |p, m| FileDiff {
            path: "x".into(),
            status: 'M',
            plus: p,
            minus: m,
            binary: false,
            hunks: Vec::new(),
        };
        assert_eq!(
            summary(&[f(90, 10), f(345, 38), f(73, 4)]),
            "3 files changed, 508 insertions(+), 52 deletions(-)"
        );
        assert_eq!(summary(&[f(1, 1)]), "1 file changed, 1 insertion(+), 1 deletion(-)");
        assert_eq!(summary(&[f(0, 2)]), "1 file changed, 2 deletions(-)");
    }

    /// `cmd_merged` exit contract: Ok when src is already in dest, Err when not.
    #[test]
    fn merged_reports_ancestor_and_non_ancestor() {
        let tmp = std::env::temp_dir().join(format!(
            "git-wt-merged-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str]) {
            let mut c = std::process::Command::new("git");
            c.current_dir(dir).args(args);
            let out = c.output().unwrap();
            assert!(out.status.success(), "git {:?} failed: {:?}", args, out);
        }

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp,
            &[
                "init",
                "--quiet",
                "--initial-branch=main",
            ],
        );
        git(&tmp, &["config", "user.email", "t@test"]);
        git(&tmp, &["config", "user.name", "t"]);
        std::fs::write(tmp.join("x.txt"), "init").unwrap();
        git(&tmp, &["add", "x.txt"]);
        git(&tmp, &["commit", "--quiet", "-m", "init"]);
        git(&tmp, &["branch", "feat"]);
        git(&tmp, &["checkout", "--quiet", "feat"]);
        std::fs::write(tmp.join("y.txt"), "a").unwrap();
        git(&tmp, &["add", "y.txt"]);
        git(&tmp, &["commit", "--quiet", "-m", "add"]);

        // main is an ancestor of feat.
        assert!(cmd_merged(&tmp, "main", "feat").is_ok());
        // feat is not an ancestor of main: 1 commit ahead.
        let err = cmd_merged(&tmp, "feat", "main").unwrap_err();
        assert!(err.contains("Ahead feat is NOT in main"), "{err}");
        assert!(err.contains("ahead 1"), "{err}");
        // A non-existent ref propagates git's error.
        let err = cmd_merged(&tmp, "no-such-ref", "main").unwrap_err();
        assert!(err.contains("no-such-ref") || err.contains("Not a valid object"), "{err}");

        std::fs::remove_dir_all(&tmp).ok();
    }
}
