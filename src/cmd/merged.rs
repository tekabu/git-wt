use std::io::IsTerminal;
use std::path::Path;

use crate::cmd::list::{col_header, last_commit, render_row};
use crate::git::{git_cmd, git_stdout};
use crate::ui::{color_enabled, ellipsize, paint, term_width, BRANCH_MIN, DIM, GREEN};
use crate::worktree::{label, ref_of, status_text, worktree_status, worktrees, Status, Worktree};

// ---------------------------------------------------------------------------
// Merged: git-wt <N> merged [<M|BRANCH>] | git-wt <N>,<M> merged
// ---------------------------------------------------------------------------

/// List every worktree and whether its branch is already merged into the
/// selected worktree's branch, plus when it was merged there.
pub(crate) fn cmd_merged_others(
    root: &Path,
    trees: &[Worktree],
    idx: usize,
    show_path: bool,
) -> Result<(), String> {
    let dest = ref_of(&trees[idx])?;
    cmd_list_with_ref(root, &dest, show_path)
}

/// Shared implementation for the "list relative to a reference branch" view.
/// Used by `cmd_merged_others`; columns 7/8 are the merged-into-ref data.
pub(crate) fn cmd_list_with_ref(root: &Path, merged_ref: &str, show_path: bool) -> Result<(), String> {
    let trees = worktrees(root)?;
    let rows: Vec<(usize, &Worktree)> = trees.iter().enumerate().collect();

    let stdout_tty = std::io::stdout().is_terminal();
    let color = color_enabled(stdout_tty);

    let cols: Vec<usize> = if stdout_tty {
        if show_path {
            vec![1, 2, 3, 4, 5, 7, 8]
        } else {
            vec![1, 2, 4, 5, 7, 8]
        }
    } else {
        vec![1, 2, 3, 7, 8]
    };

    let need_status = color || cols.contains(&4);
    let need_last = cols.contains(&5);
    let need_merged_ref = cols.contains(&7);
    let need_merged_at = cols.contains(&8);
    let header = stdout_tty;

    let numw = trees.len().to_string().len();

    let meta: Vec<(Status, String, String, String)> = rows
        .iter()
        .map(|(_, w)| {
            let st = if need_status && !w.bare {
                worktree_status(&w.path)
            } else {
                Status::Unknown
            };
            let last = if need_last { last_commit(&w.path) } else { String::new() };
            let (merged_r, merged_a) = if need_merged_ref || need_merged_at {
                merged_text_at(root, w, merged_ref)
            } else {
                (String::new(), String::new())
            };
            (st, last, merged_r, merged_a)
        })
        .collect();

    let cells: Vec<Vec<String>> = rows
        .iter()
        .zip(&meta)
        .map(|((i, w), (st, last, merged_r, merged_a))| {
            cols.iter()
                .map(|c| match c {
                    1 => format!("{:>numw$}", i + 1, numw = numw),
                    2 => label(w),
                    3 => w.path.display().to_string(),
                    4 => status_text(*st).to_string(),
                    5 => last.clone(),
                    7 => merged_r.clone(),
                    _ => merged_a.clone(),
                })
                .collect()
        })
        .collect();

    let header_cells: Vec<String> = cols.iter().map(|c| col_header(*c).to_string()).collect();
    let mut cells = cells;

    let mut widths = vec![0usize; cols.len()];
    for row in cells.iter().chain(header.then_some(&header_cells)) {
        for (k, cell) in row.iter().enumerate() {
            widths[k] = widths[k].max(cell.chars().count());
        }
    }

    if let (Some(term), Some(k)) = (term_width(stdout_tty), cols.iter().position(|c| *c == 2)) {
        let gaps = 2 * (cols.len() - 1);
        let others: usize = widths
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != k)
            .map(|(_, w)| w)
            .sum();
        let budget = term.saturating_sub(gaps + others);
        if budget < widths[k] {
            let cap = budget.max(BRANCH_MIN);
            for row in cells.iter_mut() {
                row[k] = ellipsize(&row[k], cap);
            }
            widths[k] = cells.iter().map(|r| r[k].chars().count()).max().unwrap_or(0);
            if header {
                widths[k] = widths[k].max(header_cells[k].chars().count());
            }
        }
    }

    if header {
        let line = render_row(
            &header_cells,
            &cols,
            &widths,
            Status::Unknown,
            false,
        );
        println!("{}", paint(&line, DIM, color));
    }

    for (row, (st, _, _, _)) in cells.iter().zip(&meta) {
        let line = render_row(row, &cols, &widths, *st, color);
        println!("{line}");
    }
    Ok(())
}

/// Short text for the "merged" column: whether `w`'s branch is already in the
/// branch we are standing in (`here`). `-` for bare worktrees or failures.
pub(crate) fn merged_text(root: &Path, w: &Worktree, here: &str) -> String {
    let Some(src) = w.branch.as_deref() else {
        return "-".into();
    };
    merged_status_text(root, src, here)
}

/// Merge status text for a source branch relative to a destination branch.
pub(crate) fn merged_status_text(root: &Path, src: &str, dest: &str) -> String {
    match git_cmd(root, &["merge-base", "--is-ancestor", src, dest])
        .output()
    {
        Ok(out) => match out.status.code() {
            Some(0) => "merged".into(),
            Some(1) => match ahead_count(root, src, dest) {
                Ok(n) => format!("ahead {n}"),
                Err(_) => "ahead".into(),
            },
            _ => "-".into(),
        },
        Err(_) => "-".into(),
    }
}

/// Merge status text and, if merged, the relative time of the most recent merge
/// commit on `dest` that made `src` reachable. `-` for not-merged, bare,
/// fast-forward, or failures.
pub(crate) fn merged_text_at(root: &Path, w: &Worktree, dest: &str) -> (String, String) {
    let Some(src) = w.branch.as_deref() else {
        return ("-".into(), "-".into());
    };
    if src == dest {
        return ("self".into(), "-".into());
    }
    let status = merged_status_text(root, src, dest);
    let at = if status == "merged" {
        last_merge_date(root, src, dest)
    } else {
        "-".into()
    };
    (status, at)
}

/// Relative time of the most recent merge commit on `dest` after `src`.
/// Returns "-" when no merge commit is found (e.g. fast-forward).
pub(crate) fn last_merge_date(root: &Path, src: &str, dest: &str) -> String {
    git_stdout(
        root,
        &[
            "log",
            "-1",
            "--ancestry-path",
            "--merges",
            "--format=%ar",
            &format!("{src}..{dest}"),
        ],
    )
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "-".into())
}

/// Report whether `src` is already an ancestor of `dest`.
///
/// `git merge-base --is-ancestor` exits 0 when src is contained in dest, 1 when
/// it is not, and anything else is a real error. This is the same exit-code
/// contract `merge_dry_run` already uses, so `if git-wt 1 merged; then ...` works.
pub(crate) fn cmd_merged(dir: &Path, src: &str, dest: &str) -> Result<(), String> {
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
pub(crate) fn ahead_count(dir: &Path, src: &str, dest: &str) -> Result<usize, String> {
    let s = git_stdout(dir, &["rev-list", "--count", &format!("{dest}..{src}")])?;
    s.trim()
        .parse()
        .map_err(|e| format!("could not parse ahead count: {e}"))
}
