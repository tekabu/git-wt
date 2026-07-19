use std::collections::{HashMap, HashSet};

use crate::cmd::commits::args::{SubjectWidth, Wrap};
use crate::cmd::commits::rows::{CommitRow, FileStat, Mark};
use crate::ui::{
    abbrev, ellipsize, paint, width_bound, wrap_wide, AUTHOR_MAX, CHECK, DIM, EQUIV, GREEN,
    MIN_TEXTW, MISS, PICK_HEAD, YELLOW,
};

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
    names: &[String],
    sets: &[HashSet<String>],
    equiv: &[HashSet<String>],
    picks: Option<&HashMap<String, String>>,
    color: bool,
    width: Option<usize>,
    wrap: Wrap,
    subjectw: Option<SubjectWidth>,
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
    let legend = format!(
        "{} {}   {} {}   {} {}",
        paint(CHECK, GREEN, color),
        paint("has commit", DIM, color),
        paint(EQUIV, YELLOW, color),
        paint("same patch, other sha", DIM, color),
        paint(MISS, DIM, color),
        paint("neither", DIM, color),
    );
    println!("{}", legend);

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

    for (i, (row, text)) in rows.iter().enumerate() {
        let mut line = format!("{:<shaw$}  ", row.short);
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
        // Dim, so the marks and the subject stay what the eye lands on.
        let meta = format!("{:<authw$}  {:>datew$}", row.author, row.date);
        line.push_str(&paint(&meta, DIM, color));
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
        line.push_str(&text[0]);
        println!("{}", line.trim_end());
        // The rest of a wrapped subject, indented to the column it belongs to:
        // the row is still one commit, and the marks stay the leftmost thing
        // the eye has to scan.
        for more in &text[1..] {
            println!("{}{}", " ".repeat(fixed), more.trim_end());
        }

        // File block, tab-indented under the commit row. Kept dim so the commit
        // rows remain the primary scan target.
        if let Some(file_stats) = row_files.get(i) {
            if !file_stats.is_empty() {
                let pathw = file_stats
                    .iter()
                    .map(|f| f.path.chars().count())
                    .max()
                    .unwrap_or(0);
                let added_strs: Vec<String> = file_stats
                    .iter()
                    .map(|f| {
                        f.added
                            .map(|n| format!("+{}", n))
                            .unwrap_or_else(|| "-".to_string())
                    })
                    .collect();
                let removed_strs: Vec<String> = file_stats
                    .iter()
                    .map(|f| {
                        f.removed
                            .map(|n| format!("-{}", n))
                            .unwrap_or_else(|| "-".to_string())
                    })
                    .collect();
                let addw = added_strs
                    .iter()
                    .map(|s| width_bound(s))
                    .max()
                    .unwrap_or(1);
                let remw = removed_strs
                    .iter()
                    .map(|s| width_bound(s))
                    .max()
                    .unwrap_or(1);
                println!();
                for (f, (add_s, rem_s)) in file_stats
                    .iter()
                    .zip(added_strs.iter().zip(removed_strs.iter()))
                {
                    let file_line = format!(
                        "\t{}  {:<pathw$}  {:>addw$}  {:>remw$}",
                        f.status, f.path, add_s, rem_s
                    );
                    println!("{}", paint(&file_line, DIM, color));
                }
                println!();
            }
        }
    }
}
