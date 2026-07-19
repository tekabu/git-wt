use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Stdio;

use crate::cmd::commits::args::{DateFmt, Order};
use crate::git::{git_cmd, git_stdout};
use crate::ui::{width_bound, CHECK, DIM, EQUIV, GREEN, MISS, YELLOW};

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

/// The files a commit touched, with status and line counts.
///
/// Diffed against the first parent (or the empty tree for root commits), which
/// matches what a reader expects from a one-line log entry. Merge commits show
/// the first-parent diff only, not the combined merge.
pub(crate) fn commit_files(root: &Path, sha: &str) -> Result<Vec<FileStat>, String> {
    // First parent, or the empty tree for a root commit. The empty tree hash is
    // stable across git versions, so we use it directly rather than spawning a
    // command to compute it.
    const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
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
    let base = parents.first().map(String::as_str).unwrap_or(EMPTY_TREE);

    let status_out = git_stdout(
        root,
        &["diff-tree", "-r", "--name-status", "-M", "-C", base, sha],
    )?;
    let numstat_out = git_stdout(
        root,
        &["diff-tree", "-r", "--numstat", "-M", "-C", base, sha],
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
                // R100<tab>old<tab>new
                let Some(old) = parts.next() else {
                    continue;
                };
                let Some(new) = parts.next() else {
                    continue;
                };
                status_by_path.insert(new.to_string(), status);
                // `--numstat` reports the rename as `old => new`, so keep that
                // lookup key too.
                status_by_path.insert(format!("{} => {}", old, new), status);
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
    for line in numstat_out.lines() {
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(3, '\t');
        let Some(added_field) = parts.next() else {
            continue;
        };
        let Some(removed_field) = parts.next() else {
            continue;
        };
        let Some(path) = parts.next() else {
            continue;
        };
        let added = if added_field == "-" {
            None
        } else {
            added_field.parse::<usize>().ok()
        };
        let removed = if removed_field == "-" {
            None
        } else {
            removed_field.parse::<usize>().ok()
        };
        let status = status_by_path.get(path).copied().unwrap_or('M');
        stats.push(FileStat {
            status,
            path: path.to_string(),
            added,
            removed,
        });
    }

    sort_file_stats(&mut stats);
    Ok(stats)
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
) -> Result<Vec<CommitRow>, String> {
    let count;
    let date_arg = format!("--date=format:{}", fmt.spec());
    let mut args = vec![
        "log",
        order.flag(),
        &date_arg,
        "--format=%H%x09%aN%x09%ad%x09%as%x09%h%x09%at%x09%s",
    ];
    // Merge commits carry no work of their own; dropping them leaves the
    // commits someone actually wrote. The mark columns are unaffected: a
    // merge that is not a row is still in every rev-list that reaches it.
    if no_merges {
        args.push("--no-merges");
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

    let out = git_stdout(root, &args)?;
    Ok(out
        .lines()
        .filter_map(|line| {
            let mut f = line.splitn(7, '\t');
            Some(CommitRow {
                sha: f.next()?.to_string(),
                author: f.next()?.to_string(),
                date: f.next()?.to_string(),
                key: f.next()?.to_string(),
                short: f.next()?.to_string(),
                stamp: f.next()?.to_string(),
                text: f.next()?.to_string(),
            })
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
    /// Neither.
    Missing,
}

impl Mark {
    pub(crate) fn of(sha: &str, has: &HashSet<String>, equiv: &HashSet<String>) -> Mark {
        // Containment wins: a branch that has the commit has it, whatever a
        // patch comparison would also say about an equivalent elsewhere.
        if has.contains(sha) {
            Mark::Has
        } else if equiv.contains(sha) {
            Mark::Equivalent
        } else {
            Mark::Missing
        }
    }

    pub(crate) fn glyph(self) -> &'static str {
        match self {
            Mark::Has => CHECK,
            Mark::Equivalent => EQUIV,
            Mark::Missing => MISS,
        }
    }

    pub(crate) fn color(self) -> &'static str {
        match self {
            Mark::Has => GREEN,
            // Yellow: present, but not as the commit in this row.
            Mark::Equivalent => YELLOW,
            Mark::Missing => DIM,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::commits::args::{parse_date_filter, DateFmt};

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
        let all_rows = commit_rows(&tmp, &refs[..1], None, None, Order::Date, ISO, false).unwrap();
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
        let full = commit_rows(&tmp, &refs[..1], None, None, Order::Date, ISO, false).unwrap();
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
        let union = commit_rows(&tmp, &refs, None, None, Order::Date, ISO, false).unwrap();
        let subjects: Vec<&str> = union.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(union.len(), 3, "{subjects:?}");
        // --author-date-order, so the rows descend by the date they print.
        assert!(union[0].text.ends_with("on-feat"), "{:?}", union[0].text);
        let shared = union.iter().find(|r| r.text.ends_with("shared")).unwrap();
        assert!(ref_shas(&tmp, "main", None).unwrap().contains(&shared.sha));
        assert!(feat_all.contains(&shared.sha));

        // -n caps the rows, newest first.
        let capped = commit_rows(&tmp, &refs, None, Some(1), Order::Date, ISO, false).unwrap();
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

        let refs = vec![
            "main".to_string(),
            "feat".to_string(),
            "fix".to_string(),
        ];

        // feat and fix both forked at B, so the commits main has that either of
        // them misses are C and D; the earliest is C. The default slice should
        // include C and D (commits strictly after B), but not B or A.
        let divergent = divergent_set(&tmp, &refs[0], &refs[1..]).unwrap();
        assert!(divergent.contains(sha_by_subject(&tmp, "main", "C").as_str()));
        assert!(divergent.contains(sha_by_subject(&tmp, "main", "D").as_str()));
        assert_eq!(divergent.len(), 2);

        let full = commit_rows(
            &tmp, &refs[..1], None, None, Order::Date, ISO, false,
        ).unwrap();
        let rows = window_to_divergent(full, &divergent);
        let subjects: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(subjects, ["D", "C"], "{subjects:?}");

        // The full first-branch log with --all.
        let all_rows = commit_rows(
            &tmp, &refs[..1], None, None, Order::Date, ISO, false,
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

        let refs = vec!["main".to_string(), "feat".to_string()];

        // main has SIDE and FLOOR that feat is missing; MAINLINE is shared.
        let divergent = divergent_set(&tmp, &refs[0], &refs[1..]).unwrap();
        assert!(divergent.contains(sha_by_subject(&tmp, "main", "SIDE").as_str()));
        assert!(divergent.contains(sha_by_subject(&tmp, "main", "FLOOR").as_str()));
        assert_eq!(divergent.len(), 2, "MAINLINE is shared, not divergent");

        let full = commit_rows(&tmp, &refs[..1], None, None, Order::Date, ISO, false).unwrap();
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
            commit_rows(&tmp, &refs, None, None, o, ISO, false)
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
        let rows = commit_rows(&tmp, &refs, None, None, Order::Date, ISO, false).unwrap();
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
        let rows = commit_rows(&tmp, &refs, None, None, Order::Date, timed, false).unwrap();
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
        let rows = commit_rows(&tmp, &refs, None, None, Order::Date, human, false).unwrap();
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
        let mark = |sha: &str, col: usize| Mark::of(sha, &sets[col], &equiv[col]);
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
        assert_eq!(Mark::of(&feat_fix, &sets[main_col], &none[main_col]), Mark::Missing);

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
        // A branch holding both the commit and a copy of its patch still just
        // has the commit; '≈' would understate it.
        assert_eq!(Mark::of("a", &has, &equiv), Mark::Has);
        assert_eq!(Mark::of("b", &has, &equiv), Mark::Equivalent);
        assert_eq!(Mark::of("c", &has, &equiv), Mark::Missing);
        assert_eq!(Mark::Has.glyph(), "✓");
        assert_eq!(Mark::Equivalent.glyph(), "≈");
        assert_eq!(Mark::Missing.glyph(), "·");
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
            commit_rows(&tmp, &refs, None, None, Order::Date, ISO, no_merges)
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
        let rows = commit_rows(&tmp, &refs, None, None, Order::Date, ISO, false).unwrap();
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
}
