use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Stdio;

use crate::cmd::commits::args::{DateFmt, Order};
use crate::git::{git_cmd, git_stdout};
use crate::ui::{width_bound, CHECK, EQUIV, FINGERPRINT, MISS, TRAILER};

/// One table row: a commit, its short name, who wrote it when, and its subject.
#[derive(Clone)]
pub(crate) struct CommitRow {
    /// Full sha, for the set lookups; never printed.
    pub(crate) sha: String,
    pub(crate) short: String,
    pub(crate) text: String,
    pub(crate) author: String,
    /// Author date as printed: `2026-01-31`, or whatever `DateFmt` asked for.
    pub(crate) date: String,
    /// The same date as `YYYY-MM-DD`, which `--date` compares against.
    pub(crate) key: String,
    /// Author date as a Unix timestamp, compared numerically. The default
    /// view's floor is found on this, not on the day-granular `key`, so two
    /// commits on the same day still order against each other and the window
    /// does not swallow a whole day of shared history.
    pub(crate) stamp: String,
    /// Author email (`%ae`). Used for the author-fingerprint fallback mark;
    /// never printed.
    pub(crate) email: String,
    /// Author date in strict ISO-8601 form with timezone (`%aI`). Used for the
    /// author-fingerprint fallback mark; never printed.
    pub(crate) author_iso: String,
    /// The message below the subject, empty unless a filter asked for it.
    ///
    /// The table never prints a body of its own accord, so fetching one costs
    /// output nobody reads -- `--message` is the only thing that wants it, and
    /// only it pays.
    pub(crate) body: String,
}

/// One file touched by a commit, with status and line-count summary.
#[derive(Debug, Clone)]
pub(crate) struct FileStat {
    pub(crate) status: char,
    pub(crate) path: String,
    /// Added lines. `None` means the file is binary.
    pub(crate) added: Option<usize>,
    /// Removed lines. `None` means the file is binary.
    pub(crate) removed: Option<usize>,
}

/// One `--numstat -z` record: line counts plus the path the change lands on.
pub(crate) struct NumstatEntry {
    pub(crate) added: Option<usize>,
    pub(crate) removed: Option<usize>,
    /// The path as it exists after the change -- the new name for a rename.
    pub(crate) path: String,
    /// The pre-rename name, `None` when the file did not move.
    pub(crate) old_path: Option<String>,
}

/// Parse `--numstat -z` output.
///
/// The `-z` form exists because the plain one is ambiguous: git brace-compacts
/// a rename's common prefix, so `src/deep/old.rs -> src/deep/new.rs` prints as
/// `src/deep/{old.rs => new.rs}` -- a string that is neither path and cannot be
/// split back into two without reimplementing git's compaction. Under `-z` a
/// rename is instead three NUL-separated fields: the counts (with a trailing
/// tab and an empty third column), then old, then new.
pub(crate) fn parse_numstat_z(out: &str) -> Vec<NumstatEntry> {
    let count = |f: &str| (f != "-").then(|| f.parse::<usize>().ok()).flatten();

    let mut entries = Vec::new();
    let mut fields = out.split('\0');
    while let Some(record) = fields.next() {
        if record.is_empty() {
            continue;
        }
        let mut parts = record.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        // An empty third column marks a rename or copy: the two names follow as
        // their own NUL-separated fields.
        let (path, old_path) = if path.is_empty() {
            let (Some(old), Some(new)) = (fields.next(), fields.next()) else {
                continue;
            };
            (new.to_string(), Some(old.to_string()))
        } else {
            (path.to_string(), None)
        };
        entries.push(NumstatEntry {
            added: count(added),
            removed: count(removed),
            path,
            old_path,
        });
    }
    entries
}

/// The files a commit touched, with status and line counts.
///
/// Diffed against the first parent (or the empty tree for root commits), which
/// matches what a reader expects from a one-line log entry. Merge commits show
/// the first-parent diff only, not the combined merge.
/// A root commit's parent, for a diff that needs one: the empty tree. Its hash
/// is stable across git versions, so it costs nothing to spawn for.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// `sha`'s first parent, or the empty tree when it has none.
fn first_parent_or_empty(root: &Path, sha: &str) -> Result<String, String> {
    let parents = git_stdout(root, &["rev-list", "--parents", "-n", "1", sha])?
        .lines()
        .next()
        .map(|line| {
            line.split_whitespace()
                .skip(1)
                .map(String::from)
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();
    Ok(parents.into_iter().next().unwrap_or_else(|| EMPTY_TREE.to_string()))
}

pub(crate) fn commit_files(root: &Path, sha: &str) -> Result<Vec<FileStat>, String> {
    // First parent, or the empty tree for a root commit, which matches what a
    // reader expects from a one-line log entry: merge commits show the
    // first-parent diff only, not the combined merge.
    let base = first_parent_or_empty(root, sha)?;
    let base = base.as_str();

    let status_out = git_stdout(
        root,
        &["diff-tree", "-r", "--name-status", "-M", "-C", base, sha],
    )?;
    let numstat_out = git_stdout(
        root,
        &["diff-tree", "-r", "--numstat", "-z", "-M", "-C", base, sha],
    )?;

    // Map path -> status. Renames/copies keep the new path.
    let mut status_by_path: HashMap<String, char> = HashMap::new();
    for line in status_out.lines() {
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let Some(status_field) = parts.next() else {
            continue;
        };
        let Some(status) = status_field.chars().next() else {
            continue;
        };
        match status {
            'R' | 'C' => {
                // R100<tab>old<tab>new -- the old name is stepped over, since
                // the numstat side keys every rename on the new one.
                let (Some(_old), Some(new)) = (parts.next(), parts.next()) else {
                    continue;
                };
                status_by_path.insert(new.to_string(), status);
            }
            _ => {
                let Some(path) = parts.next() else {
                    continue;
                };
                status_by_path.insert(path.to_string(), status);
            }
        }
    }

    let mut stats: Vec<FileStat> = Vec::new();
    for entry in parse_numstat_z(&numstat_out) {
        // Status is keyed on the new name, which is what name-status reports;
        // the printed path keeps both names, since an 'R' on its own says a
        // file moved without saying from where.
        let status = status_by_path.get(&entry.path).copied().unwrap_or('M');
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

    sort_file_stats(&mut stats);
    Ok(stats)
}

/// `log`'s per-row `±` cell and the name(s) the pathspec actually matched on
/// that commit, both from one scoped `--numstat` -- not the commit-wide count
/// `--files` prints, which is every file the commit touched.
///
/// Counts are summed across every matching entry, so a path given as several
/// pathspecs (or a rename whose old and new name both match) still lands on
/// one number. `None` for either side when nothing matched at all: the commit
/// is a row only because some other ref's copy of the walk kept it (a merge
/// under `--diff-merges=first-parent` whose first parent did not touch the
/// path), and the cell prints `-` rather than a churn that never happened.
///
/// The names are the post-rename path of each matching entry -- what the
/// `path` column shows on the rows where it varies, which is exactly the
/// rows a rename or a multi-path pathspec makes different from the others.
pub(crate) fn path_row_stat(
    root: &Path,
    sha: &str,
    paths: &[String],
) -> Result<(Option<usize>, Option<usize>, Vec<String>), String> {
    let base = first_parent_or_empty(root, sha)?;
    let mut args = vec!["diff-tree", "-r", "--numstat", "-z", "-M", "-C", base.as_str(), sha, "--"];
    args.extend(paths.iter().map(String::as_str));
    let out = git_stdout(root, &args)?;
    let entries = parse_numstat_z(&out);
    if entries.is_empty() {
        return Ok((None, None, Vec::new()));
    }
    let sum = |a: Option<usize>, b: Option<usize>| match (a, b) {
        (Some(a), Some(b)) => Some(a + b),
        _ => None,
    };
    let mut added = Some(0);
    let mut removed = Some(0);
    let mut names = Vec::with_capacity(entries.len());
    for e in entries {
        added = sum(added, e.added);
        removed = sum(removed, e.removed);
        names.push(e.path);
    }
    Ok((added, removed, names))
}

/// Where a status letter sorts in a file block.
///
/// The life of a file, in order: it appears, it changes, it moves, it goes
/// away -- with untracked last, since it is not in the history at all. Sorting
/// on the letter itself would put 'D' between 'C' and 'M', and '?' ahead of
/// every letter: alphabetical order pretending to be meaning.
fn status_rank(c: char) -> u8 {
    match c {
        'A' => 0,
        'M' => 1,
        'R' => 2,
        'C' => 3,
        'T' => 4,
        'D' => 5,
        '?' => 7,
        _ => 6,
    }
}

/// Order a file block: status first, so every add sits with the adds and every
/// delete with the deletes, then path within each group. Without it a block
/// reads as one alphabetical run and the shape of the change is invisible.
pub(crate) fn sort_file_stats(stats: &mut [FileStat]) {
    stats.sort_by(|a, b| {
        status_rank(a.status)
            .cmp(&status_rank(b.status))
            .then_with(|| a.path.cmp(&b.path))
    });
}

/// Merge the per-commit file blocks into one: the block `--squash` prints in
/// place of them, once, below the whole table.
///
/// Keyed on path, so a file touched by several of the shown commits is one line
/// whose counts are the sum of theirs -- churn across the range, not a net diff:
/// a line added in one commit and removed in another shows as `+1 -1`, the work
/// that happened rather than the state left behind. This is the one reading that
/// holds for *any* row set the table can show -- a filtered, non-contiguous one
/// included -- where a true `base..tip` diff has no single base to measure from.
///
/// Binary is contagious: once a path has one uncountable change its sum cannot
/// be a number either, so a later `Some` cannot resurrect the count. The status
/// is the earliest in the file's lifecycle among the commits -- an `A` outranks
/// a later `M` -- since across a squash the file was, on balance, added.
pub(crate) fn consolidate_file_stats(row_files: &[Vec<FileStat>]) -> Vec<FileStat> {
    let mut by_path: HashMap<String, FileStat> = HashMap::new();
    for files in row_files {
        for f in files {
            let entry = by_path.entry(f.path.clone()).or_insert_with(|| FileStat {
                status: f.status,
                path: f.path.clone(),
                added: Some(0),
                removed: Some(0),
            });
            if status_rank(f.status) < status_rank(entry.status) {
                entry.status = f.status;
            }
            let sum = |a: Option<usize>, b: Option<usize>| match (a, b) {
                (Some(a), Some(b)) => Some(a + b),
                _ => None,
            };
            entry.added = sum(entry.added, f.added);
            entry.removed = sum(entry.removed, f.removed);
        }
    }
    let mut out: Vec<FileStat> = by_path.into_values().collect();
    sort_file_stats(&mut out);
    out
}

/// The lines of a file block: one tab-indented `status  path  +added  -removed`
/// per file, with the path and both counts padded to the block's own widths.
/// Binary files (and anything git gave no count for) print `-` in place of a
/// number. Shared by the commit table and `list --files` so the two blocks read
/// the same.
pub(crate) fn file_stat_lines(files: &[FileStat]) -> Vec<String> {
    let pathw = files.iter().map(|f| f.path.chars().count()).max().unwrap_or(0);
    let count = |n: Option<usize>, sign: char| {
        n.map(|n| format!("{sign}{n}")).unwrap_or_else(|| "-".to_string())
    };
    let added: Vec<String> = files.iter().map(|f| count(f.added, '+')).collect();
    let removed: Vec<String> = files.iter().map(|f| count(f.removed, '-')).collect();
    let addw = added.iter().map(|s| width_bound(s)).max().unwrap_or(1);
    let remw = removed.iter().map(|s| width_bound(s)).max().unwrap_or(1);
    files
        .iter()
        .zip(added.iter().zip(removed.iter()))
        .map(|(f, (a, r))| {
            format!(
                "\t{}  {:<pathw$}  {:>addw$}  {:>remw$}",
                f.status, f.path, a, r
            )
        })
        .collect()
}

/// Rows for the table: every commit reachable from any ref, newest first.
///
/// `%H` drives the set lookups and `%h %s` is what the row prints -- the same
/// text `git log --oneline` shows, which is the format the rows are meant to
/// read as. `%aN` respects .mailmap, so a contributor who has committed under
/// two names is one name here.
///
/// Author dates throughout, and `--author-date-order` to match the column the
/// table prints; commit dates answer "when did this land here", which is not
/// what a table about who-wrote-what is asking.
///
/// The order is ancestry first: git shows no parent before its children
/// whatever the timestamps claim, and the date only sequences commits that do
/// not descend from each other. So a commit authored before its own parent --
/// rebased, cherry-picked, or written on a machine with a bad clock -- reads as
/// out of order against its date column while the history stays true. That is
/// the right trade: a table whose rows contradicted the history would be
/// lying, where one whose dates jump is merely reporting a wrong clock.
pub(crate) fn commit_rows(
    root: &Path,
    refs: &[String],
    base: Option<&str>,
    limit: Option<usize>,
    order: Order,
    fmt: DateFmt,
    no_merges: bool,
    want_body: bool,
    // `log`'s pathspec, empty for every other caller. Appended as `-- <paths>`,
    // after `refs` and `--not base` -- git reads a pathspec only once it has
    // seen the `--`, so it has to come last whatever else the walk carries.
    paths: &[String],
    // `--follow`. Only sound with exactly one path -- git rejects more -- so
    // the caller decides, rather than this function inferring it from
    // `paths.len()` and silently doing the wrong thing for two.
    follow: bool,
) -> Result<Vec<CommitRow>, String> {
    let count;
    let date_arg = format!("--date=format:{}", fmt.spec());
    // A body holds newlines, so a record cannot end at one. `%x00` terminates
    // each record inside the format itself -- not the `-z` flag, whose meaning
    // in `git log` is bound up with the diff options -- and the body goes last,
    // where a tab of its own lands in the final field rather than inventing one.
    // Fingerprint fields (%ae, %aI) are not printed; they feed the author-date
    // fallback that detects cherry-picks whose patch text changed in conflict
    // resolution. They sit between the printed fields and the body so a body
    // with tabs cannot shift printed columns.
    let format = if want_body {
        "--format=%H%x09%aN%x09%ad%x09%as%x09%h%x09%at%x09%ae%x09%aI%x09%s%x09%b%x00"
    } else {
        "--format=%H%x09%aN%x09%ad%x09%as%x09%h%x09%at%x09%ae%x09%aI%x09%s%x00"
    };
    let mut args = vec!["log", order.flag(), &date_arg, format];
    if follow {
        args.push("--follow");
    }
    // Merge commits carry no work of their own; dropping them leaves the
    // commits someone actually wrote. The mark columns are unaffected: a
    // merge that is not a row is still in every rev-list that reaches it.
    if no_merges {
        args.push("--no-merges");
    } else if !paths.is_empty() {
        // `log -- <path>` prunes merges by default -- the same trap
        // `path_shas` documents below: a merge that brought the whole file
        // over lists nothing against the path and vanishes. Kept here (no
        // `--no-merges`, so `--merges` was asked for), `--full-history` stops
        // git from simplifying them away and `--diff-merges=first-parent`
        // makes the merge's listed diff the one its own row and `±` cell
        // agree with.
        args.push("--full-history");
        args.push("--diff-merges=first-parent");
    }
    if let Some(n) = limit {
        count = format!("-n{n}");
        args.push(&count);
    }
    args.extend(refs.iter().map(String::as_str));
    if let Some(b) = base {
        args.push("--not");
        args.push(b);
    }
    // The pathspec, last: git only reads paths after the `--`, so it has to
    // follow everything else the walk carries.
    if !paths.is_empty() {
        args.push("--");
        args.extend(paths.iter().map(String::as_str));
    }

    let out = git_stdout(root, &args)?;
    // Records are NUL-terminated, so the newline git puts between them belongs
    // to neither -- trim it off the front of each rather than into a field.
    Ok(out
        .split('\0')
        .map(|rec| rec.trim_start_matches('\n'))
        .filter(|rec| !rec.is_empty())
        .filter_map(|rec| {
            let mut f = rec.splitn(10, '\t');
            Some(CommitRow {
                sha: f.next()?.to_string(),
                author: f.next()?.to_string(),
                date: f.next()?.to_string(),
                key: f.next()?.to_string(),
                short: f.next()?.to_string(),
                stamp: f.next()?.to_string(),
                email: f.next()?.to_string(),
                author_iso: f.next()?.to_string(),
                text: f.next()?.to_string(),
                // Absent without `want_body`, and empty on a commit that has
                // no body -- the same thing to every caller.
                body: f.next().unwrap_or_default().to_string(),
            })
        })
        .collect())
}

/// How many matching body lines a row prints before the rest are counted.
///
/// Enough to show why the row was kept; past it a verbose commit would push the
/// rows around it off the screen, which is the table failing at its one job.
pub(crate) const BODY_HITS_MAX: usize = 3;

/// The body lines containing `term`, case-folded, and how many were left over.
///
/// Only the lines that matched: a body is prose, and printing all of it to
/// explain one word buries the word. Blank lines and the surrounding
/// indentation go, since neither carries the match.
pub(crate) fn body_hits(body: &str, term: &str) -> (Vec<String>, usize) {
    let hits: Vec<String> = body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && l.to_lowercase().contains(term))
        .map(String::from)
        .collect();
    let extra = hits.len().saturating_sub(BODY_HITS_MAX);
    (hits.into_iter().take(BODY_HITS_MAX).collect(), extra)
}

/// A path substring as a pathspec git will read literally.
///
/// The user typed a substring, so the glob characters in it are theirs, not
/// syntax: an escaped `[` matches a bracket instead of opening a class.
fn escape_pathspec(term: &str) -> String {
    let mut out = String::with_capacity(term.len());
    for c in term.chars() {
        if matches!(c, '\\' | '*' | '?' | '[') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The shas, among `refs`, of commits whose file block would name a path
/// containing `term`.
///
/// The block is the definition, not an approximation of it: `commit_files`
/// diffs a merge against its first parent, so this has to ask that same
/// question or `--filename` would keep rows whose block shows no match, and
/// drop rows whose block shows one.
///
/// Which rules out every plain path-limited walk. `git log -- <path>` prunes
/// merges, so a merge that brought a whole feature in lists none of its files
/// and vanishes -- the common case, and the one that made this worth fixing.
/// `--full-history` keeps merges, but keeps every merge, matching or not.
/// `--simplify-merges` prunes them again.
///
/// So: `--full-history` for the walk, and `--name-only` to check the answer.
/// The pathspec still narrows the walk to a candidate set -- `:(icase)` folds
/// case, and the bare `*term*` is the default (non-`:(glob)`) pathspec whose
/// `*` crosses directory separators, which a substring over a path must do.
/// `--diff-merges=first-parent` then makes a merge's listed files the same
/// files its block will show. A commit git kept but listed nothing for touched
/// no matching path, and is dropped here.
pub(crate) fn path_shas(root: &Path, refs: &[String], term: &str) -> Result<HashSet<String>, String> {
    let spec = format!(":(icase)*{}*", escape_pathspec(term));
    let mut args = vec![
        "log",
        "--format=%x00%H",
        "--name-only",
        "--full-history",
        "--diff-merges=first-parent",
    ];
    args.extend(refs.iter().map(String::as_str));
    args.push("--");
    args.push(&spec);
    // A git too old for --diff-merges (2.31) would fail the whole command, so
    // fall back to the walk without it: merges lose their file lists and drop
    // out, which is the old behavior rather than no answer at all.
    let out = match git_stdout(root, &args) {
        Ok(o) => o,
        Err(_) => {
            args.retain(|a| *a != "--diff-merges=first-parent");
            git_stdout(root, &args)?
        }
    };

    // Each record is a sha then the matching paths it touched, so a record with
    // no path is a commit the walk kept and the diff did not.
    Ok(out
        .split('\0')
        .filter_map(|rec| {
            let mut lines = rec.lines().map(str::trim).filter(|l| !l.is_empty());
            let sha = lines.next()?;
            lines.next().map(|_| sha.to_string())
        })
        .collect())
}

/// Resolve `r` to a commit, or say which flag could not find it.
pub(crate) fn commit_of(root: &Path, r: &str, flag: &str) -> Result<String, String> {
    git_stdout(root, &["rev-parse", "--verify", "--quiet", &format!("{r}^{{commit}}")])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{flag}: no commit '{r}'"))
}

/// The day `c` was authored, as `YYYY-MM-DD` -- the shape `--date` compares.
///
/// Author date, matching the column the table prints and the key the filters
/// test, so a bound named by a commit lands on the row that commit is.
pub(crate) fn commit_day(root: &Path, c: &str) -> Result<String, String> {
    let out = git_stdout(root, &["log", "-1", "--format=%ad", "--date=format:%Y-%m-%d", c])?;
    let day = out.trim().to_string();
    if day.is_empty() {
        return Err(format!("no author date for commit '{c}'"));
    }
    Ok(day)
}

/// The oldest commit on `target` that any source branch is missing.
///
/// For a merge request from target into each source, the missing commits are
/// `source..target` -- what target would bring. The oldest of all those sets
/// is where the relevant range of target begins.
pub(crate) fn divergent_set(root: &Path, target: &str, sources: &[String]) -> Result<HashSet<String>, String> {
    let mut out = HashSet::new();
    for src in sources {
        let range = format!("{src}..{target}");
        for sha in git_stdout(root, &["rev-list", &range])?.lines() {
            out.insert(sha.to_string());
        }
    }
    Ok(out)
}

/// Keep the first branch's log down to its earliest divergent commit: find the
/// oldest date among the divergent rows, then keep every row at least that new.
///
/// A date threshold, not a cut at a position, so the window is the same set of
/// commits whatever order produced the rows -- `--topo` regroups them, it does
/// not change which ones show. A positional cut would not: topo orders a shared
/// commit below the floor where date order keeps it above, so the two would
/// disagree on the row count. And unlike an ancestry base (`floor^@` excluded
/// from the walk) the threshold cannot leak a merge DAG's older side branches
/// past the floor -- they are older than it, so it drops them.
///
/// The floor is the oldest divergent timestamp, so every divergent row clears
/// the threshold by construction; the shared rows above it are the in-between
/// history. Timestamp, not day: a shared commit older than the floor but landed
/// on the same date stays out. Empty out means no row was divergent -- e.g.
/// `--no-merges` dropped the only commits the others were missing; the caller
/// reports it like an empty log.
pub(crate) fn window_to_divergent(rows: Vec<CommitRow>, divergent: &HashSet<String>) -> Vec<CommitRow> {
    let stamp = |r: &CommitRow| r.stamp.parse::<i64>().unwrap_or(i64::MIN);
    let Some(floor) = rows
        .iter()
        .filter(|r| divergent.contains(&r.sha))
        .map(stamp)
        .min()
    else {
        return Vec::new();
    };
    rows.into_iter().filter(|r| stamp(r) >= floor).collect()
}

/// Per column, the commits it has an *equivalent* of but not the commit itself:
/// same patch, different sha -- a cherry-pick, or a rebase's copy.
///
/// `git cherry <upstream> <head>` is exactly this question: it lists head's
/// commits since the fork and marks `-` on the ones upstream already carries
/// under another sha, comparing patch-ids rather than history. Doing it per
/// ordered pair costs N*(N-1) walks, each bounded by that pair's merge-base,
/// which is the same divergence the table is already showing.
///
/// A pair that cannot be compared (unrelated histories) is skipped rather than
/// fatal: the column simply keeps its `·`, which is what it said before.
pub(crate) fn equivalents(root: &Path, refs: &[String]) -> Vec<HashSet<String>> {
    let mut out = vec![HashSet::new(); refs.len()];
    for (i, upstream) in refs.iter().enumerate() {
        for head in refs.iter() {
            if head == upstream {
                continue;
            }
            let Ok(text) = git_stdout(root, &["cherry", upstream, head]) else {
                continue;
            };
            for line in text.lines() {
                if let Some(sha) = line.strip_prefix("- ") {
                    out[i].insert(sha.trim().to_string());
                }
            }
        }
    }
    out
}

/// Per commit, another sha carrying the same patch: the other half of an `≈`.
///
/// `git cherry` answers whether a copy exists, never which one it is, so the
/// naming is done here: patch-id every commit the refs do not share, group the
/// shas by patch, and a group of more than one is a patch someone picked. Each
/// sha in it names the first of its others -- a patch under three shas has no
/// single answer, and the first is at least a real one.
///
/// The walk is bounded at the refs' common merge-base, since a commit every ref
/// reaches by sha is not a pick to anyone; that is the same divergence `git
/// cherry` bounds each pair by, done once for all of them. Unrelated histories
/// have no such base and no shared work either, so the map comes back empty and
/// the marks keep their `≈` unexplained.
pub(crate) fn pick_ids(root: &Path, refs: &[String]) -> HashMap<String, String> {
    let mut base_args = vec!["merge-base", "--octopus"];
    base_args.extend(refs.iter().map(String::as_str));
    let base = match git_stdout(root, &base_args) {
        Ok(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return HashMap::new(),
    };

    // Merges carry no patch of their own, and `git cherry` skips them too.
    let mut args = vec!["rev-list", "--no-merges"];
    args.extend(refs.iter().map(String::as_str));
    args.push("--not");
    args.push(&base);
    let Some(pairs) = patch_ids(root, &args) else {
        return HashMap::new();
    };

    let mut by_patch: HashMap<String, Vec<String>> = HashMap::new();
    for (patch, sha) in pairs {
        by_patch.entry(patch).or_default().push(sha);
    }
    let mut out = HashMap::new();
    for shas in by_patch.values() {
        for sha in shas {
            if let Some(other) = shas.iter().find(|s| *s != sha) {
                out.insert(sha.clone(), other.clone());
            }
        }
    }
    out
}

/// `(patch-id, commit)` for every commit `rev_args` lists.
///
/// `rev-list | diff-tree --stdin -p | patch-id` is the pipeline `git cherry`
/// runs internally, and the reason for the pipe rather than three `output()`
/// calls: the patch text between the stages is the whole diff of the range,
/// which is worth streaming rather than holding.
///
/// A stage that cannot start, or a git too old for `--stable`, gives `None`:
/// the pick column goes blank, which is what it says for an unpicked commit
/// anyway. Root commits produce no patch and are simply absent.
pub(crate) fn patch_ids(root: &Path, rev_args: &[&str]) -> Option<Vec<(String, String)>> {
    let mut rev = git_cmd(root, rev_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut diff = git_cmd(root, &["diff-tree", "--stdin", "-p"])
        .stdin(Stdio::from(rev.stdout.take()?))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let out = git_cmd(root, &["patch-id", "--stable"])
        .stdin(Stdio::from(diff.stdout.take()?))
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let _ = rev.wait();
    let _ = diff.wait();
    if !out.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| {
                let (patch, sha) = l.split_once(' ')?;
                Some((patch.to_string(), sha.trim().to_string()))
            })
            .collect(),
    )
}

/// The source sha named by a `git cherry-pick -x` trailer, if any.
///
/// Git appends `(cherry picked from commit <sha>)` to the commit message.
/// The trailer is usually at the end of the body, so the last occurrence is
/// the one that matters if the message contains the marker more than once.
pub(crate) fn cherry_trailer_source(body: &str) -> Option<&str> {
    let marker = "(cherry picked from commit ";
    let idx = body.rfind(marker)?;
    let start = idx + marker.len();
    let end = body[start..].find(')')? + start;
    Some(body[start..end].trim())
}

/// Per column, the row shas that a `-x` trailer on that branch names as
/// cherry-pick sources.
///
/// Like `equivalents`, this is bounded at the refs' common merge-base: a picked
/// commit in shared history is already reachable by ancestry, so `Has` answers
/// for it and there is no pick to report.
pub(crate) fn trailer_sets(root: &Path, refs: &[String]) -> Vec<HashSet<String>> {
    let mut out = vec![HashSet::new(); refs.len()];
    let base = match merge_base(root, refs) {
        Some(b) => b,
        None => return out,
    };
    for (i, r) in refs.iter().enumerate() {
        // The full message is needed, but merges carry no cherry-pick trailer
        // of their own (they have no `-x` body in the usual sense).
        let format = "--format=%H%x09%b%x00";
        let Ok(text) = git_stdout(
            root,
            &["log", "--no-merges", format, r, "--not", &base],
        ) else {
            continue;
        };
        for rec in text.split('\0') {
            let rec = rec.trim_start_matches('\n');
            if rec.is_empty() {
                continue;
            }
            let mut f = rec.splitn(2, '\t');
            let _sha = f.next();
            let body = f.next().unwrap_or("");
            if let Some(src) = cherry_trailer_source(body) {
                out[i].insert(src.to_string());
            }
        }
    }
    out
}

/// Shared octopus merge-base for a set of refs, if there is one.
fn merge_base(root: &Path, refs: &[String]) -> Option<String> {
    if refs.len() < 2 {
        return None;
    }
    let mut args = vec!["merge-base", "--octopus"];
    args.extend(refs.iter().map(String::as_str));
    match git_stdout(root, &args) {
        Ok(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        _ => None,
    }
}

/// Author fingerprint: the three fields that `git cherry-pick` preserves
/// exactly, even through conflict resolution.
type AuthorFingerprint = (String, String, String);

/// Every commit above the octopus merge-base in each ref, with its fingerprint.
fn author_fingerprints(
    root: &Path,
    refs: &[String],
) -> Vec<Vec<(String, AuthorFingerprint)>> {
    let mut out = vec![Vec::new(); refs.len()];
    let base = match merge_base(root, refs) {
        Some(b) => b,
        None => return out,
    };
    let format = "--format=%H%x09%ae%x09%aI%x09%s%x00";
    for (i, r) in refs.iter().enumerate() {
        let Ok(text) = git_stdout(
            root,
            &["log", "--no-merges", format, r, "--not", &base],
        ) else {
            continue;
        };
        out[i] = text
            .split('\0')
            .map(|rec| rec.trim_start_matches('\n'))
            .filter(|rec| !rec.is_empty())
            .filter_map(|rec| {
                let mut f = rec.splitn(4, '\t');
                let sha = f.next()?.to_string();
                let email = f.next()?.to_string();
                let date = f.next()?.to_string();
                let subject = f.next()?.to_string();
                Some((sha, (email, date, subject)))
            })
            .collect();
    }
    out
}

/// Per column, the row shas whose author fingerprint appears on that branch
/// under a different commit sha.
///
/// The match key is author email + author date (ISO-8601 with timezone) +
/// subject. Cherry-pick preserves all three exactly, even when conflict edits
/// change the patch text enough to defeat `git patch-id`. Same author, same
/// second, same subject, different commit = near-zero false-positive rate.
pub(crate) fn author_match_sets(
    root: &Path,
    refs: &[String],
    rows: &[CommitRow],
) -> Vec<HashSet<String>> {
    let mut out = vec![HashSet::new(); refs.len()];
    let per_ref = author_fingerprints(root, refs);
    let mut by_key: HashMap<AuthorFingerprint, Vec<(String, usize)>> = HashMap::new();
    for (i, list) in per_ref.iter().enumerate() {
        for (sha, key) in list {
            by_key.entry(key.clone()).or_default().push((sha.clone(), i));
        }
    }
    for row in rows {
        let key = (row.email.clone(), row.author_iso.clone(), row.text.clone());
        if let Some(matches) = by_key.get(&key) {
            for (sha, ref_idx) in matches {
                if *sha != row.sha {
                    out[*ref_idx].insert(row.sha.clone());
                }
            }
        }
    }
    out
}

/// Every commit sha reachable from `r`, cut at `base` the same way the rows are.
pub(crate) fn ref_shas(root: &Path, r: &str, base: Option<&str>) -> Result<HashSet<String>, String> {
    let mut args = vec!["rev-list", r];
    if let Some(b) = base {
        args.push("--not");
        args.push(b);
    }
    Ok(git_stdout(root, &args)?
        .lines()
        .map(str::to_string)
        .collect())
}

/// What a branch has of a given commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mark {
    /// The commit itself.
    Has,
    /// The same patch under a different sha.
    Equivalent,
    /// A `-x` trailer on another branch names this commit as its source.
    Trailer,
    /// Another branch has a commit with the same author fingerprint.
    AuthorMatch,
    /// Neither.
    Missing,
}

impl Mark {
    pub(crate) fn of(
        sha: &str,
        has: &HashSet<String>,
        equiv: &HashSet<String>,
        trailer: &HashSet<String>,
        author_match: &HashSet<String>,
    ) -> Mark {
        // Containment wins: a branch that has the commit has it, whatever a
        // patch comparison would also say about an equivalent elsewhere.
        if has.contains(sha) {
            Mark::Has
        } else if equiv.contains(sha) {
            Mark::Equivalent
        } else if trailer.contains(sha) {
            Mark::Trailer
        } else if author_match.contains(sha) {
            Mark::AuthorMatch
        } else {
            Mark::Missing
        }
    }

    pub(crate) fn glyph(self) -> &'static str {
        match self {
            Mark::Has => CHECK,
            Mark::Equivalent => EQUIV,
            Mark::Trailer => TRAILER,
            Mark::AuthorMatch => FINGERPRINT,
            Mark::Missing => MISS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::commits::args::{parse_date_filter, DateFmt};

    /// The reason `-z` is used at all: the plain form brace-compacts a rename
    /// into a string that is neither path, so a nested rename could never be
    /// matched back to its name-status entry.
    #[test]
    fn numstat_z_reads_renames_as_separate_old_and_new_paths() {
        let out = "0\t0\t\0src/deep/old.rs\0src/deep/new.rs\0\
                   3\t1\tplain.rs\0\
                   -\t-\tlogo.png\0";
        let got = parse_numstat_z(out);

        assert_eq!(got.len(), 3);
        // The nested rename: new path stands alone, uncompacted.
        assert_eq!(got[0].path, "src/deep/new.rs");
        assert_eq!(got[0].old_path.as_deref(), Some("src/deep/old.rs"));
        assert_eq!((got[0].added, got[0].removed), (Some(0), Some(0)));
        // An ordinary edit carries no old name.
        assert_eq!(got[1].path, "plain.rs");
        assert_eq!(got[1].old_path, None);
        assert_eq!((got[1].added, got[1].removed), (Some(3), Some(1)));
        // Binary: git spells the counts "-", which is not zero.
        assert_eq!(got[2].path, "logo.png");
        assert_eq!((got[2].added, got[2].removed), (None, None));
    }

    /// The same rename, through real git rather than a literal: the unit test
    /// above pins the parse, this pins the format it is parsing. A hand-written
    /// string cannot notice if git ever changes what `--numstat -z` emits.
    #[test]
    fn a_rename_inside_a_directory_is_reported_as_a_move() {
        let tmp = std::env::temp_dir().join(format!("git-wt-rename-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str]) {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_AUTHOR_DATE", "2026-07-17T10:00:00")
                .env("GIT_COMMITTER_DATE", "2026-07-17T10:00:00")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?} failed: {out:?}");
        }

        std::fs::create_dir_all(tmp.join("src/deep")).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"]);
        git(&tmp, &["config", "user.email", "t@test"]);
        git(&tmp, &["config", "user.name", "t"]);
        // Enough lines that git scores the move as a rename, not add+delete.
        std::fs::write(tmp.join("src/deep/old.rs"), "a\nb\nc\nd\ne\n").unwrap();
        std::fs::write(tmp.join("top.rs"), "top\n").unwrap();
        git(&tmp, &["add", "-A"]);
        git(&tmp, &["commit", "--quiet", "-m", "one"]);
        // One rename nested in a directory, one at the root. Only the nested
        // one gets brace-compacted by plain --numstat, and it was the one that
        // used to fall through to 'M'.
        git(&tmp, &["mv", "src/deep/old.rs", "src/deep/new.rs"]);
        git(&tmp, &["mv", "top.rs", "bottom.rs"]);
        git(&tmp, &["commit", "--quiet", "-m", "two"]);

        let head = git_stdout(&tmp, &["rev-parse", "HEAD"]).unwrap();
        let stats = commit_files(&tmp, head.trim()).unwrap();

        assert_eq!(stats.len(), 2, "{stats:?}");
        // Both are moves, and both print where they came from.
        assert!(stats.iter().all(|s| s.status == 'R'), "{stats:?}");
        let paths: Vec<&str> = stats.iter().map(|s| s.path.as_str()).collect();
        assert!(
            paths.contains(&"src/deep/old.rs => src/deep/new.rs"),
            "{paths:?}"
        );
        assert!(paths.contains(&"top.rs => bottom.rs"), "{paths:?}");

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn body_hits_keeps_only_the_lines_that_matched() {
        let body = "fixes ISSUE-42 in the parser\n\nunrelated paragraph\n  ISSUE-42 again  \n";
        let (lines, extra) = body_hits(body, "issue-42");
        // Case-folded, trimmed, and blanks dropped -- none of which carries a
        // match, and all of which would pad the block.
        assert_eq!(lines, ["fixes ISSUE-42 in the parser", "ISSUE-42 again"]);
        assert_eq!(extra, 0);
        // A line the term is not on stays out, however near it sits.
        assert!(!lines.iter().any(|l| l.contains("unrelated")));
        // No match at all is an empty block, not a blank one.
        assert_eq!(body_hits(body, "zzz"), (Vec::new(), 0));
        assert_eq!(body_hits("", "x"), (Vec::new(), 0));
    }

    #[test]
    fn body_hits_caps_a_verbose_commit() {
        // Past the cap the rows around this one would be pushed off the screen,
        // so the rest are counted rather than printed.
        let body = (1..=7).map(|i| format!("line {i} has term")).collect::<Vec<_>>().join("\n");
        let (lines, extra) = body_hits(&body, "term");
        assert_eq!(lines.len(), BODY_HITS_MAX);
        assert_eq!(extra, 7 - BODY_HITS_MAX);
        assert_eq!(lines[0], "line 1 has term");
    }

    #[test]
    fn a_pathspec_term_is_a_substring_not_a_glob() {
        // The user typed a substring, so the glob characters in it are theirs.
        assert_eq!(escape_pathspec("render.rs"), "render.rs");
        assert_eq!(escape_pathspec("a[0]"), "a\\[0]");
        assert_eq!(escape_pathspec("*.rs"), "\\*.rs");
        assert_eq!(escape_pathspec("a?b"), "a\\?b");
        assert_eq!(escape_pathspec("a\\b"), "a\\\\b");
    }

    #[test]
    fn a_block_groups_by_status_then_path() {
        let mut files: Vec<FileStat> = ["M docs/base.txt", "A src/cli.rs", "M src/main.rs",
                                        "A src/ui.rs", "D old.rs", "? new.txt", "R moved.rs"]
            .iter()
            .map(|s| {
                let (st, path) = s.split_once(' ').unwrap();
                FileStat {
                    status: st.chars().next().unwrap(),
                    path: path.into(),
                    added: Some(1),
                    removed: Some(0),
                }
            })
            .collect();
        sort_file_stats(&mut files);
        let got: Vec<String> = files.iter().map(|f| format!("{} {}", f.status, f.path)).collect();
        // Adds, then modifications, then the move, then the delete, with the
        // untracked file last -- and alphabetical inside each group. Sorting on
        // the letter alone would lead with '?' and bury 'D' among the letters.
        assert_eq!(
            got,
            vec![
                "A src/cli.rs",
                "A src/ui.rs",
                "M docs/base.txt",
                "M src/main.rs",
                "R moved.rs",
                "D old.rs",
                "? new.txt",
            ]
        );
    }

    #[test]
    fn file_stat_lines_pads_paths_and_counts() {
        let files = vec![
            FileStat { status: 'M', path: "a.rs".into(), added: Some(4), removed: Some(1) },
            FileStat { status: 'A', path: "long/name.rs".into(), added: Some(11), removed: Some(0) },
            // Binary: no counts to print on either side.
            FileStat { status: 'M', path: "logo.png".into(), added: None, removed: None },
        ];
        assert_eq!(
            file_stat_lines(&files),
            vec![
                "\tM  a.rs           +4  -1".to_string(),
                "\tA  long/name.rs  +11  -0".to_string(),
                "\tM  logo.png        -   -".to_string(),
            ]
        );
    }

    #[test]
    fn consolidate_sums_churn_and_takes_the_earliest_status() {
        let fs = |st: char, p: &str, a: Option<usize>, r: Option<usize>| FileStat {
            status: st,
            path: p.into(),
            added: a,
            removed: r,
        };
        // Newest-first, as the rows are: a.rs added then twice modified, b.rs
        // touched once, logo.png binary in one of two commits.
        let row_files = vec![
            vec![fs('M', "a.rs", Some(2), Some(1)), fs('M', "logo.png", None, None)],
            vec![fs('M', "a.rs", Some(3), Some(0)), fs('A', "b.rs", Some(9), Some(0))],
            vec![fs('A', "a.rs", Some(5), Some(0)), fs('M', "logo.png", Some(4), Some(4))],
        ];
        let got = consolidate_file_stats(&row_files);

        // Grouped by status then path: a.rs and b.rs are both 'A' (a.rs's add
        // outranks its later modifies), logo.png stays 'M'.
        let by = |p: &str| got.iter().find(|f| f.path == p).unwrap();
        // Churn summed, not netted: every commit's count adds in.
        assert_eq!((by("a.rs").added, by("a.rs").removed), (Some(10), Some(1)));
        assert_eq!(by("a.rs").status, 'A', "the add outranks the later modifies");
        assert_eq!((by("b.rs").added, by("b.rs").removed), (Some(9), Some(0)));
        // Binary is contagious: one uncountable change makes the sum uncountable
        // however many numbers the other commits gave.
        assert_eq!((by("logo.png").added, by("logo.png").removed), (None, None));

        // One line per path, and status-then-path order like every file block.
        assert_eq!(got.len(), 3);
        let order: Vec<&str> = got.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(order, ["a.rs", "b.rs", "logo.png"], "A's before the M");

        // Nothing to consolidate is an empty block, not a panic.
        assert!(consolidate_file_stats(&[]).is_empty());
        assert!(consolidate_file_stats(&[vec![]]).is_empty());
    }

    /// The default spelling: what `commits` prints without a format flag.
    const ISO: DateFmt = DateFmt { human: false, time: false };

    #[test]
    fn commit_rows_stop_at_the_common_ancestor() {
        let tmp = std::env::temp_dir().join(format!("git-wt-commits-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str]) {
            let mut c = std::process::Command::new("git");
            c.current_dir(dir).args(args);
            let out = c.output().unwrap();
            assert!(out.status.success(), "git {:?} failed: {:?}", args, out);
        }
        // A fixed author date: the date column's format is part of the
        // contract, and "now" cannot be asserted against.
        fn commit(dir: &std::path::Path, name: &str, when: &str) {
            std::fs::write(dir.join(format!("{name}.txt")), name).unwrap();
            git(dir, &["add", "-A"]);
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(["commit", "--quiet", "-m", name])
                .env("GIT_AUTHOR_DATE", when)
                .env("GIT_COMMITTER_DATE", when)
                .output()
                .unwrap();
            assert!(out.status.success(), "commit {name} failed: {out:?}");
        }

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"]);
        git(&tmp, &["config", "user.email", "t@test"]);
        git(&tmp, &["config", "user.name", "t"]);
        commit(&tmp, "shared", "2025-12-20T10:00:00");
        git(&tmp, &["branch", "feat"]);
        git(&tmp, &["checkout", "--quiet", "feat"]);
        commit(&tmp, "on-feat", "2026-09-15T10:00:00");
        git(&tmp, &["checkout", "--quiet", "main"]);
        commit(&tmp, "on-main", "2026-01-01T10:00:00");

        let refs = vec!["main".to_string(), "feat".to_string()];

        // --all keeps the old default: the first ref's log, whole -- exactly
        // 'git log --oneline main', shared history included. feat's own commit
        // is not a row, it is a missing mark on feat's column.
        let all_rows = commit_rows(&tmp, &refs[..1], None, None, Order::Date, ISO, false, false, &[], false).unwrap();
        let subjects: Vec<&str> = all_rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(all_rows.len(), 2, "{subjects:?}");
        assert!(subjects.iter().any(|t| t.ends_with("on-main")), "{subjects:?}");
        assert!(subjects.iter().any(|t| t.ends_with("shared")), "{subjects:?}");
        assert!(!subjects.iter().any(|t| t.ends_with("on-feat")), "{subjects:?}");
        // Each field is parsed off its own tab, so nothing can shift into the
        // wrong column. The date is the format the table promises, single-digit
        // days unpadded.
        assert!(all_rows.iter().all(|r| r.author == "t"), "{:?}", all_rows[0].author);
        assert!(all_rows.iter().all(|r| !r.short.is_empty()));
        // ISO by default: the shape --from-date takes, so a date read off the
        // table pastes straight back into a filter.
        let dates: Vec<&str> = all_rows.iter().map(|r| r.date.as_str()).collect();
        assert_eq!(dates, ["2026-01-01", "2025-12-20"], "{dates:?}");

        // The default slice: rows are commits in main that feat is missing,
        // from the oldest such commit up to main's tip. Here feat forked at
        // the root, so only 'on-main' is missing from feat; 'shared' is
        // older than the missing commit and is therefore excluded.
        let divergent = divergent_set(&tmp, &refs[0], &refs[1..]).unwrap();
        assert!(!divergent.is_empty(), "feat must be missing something from main");
        let full = commit_rows(&tmp, &refs[..1], None, None, Order::Date, ISO, false, false, &[], false).unwrap();
        let rows = window_to_divergent(full, &divergent);
        let subjects: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(rows.len(), 1, "{subjects:?}");
        assert!(subjects.iter().any(|t| t.ends_with("on-main")), "{subjects:?}");
        assert!(!subjects.iter().any(|t| t.ends_with("shared")), "{subjects:?}");
        assert!(!subjects.iter().any(|t| t.ends_with("on-feat")), "{subjects:?}");

        // The columns answer for a branch's entire history. The only row is
        // 'on-main'; feat does not have it.
        let feat_all = ref_shas(&tmp, "feat", None).unwrap();
        for row in &rows {
            assert!(!feat_all.contains(&row.sha), "{}", row.text);
        }

        // The divergent set is main's commits feat is missing: here just
        // 'on-main', and it is the floor the slice stops at.
        let on_main_row = rows.iter().find(|r| r.text.ends_with("on-main")).unwrap();
        assert!(divergent.contains(&on_main_row.sha));
        assert_eq!(divergent.len(), 1);


        // --union: every ref contributes rows, so feat's commit is one too, and
        // the shared commit is checked on both.
        let union = commit_rows(&tmp, &refs, None, None, Order::Date, ISO, false, false, &[], false).unwrap();
        let subjects: Vec<&str> = union.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(union.len(), 3, "{subjects:?}");
        // --author-date-order, so the rows descend by the date they print.
        assert!(union[0].text.ends_with("on-feat"), "{:?}", union[0].text);
        let shared = union.iter().find(|r| r.text.ends_with("shared")).unwrap();
        assert!(ref_shas(&tmp, "main", None).unwrap().contains(&shared.sha));
        assert!(feat_all.contains(&shared.sha));

        // -n caps the rows, newest first.
        let capped = commit_rows(&tmp, &refs, None, Some(1), Order::Date, ISO, false, false, &[], false).unwrap();
        assert_eq!(capped.len(), 1);

        // A commit names a day for --commit-since/--commit-until, so the
        // bound is that commit's own author date -- the same key the rows
        // print and the date filters test.
        let on_main = rows.iter().find(|r| r.text.ends_with("on-main")).unwrap();
        assert_eq!(commit_day(&tmp, &on_main.sha).unwrap(), on_main.key);

        // A commit that does not resolve is named by the flag that wanted it.
        let err = commit_of(&tmp, "no-such-commit", "--commit-since").unwrap_err();
        assert_eq!(err, "--commit-since: no commit 'no-such-commit'");

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn commits_default_slice_uses_earliest_divergence() {
        let tmp = std::env::temp_dir().join(format!(
            "git-wt-commits-slice-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str], when: &str) {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_AUTHOR_DATE", when)
                .env("GIT_COMMITTER_DATE", when)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {:?} failed: {:?}", args, out);
        }
        let commit = |dir: &std::path::Path, name: &str, when: &str| {
            std::fs::write(dir.join(format!("{name}.txt")), name).unwrap();
            git(dir, &["add", "-A"], when);
            git(dir, &["commit", "--quiet", "-m", name], when);
        };

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"], "");
        git(&tmp, &["config", "user.email", "t@test"], "");
        git(&tmp, &["config", "user.name", "t"], "");
        commit(&tmp, "A", "2025-12-20T10:00:00");
        commit(&tmp, "B", "2025-12-21T10:00:00");
        git(&tmp, &["branch", "feat"], "");
        git(&tmp, &["branch", "fix"], "");
        git(&tmp, &["checkout", "--quiet", "feat"], "");
        commit(&tmp, "on-feat", "2025-12-22T10:00:00");
        git(&tmp, &["checkout", "--quiet", "fix"], "");
        commit(&tmp, "on-fix", "2025-12-23T10:00:00");
        git(&tmp, &["checkout", "--quiet", "main"], "");
        commit(&tmp, "C", "2025-12-24T10:00:00");
        commit(&tmp, "D", "2025-12-25T10:00:00");

        let refs = ["main".to_string(),
            "feat".to_string(),
            "fix".to_string()];

        // feat and fix both forked at B, so the commits main has that either of
        // them misses are C and D; the earliest is C. The default slice should
        // include C and D (commits strictly after B), but not B or A.
        let divergent = divergent_set(&tmp, &refs[0], &refs[1..]).unwrap();
        assert!(divergent.contains(sha_by_subject(&tmp, "main", "C").as_str()));
        assert!(divergent.contains(sha_by_subject(&tmp, "main", "D").as_str()));
        assert_eq!(divergent.len(), 2);

        let full = commit_rows(
            &tmp, &refs[..1], None, None, Order::Date, ISO, false, false, &[], false,
        ).unwrap();
        let rows = window_to_divergent(full, &divergent);
        let subjects: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(subjects, ["D", "C"], "{subjects:?}");

        // The full first-branch log with --all.
        let all_rows = commit_rows(
            &tmp, &refs[..1], None, None, Order::Date, ISO, false, false, &[], false,
        ).unwrap();
        let all_subjects: Vec<&str> = all_rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(all_subjects, ["D", "C", "B", "A"], "{all_subjects:?}");

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn commits_window_does_not_leak_merge_side_branches() {
        // The bug positional truncation fixes: on a merge DAG, the floor is a
        // commit on a side branch merged into the target late. An ancestry base
        // (`floor^@` excluded) only prunes the floor's own parent line, so a
        // shared commit on the *other* merge parent -- older than the floor,
        // and one the source branch also has -- leaks in as a row below it.
        let tmp = std::env::temp_dir().join(format!(
            "git-wt-window-leak-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str], when: &str) {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_AUTHOR_DATE", when)
                .env("GIT_COMMITTER_DATE", when)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?} failed: {out:?}");
        }
        let commit = |dir: &std::path::Path, name: &str, when: &str| {
            std::fs::write(dir.join(format!("{name}.txt")), name).unwrap();
            git(dir, &["add", "-A"], when);
            git(dir, &["commit", "--quiet", "-m", name], when);
        };

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"], "");
        git(&tmp, &["config", "user.email", "t@test"], "");
        git(&tmp, &["config", "user.name", "t"], "");
        // A -> MAINLINE on main; feat forks at MAINLINE (so feat has both).
        commit(&tmp, "A", "2025-12-20T10:00:00");
        git(&tmp, &["branch", "other"], "");
        commit(&tmp, "MAINLINE", "2025-12-21T10:00:00");
        git(&tmp, &["branch", "feat"], "");
        // A side branch off A, then merged back into main as the FLOOR merge.
        // FLOOR's first parent is MAINLINE, its second is SIDE (parent A) --
        // MAINLINE is not an ancestor of SIDE.
        git(&tmp, &["checkout", "--quiet", "other"], "");
        commit(&tmp, "SIDE", "2025-12-22T10:00:00");
        git(&tmp, &["checkout", "--quiet", "main"], "");
        git(
            &tmp,
            &["merge", "--no-ff", "--quiet", "-m", "FLOOR", "other"],
            "2025-12-23T10:00:00",
        );

        let refs = ["main".to_string(), "feat".to_string()];

        // main has SIDE and FLOOR that feat is missing; MAINLINE is shared.
        let divergent = divergent_set(&tmp, &refs[0], &refs[1..]).unwrap();
        assert!(divergent.contains(sha_by_subject(&tmp, "main", "SIDE").as_str()));
        assert!(divergent.contains(sha_by_subject(&tmp, "main", "FLOOR").as_str()));
        assert_eq!(divergent.len(), 2, "MAINLINE is shared, not divergent");

        let full = commit_rows(&tmp, &refs[..1], None, None, Order::Date, ISO, false, false, &[], false).unwrap();
        let rows = window_to_divergent(full.clone(), &divergent);
        let subjects: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        // FLOOR down to SIDE, and nothing below: MAINLINE must not leak in even
        // though it is reachable from main outside SIDE's ancestry.
        assert_eq!(subjects, ["FLOOR", "SIDE"], "{subjects:?}");

        // The window is a set, not a slice of one ordering: feeding the rows in
        // any order -- as --topo would -- keeps the same commits, so --topo can
        // only regroup the table, never change its row count.
        let mut scrambled = full;
        scrambled.reverse();
        let sorted_shas = |v: Vec<CommitRow>| {
            let mut s: Vec<String> = v.into_iter().map(|r| r.sha).collect();
            s.sort();
            s
        };
        assert_eq!(
            sorted_shas(window_to_divergent(scrambled, &divergent)),
            sorted_shas(rows),
            "window must be order-independent",
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    fn sha_by_subject(
        root: &std::path::Path,
        branch: &str,
        subject: &str,
    ) -> String {
        let rows = commit_rows(
            root,
            &[branch.to_string()],
            None,
            None,
            Order::Date,
            ISO,
            false,
            false,
            &[],
            false,
        )
        .unwrap();
        rows.iter()
            .find(|r| r.text == subject)
            .map(|r| r.sha.clone())
            .unwrap_or_else(|| panic!("no row for subject '{}'", subject))
    }

    #[test]
    fn topo_groups_the_branches_that_date_order_interleaves() {
        let tmp = std::env::temp_dir().join(format!("git-wt-topo-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str], when: &str) {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_AUTHOR_DATE", when)
                .env("GIT_COMMITTER_DATE", when)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?} failed: {out:?}");
        }
        let commit = |name: &str, when: &str| {
            git(&tmp, &["commit", "--quiet", "--allow-empty", "-m", name], when);
        };

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"], "");
        git(&tmp, &["config", "user.email", "t@test"], "");
        git(&tmp, &["config", "user.name", "t"], "");

        // Two branches whose commits alternate in time: main in the even
        // months, feat in the odd ones. The orders disagree maximally here.
        commit("base", "2026-01-01T10:00:00");
        git(&tmp, &["branch", "feat"], "");
        commit("main-02", "2026-02-01T10:00:00");
        commit("main-04", "2026-04-01T10:00:00");
        git(&tmp, &["checkout", "--quiet", "feat"], "");
        commit("feat-03", "2026-03-01T10:00:00");
        commit("feat-05", "2026-05-01T10:00:00");

        let refs = vec!["main".to_string(), "feat".to_string()];
        let subjects = |o: Order| -> Vec<String> {
            commit_rows(&tmp, &refs, None, None, o, ISO, false, false, &[], false)
                .unwrap()
                .iter()
                .map(|r| r.text.clone())
                .collect()
        };

        // By date: strictly newest-first, so the branches interleave and a
        // row's neighbors are the commits written around the same time.
        assert_eq!(
            subjects(Order::Date),
            ["feat-05", "main-04", "feat-03", "main-02", "base"]
        );
        // By topology: each branch's line stays in one block, so the table
        // reads as one branch's story then the other's.
        assert_eq!(
            subjects(Order::Topo),
            ["feat-05", "feat-03", "main-04", "main-02", "base"]
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn same_day_rows_are_ordered_by_time_of_day() {
        let tmp = std::env::temp_dir().join(format!("git-wt-sameday-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str], when: &str) {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_AUTHOR_DATE", when)
                .env("GIT_COMMITTER_DATE", when)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?} failed: {out:?}");
        }
        let commit = |name: &str, when: &str| {
            git(&tmp, &["commit", "--quiet", "--allow-empty", "-m", name], when);
        };

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"], "");
        git(&tmp, &["config", "user.email", "t@test"], "");
        git(&tmp, &["config", "user.name", "t"], "");

        // Two branches, four commits, one calendar day. The column prints the
        // day, so every row looks tied; only the time can order them.
        commit("base", "2026-07-01T10:00:00");
        git(&tmp, &["branch", "feat"], "");
        commit("main-09h", "2026-07-17T09:00:00");
        commit("main-17h", "2026-07-17T17:00:00");
        git(&tmp, &["checkout", "--quiet", "feat"], "");
        commit("feat-13h", "2026-07-17T13:00:00");
        commit("feat-21h", "2026-07-17T21:00:00");

        let refs = vec!["main".to_string(), "feat".to_string()];
        let rows = commit_rows(&tmp, &refs, None, None, Order::Date, ISO, false, false, &[], false).unwrap();
        let seen: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();

        // Ordering reads the full timestamp, not the printed day: the branches
        // interleave by hour even though all four rows show '2026-07-17'.
        assert_eq!(seen, ["feat-21h", "main-17h", "feat-13h", "main-09h", "base"]);
        assert!(rows[..4].iter().all(|r| r.date == "2026-07-17"));

        // The filter key is the day, so one --date takes every hour in it.
        let day = parse_date_filter("2026-07-17").unwrap();
        assert_eq!(rows.iter().filter(|r| day.admits(&r.key)).count(), 4);

        // --time is what tells those four rows apart, 24-hour so they sort
        // the way they read; the day stays ISO beside it.
        let timed = DateFmt { human: false, time: true };
        let rows = commit_rows(&tmp, &refs, None, None, Order::Date, timed, false, false, &[], false).unwrap();
        let stamps: Vec<&str> = rows[..4].iter().map(|r| r.date.as_str()).collect();
        assert_eq!(
            stamps,
            [
                "2026-07-17 21:00:00",
                "2026-07-17 17:00:00",
                "2026-07-17 13:00:00",
                "2026-07-17 09:00:00",
            ]
        );

        // --date-human is the old spelling, single-digit days unpadded.
        let human = DateFmt { human: true, time: false };
        let rows = commit_rows(&tmp, &refs, None, None, Order::Date, human, false, false, &[], false).unwrap();
        assert_eq!(rows[4].date, "Jul. 1, 2026");
        // The filter key never changes shape, whatever the column is spelled
        // as: --date compares ISO no matter what you are looking at.
        assert_eq!(rows[4].key, "2026-07-01");

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn a_cherry_picked_patch_is_neither_present_nor_missing() {
        let tmp = std::env::temp_dir().join(format!("git-wt-cherry-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str]) -> String {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_AUTHOR_DATE", "2026-07-17T10:00:00")
                .env("GIT_COMMITTER_DATE", "2026-07-17T10:00:00")
                .env("GIT_EDITOR", "true")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?} failed: {out:?}");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }
        let commit = |name: &str, file: &str| {
            std::fs::write(tmp.join(file), name).unwrap();
            git(&tmp, &["add", "-A"]);
            git(&tmp, &["commit", "--quiet", "-m", name]);
        };

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"]);
        git(&tmp, &["config", "user.email", "t@test"]);
        git(&tmp, &["config", "user.name", "t"]);
        commit("base", "base.txt");
        git(&tmp, &["checkout", "--quiet", "-b", "feat"]);
        commit("shared-fix", "fix.txt");
        let feat_fix = git(&tmp, &["rev-parse", "HEAD"]);
        commit("feat-only", "only.txt");
        let feat_only = git(&tmp, &["rev-parse", "HEAD"]);
        git(&tmp, &["checkout", "--quiet", "main"]);
        // main needs work of its own first: onto the same parent, with the
        // dates pinned, a pick reproduces every input of the original and so
        // reproduces its sha -- the same commit, not a copy of it.
        commit("main-work", "mainwork.txt");
        // main takes the fix by cherry-pick: same patch, its own sha.
        git(&tmp, &["cherry-pick", &feat_fix]);
        let main_fix = git(&tmp, &["rev-parse", "HEAD"]);
        assert_ne!(feat_fix, main_fix, "a pick makes a new commit");

        let refs = vec!["main".to_string(), "feat".to_string()];
        let equiv = equivalents(&tmp, &refs);
        let sets: Vec<HashSet<String>> = refs
            .iter()
            .map(|r| ref_shas(&tmp, r, None).unwrap())
            .collect();
        let empty: HashSet<String> = HashSet::new();
        let mark =
            |sha: &str, col: usize| Mark::of(sha, &sets[col], &equiv[col], &empty, &empty);
        let (main_col, feat_col) = (0, 1);

        // Each side has its own sha of the fix, and an equivalent of the
        // other's: same patch, so neither '✓' nor '·' is the truth.
        assert_eq!(mark(&main_fix, main_col), Mark::Has);
        assert_eq!(mark(&main_fix, feat_col), Mark::Equivalent);
        assert_eq!(mark(&feat_fix, feat_col), Mark::Has);
        assert_eq!(mark(&feat_fix, main_col), Mark::Equivalent);

        // The commit main really is missing stays missing: '≈' must mean
        // something, so it cannot leak onto work nobody picked.
        assert_eq!(mark(&feat_only, feat_col), Mark::Has);
        assert_eq!(mark(&feat_only, main_col), Mark::Missing);

        // --no-cherry is the old answer: equivalence unasked, so the picked
        // commit reads as absent again.
        let none = vec![HashSet::new(); refs.len()];
        assert_eq!(
            Mark::of(&feat_fix, &sets[main_col], &none[main_col], &none[main_col], &none[main_col]),
            Mark::Missing
        );

        // --pick-id's column: each copy of the fix names the other's sha, and
        // the work nobody picked names nothing.
        let picks = pick_ids(&tmp, &refs);
        assert_eq!(picks.get(&main_fix), Some(&feat_fix));
        assert_eq!(picks.get(&feat_fix), Some(&main_fix));
        assert_eq!(picks.get(&feat_only), None);
        // Every '≈' the marks report is a sha the column can name: the two
        // answers come from one patch comparison and must not disagree.
        for (col, r) in refs.iter().enumerate() {
            for sha in ref_shas(&tmp, r, None).unwrap() {
                if mark(&sha, col) == Mark::Equivalent {
                    assert!(picks.contains_key(&sha), "no pick for '≈' {sha}");
                }
            }
        }

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn containment_beats_equivalence_in_a_mark() {
        let has: HashSet<String> = ["a".to_string()].into_iter().collect();
        let equiv: HashSet<String> = ["a".to_string(), "b".to_string()].into_iter().collect();
        let trailer: HashSet<String> = ["c".to_string()].into_iter().collect();
        let author: HashSet<String> = ["d".to_string()].into_iter().collect();
        // A branch holding both the commit and a copy of its patch still just
        // has the commit; '≈' would understate it.
        assert_eq!(Mark::of("a", &has, &equiv, &trailer, &author), Mark::Has);
        assert_eq!(Mark::of("b", &has, &equiv, &trailer, &author), Mark::Equivalent);
        assert_eq!(Mark::of("c", &has, &equiv, &trailer, &author), Mark::Trailer);
        assert_eq!(Mark::of("d", &has, &equiv, &trailer, &author), Mark::AuthorMatch);
        assert_eq!(Mark::of("e", &has, &equiv, &trailer, &author), Mark::Missing);
        assert_eq!(Mark::Has.glyph(), "✓");
        assert_eq!(Mark::Equivalent.glyph(), "≈");
        assert_eq!(Mark::Trailer.glyph(), "←");
        assert_eq!(Mark::AuthorMatch.glyph(), "~");
        assert_eq!(Mark::Missing.glyph(), "·");
    }

    #[test]
    fn cherry_trailer_source_finds_the_last_marker() {
        assert_eq!(
            cherry_trailer_source("some body\n\n(cherry picked from commit abc123)"),
            Some("abc123")
        );
        // A message that mentions the phrase earlier keeps the last one.
        assert_eq!(
            cherry_trailer_source(
                "discussed (cherry picked from commit old)\n\n(cherry picked from commit new)"
            ),
            Some("new")
        );
        assert!(cherry_trailer_source("plain body").is_none());
        assert!(cherry_trailer_source(
            "body without a closing paren (cherry picked from commit abc123"
        )
        .is_none());
    }

    #[test]
    fn a_cherry_pick_with_x_trailer_is_marked_trailer() {
        let tmp = std::env::temp_dir().join(format!("git-wt-trailer-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str]) -> String {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_AUTHOR_DATE", "2026-07-17T10:00:00")
                .env("GIT_COMMITTER_DATE", "2026-07-17T10:00:00")
                .env("GIT_EDITOR", "true")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {:?} failed: {:?}", args, out);
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }
        let commit = |dir: &std::path::Path, name: &str, file: &str| {
            std::fs::write(tmp.join(file), name).unwrap();
            git(dir, &["add", "-A"]);
            git(dir, &["commit", "--quiet", "-m", name]);
        };

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp,
            &["init", "--quiet", "--initial-branch=main"],
        );
        git(&tmp,
            &["config", "user.email", "t@test"],
        );
        git(&tmp,
            &["config", "user.name", "t"],
        );
        commit(&tmp, "base", "base.txt");
        git(&tmp,
            &["checkout", "--quiet", "-b", "feat"],
        );
        commit(&tmp, "shared-fix", "fix.txt");
        let feat_fix = git(&tmp,
            &["rev-parse", "HEAD"],
        );
        git(&tmp,
            &["checkout", "--quiet", "main"],
        );
        commit(&tmp, "main-work", "mainwork.txt");
        // A real cherry-pick with -x: the patch is the same, so patch-id already
        // catches it, but the trailer is the stronger signal we want to surface.
        git(&tmp,
            &["cherry-pick", "-x", &feat_fix],
        );

        let refs = vec!["main".to_string(), "feat".to_string()];
        let sets: Vec<HashSet<String>> = refs
            .iter()
            .map(|r| ref_shas(&tmp, r, None).unwrap())
            .collect();
        let equiv = equivalents(&tmp,
            &refs,
        );
        let trailer = trailer_sets(&tmp,
            &refs,
        );
        let author = author_match_sets(&tmp,
            &refs,
            &commit_rows(
                &tmp,
                &refs,
                None,
                None,
                Order::Date,
                ISO,
                false,
                false,
                &[],
                false,
            )
            .unwrap(),
        );
        let mark =
            |sha: &str, col: usize| Mark::of(sha, &sets[col], &equiv[col], &trailer[col], &author[col]);
        let (main_col, feat_col) = (0, 1);

        assert_eq!(mark(&feat_fix, feat_col), Mark::Has);
        // The trailer set really was populated by the -x pick...
        assert!(trailer[main_col].contains(&feat_fix));
        // ...but a clean pick's patch-id already matches, and containment/
        // equivalence outrank the trailer in `Mark::of`'s precedence, so the
        // mark itself still reads '≈' here. Trailer is a fallback for picks
        // patch-id can't see, not a replacement for it.
        assert_eq!(mark(&feat_fix, main_col), Mark::Equivalent);
        // The copy itself, read from main's side, is Has on main.
        let main_fix = git(&tmp,
            &["rev-parse", "HEAD"],
        );
        assert_eq!(mark(&main_fix, main_col), Mark::Has);
        assert_eq!(mark(&main_fix, feat_col), Mark::Equivalent);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn a_conflict_pick_without_x_is_marked_by_author_fingerprint() {
        let tmp = std::env::temp_dir().join(format!(
            "git-wt-fingerprint-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str], env: &[(&str, &str)]) -> String {
            let mut c = std::process::Command::new("git");
            c.current_dir(dir).args(args);
            for (k, v) in env {
                c.env(k, v);
            }
            let out = c.output().unwrap();
            assert!(out.status.success(), "git {:?} failed: {:?}", args, out);
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }
        fn write(dir: &std::path::Path, path: &str, text: &str) {
            let full = dir.join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, text).unwrap();
        }

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp,
            &["init", "--quiet", "--initial-branch=main"],
            &[],
        );
        git(&tmp,
            &["config", "user.email", "kevin.mensah@fireflyelectric.com"],
            &[],
        );
        git(&tmp,
            &["config", "user.name", "Kevin Mensah"],
            &[],
        );
        write(&tmp, "f.txt", "base\n");
        git(&tmp,
            &["add", "-A"],
            &[],
        );
        git(
            &tmp,
            &["commit", "--quiet", "-m", "base"],
            &[("GIT_AUTHOR_DATE", "2026-02-01T10:00:00+08:00")],
        );

        git(&tmp,
            &["checkout", "--quiet", "-b", "uat"],
            &[],
        );
        write(&tmp, "f.txt", "base\nuat-line\n");
        git(&tmp,
            &["add", "-A"],
            &[],
        );
        git(
            &tmp,
            &["commit", "--quiet", "-m", "update in booking report and po attachment"],
            &[("GIT_AUTHOR_DATE", "2026-02-12T11:37:03+08:00")],
        );
        let uat_commit = git(
            &tmp,
            &["rev-parse", "HEAD"],
            &[],
        );

        git(&tmp,
            &["checkout", "--quiet", "main"],
            &[],
        );
        // main diverges on the same file so the pick will conflict.
        write(&tmp, "f.txt", "base\nmain-line\n");
        git(&tmp,
            &["add", "-A"],
            &[],
        );
        git(
            &tmp,
            &["commit", "--quiet", "-m", "main diverges"],
            &[("GIT_AUTHOR_DATE", "2026-02-13T10:00:00+08:00")],
        );

        // Pick without -x, resolving by taking the destination's side for the
        // conflicting hunk. The resulting patch-id differs from the original,
        // but author + date + subject stay identical.
        let pick_out = std::process::Command::new("git")
            .current_dir(&tmp)
            .args([
                "cherry-pick",
                "--no-commit",
                "-X",
                "theirs",
                &uat_commit,
            ])
            .output()
            .unwrap();
        assert!(pick_out.status.success(), "cherry-pick failed: {:?}", pick_out);
        git(
            &tmp,
            &["commit", "--quiet", "-m", "update in booking report and po attachment"],
            &[(
                "GIT_AUTHOR_DATE",
                "2026-02-12T11:37:03+08:00",
            )],
        );
        let prd_commit = git(
            &tmp,
            &["rev-parse", "HEAD"],
            &[],
        );

        let refs = vec!["uat".to_string(), "main".to_string()];
        let rows = commit_rows(
            &tmp,
            &refs,
            None,
            None,
            Order::Date,
            ISO,
            false,
            false,
            &[],
            false,
        )
        .unwrap();
        let sets: Vec<HashSet<String>> = refs
            .iter()
            .map(|r| ref_shas(&tmp, r, None).unwrap())
            .collect();
        let equiv = equivalents(&tmp,
            &refs,
        );
        let trailer = trailer_sets(
            &tmp,
            &refs,
        );
        let author = author_match_sets(
            &tmp,
            &refs,
            &rows,
        );
        let mark =
            |sha: &str, col: usize| Mark::of(sha, &sets[col], &equiv[col], &trailer[col], &author[col]);
        let (uat_col, main_col) = (0, 1);

        // The patch changed in resolution, so patch-id does not match.
        assert_ne!(mark(&uat_commit, main_col), Mark::Equivalent);
        // The author fingerprint still finds the copy on main.
        assert_eq!(mark(&uat_commit, main_col), Mark::AuthorMatch);
        assert_eq!(mark(
            &prd_commit, main_col
        ), Mark::Has);
        assert_eq!(mark(
            &prd_commit, uat_col
        ), Mark::AuthorMatch);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn a_merge_matches_the_paths_its_block_will_show() {
        // The bug this exists for: `git log -- <path>` prunes merges, so a
        // merge that brought a whole feature in matched nothing -- while its
        // file block sat right there listing the very files searched for.
        let tmp = std::env::temp_dir().join(format!("git-wt-pathmerge-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str]) {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_AUTHOR_DATE", "2026-07-17T10:00:00")
                .env("GIT_COMMITTER_DATE", "2026-07-17T10:00:00")
                .env("GIT_MERGE_AUTOEDIT", "no")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?} failed: {out:?}");
        }
        fn write(dir: &std::path::Path, path: &str, text: &str) {
            let full = dir.join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, text).unwrap();
        }

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"]);
        git(&tmp, &["config", "user.email", "t@test"]);
        git(&tmp, &["config", "user.name", "t"]);
        write(&tmp, "base.txt", "base");
        git(&tmp, &["add", "-A"]);
        git(&tmp, &["commit", "--quiet", "-m", "base"]);

        // The feature branch, merged with a commit of its own.
        git(&tmp, &["checkout", "--quiet", "-b", "feature"]);
        write(&tmp, "app/Expense/Report.php", "x");
        git(&tmp, &["add", "-A"]);
        git(&tmp, &["commit", "--quiet", "-m", "add expense report"]);
        git(&tmp, &["checkout", "--quiet", "main"]);
        git(&tmp, &["merge", "--no-ff", "-m", "merge-expense", "feature"]);

        // An unrelated branch, merged the same way: --full-history keeps every
        // merge, so this is the one the name-only check has to throw back.
        git(&tmp, &["checkout", "--quiet", "-b", "other", "HEAD~1"]);
        write(&tmp, "unrelated.txt", "z");
        git(&tmp, &["add", "-A"]);
        git(&tmp, &["commit", "--quiet", "-m", "unrelated"]);
        git(&tmp, &["checkout", "--quiet", "main"]);
        git(&tmp, &["merge", "--no-ff", "-m", "merge-other", "other"]);

        let refs = vec!["main".to_string()];
        let hits = path_shas(&tmp, &refs, "expense").unwrap();
        let subject = |sha: &str| -> String {
            let out = std::process::Command::new("git")
                .current_dir(&tmp)
                .args(["log", "-1", "--format=%s", sha])
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        let subjects: HashSet<String> = hits.iter().map(|s| subject(s)).collect();

        // The commit that wrote the file, and the merge that brought it in --
        // whose block will list it, which is what makes it a match.
        assert!(subjects.contains("add expense report"), "{subjects:?}");
        assert!(subjects.contains("merge-expense"), "{subjects:?}");
        // ...and not the merge that touched nothing matching, nor the base.
        assert!(!subjects.contains("merge-other"), "{subjects:?}");
        assert!(!subjects.contains("unrelated"), "{subjects:?}");
        assert!(!subjects.contains("base"), "{subjects:?}");

        // Case-folded, and a substring: the term is the user's, not a glob.
        assert_eq!(path_shas(&tmp, &refs, "EXPENSE").unwrap(), hits);
        assert_eq!(path_shas(&tmp, &refs, "app/Expense").unwrap(), hits);
        assert!(path_shas(&tmp, &refs, "zzz").unwrap().is_empty());

        std::fs::remove_dir_all(&tmp).ok();
    }

    /// `merge --review`'s range, on the case that regresses silently.
    ///
    /// `--review` shows `dest..src` and keeps merge commits, so an inherited
    /// `--filename` lands on exactly the bug commit `2c7b804` was written for:
    /// `git log -- <path>` prunes merges, so a merge whose block lists thirty
    /// matching files used to match nothing. The `commits` suite pins that over
    /// a whole branch; this pins it over a range, which is where a future
    /// narrowing of the review window would quietly undo it.
    #[test]
    fn a_review_range_matches_the_merge_that_carried_the_files() {
        let tmp =
            std::env::temp_dir().join(format!("git-wt-reviewpath-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str]) {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_AUTHOR_DATE", "2026-07-17T10:00:00")
                .env("GIT_COMMITTER_DATE", "2026-07-17T10:00:00")
                .env("GIT_MERGE_AUTOEDIT", "no")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?} failed: {out:?}");
        }
        fn write(dir: &std::path::Path, path: &str, text: &str) {
            let full = dir.join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, text).unwrap();
        }

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"]);
        git(&tmp, &["config", "user.email", "t@test"]);
        git(&tmp, &["config", "user.name", "t"]);
        write(&tmp, "base.txt", "base");
        git(&tmp, &["add", "-A"]);
        git(&tmp, &["commit", "--quiet", "-m", "base"]);

        // The review source: a branch that merged a sub-branch into itself, so
        // the range `main..release` carries a merge commit.
        git(&tmp, &["checkout", "--quiet", "-b", "release"]);
        git(&tmp, &["checkout", "--quiet", "-b", "feature"]);
        write(&tmp, "app/Expense/Report.php", "x");
        git(&tmp, &["add", "-A"]);
        git(&tmp, &["commit", "--quiet", "-m", "add expense report"]);
        git(&tmp, &["checkout", "--quiet", "release"]);
        git(&tmp, &["merge", "--no-ff", "-m", "merge-expense", "feature"]);
        // One more on the source, so the range is not merely the merge.
        write(&tmp, "notes.txt", "n");
        git(&tmp, &["add", "-A"]);
        git(&tmp, &["commit", "--quiet", "-m", "notes"]);
        git(&tmp, &["checkout", "--quiet", "main"]);

        let refs = vec!["release".to_string()];
        // What `--review` runs: the source's log, cut at the destination.
        let subjects = |merges: bool| -> Vec<String> {
            commit_rows(&tmp, &refs, Some("main"), None, Order::Date, ISO, !merges, false, &[], false)
                .unwrap()
                .iter()
                .map(|r| r.text.clone())
                .collect()
        };

        // The merges default flips under --review, and both directions are
        // asserted: a test that only pinned the new one would pass just as well
        // against an inverted flag.
        let kept = subjects(true);
        assert!(kept.contains(&"merge-expense".to_string()), "{kept:?}");
        assert_eq!(kept.len(), 3, "{kept:?}");
        let dropped = subjects(false);
        assert!(!dropped.contains(&"merge-expense".to_string()), "{dropped:?}");
        assert_eq!(dropped.len(), 2, "{dropped:?}");
        // The destination's own history is never in the range, either way.
        for s in [&kept, &dropped] {
            assert!(!s.contains(&"base".to_string()), "{s:?}");
        }

        // The filter, over the rows the range produced: the merge matches
        // because its block will list the file, which is the whole fix.
        let hits = path_shas(&tmp, &refs, "expense").unwrap();
        let by_sha: HashMap<String, String> = commit_rows(
            &tmp, &refs, Some("main"), None, Order::Date, ISO, false, false, &[], false,
        )
        .unwrap()
        .into_iter()
        .map(|r| (r.sha, r.text))
        .collect();
        let matched: HashSet<&String> = by_sha
            .iter()
            .filter(|(sha, _)| hits.contains(*sha))
            .map(|(_, text)| text)
            .collect();
        assert!(matched.contains(&"merge-expense".to_string()), "{matched:?}");
        assert!(matched.contains(&"add expense report".to_string()), "{matched:?}");
        assert!(!matched.contains(&"notes".to_string()), "{matched:?}");

        std::fs::remove_dir_all(&tmp).ok();
    }

    /// The destination column's `≈`, which is the reason `--review` has a
    /// column at all: a commit cherry-picked onto the destination is absent
    /// from it *by sha*, so the range still lists it even though the work has
    /// landed. `equivalents` indexes by upstream, so the destination's answer
    /// is entry 0 of `[dest, src]` -- the index `commits_view` truncates to.
    #[test]
    fn a_review_marks_the_commit_the_destination_already_cherry_picked() {
        let tmp =
            std::env::temp_dir().join(format!("git-wt-reviewpick-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str]) -> String {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_AUTHOR_DATE", "2026-07-17T10:00:00")
                .env("GIT_COMMITTER_DATE", "2026-07-17T10:00:00")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?} failed: {out:?}");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"]);
        git(&tmp, &["config", "user.email", "t@test"]);
        git(&tmp, &["config", "user.name", "t"]);
        std::fs::write(tmp.join("base.txt"), "base\n").unwrap();
        git(&tmp, &["add", "-A"]);
        git(&tmp, &["commit", "--quiet", "-m", "base"]);

        git(&tmp, &["checkout", "--quiet", "-b", "feature"]);
        std::fs::write(tmp.join("fix.txt"), "the fix\n").unwrap();
        git(&tmp, &["add", "-A"]);
        git(&tmp, &["commit", "--quiet", "-m", "the urgent fix"]);
        let fix = git(&tmp, &["rev-parse", "HEAD"]);
        std::fs::write(tmp.join("later.txt"), "later\n").unwrap();
        git(&tmp, &["add", "-A"]);
        git(&tmp, &["commit", "--quiet", "-m", "later work"]);

        // The hotfix path: the fix is picked straight onto main, so main holds
        // the patch under a new sha while feature keeps the original.
        //
        // main commits first so the pick lands on a different parent. Onto the
        // same parent, with this fixture's pinned dates, the copy would hash to
        // the very same sha -- a real cherry-pick, and not the case this test
        // is about.
        git(&tmp, &["checkout", "--quiet", "main"]);
        std::fs::write(tmp.join("base.txt"), "base\nmoved on\n").unwrap();
        git(&tmp, &["commit", "--quiet", "-am", "main moves on"]);
        git(&tmp, &["cherry-pick", &fix]);

        let rows = commit_rows(
            &tmp,
            &["feature".to_string()],
            Some("main"),
            None,
            Order::Date,
            ISO,
            false,
            false,
            &[],
            false,
        )
        .unwrap();
        // Absent by sha, so the merge would still bring it: that is why the
        // row is there to be marked in the first place.
        let subjects: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        assert!(rows.iter().any(|r| r.sha == fix), "{subjects:?}");

        let equiv = equivalents(&tmp, &["main".to_string(), "feature".to_string()]);
        let dest = &equiv[0];
        assert!(dest.contains(&fix), "the picked commit should be '≈' in main");
        let later = rows.iter().find(|r| r.text == "later work").unwrap();
        assert!(!dest.contains(&later.sha), "genuinely new work should be '·'");

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn no_merges_drops_only_the_merge_commits() {
        let tmp = std::env::temp_dir().join(format!("git-wt-merges-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str]) {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_AUTHOR_DATE", "2026-07-17T10:00:00")
                .env("GIT_COMMITTER_DATE", "2026-07-17T10:00:00")
                .env("GIT_MERGE_AUTOEDIT", "no")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?} failed: {out:?}");
        }

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"]);
        git(&tmp, &["config", "user.email", "t@test"]);
        git(&tmp, &["config", "user.name", "t"]);
        git(&tmp, &["commit", "--quiet", "--allow-empty", "-m", "base"]);
        git(&tmp, &["checkout", "--quiet", "-b", "side"]);
        git(&tmp, &["commit", "--quiet", "--allow-empty", "-m", "on-side"]);
        git(&tmp, &["checkout", "--quiet", "main"]);
        git(&tmp, &["commit", "--quiet", "--allow-empty", "-m", "on-main"]);
        // A real merge commit: two parents, no work of its own.
        git(&tmp, &["merge", "--no-ff", "-m", "merge-side", "side"]);

        let refs = vec!["main".to_string()];
        let rows = |no_merges: bool| -> Vec<String> {
            commit_rows(&tmp, &refs, None, None, Order::Date, ISO, no_merges, false, &[], false)
                .unwrap()
                .iter()
                .map(|r| r.text.clone())
                .collect()
        };

        let all = rows(false);
        assert!(all.contains(&"merge-side".to_string()), "{all:?}");
        assert_eq!(all.len(), 4);

        // Only the merge goes: the commits it joined are still there, which is
        // the point -- the work survives, the bookkeeping row does not.
        let kept = rows(true);
        assert!(!kept.contains(&"merge-side".to_string()), "{kept:?}");
        assert_eq!(kept.len(), 3);
        for c in ["base", "on-side", "on-main"] {
            assert!(kept.contains(&c.to_string()), "{c} should survive: {kept:?}");
        }
    }

    #[test]
    fn rows_follow_ancestry_even_when_the_dates_disagree() {
        let tmp = std::env::temp_dir().join(format!("git-wt-order-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        fn git(dir: &std::path::Path, args: &[&str], when: &str) {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .env("GIT_AUTHOR_DATE", when)
                .env("GIT_COMMITTER_DATE", when)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?} failed: {out:?}");
        }

        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"], "");
        git(&tmp, &["config", "user.email", "t@test"], "");
        git(&tmp, &["config", "user.name", "t"], "");

        // The parent is authored in May, its child in January: a rebase, a
        // cherry-pick, or a bad clock all produce exactly this.
        git(&tmp, &["commit", "--quiet", "--allow-empty", "-m", "parent"], "2026-05-01T10:00:00");
        git(&tmp, &["commit", "--quiet", "--allow-empty", "-m", "child"], "2026-01-01T10:00:00");

        let refs = vec!["main".to_string()];
        let rows = commit_rows(&tmp, &refs, None, None, Order::Date, ISO, false, false, &[], false).unwrap();
        let seen: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();

        // Ancestry wins: the child is listed above the parent it descends from,
        // so reading down the table is reading real history. The date column
        // ascends across that pair, which is the wrong clock showing through --
        // not the rows lying about what came from what.
        assert_eq!(seen, ["child", "parent"], "a parent must never precede its child");
        assert_eq!(rows[0].key, "2026-01-01");
        assert_eq!(rows[1].key, "2026-05-01");

        std::fs::remove_dir_all(&tmp).ok();
    }

    fn git(dir: &std::path::Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?} failed: {out:?}");
    }

    fn write_commit(dir: &std::path::Path, path: &str, content: &str, msg: &str) {
        let full = dir.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(full, content).unwrap();
        git(dir, &["add", "-A"]);
        git(dir, &["commit", "--quiet", "-m", msg]);
    }

    /// `log`'s row source: a pathspec on the same walk, so only commits that
    /// touched the path become rows -- and a merge that carried the whole file
    /// over is the `path_shas` trap this function has to dodge the same way.
    #[test]
    fn a_pathspec_narrows_the_rows_and_the_merge_trap_is_dodged() {
        let tmp = std::env::temp_dir().join(format!("git-wt-log-path-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"]);
        git(&tmp, &["config", "user.email", "t@test"]);
        git(&tmp, &["config", "user.name", "t"]);

        write_commit(&tmp, "src/ui.rs", "one", "touch ui");
        write_commit(&tmp, "other.rs", "one", "touch other");
        git(&tmp, &["branch", "feat"]);
        git(&tmp, &["checkout", "--quiet", "feat"]);
        write_commit(&tmp, "src/ui.rs", "two", "feat touches ui");
        git(&tmp, &["checkout", "--quiet", "main"]);
        write_commit(&tmp, "other.rs", "two", "main touches other");
        git(&tmp, &["merge", "--quiet", "--no-ff", "-m", "merge feat", "feat"]);

        let refs = vec!["main".to_string()];
        let paths = vec!["src/ui.rs".to_string()];

        // Merges kept (--merges, so no_merges is false): the merge brought
        // ui.rs's feat-side change over relative to main's prior tip, and
        // without --full-history/--diff-merges=first-parent a plain pathspec
        // walk would prune it -- the trap `path_shas` already documents,
        // dodged here the same way.
        let rows = commit_rows(
            &tmp, &refs, None, None, Order::Date, ISO, false, false, &paths, false,
        )
        .unwrap();
        let subjects: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(subjects, ["merge feat", "feat touches ui", "touch ui"], "{subjects:?}");
        assert!(!subjects.contains(&"touch other"), "{subjects:?}");
        assert!(!subjects.contains(&"main touches other"), "{subjects:?}");

        // Merges dropped (the default, no_merges = true): plain --no-merges,
        // and the merge row is simply absent -- no pruning trap in play
        // because there is no merge row to begin with.
        let dropped = commit_rows(
            &tmp, &refs, None, None, Order::Date, ISO, true, false, &paths, false,
        )
        .unwrap();
        let subjects: Vec<&str> = dropped.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(subjects, ["feat touches ui", "touch ui"], "{subjects:?}");

        std::fs::remove_dir_all(&tmp).ok();
    }

    /// `--follow` spans a rename; without it the walk stops at the boundary.
    /// Only sound with exactly one path, which is the caller's job to enforce
    /// -- this pins what the flag does once that condition holds.
    #[test]
    fn follow_spans_a_rename_and_plain_does_not() {
        let tmp = std::env::temp_dir().join(format!("git-wt-log-follow-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"]);
        git(&tmp, &["config", "user.email", "t@test"]);
        git(&tmp, &["config", "user.name", "t"]);

        write_commit(&tmp, "src/old.rs", "a\nb\nc\nd\ne\n", "add old");
        git(&tmp, &["mv", "src/old.rs", "src/new.rs"]);
        git(&tmp, &["commit", "--quiet", "-m", "rename"]);
        write_commit(&tmp, "src/new.rs", "a\nb\nc\nd\ne\nf\n", "edit new");

        let refs = vec!["main".to_string()];

        let followed = commit_rows(
            &tmp, &refs, None, None, Order::Date, ISO, true, false,
            &["src/new.rs".to_string()], true,
        )
        .unwrap();
        let subjects: Vec<&str> = followed.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(subjects, ["edit new", "rename", "add old"], "{subjects:?}");

        let plain = commit_rows(
            &tmp, &refs, None, None, Order::Date, ISO, true, false,
            &["src/new.rs".to_string()], false,
        )
        .unwrap();
        let subjects: Vec<&str> = plain.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(subjects, ["edit new", "rename"], "{subjects:?}");

        std::fs::remove_dir_all(&tmp).ok();
    }

    /// The `±` cell and the `path` column's source: per-path churn and the
    /// matched name(s), from one scoped `--numstat`, not the commit-wide
    /// count `--files` prints.
    #[test]
    fn path_row_stat_counts_only_the_named_path() {
        let tmp = std::env::temp_dir().join(format!("git-wt-log-numstat-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "--quiet", "--initial-branch=main"]);
        git(&tmp, &["config", "user.email", "t@test"]);
        git(&tmp, &["config", "user.name", "t"]);

        std::fs::write(tmp.join("a.rs"), "1\n2\n3\n").unwrap();
        std::fs::write(tmp.join("b.rs"), "1\n2\n").unwrap();
        git(&tmp, &["add", "-A"]);
        git(&tmp, &["commit", "--quiet", "-m", "two files, one commit"]);

        let head = git_stdout(&tmp, &["rev-parse", "HEAD"]).unwrap();
        let (added, removed, names) = path_row_stat(&tmp, head.trim(), &["a.rs".to_string()]).unwrap();
        assert_eq!((added, removed), (Some(3), Some(0)));
        assert_eq!(names, vec!["a.rs".to_string()]);

        // A path the commit never touched has nothing to sum: '-', not 0.
        let (added, removed, names) = path_row_stat(&tmp, head.trim(), &["c.rs".to_string()]).unwrap();
        assert_eq!((added, removed), (None, None));
        assert!(names.is_empty());

        std::fs::remove_dir_all(&tmp).ok();
    }
}
