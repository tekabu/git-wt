use std::collections::{HashMap, HashSet};
use std::io::{IsTerminal, Write};
use std::path::Path;

use crate::git::git_stdout;
use crate::cli::check_index;
use crate::cmd::commits::rows::{file_stat_lines, parse_numstat_z, sort_file_stats, FileStat};
use crate::cmd::merged::{merged_text, merged_text_at};
use crate::ui::{
    color_enabled, ellipsize, fzf_pick_plain, paint, paint_matches, term_width, BLUE, BRANCH_MIN,
    DIM, HEADER_COLORS, SEARCH_MATCH,
};
use crate::worktree::{
    current_ref, current_worktree_index, label, status_color, status_text, worktree_status,
    worktrees, Status, Worktree,
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
///
/// `search`, when given, lights every literal-substring occurrence in the
/// branch (col 2) and path (col 3) cells in `SEARCH_MATCH`, overwriting
/// whatever tint the cell already carries for just that span.
///
/// `current`, when set, marks the row as the worktree the caller is standing
/// in: its branch (col 2) is tinted `BLUE` instead of by status, so the
/// current tree stands out regardless of clean/dirty state.
pub(crate) fn render_row(
    row: &[String],
    cols: &[usize],
    widths: &[usize],
    st: Status,
    color: bool,
    search: Option<&str>,
    current: bool,
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
        let code = if current && cols[k] == 2 { BLUE } else { status_color(st) };
        let tinted = matches!(cols[k], 2 | 4) && color && !code.is_empty();
        let searchable = matches!(cols[k], 2 | 3);
        if let Some(term) = search.filter(|_| searchable) {
            let base = if tinted { code } else { "" };
            line.push_str(&paint_matches(&padded, term, SEARCH_MATCH, base, color));
        } else if tinted {
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
        if !(1..=10).contains(&n) {
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

/// The uncommitted files in a worktree, with status and line counts -- the
/// working-tree answer to `commits --files`' per-commit one.
///
/// Statuses come from `git status --porcelain` so untracked files are included;
/// the counts come from `git diff --numstat HEAD`, which covers staged and
/// unstaged changes together. An untracked file has no diff to count, so its
/// added lines are counted from the file itself and nothing is removed.
///
/// A bare worktree has no working tree at all, and a git failure means we have
/// nothing to say: both give an empty block rather than a wrong one.
pub(crate) fn worktree_files(w: &Worktree) -> Vec<FileStat> {
    if w.bare {
        return Vec::new();
    }
    let Ok(porcelain) = git_stdout(&w.path, &["status", "--porcelain"]) else {
        return Vec::new();
    };

    // path -> status char. The index column wins when it has one, so a staged
    // add that was then edited still reads as an add.
    let mut status_by_path: HashMap<String, char> = HashMap::new();
    let mut untracked: Vec<String> = Vec::new();
    for line in porcelain.lines() {
        if line.len() < 4 {
            continue;
        }
        let mut chars = line.chars();
        let x = chars.next().unwrap_or(' ');
        let y = chars.next().unwrap_or(' ');
        let rest = &line[3..];
        if x == '?' {
            untracked.push(rest.to_string());
            status_by_path.insert(rest.to_string(), '?');
            continue;
        }
        let status = if x != ' ' { x } else { y };
        // "R  old -> new": the new path is the one that exists now, and it is
        // the one `--numstat -z` keys the rename on.
        let path = match rest.split_once(" -> ") {
            Some((_old, new)) => new,
            None => rest,
        };
        status_by_path.insert(path.to_string(), status);
    }

    let mut stats: Vec<FileStat> = Vec::new();
    let mut counted: HashSet<String> = HashSet::new();
    if let Ok(numstat) = git_stdout(&w.path, &["diff", "--numstat", "-z", "HEAD"]) {
        for entry in parse_numstat_z(&numstat) {
            let status = status_by_path.get(&entry.path).copied().unwrap_or('M');
            counted.insert(entry.path.clone());
            let path = match &entry.old_path {
                Some(old) => format!("{old} => {}", entry.path),
                None => entry.path.clone(),
            };
            stats.push(FileStat {
                status,
                path,
                added: entry.added,
                removed: entry.removed,
            });
        }
    }

    // Untracked files are absent from every diff, so they are added here with
    // their own line count. A trailing '/' is a whole untracked directory,
    // which git collapses to one entry and we leave uncounted.
    for path in untracked {
        if counted.contains(&path) {
            continue;
        }
        let added = if path.ends_with('/') {
            None
        } else {
            std::fs::read_to_string(w.path.join(&path))
                .ok()
                .map(|s| s.lines().count())
        };
        stats.push(FileStat {
            status: '?',
            path,
            added,
            removed: added.map(|_| 0),
        });
    }

    sort_file_stats(&mut stats);
    stats
}

/// Verbosity for `list`. Normal enriches to status + last-commit only on a
/// terminal; on a pipe it stays the plain id/branch/dir contract.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListMode {
    Short,
    Normal,
    Long,
}

/// The worktree table.
///
/// `merged_ref` is what `--others` adds: every row is measured against that ref
/// instead of the branch we are standing in, and the default columns carry the
/// merged-into-ref pair (7/8) rather than the ahead/behind pair. It is the only
/// difference between the two views, so they share this one body -- a second
/// copy meant every table fix had to be made twice.
pub(crate) fn cmd_list(
    root: &Path,
    search: Option<&str>,
    cols: Option<Vec<usize>>,
    mode: ListMode,
    show_path: bool,
    files: bool,
    merged_ref: Option<&str>,
) -> Result<(), String> {
    let trees = worktrees(root)?;
    let cur_idx = current_worktree_index(&trees);

    // `search` only highlights now -- every worktree still gets a row, and its
    // original 1-based index, so `git-wt <N> ...` never shifts under a search.
    let rows: Vec<(usize, &Worktree)> = trees.iter().enumerate().collect();

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
    //
    // Under `--others` the merged-into-ref pair replaces the ahead/behind pair:
    // the question that view asks is what has landed in the ref, and a piped
    // one still answers it, since that is the whole point of the command.
    let cols = match (cols, merged_ref, mode) {
        (Some(c), _, _) => c,
        (None, Some(_), _) if stdout_tty && show_path => vec![1, 2, 3, 4, 5, 7, 8],
        (None, Some(_), _) if stdout_tty => vec![1, 2, 4, 5, 7, 8],
        (None, Some(_), _) => vec![1, 2, 3, 7, 8],
        (None, None, ListMode::Short) => vec![1, 2, 4],
        (None, None, ListMode::Long) => vec![1, 2, 3, 4, 5, 6, 9, 10],
        (None, None, ListMode::Normal) if stdout_tty && show_path => {
            vec![1, 2, 3, 4, 5, 6, 9, 10]
        }
        (None, None, ListMode::Normal) if stdout_tty => vec![1, 2, 4, 5, 6, 9, 10],
        (None, None, ListMode::Normal) => vec![1, 2, 3],
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
    // Columns 7/8 follow `merged_ref` when one was given, so the branch we are
    // standing in is only worth looking up when something still asks for it.
    let need_here = need_merged || ((need_merged_ref || need_merged_at) && merged_ref.is_none());
    let here = if need_here { current_ref() } else { String::new() };
    let merged_ref = match merged_ref {
        Some(r) => r.to_string(),
        None => here.clone(),
    };

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

    // Computed above, before the pager repoints stdout at a pipe -- `stdout_tty`
    // and the `tput` behind `term_width` both need the real terminal, not it.
    let _pager = crate::ui::Pager::start(stdout_tty);

    if header {
        // Each label its own bright color, cycled, so the header reads as a row
        // of named columns rather than one flat dim line -- the same treatment
        // `commits`/`log` give their headers. Padded before coloring, like
        // `render_row`, so the escapes never skew the columns.
        let mut hue = HEADER_COLORS.iter().cycle();
        let last = header_cells.len() - 1;
        let mut line = String::new();
        for (k, cell) in header_cells.iter().enumerate() {
            if k > 0 {
                line.push_str("  ");
            }
            let padded = if k == last {
                cell.clone()
            } else {
                format!("{:<w$}", cell, w = widths[k])
            };
            line.push_str(&paint(&padded, hue.next().unwrap(), color));
        }
        println!("{}", line);
    }

    // With file blocks the listing stops being a table and becomes a series of
    // groups, so each worktree is fenced off by a blank line -- including the
    // ones with no files to show, which would otherwise huddle against the
    // block above them and read as part of it.
    for (i, ((row, (st, _, _, _, _, _, _)), (orig_idx, w))) in
        cells.iter().zip(&meta).zip(&rows).enumerate()
    {
        if files && i > 0 {
            println!();
        }
        let line = render_row(row, &cols, &widths, *st, color, search, cur_idx == Some(*orig_idx));
        println!("{line}");
        // The same file block `commits --files` prints under a commit, here
        // under the branch it belongs to: every worktree that is not clean gets
        // one, and a clean one gets nothing.
        if files {
            let stats = worktree_files(w);
            if !stats.is_empty() {
                println!();
                for file_line in file_stat_lines(&stats) {
                    println!("{}", paint(&file_line, DIM, color));
                }
            }
        }
    }
    Ok(())
}

/// Sentinel string for "the picker was cancelled" -- ESC/Ctrl-C in fzf, or an
/// empty Enter/EOF at the numbered prompt -- so `cmd_switch` can tell it apart
/// from a real failure and exit quietly instead of printing it as an error.
const CANCELLED: &str = "cancelled";

/// Interactive `switch`: pick a worktree with fzf's arrow-key/type-to-filter
/// list, falling back to a numbered prompt when fzf isn't installed -- the
/// same two-step `add`'s own branch picker already takes. Only the picked
/// path goes to stdout, so the shell wrapper's `cd "$(git-wt switch)"` stays
/// clean; the branch, like `<N> switch`, is status and goes to stderr.
pub(crate) fn cmd_switch(root: &Path) -> Result<(), String> {
    let trees = worktrees(root)?;
    if trees.is_empty() {
        return Err("no worktrees (see 'git-wt list')".into());
    }

    // Each line leads with its 1-based number so the selection maps straight
    // back to an index, whichever picker made it and however branch names
    // collide (a detached worktree has none at all).
    let numw = trees.len().to_string().len();
    let bw = trees.iter().map(|w| label(w).chars().count()).max().unwrap_or(0);
    let items: Vec<String> = trees
        .iter()
        .enumerate()
        .map(|(i, w)| {
            format!(
                "{:>numw$}  {:<bw$}  {}",
                i + 1,
                label(w),
                w.path.display(),
                numw = numw,
                bw = bw
            )
        })
        .collect();

    // Cancelling (ESC/Ctrl-C in fzf, empty Enter at the numbered prompt) is
    // "changed my mind", not a failure -- it exits quietly, code 0, with
    // nothing on either stream, the same as a plain `cd` typed and then
    // abandoned.
    let sel = match fzf_pick_plain(&items, "worktree> ", CANCELLED) {
        Ok(Some(s)) => s,
        Ok(None) => match number_pick_worktree(&trees) {
            Ok(s) => s,
            Err(e) if e == CANCELLED => std::process::exit(0),
            Err(e) => return Err(e),
        },
        Err(e) if e == CANCELLED => std::process::exit(0),
        Err(e) => return Err(e),
    };
    let n: usize = match sel.split_whitespace().next().and_then(|s| s.parse().ok()) {
        Some(n) => n,
        None => std::process::exit(0),
    };
    let idx = check_index(n, trees.len())?;

    eprintln!("{}", label(&trees[idx]));
    println!("{}", trees[idx].path.display());
    Ok(())
}

/// Numbered fallback for `cmd_switch`, in the same shape as `add`'s own
/// `number_pick`: the list on stderr, a number read from stdin. Returns just
/// the typed number, which `cmd_switch` parses the same way it would parse
/// fzf's picked line (that line also leads with the number).
fn number_pick_worktree(trees: &[Worktree]) -> Result<String, String> {
    let color = color_enabled(std::io::stderr().is_terminal());
    eprintln!("Worktrees:");
    let numw = trees.len().to_string().len();
    let bw = trees.iter().map(|w| label(w).chars().count()).max().unwrap_or(0);
    for (i, w) in trees.iter().enumerate() {
        eprintln!(
            "  {:>numw$}  {:<bw$}  {}",
            i + 1,
            label(w),
            paint(&w.path.display().to_string(), DIM, color),
            numw = numw,
            bw = bw
        );
    }
    eprint!("Pick a number (Enter to cancel): ");
    std::io::stderr().flush().ok();
    let mut line = String::new();
    let n = std::io::stdin().read_line(&mut line).map_err(|e| e.to_string())?;
    if n == 0 {
        return Err(CANCELLED.into());
    }
    let s = line.trim();
    if s.is_empty() {
        return Err(CANCELLED.into());
    }
    Ok(s.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;


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
        let plain = render_row(&row, &cols, &widths, Status::Clean, false, None, false);
        assert_eq!(plain, "1  main");
        // Color: branch cell tinted green (padding inside the escape).
        let tinted = render_row(&row, &cols, &widths, Status::Clean, true, None, false);
        assert_eq!(tinted, "1  \x1b[32mmain\x1b[0m");
    }

    #[test]
    fn render_row_search_overwrites_the_tint_for_the_match_only() {
        let cols = vec![1, 2];
        let row = vec!["1".to_string(), "main".to_string()];
        let widths = vec![1, 7];
        // The matched span is repainted SEARCH_MATCH; the rest of the cell
        // keeps its status tint -- the highlight overwrites, not replaces.
        let hit = render_row(&row, &cols, &widths, Status::Clean, true, Some("ai"), false);
        assert_eq!(
            hit,
            format!(
                "1  \x1b[32mm\x1b[0m\x1b[{SEARCH_MATCH}mai\x1b[0m\x1b[32mn\x1b[0m"
            )
        );
        // No color: search still highlights nothing extra, plain text only --
        // ANSI never appears when the stream isn't a terminal.
        let no_color = render_row(&row, &cols, &widths, Status::Clean, false, Some("ai"), false);
        assert_eq!(no_color, "1  main");
    }

}
