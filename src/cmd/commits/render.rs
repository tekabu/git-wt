use std::collections::{HashMap, HashSet};

use crate::cmd::commits::args::{BranchWidth, SubjectWidth, Wrap};
use crate::cmd::commits::rows::{consolidate_file_stats, file_stat_lines, CommitRow, FileStat, Mark};
use crate::ui::{
    abbrev, ellipsize, paint, wrap_wide, AUTHOR_MAX, BLUE, BRANCH_HEAD_MAX, CHECK, DIM, EQUIV,
    FINGERPRINT, GREEN, MAGENTA, MIN_TEXTW, MISS, PICK_HEAD, TRAILER, YELLOW,
};
use stanza::renderer::console::Console;
use stanza::renderer::Renderer;
use stanza::style::{Bold, HAlign, Header, MaxWidth, MinWidth, Palette16, Styles, TextFg};
use stanza::table::{Cell, Col, Row, Table};

/// Which cells a filter acted on, so the eye can find them in a long table.
///
/// A filtered table is all matches by definition, so highlighting is about
/// *where* the answer lives, not which rows survived: the column a date filter
/// read, the column --author read. `shas` is the exception and the useful one --
/// a commit named outright, or the anchor a bound was measured from, is one row
/// among rows that merely fall on the right side of it.
/// The text filters go further still: they light the matched *characters*, not
/// the cell around them, because a subject is a sentence and the word that was
/// searched for is a few letters of it.
#[derive(Default)]
pub(crate) struct Highlight {
    pub(crate) date: bool,
    pub(crate) author: bool,
    pub(crate) shas: HashSet<String>,
    /// Lowercased terms, or None when the filter was not asked for.
    pub(crate) message: Option<String>,
    pub(crate) file: Option<String>,
    /// `--search`'s `|`-split terms, lowercased. Highlight only -- unlike every
    /// other field here it names nothing a row was kept or dropped for, so it
    /// is lit wherever it sits: sha, author, date, subject, file paths.
    ///
    /// Stanza colors a cell, not a character, so a hit no longer picks out its
    /// own span within the text -- it tints the whole cell one accent instead.
    pub(crate) search: Vec<String>,
}

/// Does `text` contain any of `hl.search`'s terms, or `term`, case-insensitively?
fn hits(text: &str, hl: &Highlight, term: Option<&str>) -> bool {
    let text = text.to_lowercase();
    term.is_some_and(|t| !t.is_empty() && text.contains(&t.to_lowercase()))
        || hl.search.iter().any(|t| !t.is_empty() && text.contains(&t.to_lowercase()))
}

/// The one accent every search/filter hit shares -- bold bright blue, the same
/// hue `list --search` uses, so a hit reads the same way in every table.
const HIT: Palette16 = Palette16::BrightBlue;

/// The header row's per-column hues -- the same six `list`'s header cycles,
/// so every migrated table's header reads the same way.
const HEADER_HUES: [Palette16; 6] = [
    Palette16::BrightRed,
    Palette16::BrightMagenta,
    Palette16::BrightBlue,
    Palette16::BrightCyan,
    Palette16::BrightGreen,
    Palette16::BrightYellow,
];

/// Stanza foreground for a mark glyph, or None for `Missing` -- Stanza's
/// 16-color palette has no dim/gray, so a miss just stays the terminal's
/// default rather than fighting for a hue that isn't there.
pub(crate) fn mark_fg(m: Mark) -> Option<Palette16> {
    match m {
        Mark::Has => Some(Palette16::Green),
        Mark::Equivalent => Some(Palette16::Yellow),
        Mark::Trailer => Some(Palette16::Blue),
        Mark::AuthorMatch => Some(Palette16::Magenta),
        Mark::Missing => None,
    }
}

/// One space of padding on each side, baked into the cell content itself --
/// see the comment on `list`'s `pad_cell`. `lines` may be more than one: a
/// wrapped subject is still one cell, each of its lines padded the same way
/// and joined back with the newlines Stanza's own wrapper already respects.
fn pad_lines(lines: &[String], width: usize) -> String {
    lines
        .iter()
        .map(|l| format!(" {l:width$} "))
        .collect::<Vec<_>>()
        .join("\n")
}

fn pad_cell(s: &str, width: usize) -> String {
    format!(" {s:width$} ")
}

fn fixed_col(width: usize, align: HAlign) -> Col {
    Col::new(
        Styles::default()
            .with(MinWidth(width + 2))
            .with(MaxWidth(width + 2))
            .with(align),
    )
}

/// Print the table: sha, author, date, a mark per branch, then the subject.
///
/// The subject comes last because it is the only cell holding arbitrary text.
/// Stanza wraps and pads it itself, so an emoji-widened subject can no longer
/// throw off a column the way it could when this table padded cells by hand.
pub(crate) fn render_commits(
    rows: &[CommitRow],
    row_files: &[Vec<FileStat>],
    row_bodies: &[(Vec<String>, usize)],
    names: &[String],
    sets: &[HashSet<String>],
    equiv: &[HashSet<String>],
    trailer: &[HashSet<String>],
    author_match: &[HashSet<String>],
    picks: Option<&HashMap<String, String>>,
    squash: bool,
    color: bool,
    width: Option<usize>,
    wrap: Wrap,
    subjectw: Option<SubjectWidth>,
    branchw: Option<BranchWidth>,
    hl: &Highlight,
) {
    // Cut a long branch name before it dictates the column: nothing left of
    // the marks bounds it the way the terminal bounds the subject, so an
    // issue-shaped name would otherwise drag the marks and the subject off
    // the edge on every row. `Full` (or an explicit width) opts out.
    let cap = match branchw {
        Some(BranchWidth::Full) => usize::MAX,
        Some(BranchWidth::Cols(n)) => n,
        None => BRANCH_HEAD_MAX,
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

    // A picked sha is abbreviated to the same length the rows' own shas are, so
    // the two columns read as the one kind of thing they are -- and so a sha
    // named here is a sha you can find in the commit column of another row.
    let pickw = picks.map(|_| shaw.max(PICK_HEAD.len()));
    let pickcol = pickw.map_or(0, |w| w + 2 + 2);

    // The author column is sized to its longest name, but a name is not worth
    // unbounded width when the subject is competing for the same line; on a
    // terminal it caps, and a piped table keeps every name whole.
    let mut authw = rows
        .iter()
        .map(|r| r.author.chars().count())
        .chain(std::iter::once("author".len()))
        .max()
        .unwrap_or(0);
    if width.is_some() {
        authw = authw.min(AUTHOR_MAX);
    }

    // The date is never cut: half a date is not a date. It is ASCII and a fixed
    // shape, so it costs the same on every row.
    let datew = rows
        .iter()
        .map(|r| r.date.chars().count())
        .chain(std::iter::once("date".len()))
        .max()
        .unwrap_or(0);

    // Everything left of the subject -- used only to size it, since Stanza now
    // owns every column's actual position.
    let fixed = shaw + 2 + 2 + pickcol + authw + 2 + 2 + datew + 2 + 2 + marksw + 2 + 2;

    // What the subject gets. A width asked for is the width, terminal or not:
    // an explicit one is an answer, where the terminal's is only a default --
    // so '--subject-width 100' on an 80-column terminal runs the line past the
    // edge on purpose, and off a terminal it cuts where nothing was cut before.
    // File and body lines now live in the subject column too -- see the loop
    // below -- so an unbudgeted (piped, no terminal) table widens to fit their
    // longest line as well, the same "never cut off a terminal" rule the
    // subject itself gets. A budgeted (terminal) table leaves them to be
    // ellipsized instead, same as a long branch or path name elsewhere.
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
        // Only the tail is budgeted, and only to keep a long subject from
        // wrapping where it was not asked to; piped output has no terminal to
        // fit, so it is never cut and never wrapped.
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

    let rows: Vec<(CommitRow, Vec<String>)> = rows
        .iter()
        .map(|r| {
            let text = wrap_wide(&r.text, textw, wrap.lines());
            let row = CommitRow {
                sha: r.sha.clone(),
                short: r.short.clone(),
                author: ellipsize(&r.author, authw),
                date: r.date.clone(),
                key: r.key.clone(),
                stamp: r.stamp.clone(),
                email: r.email.clone(),
                author_iso: r.author_iso.clone(),
                text: r.text.clone(),
                body: r.body.clone(),
            };
            (row, text)
        })
        .collect();
    let rows = &rows;

    // Legend above the table: the marks are the point of it and the '≈'/'·'
    // distinction is not self-evident, so name each glyph once up top.
    // '≈' is named only when it can appear: --no-cherry skips the patch-id walk
    // and leaves every equivalence set empty, so advertising the glyph there
    // promises a mark the table can never carry.
    //
    // With no mark columns -- one worktree, so nothing to compare against --
    // there are no glyphs to name, and the legend is dropped with them.
    if !names.is_empty() {
        let mut legend = String::new();
        // Same rule for '✓', and it is `merge --review` that needs it: its rows
        // are the range the destination is *missing*, so its one column cannot
        // carry a check by construction. Asked of the rows rather than the sets
        // -- a set is non-empty there and still holds none of them.
        if rows.iter().any(|(r, _)| sets.iter().any(|s| s.contains(&r.sha))) {
            legend.push_str(&format!(
                "{} {}",
                paint(CHECK, GREEN, color),
                paint("has commit", DIM, color),
            ));
        }
        if equiv.iter().any(|e| !e.is_empty()) {
            if !legend.is_empty() {
                legend.push_str("   ");
            }
            legend.push_str(&format!(
                "{} {}",
                paint(EQUIV, YELLOW, color),
                paint("same patch, other sha", DIM, color),
            ));
        }
        if trailer.iter().any(|t| !t.is_empty()) {
            if !legend.is_empty() {
                legend.push_str("   ");
            }
            legend.push_str(&format!(
                "{} {}",
                paint(TRAILER, BLUE, color),
                paint("picked via -x trailer", DIM, color),
            ));
        }
        if author_match.iter().any(|a| !a.is_empty()) {
            if !legend.is_empty() {
                legend.push_str("   ");
            }
            legend.push_str(&format!(
                "{} {}",
                paint(FINGERPRINT, MAGENTA, color),
                paint("same author/date/subject", DIM, color),
            ));
        }
        // '·' is defined by contrast -- "neither of the above" -- so on its own
        // it names nothing. That is the all-'·' review, where every row is new
        // to the destination and the column header already says so.
        if !legend.is_empty() {
            legend.push_str(&format!(
                "   {} {}",
                paint(MISS, DIM, color),
                paint("neither", DIM, color),
            ));
            println!("{}", legend);
        }
    }

    // Columns: commit, [pick], author, date, one per branch mark, subject.
    let mut table_cols = vec![fixed_col(shaw, HAlign::Left)];
    if let Some(w) = pickw {
        table_cols.push(fixed_col(w, HAlign::Left));
    }
    table_cols.push(fixed_col(authw, HAlign::Left));
    table_cols.push(fixed_col(datew, HAlign::Right));
    for w in &widths {
        table_cols.push(fixed_col(*w, HAlign::Centred));
    }
    table_cols.push(
        Col::new(
            Styles::default()
                .with(MinWidth(textw + 2))
                .with(MaxWidth(textw + 2)),
        ),
    );

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
    header_cells.push(Cell::new(next_hue(), pad_cell("subject", textw).into()));
    let mut table_rows = vec![Row::new(Styles::default().with(Header(true)), header_cells)];

    // A supplementary line (a matched body line, a touched file) that belongs
    // to the row above it rather than being a commit of its own: every column
    // but the subject stays blank, so it reads as an indented continuation of
    // that row instead of a row in its own right.
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
    };
    let push_extra_line = |table_rows: &mut Vec<Row>, text: &str, fg: Option<Palette16>| {
        let mut cells = Vec::new();
        blank_lead(&mut cells);
        let content = format!("  {}", ellipsize(text, textw.saturating_sub(2)));
        cells.push(Cell::new(fg_style(fg), pad_cell(&content, textw).into()));
        table_rows.push(Row::new(Styles::default(), cells));
    };

    for (i, (row, text)) in rows.iter().enumerate() {
        // A sha the filter named outright, or the anchor a bound was measured
        // from: the one row in the table that is not merely on the right side
        // of the answer, but is the answer.
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
        let mut cells = vec![Cell::new(
            fg_style(sha_fg),
            pad_cell(&row.short, shaw).into(),
        )];

        if let Some(w) = pickw {
            // Blank, not '·': the column names a sha or it has nothing to say,
            // where the marks' '·' is an answer about a branch.
            let cell = picks
                .and_then(|p| p.get(&row.sha))
                .map(|s| abbrev(s, shaw))
                .unwrap_or_default();
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
        } else if hits(&row.author, hl, None) {
            Some(HIT)
        } else if hl.author {
            Some(Palette16::BrightYellow)
        } else {
            None
        };
        cells.push(Cell::new(
            fg_style(author_fg),
            pad_cell(&row.author, authw).into(),
        ));

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

        for (col, _) in widths.iter().enumerate() {
            let mark = Mark::of(&row.sha, &sets[col], &equiv[col], &trailer[col], &author_match[col]);
            let fg = if color { mark_fg(mark) } else { None };
            cells.push(Cell::new(fg_style(fg), mark.glyph().into()));
        }

        let subject_fg = if color && hits(&row.text, hl, hl.message.as_deref()) {
            Some(HIT)
        } else {
            None
        };
        cells.push(Cell::new(fg_style(subject_fg), pad_lines(text, textw).into()));

        table_rows.push(Row::new(Styles::default(), cells));

        // The body lines --message matched on, folded into the table right
        // under the commit they belong to -- see `push_extra_line`. Printed
        // because the row was kept for words that are in none of the cells
        // above: without them the match is invisible and the table is
        // asserting something it never shows.
        if let Some((lines, extra)) = row_bodies.get(i) {
            for body_line in lines {
                let fg = if color && hits(body_line, hl, hl.message.as_deref()) { Some(HIT) } else { None };
                push_extra_line(&mut table_rows, body_line, fg);
            }
            if *extra > 0 {
                push_extra_line(&mut table_rows, &format!("+{extra} more"), None);
            }
        }

        // File block, folded into the table right under the commit row it
        // belongs to. Off under --squash: the files are consolidated into
        // one block below, not repeated per commit.
        if let Some(file_stats) = (!squash).then(|| row_files.get(i)).flatten() {
            for file_line in file_stat_lines(file_stats) {
                // Every file the commit touched, even under --filename: the
                // filter chose the row, and a trimmed block could not answer
                // what that commit did. A line the filter or --search read
                // is lit; the rest stays plain.
                let fg = if color && hits(&file_line, hl, hl.file.as_deref()) { Some(HIT) } else { None };
                push_extra_line(&mut table_rows, &file_line, fg);
            }
        }
    }

    // The one block --squash prints in place of the per-commit ones: every file
    // the shown commits touched, counts summed, once, folded into the table
    // right after the last commit row. Empty when no shown commit touched a
    // file -- then nothing to consolidate, and no rows are added for it.
    if squash {
        let consolidated = consolidate_file_stats(row_files);
        if !consolidated.is_empty() {
            push_extra_line(&mut table_rows, "consolidated files", None);
            for file_line in file_stat_lines(&consolidated) {
                let fg = if color && hits(&file_line, hl, hl.file.as_deref()) { Some(HIT) } else { None };
                push_extra_line(&mut table_rows, &file_line, fg);
            }
        }
    }

    let table = Table::new(Styles::default(), table_cols, table_rows);
    println!("{}", Console::default().render(&table));
}

fn fg_style(fg: Option<Palette16>) -> Styles {
    match fg {
        Some(p) => Styles::default().with(Bold(true)).with(TextFg(p)),
        None => Styles::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_fg_maps_known_marks() {
        assert!(matches!(mark_fg(Mark::Has), Some(Palette16::Green)));
        assert!(matches!(mark_fg(Mark::Equivalent), Some(Palette16::Yellow)));
        assert!(matches!(mark_fg(Mark::Trailer), Some(Palette16::Blue)));
        assert!(matches!(mark_fg(Mark::AuthorMatch), Some(Palette16::Magenta)));
        assert!(mark_fg(Mark::Missing).is_none());
    }

    #[test]
    fn hits_checks_search_terms_and_an_extra_needle_case_insensitively() {
        let hl = Highlight {
            search: vec!["EAT".to_string()],
            ..Default::default()
        };
        assert!(hits("feature", &hl, None));
        assert!(hits("anything", &hl, Some("thing")));
        assert!(!hits("nothing here", &hl, Some("")));
    }
}
