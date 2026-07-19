use std::collections::{HashMap, HashSet};

use crate::cmd::commits::args::{SubjectWidth, Wrap};
use crate::cmd::commits::rows::{file_stat_lines, CommitRow, FileStat, Mark};
use crate::ui::{
    abbrev, ellipsize, paint, paint_matches, wrap_wide, AUTHOR_MAX, CHECK, DIM, EQUIV, GREEN,
    MATCH, MIN_TEXTW, MISS, PICK_HEAD, YELLOW,
};

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
}

/// Light a --message match where it sits in a subject line.
///
/// The term may be in the body instead, in which case nothing here matches and
/// the body block below the row is what shows it.
fn hl_text(line: &str, hl: &Highlight, color: bool) -> String {
    match &hl.message {
        None => line.to_string(),
        Some(t) => paint_matches(line, t, MATCH, "", color),
    }
}

/// Print the table: sha, author, date, a mark per branch, then the subject.
///
/// The subject comes last because it is the only cell holding arbitrary text.
/// Padding a cell means knowing its rendered width, and an emoji subject is
/// wider than its `chars().count()` -- so a padded subject column shifts every
/// column after it, which is precisely the table failing to line up. Last, it
/// is never padded, and no width table is needed to keep the marks straight.
///
/// Widths are measured on the plain text and color applied after, so the ANSI
/// escapes never skew the columns either.
pub(crate) fn render_commits(
    rows: &[CommitRow],
    row_files: &[Vec<FileStat>],
    row_bodies: &[(Vec<String>, usize)],
    names: &[String],
    sets: &[HashSet<String>],
    equiv: &[HashSet<String>],
    picks: Option<&HashMap<String, String>>,
    color: bool,
    width: Option<usize>,
    wrap: Wrap,
    subjectw: Option<SubjectWidth>,
    hl: &Highlight,
) {
    let widths: Vec<usize> = names.iter().map(|n| n.chars().count().max(1)).collect();
    let marksw: usize = widths.iter().map(|w| w + 2).sum();

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
    let pickcol = pickw.map_or(0, |w| w + 2);

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

    // Everything left of the subject, which is both what the subject has to
    // fit beside and what a wrapped line is indented past to line up under it.
    let fixed = shaw + 2 + pickcol + authw + 2 + datew + marksw + 2;

    // What the subject gets. A width asked for is the width, terminal or not:
    // an explicit one is an answer, where the terminal's is only a default --
    // so '--subject-width 100' on an 80-column terminal runs the line past the
    // edge on purpose, and off a terminal it cuts where nothing was cut before.
    let textw = match subjectw {
        Some(SubjectWidth::Cols(n)) => Some(n),
        Some(SubjectWidth::Full) => None,
        // Only the tail is budgeted, and only to keep a long subject from
        // wrapping where it was not asked to; piped output has no terminal to
        // fit, so it is never cut and never wrapped.
        None => width.map(|w| w.saturating_sub(fixed).max(MIN_TEXTW)),
    };

    let rows: Vec<(CommitRow, Vec<String>)> = rows
        .iter()
        .map(|r| {
            let text = match textw {
                Some(tw) => wrap_wide(&r.text, tw, wrap.lines()),
                None => vec![r.text.clone()],
            };
            let row = CommitRow {
                sha: r.sha.clone(),
                short: r.short.clone(),
                author: ellipsize(&r.author, authw),
                date: r.date.clone(),
                key: r.key.clone(),
                stamp: r.stamp.clone(),
                text: r.text.clone(),
                body: r.body.clone(),
            };
            (row, text)
        })
        .collect();
    let rows = &rows;

    // The date is right-aligned so the years line up under --date-human, where
    // an unpadded day makes 'Jan. 1, 2026' a character shorter than
    // 'Sep. 15, 2026'; left-aligned, that ragged edge is the first thing you
    // see. ISO is one width, so the alignment is moot there -- and free.
    // Legend above the header: the marks are the point of the table and the
    // '≈'/'·' distinction is not self-evident, so name each glyph once up top.
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

    let mut head = format!("{:<shaw$}  ", "commit");
    if let Some(w) = pickw {
        head.push_str(&format!("{PICK_HEAD:<w$}  "));
    }
    head.push_str(&format!("{:<authw$}  {:>datew$}", "author", "date"));
    for (n, w) in names.iter().zip(&widths) {
        head.push_str("  ");
        head.push_str(&format!("{n:<w$}"));
    }
    head.push_str("  subject");
    println!("{}", paint(&head, DIM, color));

    // With file blocks the table becomes a series of groups, so every commit is
    // fenced off by a blank line -- including one whose block is empty, which
    // would otherwise huddle against the block above and read as part of it.
    let grouped = row_files.iter().any(|f| !f.is_empty())
        || row_bodies.iter().any(|(lines, _)| !lines.is_empty());

    for (i, (row, text)) in rows.iter().enumerate() {
        if grouped && i > 0 {
            println!();
        }
        // A sha the filter named outright, or the anchor a bound was measured
        // from: the one row in the table that is not merely on the right side
        // of the answer, but is the answer.
        let anchored = hl.shas.contains(&row.sha);
        let sha_cell = format!("{:<shaw$}  ", row.short);
        let mut line = if anchored {
            paint(&sha_cell, MATCH, color)
        } else {
            sha_cell
        };
        if let Some(w) = pickw {
            // Blank, not '·': the column names a sha or it has nothing to say,
            // where the marks' '·' is an answer about a branch.
            let cell = picks
                .and_then(|p| p.get(&row.sha))
                .map(|s| abbrev(s, shaw))
                .unwrap_or_default();
            // Yellow, like the '≈' it explains.
            line.push_str(&paint(&format!("{cell:<w$}"), YELLOW, color));
            line.push_str("  ");
        }
        // Dim, so the marks and the subject stay what the eye lands on -- unless
        // a filter read this cell, in which case it is exactly what the eye came
        // for. Padded before coloring, so the escapes never skew the column.
        let author_cell = format!("{:<authw$}", row.author);
        let date_cell = format!("{:>datew$}", row.date);
        let dim_or = |cell: &str, lit: bool| {
            paint(cell, if lit { MATCH } else { DIM }, color)
        };
        line.push_str(&dim_or(&author_cell, hl.author));
        line.push_str("  ");
        line.push_str(&dim_or(&date_cell, hl.date));
        for ((set, eq), w) in sets.iter().zip(equiv).zip(&widths) {
            let mark = Mark::of(&row.sha, set, eq);
            // Center the one-cell mark under its header.
            let pad = (w - 1) / 2;
            line.push_str("  ");
            line.push_str(&" ".repeat(pad));
            line.push_str(&paint(mark.glyph(), mark.color(), color));
            line.push_str(&" ".repeat(w - 1 - pad));
        }
        line.push_str("  ");
        // Painted after the wrap, never before: the budget is measured on plain
        // text, and an escape counted as a column would cut the subject short.
        // A term split across a wrap boundary lights on neither half -- the row
        // is still right, just unmarked.
        line.push_str(&hl_text(&text[0], hl, color));
        println!("{}", line.trim_end());
        // The rest of a wrapped subject, indented to the column it belongs to:
        // the row is still one commit, and the marks stay the leftmost thing
        // the eye has to scan.
        for more in &text[1..] {
            println!("{}{}", " ".repeat(fixed), hl_text(more.trim_end(), hl, color));
        }

        // The body lines --message matched on, indented to the subject column
        // they continue. Printed because the row was kept for words that are
        // in none of the cells above: without them the match is invisible and
        // the table is asserting something it never shows.
        if let Some((lines, extra)) = row_bodies.get(i) {
            if !lines.is_empty() {
                println!();
                let term = hl.message.as_deref().unwrap_or_default();
                for body_line in lines {
                    println!(
                        "{}{}",
                        " ".repeat(fixed),
                        paint_matches(body_line, term, MATCH, DIM, color)
                    );
                }
                if *extra > 0 {
                    let more = format!("+{extra} more");
                    println!("{}{}", " ".repeat(fixed), paint(&more, DIM, color));
                }
            }
        }

        // File block, tab-indented under the commit row. Kept dim so the commit
        // rows remain the primary scan target.
        if let Some(file_stats) = row_files.get(i) {
            if !file_stats.is_empty() {
                println!();
                for file_line in file_stat_lines(file_stats) {
                    // Every file the commit touched, even under --filename: the
                    // filter chose the row, and a trimmed block could not answer
                    // what that commit did. The matched paths are lit instead.
                    let term = hl.file.as_deref().unwrap_or_default();
                    println!("{}", paint_matches(&file_line, term, MATCH, DIM, color));
                }
            }
        }
    }
}
