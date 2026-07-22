use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

use crate::cmd::commits::rows::{consolidate_file_stats, CommitRow, FileStat, Mark};
use crate::ui::{abbrev, PICK_HEAD};

/// `commits_2026-07-17_14-30-05.md`: ISO, so the names sort the way the dates
/// do, and stamped to the second so a re-run never silently eats the last one.
///
/// The stamp comes from `date`, for the same reason the terminal width comes
/// from `tput`: turning a unix timestamp into the user's local calendar needs
/// a timezone database this crate has no dependency for.
pub(crate) fn md_filename() -> String {
    let stamp = Command::new("date")
        .arg("+%Y-%m-%d_%H-%M-%S")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            // No `date`: seconds since the epoch still sorts and still differs
            // from the last run, which is all the name owes anyone.
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs().to_string())
                .unwrap_or_else(|_| "report".into())
        });
    format!("commits_{stamp}.md")
}

/// Escape a cell so its content cannot be read as table syntax.
///
/// A `|` in a commit subject would end the cell and shift every column after
/// it -- the markdown twin of the emoji-width bug, and this one silently
/// invents columns rather than merely misaligning them.
pub(crate) fn md_cell(s: &str) -> String {
    s.replace('\\', "\\\\").replace('|', "\\|")
}

/// What the report calls itself, since two commands render through `write_md`
/// and a review is not a `commits` run: its heading names the merge, and its
/// `labels` list is the branch coming over rather than worktrees being
/// compared. The mark column is named by `names`, as ever.
pub(crate) struct MdHead {
    pub(crate) title: &'static str,
    /// The word before the `labels` list.
    pub(crate) labels: &'static str,
}

impl MdHead {
    pub(crate) fn commits() -> Self {
        MdHead { title: "git-wt commits", labels: "Worktrees" }
    }

    pub(crate) fn review() -> Self {
        MdHead { title: "git-wt merge --review", labels: "Merging" }
    }

    pub(crate) fn log() -> Self {
        MdHead { title: "git-wt log", labels: "Branches" }
    }
}

/// Write the table as a markdown file, and say where it went.
///
/// Subjects are never truncated here: a file has no right edge to run out of,
/// so the terminal's budget would only lose information the reader asked for.
pub(crate) fn write_md(
    path: &Path,
    rows: &[CommitRow],
    row_files: &[Vec<FileStat>],
    row_bodies: &[(Vec<String>, usize)],
    labels: &[String],
    names: &[String],
    sets: &[HashSet<String>],
    equiv: &[HashSet<String>],
    trailer: &[HashSet<String>],
    author_match: &[HashSet<String>],
    picks: Option<&HashMap<String, String>>,
    squash: bool,
    cmd: &str,
    head: &MdHead,
) -> Result<(), String> {
    let mut out = String::new();
    // Named by the caller, because `merge --review` renders through here too:
    // a heading saying `commits` would contradict the Command line two lines
    // below it, and its `labels` is one branch being merged rather than the
    // worktrees a comparison is between.
    out.push_str(&format!("# {}\n\n", head.title));
    out.push_str(&format!("- Command: `{}`\n", md_cell(cmd)));
    // The labels, not the column names: one worktree has no mark columns, and
    // the report still has to say whose log this is.
    out.push_str(&format!("- {}: {}\n", head.labels, labels.iter()
        .map(|n| format!("`{}`", md_cell(n)))
        .collect::<Vec<_>>()
        .join(", ")));
    out.push_str(&format!("- Commits: {}\n", rows.len()));
    // The glyphs are the whole content of the table; a reader who was not at
    // the terminal has nowhere else to learn them. Each is named only when the
    // table can carry it -- the same rule, and the same two predicates, as
    // render_commits. '✓' asks the rows rather than the sets, because a
    // `merge --review` table is the range its one column is *missing*: the set
    // is full and holds none of these rows. No columns, no glyphs: a
    // one-worktree report has nothing to explain.
    if !names.is_empty() {
        let mut legend: Vec<&str> = Vec::new();
        if rows.iter().any(|r| sets.iter().any(|s| s.contains(&r.sha))) {
            legend.push("`✓` has the commit");
        }
        if equiv.iter().any(|e| !e.is_empty()) {
            legend.push("`≈` has the same patch under another sha");
        }
        if trailer.iter().any(|t| !t.is_empty()) {
            legend.push("`←` picked via `-x` trailer from this commit");
        }
        if author_match.iter().any(|a| !a.is_empty()) {
            legend.push("`~` same author/date/subject under another sha");
        }
        // '·' is "neither of the above", so on its own it names nothing.
        if !legend.is_empty() {
            legend.push("`·` has neither");
            out.push_str(&format!("- Legend: {}\n", legend.join(" · ")));
        }
    }
    if picks.is_some() {
        out.push_str("- `pick`: the sha that other copy of the patch was committed under\n");
    }
    out.push('\n');

    out.push_str("| commit |");
    if picks.is_some() {
        out.push_str(&format!(" {PICK_HEAD} |"));
    }
    out.push_str(" author | date |");
    for n in names {
        out.push_str(&format!(" {} |", md_cell(n)));
    }
    out.push_str(" subject |\n|---|");
    if picks.is_some() {
        out.push_str("---|");
    }
    out.push_str("---|---|");
    for _ in names {
        out.push_str(":-:|");
    }
    out.push_str("---|\n");

    // The shas the rows print, so a picked sha is one the table itself names.
    let shaw = rows
        .iter()
        .map(|r| r.short.chars().count())
        .max()
        .unwrap_or(0);

    for (i, row) in rows.iter().enumerate() {
        out.push_str(&format!("| `{}` |", md_cell(&row.short)));
        if let Some(p) = picks {
            match p.get(&row.sha) {
                Some(s) => out.push_str(&format!(" `{}` |", md_cell(&abbrev(s, shaw)))),
                None => out.push_str("  |"),
            }
        }
        out.push_str(&format!(
            " {} | {} |",
            md_cell(&row.author),
            md_cell(&row.date)
        ));
        for (col, set) in sets.iter().enumerate() {
            out.push_str(&format!(
                " {} |",
                Mark::of(&row.sha, set, &equiv[col], &trailer[col], &author_match[col]).glyph()
            ));
        }
        let mut subject = md_cell(&row.text);
        // The body lines --message matched on, for the same reason the terminal
        // prints them: the row was kept for words no cell here holds.
        if let Some((lines, extra)) = row_bodies.get(i) {
            if !lines.is_empty() {
                subject.push_str("<br><br>");
                for l in lines {
                    subject.push_str(&format!("{}<br>", md_cell(l)));
                }
                if *extra > 0 {
                    subject.push_str(&format!("+{extra} more<br>"));
                }
            }
        }
        // The per-commit block, unless --squash folded them into the one below.
        if let Some(file_stats) = (!squash).then(|| row_files.get(i)).flatten() {
            if !file_stats.is_empty() {
                let mut lines = String::from("<br><br>");
                for f in file_stats {
                    lines.push_str(&format!(
                        "{} {} +{} -{}<br>",
                        f.status,
                        md_cell(&f.path),
                        f.added.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string()),
                        f.removed.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string()),
                    ));
                }
                subject.push_str(&lines);
            }
        }
        out.push_str(&format!(" {} |\n", subject));
    }

    // The consolidated block --squash prints below the table: every file the
    // shown commits touched, counts summed, as its own small table so it reads
    // as a file list rather than a run of `<br>` lines in a cell.
    if squash {
        let consolidated = consolidate_file_stats(row_files);
        if !consolidated.is_empty() {
            out.push_str("\n## Consolidated files\n\n");
            out.push_str("| status | file | +added | -removed |\n|---|---|--:|--:|\n");
            for f in &consolidated {
                out.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    f.status,
                    md_cell(&f.path),
                    f.added.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string()),
                    f.removed.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string()),
                ));
            }
        }
    }

    std::fs::write(path, out).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    eprintln!("Wrote {} ({} commits)", path.display(), rows.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md_cells_cannot_invent_columns() {
        assert_eq!(md_cell("plain subject"), "plain subject");
        // A '|' would end the cell and shift every column after it -- the
        // markdown twin of the emoji-width bug, and a silent one.
        assert_eq!(md_cell("fix: a|pipe"), "fix: a\\|pipe");
        assert_eq!(md_cell("a|b|c"), "a\\|b\\|c");
        // The backslash goes first, or escaping the pipe would leave a stray
        // '\' that eats the escape we just added.
        assert_eq!(md_cell("back\\slash"), "back\\\\slash");
        assert_eq!(md_cell("both\\|here"), "both\\\\\\|here");
        // Emoji and CJK pass through: a file has no columns to misalign.
        assert_eq!(md_cell("🚀 ship 日本語"), "🚀 ship 日本語");
    }

    #[test]
    fn md_filename_is_stamped_and_sorts() {
        let name = md_filename();
        assert!(name.starts_with("commits_"), "{name}");
        assert!(name.ends_with(".md"), "{name}");
        // No path separator: it lands in the cwd, and cannot be read as a
        // directory that may not exist.
        assert!(!name.contains('/'), "{name}");
    }
}
