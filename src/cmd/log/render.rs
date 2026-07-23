use std::collections::HashMap;
use std::collections::HashSet;

use crate::cmd::commits::args::{BranchWidth, PathWidth, SubjectWidth, Wrap};
use crate::cmd::commits::render::{mark_fg, Highlight};
use crate::cmd::commits::rows::{file_stat_lines, CommitRow, FileStat, Mark};
use crate::ui::{
    abbrev, ellipsize, paint, wrap_wide, AUTHOR_MAX, BRANCH_HEAD_MAX, DIM, MIN_TEXTW, PATH_MAX,
    PICK_HEAD,
};
use stanza::renderer::console::Console;
use stanza::renderer::Renderer;
use stanza::style::{Bold, HAlign, Header, MaxWidth, MinWidth, Palette16, Styles, TextFg};
use stanza::table::{Cell, Col, Row, Table};

/// One row's `±` cell: churn scoped to the path alone.
pub(crate) fn stat_cell(added: Option<usize>, removed: Option<usize>) -> String {
    let part = |n: Option<usize>, sign: char| n.map(|n| format!("{sign}{n}")).unwrap_or_else(|| "-".to_string());
    format!("{} {}", part(added, '+'), part(removed, '-'))
}

/// The one accent every search/filter hit shares -- see `commits::render::HIT`.
const HIT: Palette16 = Palette16::BrightBlue;

const HEADER_HUES: [Palette16; 6] = [
    Palette16::BrightRed,
    Palette16::BrightMagenta,
    Palette16::BrightBlue,
    Palette16::BrightCyan,
    Palette16::BrightGreen,
    Palette16::BrightYellow,
];

fn hits(text: &str, hl: &Highlight, term: Option<&str>) -> bool {
    let text = text.to_lowercase();
    term.is_some_and(|t| !t.is_empty() && text.contains(&t.to_lowercase()))
        || hl.search.iter().any(|t| !t.is_empty() && text.contains(&t.to_lowercase()))
}

fn pad_cell(s: &str, width: usize) -> String {
    format!(" {s:width$} ")
}

fn pad_lines(lines: &[String], width: usize) -> String {
    lines
        .iter()
        .map(|l| format!(" {l:width$} "))
        .collect::<Vec<_>>()
        .join("\n")
}

fn fixed_col(width: usize, align: HAlign) -> Col {
    Col::new(
        Styles::default()
            .with(MinWidth(width + 2))
            .with(MaxWidth(width + 2))
            .with(align),
    )
}

fn fg_style(fg: Option<Palette16>) -> Styles {
    match fg {
        Some(p) => Styles::default().with(Bold(true)).with(TextFg(p)),
        None => Styles::default(),
    }
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
    let marksw: usize = widths.iter().map(|w| w + 2 + 2).sum();

    let shaw = rows
        .iter()
        .map(|r| r.short.chars().count())
        .chain(std::iter::once("commit".len()))
        .max()
        .unwrap_or(0);

    let pickw = picks.map(|_| shaw.max(PICK_HEAD.len()));
    let pickcol = pickw.map_or(0, |w| w + 2 + 2);

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
    let pathcol = pathw.map_or(0, |w| w + 2 + 2);

    let fixed = shaw + 2 + 2 + pickcol + authw + 2 + 2 + datew + 2 + 2 + marksw + 2 + 2 + statw + 2 + 2 + pathcol;

    // File and body lines fold into the subject column too -- see the loop
    // below -- so an unbudgeted (piped) table widens to fit their longest
    // line as well.
    let extra_lines_max = row_files
        .iter()
        .flat_map(|f| file_stat_lines(f))
        .chain(row_bodies.iter().flat_map(|(l, _)| l.iter().cloned()))
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0);

    let textw = match subjectw {
        Some(SubjectWidth::Cols(n)) => n,
        Some(SubjectWidth::Full) => rows.iter().map(|r| r.text.chars().count()).max().unwrap_or(MIN_TEXTW).max(MIN_TEXTW),
        None => match width {
            Some(w) => w.saturating_sub(fixed).max(MIN_TEXTW),
            None => rows
                .iter()
                .map(|r| r.text.chars().count())
                .chain(std::iter::once(extra_lines_max))
                .max()
                .unwrap_or(MIN_TEXTW)
                .max(MIN_TEXTW),
        },
    };

    let rows_text: Vec<(String, Vec<String>)> = rows
        .iter()
        .map(|r| {
            let text = wrap_wide(&r.text, textw, wrap.lines());
            (ellipsize(&r.author, authw), text)
        })
        .collect();

    // Legend: same glyphs, same rule -- only named when the table can carry
    // them, and dropped whole when there are no mark columns to explain.
    if !names.is_empty() {
        let mut legend = String::new();
        if rows.iter().any(|r| sets.iter().any(|s| s.contains(&r.sha))) {
            legend.push_str(&format!(
                "{} {}",
                paint(crate::ui::CHECK, crate::ui::GREEN, color),
                paint("has commit", DIM, color)
            ));
        }
        if equiv.iter().any(|e| !e.is_empty()) {
            if !legend.is_empty() {
                legend.push_str("   ");
            }
            legend.push_str(&format!(
                "{} {}",
                paint(crate::ui::EQUIV, crate::ui::YELLOW, color),
                paint("same patch, other sha", DIM, color)
            ));
        }
        if trailer.iter().any(|t| !t.is_empty()) {
            if !legend.is_empty() {
                legend.push_str("   ");
            }
            legend.push_str(&format!(
                "{} {}",
                paint(crate::ui::TRAILER, crate::ui::BLUE, color),
                paint("picked via -x trailer", DIM, color)
            ));
        }
        if author_match.iter().any(|a| !a.is_empty()) {
            if !legend.is_empty() {
                legend.push_str("   ");
            }
            legend.push_str(&format!(
                "{} {}",
                paint(crate::ui::FINGERPRINT, crate::ui::MAGENTA, color),
                paint("same author/date/subject", DIM, color)
            ));
        }
        if !legend.is_empty() {
            legend.push_str(&format!(
                "   {} {}",
                paint(crate::ui::MISS, DIM, color),
                paint("neither", DIM, color)
            ));
            println!("{}", legend);
        }
    }
    // Columns: commit, [pick], author, date, one per branch mark, ±, [path], subject.
    let mut table_cols = vec![fixed_col(shaw, HAlign::Left)];
    if let Some(w) = pickw {
        table_cols.push(fixed_col(w, HAlign::Left));
    }
    table_cols.push(fixed_col(authw, HAlign::Left));
    table_cols.push(fixed_col(datew, HAlign::Right));
    for w in &widths {
        table_cols.push(fixed_col(*w, HAlign::Centred));
    }
    table_cols.push(fixed_col(statw, HAlign::Right));
    if let Some(w) = pathw {
        table_cols.push(fixed_col(w, HAlign::Left));
    }
    table_cols.push(Col::new(
        Styles::default()
            .with(MinWidth(textw + 2))
            .with(MaxWidth(textw + 2)),
    ));

    let mut hue = HEADER_HUES.iter().cycle();
    let mut next_hue = || {
        let h = hue.next().unwrap().clone();
        if color {
            Styles::default().with(TextFg(h))
        } else {
            Styles::default()
        }
    };
    let mut header_cells = vec![Cell::new(next_hue(), pad_cell("commit", shaw).into())];
    if let Some(w) = pickw {
        header_cells.push(Cell::new(next_hue(), pad_cell(PICK_HEAD, w).into()));
    }
    header_cells.push(Cell::new(next_hue(), pad_cell("author", authw).into()));
    header_cells.push(Cell::new(next_hue(), pad_cell("date", datew).into()));
    for (n, w) in names.iter().zip(&widths) {
        header_cells.push(Cell::new(next_hue(), pad_cell(n, *w).into()));
    }
    header_cells.push(Cell::new(next_hue(), pad_cell("\u{b1}", statw).into()));
    if let Some(w) = pathw {
        header_cells.push(Cell::new(next_hue(), pad_cell("path", w).into()));
    }
    header_cells.push(Cell::new(next_hue(), pad_cell("subject", textw).into()));
    let mut table_rows = vec![Row::new(Styles::default().with(Header(true)), header_cells)];

    // A supplementary line (a matched body line, a touched file, a rename's
    // other name) that belongs to the row above it rather than being a
    // commit of its own -- see `commits::render`'s twin of this helper.
    let blank_lead = |cells: &mut Vec<Cell>| {
        cells.push(Cell::new(Styles::default(), pad_cell("", shaw).into()));
        if let Some(w) = pickw {
            cells.push(Cell::new(Styles::default(), pad_cell("", w).into()));
        }
        cells.push(Cell::new(Styles::default(), pad_cell("", authw).into()));
        cells.push(Cell::new(Styles::default(), pad_cell("", datew).into()));
        for w in &widths {
            cells.push(Cell::new(Styles::default(), pad_cell("", *w).into()));
        }
        cells.push(Cell::new(Styles::default(), pad_cell("", statw).into()));
        if let Some(w) = pathw {
            cells.push(Cell::new(Styles::default(), pad_cell("", w).into()));
        }
    };
    let push_extra_line = |table_rows: &mut Vec<Row>, text: &str, fg: Option<Palette16>| {
        let mut cells = Vec::new();
        blank_lead(&mut cells);
        let content = format!("  {}", ellipsize(text, textw.saturating_sub(2)));
        cells.push(Cell::new(fg_style(fg), pad_cell(&content, textw).into()));
        table_rows.push(Row::new(Styles::default(), cells));
    };

    for (i, (row, (author, text))) in rows.iter().zip(rows_text.iter()).enumerate() {
        let anchored = hl.shas.contains(&row.sha);
        let sha_fg = if !color {
            None
        } else if hits(&row.short, hl, None) {
            Some(HIT)
        } else if anchored {
            Some(Palette16::BrightYellow)
        } else {
            None
        };
        let mut cells = vec![Cell::new(fg_style(sha_fg), pad_cell(&row.short, shaw).into())];

        if let Some(w) = pickw {
            let cell = picks.and_then(|p| p.get(&row.sha)).map(|s| abbrev(s, shaw)).unwrap_or_default();
            let fg = if !color {
                None
            } else if hits(&cell, hl, None) {
                Some(HIT)
            } else {
                Some(Palette16::Yellow)
            };
            cells.push(Cell::new(fg_style(fg), pad_cell(&cell, w).into()));
        }

        let author_fg = if !color {
            None
        } else if hits(author, hl, None) {
            Some(HIT)
        } else if hl.author {
            Some(Palette16::BrightYellow)
        } else {
            None
        };
        cells.push(Cell::new(fg_style(author_fg), pad_cell(author, authw).into()));

        let date_fg = if !color {
            None
        } else if hits(&row.date, hl, None) {
            Some(HIT)
        } else if hl.date {
            Some(Palette16::BrightYellow)
        } else {
            None
        };
        cells.push(Cell::new(fg_style(date_fg), pad_cell(&row.date, datew).into()));

        for col in 0..widths.len() {
            let mark = Mark::of(&row.sha, &sets[col], &equiv[col], &trailer[col], &author_match[col]);
            let fg = if color { mark_fg(mark) } else { None };
            cells.push(Cell::new(fg_style(fg), mark.glyph().into()));
        }

        cells.push(Cell::new(Styles::default(), pad_cell(&stat_cells[i], statw).into()));

        let row_path_list: &[String] = row_paths.and_then(|ps| ps.get(i)).map(Vec::as_slice).unwrap_or(&[]);
        if let Some(w) = pathw {
            let p = row_path_list.first().map(String::as_str).unwrap_or("");
            let p = ellipsize(p, w);
            let fg = if color && hits(&p, hl, None) { Some(HIT) } else { None };
            cells.push(Cell::new(fg_style(fg), pad_cell(&p, w).into()));
        }

        let subject_fg = if color && hits(&row.text, hl, hl.message.as_deref()) {
            Some(HIT)
        } else {
            None
        };
        cells.push(Cell::new(fg_style(subject_fg), pad_lines(text, textw).into()));

        table_rows.push(Row::new(Styles::default(), cells));

        // A rename's other name, or a multi-path call's other files -- folded
        // into the table rather than crammed into the one-line path cell,
        // which only ever shows the first.
        if pathw.is_some() {
            for p in row_path_list.iter().skip(1) {
                let fg = if color && hits(p, hl, None) { Some(HIT) } else { None };
                push_extra_line(&mut table_rows, p, fg);
            }
        }

        if let Some((lines, extra)) = row_bodies.get(i) {
            for body_line in lines {
                let fg = if color && hits(body_line, hl, hl.message.as_deref()) { Some(HIT) } else { None };
                push_extra_line(&mut table_rows, body_line, fg);
            }
            if *extra > 0 {
                push_extra_line(&mut table_rows, &format!("+{extra} more"), None);
            }
        }

        if let Some(files) = row_files.get(i) {
            for file_line in file_stat_lines(files) {
                let fg = if color && hits(&file_line, hl, hl.file.as_deref()) { Some(HIT) } else { None };
                push_extra_line(&mut table_rows, &file_line, fg);
            }
        }
    }

    let table = Table::new(Styles::default(), table_cols, table_rows);
    println!("{}", Console::default().render(&table));
}
