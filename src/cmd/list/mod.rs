pub(crate) mod args;

use std::collections::{HashMap, HashSet};
use std::io::IsTerminal;
use std::path::Path;

use crate::cmd::list::args::ListArgs;
use crate::cmd::merged::{merged_text, merged_text_at};
use crate::git::git_stdout;
use crate::ui::{
    color_enabled, ellipsize, paint, paint_matches, term_width, BLUE, BRANCH_MIN, DIM,
    HEADER_COLORS, SEARCH_MATCH,
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

/// Relative time of the worktree's last commit.
pub(crate) fn last_commit(path: &Path) -> String {
    git_stdout(path, &["log", "-1", "--format=%ar"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// The uncommitted files in a worktree, with status and line counts.
pub(crate) fn worktree_files(w: &Worktree) -> Vec<FileStat> {
    if w.bare {
        return Vec::new();
    }
    let Ok(porcelain) = git_stdout(&w.path, &["status", "--porcelain"]) else {
        return Vec::new();
    };

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

use crate::cmd::commits::rows::{file_stat_lines, parse_numstat_z, sort_file_stats, FileStat};

/// Verbosity for `list`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListMode {
    Short,
    Normal,
    Long,
}

pub(crate) fn cmd_list(root: &Path, args: ListArgs) -> Result<(), String> {
    cmd_list_from_args(root, args)
}

/// The worktree table.
pub(crate) fn cmd_list_impl(
    root: &Path,
    search: Option<&str>,
    cols: Option<Vec<usize>>,
    mode: ListMode,
    show_path: bool,
    files: bool,
    merged_ref: Option<&str>,
    use_pager: bool,
) -> Result<(), String> {
    let trees = worktrees(root)?;
    let cur_idx = current_worktree_index(&trees);

    let rows: Vec<(usize, &Worktree)> = trees.iter().enumerate().collect();

    let stdout_tty = std::io::stdout().is_terminal();
    let color = color_enabled(stdout_tty);
    let explicit = cols.is_some();

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

    let need_status = color || cols.contains(&4);
    let need_last = cols.contains(&5);
    let need_merged = cols.contains(&6);
    let need_merged_ref = cols.contains(&7);
    let need_merged_at = cols.contains(&8);
    let need_push = cols.contains(&9) || cols.contains(&10);
    let header = !explicit && stdout_tty && mode != ListMode::Short;

    let numw = trees.len().to_string().len();

    let need_here = need_merged || ((need_merged_ref || need_merged_at) && merged_ref.is_none());
    let here = if need_here { current_ref() } else { String::new() };
    let merged_ref = match merged_ref {
        Some(r) => r.to_string(),
        None => here.clone(),
    };

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

    let _pager = crate::ui::Pager::start(stdout_tty && use_pager);

    if header {
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

    for (i, ((row, (st, _, _, _, _, _, _)), (orig_idx, w))) in
        cells.iter().zip(&meta).zip(&rows).enumerate()
    {
        if files && i > 0 {
            println!();
        }
        let line = render_row(row, &cols, &widths, *st, color, search, cur_idx == Some(*orig_idx));
        println!("{line}");
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

/// Text for the "push" and "pull" columns.
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

/// Run `list` from parsed `ListArgs`.
pub(crate) fn cmd_list_from_args(root: &Path, args: ListArgs) -> Result<(), String> {
    let mode = if args.long {
        ListMode::Long
    } else if args.short {
        ListMode::Short
    } else {
        ListMode::Normal
    };
    let cols = args.col.as_deref().map(parse_cols).transpose()?;
    cmd_list_impl(root, args.search.as_deref(), cols, mode, args.show_path, args.files, None, args.less)
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
        let plain = render_row(&row, &cols, &widths, Status::Clean, false, None, false);
        assert_eq!(plain, "1  main");
        let tinted = render_row(&row, &cols, &widths, Status::Clean, true, None, false);
        assert_eq!(tinted, "1  \x1b[32mmain\x1b[0m");
    }

    #[test]
    fn render_row_search_overwrites_the_tint_for_the_match_only() {
        let cols = vec![1, 2];
        let row = vec!["1".to_string(), "main".to_string()];
        let widths = vec![1, 7];
        let hit = render_row(&row, &cols, &widths, Status::Clean, true, Some("ai"), false);
        assert_eq!(
            hit,
            format!(
                "1  \x1b[32mm\x1b[0m\x1b[{SEARCH_MATCH}mai\x1b[0m\x1b[32mn\x1b[0m"
            )
        );
        let no_color = render_row(&row, &cols, &widths, Status::Clean, false, Some("ai"), false);
        assert_eq!(no_color, "1  main");
    }
}
