use std::collections::HashMap;
use std::collections::HashSet;

use crate::cmd::commits::args::{BranchWidth, PathWidth, SubjectWidth, Wrap};
use crate::cmd::commits::render::{search_layers, Highlight};
use crate::cmd::commits::rows::{file_stat_lines, CommitRow, FileStat, Mark};
use crate::ui::{
    abbrev, ellipsize, paint, paint_layers, wrap_wide, AUTHOR_MAX, BLUE, BRANCH_HEAD_MAX, CHECK,
    DIM, EQUIV, FINGERPRINT, GREEN, HEADER_COLORS, MAGENTA, MATCH, MIN_TEXTW, MISS, PATH_MAX,
    PICK_HEAD, TRAILER, YELLOW,
};

/// One row's `±` cell: churn scoped to the path alone.
pub(crate) fn stat_cell(added: Option<usize>, removed: Option<usize>) -> String {
    let part = |n: Option<usize>, sign: char| n.map(|n| format!("{sign}{n}")).unwrap_or_else(|| "-".to_string());
    format!("{} {}", part(added, '+'), part(removed, '-'))
}

/// Print `log`'s table: the same shape `commits` renders, with a `±` cell
/// scoped to the path and, when it varies, a `path` cell -- a rename under
/// `--follow`, or more than one path given.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_log(
    rows: &[CommitRow],
    stats: &[(Option<usize>, Option<usize>)],
    row_paths: Option<&[Vec<String>]>,
    row_files: &[Vec<FileStat>],
    row_bodies: &[(Vec<String>, usize)],
    names: &[String],
    sets: &[HashSet<String>],
    equiv: &[HashSet<String>],
    trailer: &[HashSet<String>],
    author_match: &[HashSet<String>],
    picks: Option<&HashMap<String, String>>,
    color: bool,
    width: Option<usize>,
    wrap: Wrap,
    subjectw: Option<SubjectWidth>,
    branchw: Option<BranchWidth>,
    pathw_arg: Option<PathWidth>,
    hl: &Highlight,
) {
    let cap = match branchw {
        Some(BranchWidth::Full) => usize::MAX,
        Some(BranchWidth::Cols(n)) => n,
        None => BRANCH_HEAD_MAX,
    };
    let path_cap = match pathw_arg {
        Some(PathWidth::Full) => usize::MAX,
        Some(PathWidth::Cols(n)) => n,
        None => PATH_MAX,
    };
    let names: Vec<String> = names.iter().map(|n| ellipsize(n, cap)).collect();
    let names = &names;
    let widths: Vec<usize> = names.iter().map(|n| n.chars().count().max(1)).collect();
    let marksw: usize = widths.iter().map(|w| w + 2).sum();

    let shaw = rows
        .iter()
        .map(|r| r.short.chars().count())
        .chain(std::iter::once("commit".len()))
        .max()
        .unwrap_or(0);

    let pickw = picks.map(|_| shaw.max(PICK_HEAD.len()));
    let pickcol = pickw.map_or(0, |w| w + 2);

    let mut authw = rows
        .iter()
        .map(|r| r.author.chars().count())
        .chain(std::iter::once("author".len()))
        .max()
        .unwrap_or(0);
    if width.is_some() {
        authw = authw.min(AUTHOR_MAX);
    }

    let datew = rows
        .iter()
        .map(|r| r.date.chars().count())
        .chain(std::iter::once("date".len()))
        .max()
        .unwrap_or(0);

    // The `±` cell, e.g. "+12 -3", widened to the widest one shown; "- -" when
    // a merge's first-parent diff never touched the path.
    let stat_cells: Vec<String> = stats.iter().map(|(a, r)| stat_cell(*a, *r)).collect();
    let statw = stat_cells
        .iter()
        .map(|s| s.chars().count())
        .chain(std::iter::once(1))
        .max()
        .unwrap_or(1);

    let pathw = row_paths.map(|p| {
        p.iter()
            .flat_map(|list| list.iter())
            .map(|s| s.chars().count())
            .chain(std::iter::once("path".len()))
            .max()
            .unwrap_or(0)
            .min(path_cap)
    });
    let pathcol = pathw.map_or(0, |w| w + 2);

    let fixed = shaw + 2 + pickcol + authw + 2 + datew + marksw + 2 + statw + 2 + pathcol;
    // Where a row's second-and-later path (a rename's other name, or a
    // multi-path call's other file) lands: right under the path column,
    // on its own line, rather than the comma-joined cell this used to be
    // -- and the ellipsis that then had to eat whatever overflowed it.
    let path_prefix = fixed - pathcol;

    let textw = match subjectw {
        Some(SubjectWidth::Cols(n)) => Some(n),
        Some(SubjectWidth::Full) => None,
        None => width.map(|w| w.saturating_sub(fixed).max(MIN_TEXTW)),
    };

    let rows_text: Vec<(String, Vec<String>)> = rows
        .iter()
        .map(|r| {
            let text = match textw {
                Some(tw) => wrap_wide(&r.text, tw, wrap.lines()),
                None => vec![r.text.clone()],
            };
            (ellipsize(&r.author, authw), text)
        })
        .collect();

    // Legend: same glyphs, same rule -- only named when the table can carry
    // them, and dropped whole when there are no mark columns to explain.
    if !names.is_empty() {
        let mut legend = String::new();
        if rows.iter().any(|r| sets.iter().any(|s| s.contains(&r.sha))) {
            legend.push_str(&format!("{} {}", paint(CHECK, GREEN, color), paint("has commit", DIM, color)));
        }
        if equiv.iter().any(|e| !e.is_empty()) {
            if !legend.is_empty() {
                legend.push_str("   ");
            }
            legend.push_str(&format!(
                "{} {}",
                paint(EQUIV, YELLOW, color),
                paint("same patch, other sha", DIM, color)
            ));
        }
        if trailer.iter().any(|t| !t.is_empty()) {
            if !legend.is_empty() {
                legend.push_str("   ");
            }
            legend.push_str(&format!(
                "{} {}",
                paint(TRAILER, BLUE, color),
                paint("picked via -x trailer", DIM, color)
            ));
        }
        if author_match.iter().any(|a| !a.is_empty()) {
            if !legend.is_empty() {
                legend.push_str("   ");
            }
            legend.push_str(&format!(
                "{} {}",
                paint(FINGERPRINT, MAGENTA, color),
                paint("same author/date/subject", DIM, color)
            ));
        }
        if !legend.is_empty() {
            legend.push_str(&format!("   {} {}", paint(MISS, DIM, color), paint("neither", DIM, color)));
            println!("{}", legend);
        }
    }

    let mut hue = HEADER_COLORS.iter().cycle();
    let mut next_hue = || hue.next().unwrap();
    let mut head = paint(&format!("{:<shaw$}", "commit"), next_hue(), color);
    head.push_str("  ");
    if let Some(w) = pickw {
        head.push_str(&paint(&format!("{PICK_HEAD:<w$}"), next_hue(), color));
        head.push_str("  ");
    }
    head.push_str(&paint(&format!("{:<authw$}", "author"), next_hue(), color));
    head.push_str("  ");
    head.push_str(&paint(&format!("{:>datew$}", "date"), next_hue(), color));
    for (n, w) in names.iter().zip(&widths) {
        head.push_str("  ");
        head.push_str(&paint(&format!("{n:<w$}"), next_hue(), color));
    }
    head.push_str("  ");
    head.push_str(&paint(&format!("{:>statw$}", "\u{b1}"), next_hue(), color));
    if let Some(w) = pathw {
        head.push_str("  ");
        head.push_str(&paint(&format!("{:<w$}", "path"), next_hue(), color));
    }
    head.push_str("  ");
    head.push_str(&paint("subject", next_hue(), color));
    println!("{}", head);

    let grouped = row_files.iter().any(|f| !f.is_empty()) || row_bodies.iter().any(|(l, _)| !l.is_empty());

    for (i, (row, (author, text))) in rows.iter().zip(rows_text.iter()).enumerate() {
        if grouped && i > 0 {
            println!();
        }
        let anchored = hl.shas.contains(&row.sha);
        let sha_cell = format!("{:<shaw$}  ", row.short);
        let mut line = hl_cell(&sha_cell, hl, if anchored { MATCH } else { "" }, color);
        if let Some(w) = pickw {
            let cell = picks.and_then(|p| p.get(&row.sha)).map(|s| abbrev(s, shaw)).unwrap_or_default();
            line.push_str(&hl_cell(&format!("{cell:<w$}"), hl, YELLOW, color));
            line.push_str("  ");
        }
        let author_cell = format!("{:<authw$}", author);
        let date_cell = format!("{:>datew$}", row.date);
        let dim_or = |cell: &str, lit: bool| hl_cell(cell, hl, if lit { MATCH } else { DIM }, color);
        line.push_str(&dim_or(&author_cell, hl.author));
        line.push_str("  ");
        line.push_str(&dim_or(&date_cell, hl.date));
        for (col, w) in widths.iter().enumerate() {
            let mark = Mark::of(&row.sha, &sets[col], &equiv[col], &trailer[col], &author_match[col]);
            let pad = (w - 1) / 2;
            line.push_str("  ");
            line.push_str(&" ".repeat(pad));
            line.push_str(&paint(mark.glyph(), mark.color(), color));
            line.push_str(&" ".repeat(w - 1 - pad));
        }
        line.push_str("  ");
        line.push_str(&paint(&format!("{:>statw$}", stat_cells[i]), DIM, color));
        let row_path_list: &[String] = row_paths.and_then(|ps| ps.get(i)).map(Vec::as_slice).unwrap_or(&[]);
        if let Some(w) = pathw {
            line.push_str("  ");
            let p = row_path_list.first().map(String::as_str).unwrap_or("");
            line.push_str(&hl_cell(&format!("{:<w$}", ellipsize(p, w)), hl, DIM, color));
        }
        line.push_str("  ");
        line.push_str(&hl_text(&text[0], hl, color));
        println!("{}", line.trim_end());
        if let Some(w) = pathw {
            for p in row_path_list.iter().skip(1) {
                println!("{}{}", " ".repeat(path_prefix), hl_cell(&ellipsize(p, w), hl, DIM, color));
            }
        }
        for more in &text[1..] {
            println!("{}{}", " ".repeat(fixed), hl_text(more.trim_end(), hl, color));
        }

        if let Some((lines, extra)) = row_bodies.get(i) {
            if !lines.is_empty() {
                println!();
                let mut layers: Vec<(&str, &str)> =
                    hl.message.as_deref().map(|t| (t, MATCH)).into_iter().collect();
                layers.extend(search_layers(hl));
                for body_line in lines {
                    println!("{}{}", " ".repeat(fixed), paint_layers(body_line, &layers, DIM, color));
                }
                if *extra > 0 {
                    println!("{}{}", " ".repeat(fixed), paint(&format!("+{extra} more"), DIM, color));
                }
            }
        }

        if let Some(files) = row_files.get(i) {
            if !files.is_empty() {
                println!();
                for file_line in file_stat_lines(files) {
                    println!("{}", hl_cell(&file_line, hl, DIM, color));
                }
            }
        }
    }
}

fn hl_text(line: &str, hl: &Highlight, color: bool) -> String {
    let mut layers: Vec<(&str, &str)> = hl.message.as_deref().map(|t| (t, MATCH)).into_iter().collect();
    layers.extend(search_layers(hl));
    if layers.is_empty() {
        line.to_string()
    } else {
        paint_layers(line, &layers, "", color)
    }
}

/// See `commits::render::hl_cell` -- same rule, `log`'s own cells.
fn hl_cell(cell: &str, hl: &Highlight, base: &str, color: bool) -> String {
    let layers = search_layers(hl);
    if layers.is_empty() {
        if base.is_empty() { cell.to_string() } else { paint(cell, base, color) }
    } else {
        paint_layers(cell, &layers, base, color)
    }
}
