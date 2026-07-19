use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

use crate::cmd::commits::rows::{CommitRow, FileStat, Mark};
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
    picks: Option<&HashMap<String, String>>,
    cmd: &str,
) -> Result<(), String> {
    let mut out = String::new();
    out.push_str("# git-wt commits\n\n");
    out.push_str(&format!("- Command: `{}`\n", md_cell(cmd)));
    // The labels, not the column names: one worktree has no mark columns, and
    // the report still has to say whose log this is.
    out.push_str(&format!("- Worktrees: {}\n", labels.iter()
        .map(|n| format!("`{}`", md_cell(n)))
        .collect::<Vec<_>>()
        .join(", ")));
    out.push_str(&format!("- Commits: {}\n", rows.len()));
    // The glyphs are the whole content of the table; a reader who was not at
    // the terminal has nowhere else to learn them. '≈' is named only when the
    // table can carry it -- see the same rule in render_commits. No columns,
    // no glyphs: a one-worktree report has nothing to explain.
    if !names.is_empty() {
        if equiv.iter().any(|e| !e.is_empty()) {
            out.push_str("- Legend: `✓` has the commit · `≈` has the same patch under another sha · `·` has neither\n");
        } else {
            out.push_str("- Legend: `✓` has the commit · `·` has neither\n");
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
        for (set, eq) in sets.iter().zip(equiv) {
            out.push_str(&format!(" {} |", Mark::of(&row.sha, set, eq).glyph()));
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
        if let Some(file_stats) = row_files.get(i) {
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
