use std::io::IsTerminal;
use std::path::Path;

use crate::git::git_stdout;
use crate::cmd::merged::{merged_text, merged_text_at};
use crate::ui::{color_enabled, ellipsize, paint, term_width, BRANCH_MIN, DIM};
use crate::worktree::{
    current_ref, label, status_color, status_text, worktree_status, worktrees, Status, Worktree,
};

/// Header label for a column id.
pub(crate) fn col_header(c: usize) -> &'static str {
    match c {
        1 => "#",
        2 => "branch",
        3 => "path",
        4 => "status",
        5 => "last-commit",
        6 => "merged",
        7 => "merged",
        8 => "merged-at",
        9 => "push",
        _ => "pull",
    }
}

/// Join one row's cells with two-space gaps, padding all but the last column.
/// When `color`, the branch (col 2) and status (col 4) cells are tinted by
/// `st`. Padding is computed on the plain text, then color wraps it, so ANSI
/// never affects alignment.
pub(crate) fn render_row(
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
/// 1=id, 2=branch, 3=dir, 4=status, 5=last-commit, 6=merged-into-current,
/// 7=merged-into-ref, 8=merged-at.
pub(crate) const COL_HELP: &str = "1=id, 2=branch, 3=dir, 4=status, 5=last-commit, 6=merged, 7=merged-ref, 8=merged-at, 9=push, 10=pull";

pub(crate) fn parse_cols(s: &str) -> Result<Vec<usize>, String> {
    let mut v = Vec::new();
    for part in s.split(',') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let n: usize = p
            .parse()
            .map_err(|_| format!("bad column '{p}' (use {COL_HELP})"))?;
        if n < 1 || n > 10 {
            return Err(format!("no column {n} (use {COL_HELP})"));
        }
        v.push(n);
    }
    if v.is_empty() {
        return Err("--col needs columns, e.g. 1,2,3".into());
    }
    Ok(v)
}

/// Relative time of the worktree's last commit (e.g. "2 minutes ago"), or ""
/// when unavailable (bare / no commits).
pub(crate) fn last_commit(path: &Path) -> String {
    git_stdout(path, &["log", "-1", "--format=%ar"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Verbosity for `list`. Normal enriches to status + last-commit only on a
/// terminal; on a pipe it stays the plain id/branch/dir contract.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListMode {
    Short,
    Normal,
    Long,
}

pub(crate) fn cmd_list(
    root: &Path,
    search: Option<&str>,
    cols: Option<Vec<usize>>,
    mode: ListMode,
    show_path: bool,
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

    // Columns to show, in order: 1=id, 2=branch, 3=dir, 4=status, 5=last-commit,
    // 6=merged-into-current, 7=merged-into-ref, 8=merged-at.
    // Without --col: Short is a compact summary, Long shows everything, and
    // Normal enriches only on a TTY so a piped `git-wt list` keeps the plain
    // id/branch/dir contract.
    //
    // A path is the widest cell and the least read -- it is the branch name
    // with a prefix, and `git-wt <N> path` is how a script gets one anyway --
    // so on a terminal it waits for --show-path. A pipe keeps it unasked: the
    // id/branch/dir contract is what the flagless form has always emitted.
    let cols = match (cols, mode) {
        (Some(c), _) => c,
        (None, ListMode::Short) => vec![1, 2, 4],
        (None, ListMode::Long) => vec![1, 2, 3, 4, 5, 9, 10],
        (None, ListMode::Normal) if stdout_tty && show_path => vec![1, 2, 3, 4, 5, 9, 10],
        (None, ListMode::Normal) if stdout_tty => vec![1, 2, 4, 5, 9, 10],
        (None, ListMode::Normal) => vec![1, 2, 3],
    };

    // Branch color needs status too, so fetch it whenever we color or show it.
    let need_status = color || cols.contains(&4);
    let need_last = cols.contains(&5);
    let need_merged = cols.contains(&6);
    let need_merged_ref = cols.contains(&7);
    let need_merged_at = cols.contains(&8);
    let need_push = cols.contains(&9) || cols.contains(&10);
    let header = !explicit && stdout_tty && mode != ListMode::Short;

    // Right-align the index to the widest possible so filtered output lines up.
    let numw = trees.len().to_string().len();

    // The branch we are standing in; column 6 asks whether each row's branch is
    // already contained in it. Columns 7/8 use the same reference in normal list
    // mode; the `--others` command overrides the reference explicitly.
    let here = if need_merged || need_merged_ref || need_merged_at {
        current_ref()
    } else {
        String::new()
    };
    let merged_ref = here.clone();

    // Per-row metadata, fetched once (read-only git calls).
    let meta: Vec<(Status, String, String, String, String, String, String)> = rows
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
            let (merged_r, merged_a) = if need_merged_ref || need_merged_at {
                merged_text_at(root, w, &merged_ref)
            } else {
                (String::new(), String::new())
            };
            let (push, pull) = if need_push {
                push_pull_text(w)
            } else {
                (String::new(), String::new())
            };
            (st, last, merged, merged_r, merged_a, push, pull)
        })
        .collect();

    // Plain (uncolored) cells drive column widths; color is applied at print
    // time so the ANSI escapes never skew alignment.
    let cells: Vec<Vec<String>> = rows
        .iter()
        .zip(&meta)
        .map(|((i, w), (st, last, merged, merged_r, merged_a, push, pull))| {
            cols.iter()
                .map(|c| match c {
                    1 => format!("{:>numw$}", i + 1, numw = numw),
                    2 => label(w),
                    3 => w.path.display().to_string(),
                    4 => status_text(*st).to_string(),
                    5 => last.clone(),
                    6 => merged.clone(),
                    7 => merged_r.clone(),
                    8 => merged_a.clone(),
                    9 => push.clone(),
                    _ => pull.clone(),
                })
                .collect()
        })
        .collect();

    let header_cells: Vec<String> = cols.iter().map(|c| col_header(*c).to_string()).collect();
    let mut cells = cells;

    // Per-column width over the header and every data row.
    let mut widths = vec![0usize; cols.len()];
    for row in cells.iter().chain(header.then_some(&header_cells)) {
        for (k, cell) in row.iter().enumerate() {
            widths[k] = widths[k].max(cell.chars().count());
        }
    }

    // A branch name is the one cell with no bound, and an issue-shaped one is
    // long enough to wrap the row on its own. So it gets whatever width the
    // terminal has left once every other column is paid for, and the overflow
    // is cut off each cell. `term_width` is None off a terminal, so a pipe
    // still sees every name whole.
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
            // Below the floor the row wraps anyway, and a name cut to nothing
            // names nothing: keep enough to tell two branches apart.
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

    for (row, (st, _, _, _, _, _, _)) in cells.iter().zip(&meta) {
        let line = render_row(row, &cols, &widths, *st, color);
        println!("{line}");
    }
    Ok(())
}

/// Text for the "push" (col 9) and "pull" (col 10) columns: how far the
/// worktree's branch is ahead of and behind its upstream. Both are "-" when
/// bare or no upstream is configured. Counts come from the remote-tracking ref,
/// so they are as fresh as the last fetch -- no network call is made here.
pub(crate) fn push_pull_text(w: &Worktree) -> (String, String) {
    let dash = || ("-".to_string(), "-".to_string());
    if w.bare {
        return dash();
    }
    let Ok(out) = git_stdout(
        &w.path,
        &["rev-list", "--left-right", "--count", "@{upstream}...HEAD"],
    ) else {
        return dash();
    };
    let mut it = out.split_whitespace();
    let (Some(behind), Some(ahead)) = (it.next(), it.next()) else {
        return dash();
    };
    let push = if ahead == "0" {
        "-".to_string()
    } else {
        format!("ahead {ahead}")
    };
    let pull = if behind == "0" {
        "-".to_string()
    } else {
        format!("behind {behind}")
    };
    (push, pull)
}

/// Case-insensitive subsequence match over "<label> <path>".
pub(crate) fn fuzzy_match(w: &Worktree, needle: &str) -> bool {
    let hay = format!("{} {}", label(w), w.path.display()).to_lowercase();
    is_subseq(&hay, &needle.to_lowercase())
}

/// True when every char of `needle` appears in `hay`, in order.
pub(crate) fn is_subseq(hay: &str, needle: &str) -> bool {
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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subseq_matches_in_order() {
        assert!(is_subseq("feature-login", "flogin"));
        assert!(is_subseq("feature-login", "feat"));
        assert!(!is_subseq("feature-login", "zzz"));
        assert!(!is_subseq("abc", "cba"));
    }


    #[test]
    fn parse_cols_accepts_status_last_and_merged() {
        assert_eq!(parse_cols("1,4,5").unwrap(), vec![1, 4, 5]);
        assert_eq!(parse_cols("1,2,6").unwrap(), vec![1, 2, 6]);
        assert_eq!(parse_cols("1,7,8").unwrap(), vec![1, 7, 8]);
        assert_eq!(parse_cols("1,9,10").unwrap(), vec![1, 9, 10]);
        assert!(parse_cols("11").is_err());
    }


    #[test]
    fn col_header_uses_last_commit_name() {
        assert_eq!(col_header(5), "last-commit");
        assert_eq!(col_header(7), "merged");
        assert_eq!(col_header(8), "merged-at");
        assert_eq!(col_header(9), "push");
        assert_eq!(col_header(10), "pull");
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

}
