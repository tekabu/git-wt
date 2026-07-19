pub(crate) mod args;
pub(crate) mod md;
pub(crate) mod render;
pub(crate) mod rows;

use std::collections::HashSet;
use std::io::IsTerminal;
use std::path::Path;

use crate::cmd::commits::args::{parse_commits_args, DateFilter, DateOp, Order};
use crate::cmd::commits::md::{md_filename, write_md};
use crate::cmd::commits::render::{render_commits, Highlight};
use crate::cmd::commits::rows::{
    body_hits, commit_day, commit_files, commit_of, commit_rows, divergent_set, equivalents,
    path_shas, pick_ids, ref_shas, window_to_divergent, CommitRow, FileStat,
};
use crate::ui::{color_enabled, is_subseq, term_width};
use crate::worktree::{label, ref_of, Worktree};

/// Print a commit-by-branch table for the listed worktrees.
///
/// Refs, not directories, and commits rather than content: this is the question
/// `diff` cannot answer once there are three branches in play -- not "how do
/// these differ" but "which of them has this commit". Rows come from one `git
/// log` over every ref at once, so they are interleaved by date; columns come
/// from one `rev-list` per ref, as sha sets to test each row against.
pub(crate) fn cmd_commits(
    root: &Path,
    trees: &[Worktree],
    idxs: &[usize],
    rest: &[String],
) -> Result<(), String> {
    if idxs.len() < 2 {
        return Err("commits needs 2 or more worktrees, e.g. 'git-wt 1,2,3 commits'".into());
    }
    for (i, a) in idxs.iter().enumerate() {
        if idxs[i + 1..].contains(a) {
            return Err(format!("worktree #{} listed twice", a + 1));
        }
    }
    let args = parse_commits_args(rest)?;

    let refs: Vec<String> = idxs
        .iter()
        .map(|&i| ref_of(&trees[i]))
        .collect::<Result<_, _>>()?;

    // Three row-source modes:
    //   --union: every branch contributes rows (full logs, unioned).
    //   --all:   only the first branch contributes rows (its full log).
    //   default: the first branch's log, cut at its earliest divergent commit
    //            -- a merge-request view of what it has that the others do not,
    //            from the furthest divergence up to its tip. Shared commits
    //            newer than that floor stay in; the floor is a date, not a
    //            position or an ancestry base, so a merge DAG's older side
    //            branches cannot leak past it and --topo only regroups the same
    //            rows rather than changing which ones show.
    //
    // The column marks are always computed against each branch's full history,
    // so a shared commit inside the range still shows as present in the other
    // columns.
    let row_refs: &[String] = if args.union { &refs } else { &refs[..1] };
    // The set whose earliest member is the default view's floor: commits the
    // first branch has that at least one other is missing. `None` under --union
    // or --all, where the whole log is the rows and nothing is trimmed.
    let divergent = if args.union || args.all {
        None
    } else {
        let d = divergent_set(root, &refs[0], &refs[1..])?;
        if d.is_empty() {
            eprintln!("no commits ahead of {}", label(&trees[idxs[0]]));
            return Ok(());
        }
        Some(d)
    };

    // A filter runs here rather than in git, so `-n` has to as well: git's -n
    // caps the walk, and capping before the filter would leave rows the filter
    // was going to drop, i.e. fewer than asked for. Unfiltered, git can cap it
    // and skip the walk it saves. The default view walks whole too: its floor
    // can sit past any -n, and letting git cap first would hide it.
    let filtered = !args.dates.is_empty()
        || args.commit_since.is_some()
        || args.commit_until.is_some()
        || !args.commits.is_empty()
        || args.author.is_some()
        || args.message.is_some()
        || args.filename.is_some();
    let git_limit = if filtered || divergent.is_some() { None } else { args.limit };
    let order = if args.topo { Order::Topo } else { Order::Date };
    let all_rows = commit_rows(
        root,
        row_refs,
        None,
        git_limit,
        order,
        args.fmt,
        args.no_merges,
        // The body is fetched only for the filter that reads it: every other
        // run would be paying for text the table never prints.
        args.message.is_some(),
    )?;
    // Default view: keep the log down to its earliest divergent date, shared
    // commits above the floor included. A date threshold, so --topo shows the
    // same rows this does, only regrouped.
    let all_rows = match &divergent {
        Some(d) => window_to_divergent(all_rows, d),
        None => all_rows,
    };
    let unfiltered = all_rows.len();

    // A commit names a day here, nothing more: '--commit-since X' is
    // '--date-since <the day X was authored>'. Ancestry would answer a
    // different question -- what descends from X -- and a branch that forked
    // before X but committed after it is exactly the thing you are looking for
    // when you name a commit as a starting point.
    //
    // Both bounds resolve before any row is judged, so a typo'd ref is an error
    // rather than a quietly empty table.
    let mut dates: Vec<DateFilter> = Vec::new();
    // The anchors a filter named: the commit a bound was measured from, and any
    // commit named outright. They get highlighted, because in a table where
    // every row matched they are the rows that were actually asked for.
    let mut anchors: HashSet<String> = HashSet::new();
    if let Some(r) = &args.commit_since {
        let c = commit_of(root, r, "--commit-since")?;
        dates.push(DateFilter { op: DateOp::Ge, date: commit_day(root, &c)? });
        anchors.insert(c);
    }
    if let Some(r) = &args.commit_until {
        let c = commit_of(root, r, "--commit-until")?;
        dates.push(DateFilter { op: DateOp::Le, date: commit_day(root, &c)? });
        anchors.insert(c);
    }

    // '--commits a,b' names the rows outright. Each id resolves first, for the
    // same reason the bounds do, and is compared as a full sha so 'af48509' and
    // the whole 40 characters both land on the same row.
    let mut wanted: HashSet<String> = HashSet::new();
    for id in &args.commits {
        let c = commit_of(root, id, "--commits")?;
        wanted.insert(c.clone());
        anchors.insert(c);
    }

    // Fuzzy, and the same fuzzy `list` uses: a subsequence, case-folded, so
    // '--author nes' finds 'Nino Escalera' and nobody types a full name twice.
    let needle = args.author.as_ref().map(|a| a.to_lowercase());

    // A substring rather than a subsequence: a name is one word typed from
    // memory, where a message is prose, and a subsequence over prose matches
    // nearly all of it.
    let msg = args.message.as_ref().map(|s| s.to_lowercase());
    // A pathspec, so git does the walk once instead of a diff per commit.
    let paths = args
        .filename
        .as_ref()
        .map(|t| path_shas(root, row_refs, t))
        .transpose()?;

    let mut rows: Vec<CommitRow> = all_rows
        .into_iter()
        .filter(|r| args.dates.iter().all(|f| f.admits(&r.key)))
        .filter(|r| dates.iter().all(|f| f.admits(&r.key)))
        .filter(|r| wanted.is_empty() || wanted.contains(&r.sha))
        .filter(|r| {
            needle
                .as_ref()
                .is_none_or(|n| is_subseq(&r.author.to_lowercase(), n))
        })
        // Subject or body: a term someone remembers from a commit is as likely
        // to be in the explanation as in the one line summarizing it.
        .filter(|r| {
            msg.as_ref().is_none_or(|n| {
                r.text.to_lowercase().contains(n) || r.body.to_lowercase().contains(n)
            })
        })
        .filter(|r| paths.as_ref().is_none_or(|p| p.contains(&r.sha)))
        .collect();
    if let Some(n) = args.limit {
        rows.truncate(n);
    }
    // After the cap, not before: '-n 10 --reverse' is the same ten commits as
    // '-n 10', read bottom-up. Reversing first would cap the oldest ten
    // instead, which is a different question nobody asked.
    if args.reverse {
        rows.reverse();
    }

    // File stats are scoped to the displayed rows, so a large log only pays for
    // what the user is looking at. Merge commits diff against their first parent.
    let mut row_files: Vec<Vec<FileStat>> = if args.files {
        rows.iter()
            .map(|r| commit_files(root, &r.sha))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };
    // --filename shows the whole block by default: the filter chose the row,
    // and the block answers what that commit did. On a merge carrying a hundred
    // files that answer buries the three that matched, so --matched-files cuts
    // it to them -- at the cost of counts that no longer sum to the commit.
    if args.matched_files {
        if let Some(t) = &args.filename.as_ref().map(|s| s.to_lowercase()) {
            for files in &mut row_files {
                files.retain(|f| f.path.to_lowercase().contains(t));
            }
        }
    }

    // The body lines a --message row matched on, scoped to the displayed rows
    // like the file blocks are. Empty when the match was in the subject, which
    // the table prints anyway.
    let row_bodies: Vec<(Vec<String>, usize)> = match &msg {
        Some(n) => rows.iter().map(|r| body_hits(&r.body, n)).collect(),
        None => Vec::new(),
    };

    if rows.is_empty() {
        // A filter that matched nothing is a different story from a history
        // with nothing in it: say which one happened.
        let msg = if filtered && unfiltered > 0 {
            let mut m = format!("no commits match those filters: {unfiltered} commits, none kept");
            // These rows are a slice, and an upper bound or an author filter
            // never widened it -- so the commits being asked about may simply
            // be older than the floor rather than absent.
            if !args.all && !args.union {
                // Suggest the lower bound in the vocabulary they were already
                // speaking: a commit bound is answered by a commit bound.
                let back = if args.commit_until.is_some() && args.dates.is_empty() {
                    "--commit-since"
                } else {
                    "--date-since"
                };
                m.push_str(&format!(
                    "\nhint: these are only the rows ahead of the other branches -- \
                     try --all (this branch's whole log), --union (every branch listed), \
                     or {back} to start further back"
                ));
            }
            m
        } else if args.union {
            "no commits".to_string()
        } else if args.all {
            format!("no commits on {}", label(&trees[idxs[0]]))
        } else {
            format!("no commits ahead of {}", label(&trees[idxs[0]]))
        };
        eprintln!("{msg}");
        return Ok(());
    }

    // A row is checked when the ref's own walk contains it. The walks are whole,
    // like the rows: the marks answer for a branch's entire history, so a row is
    // checked wherever that commit really is.
    let sets: Vec<HashSet<String>> = refs
        .iter()
        .map(|r| ref_shas(root, r, None))
        .collect::<Result<_, _>>()?;

    // Patch equivalence is what tells "not merged yet" from "already there,
    // under a different sha" -- the difference between work to do and work
    // done, which a bare '·' reports as the same thing. It costs a patch-id
    // walk per ordered pair, so --no-cherry buys the old, cheaper answer back
    // on a repo whose branches have diverged enormously.
    let equiv = if args.no_cherry {
        vec![HashSet::new(); refs.len()]
    } else {
        equivalents(root, &refs)
    };

    // Which sha the '≈' is pointing at, asked only when the column will print
    // it: it is a second patch-id walk over the same divergence.
    let picks = args.pick.then(|| pick_ids(root, &refs));

    let names: Vec<String> = idxs.iter().map(|&i| label(&trees[i])).collect();

    if let Some(path) = &args.md {
        let file = path.clone().unwrap_or_else(md_filename);
        let cmd = format!(
            "git-wt {} commits{}{}",
            idxs.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(","),
            if rest.is_empty() { "" } else { " " },
            rest.join(" ")
        );
        return write_md(
            Path::new(&file),
            &rows,
            &row_files,
            &row_bodies,
            &names,
            &sets,
            &equiv,
            picks.as_ref(),
            &cmd,
        );
    }

    let tty = std::io::stdout().is_terminal();
    render_commits(
        &rows,
        &row_files,
        &row_bodies,
        &names,
        &sets,
        &equiv,
        picks.as_ref(),
        color_enabled(tty),
        term_width(tty),
        args.wrap,
        args.subjectw,
        &Highlight {
            // The flags actually typed, not the filters they became. A commit
            // bound is a date bound underneath, but the user named a commit --
            // lighting the date column there answers a question nobody asked
            // and paints most of the table.
            date: !args.dates.is_empty(),
            author: args.author.is_some(),
            shas: anchors,
            // The term itself, so the match is lit where it sits rather than
            // the whole cell holding it.
            message: msg.clone(),
            file: args.filename.as_ref().map(|s| s.to_lowercase()),
        },
    );
    Ok(())
}
