pub(crate) mod args;
pub(crate) mod md;
pub(crate) mod render;
pub(crate) mod rows;

use std::collections::HashSet;
use std::io::IsTerminal;
use std::path::Path;

use crate::cli::{branch_targets, extract_branch_flag};
use crate::cmd::commits::args::{parse_commits_args_with, DateFilter, DateOp, Order};
use crate::cmd::commits::md::{md_filename, write_md, MdHead};
use crate::cmd::commits::render::{render_commits, Highlight};
use crate::cmd::commits::rows::{
    author_match_sets, body_hits, commit_day, commit_files, commit_of, commit_rows, divergent_set,
    equivalents, path_shas, pick_ids, ref_shas, trailer_sets, window_to_divergent, CommitRow,
    FileStat,
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
    // `-b`/`--branch` can ride anywhere in a `commits` line, not only in front
    // of it: 'git-wt commits -b 2' adds worktree 2 to the lone target the bare
    // form already picked, same as spelling it 'git-wt <cur>,2 commits'.
    let (rest, branch) = extract_branch_flag(rest)?;
    let mut idxs = idxs.to_vec();
    if let Some(v) = branch {
        for i in branch_targets(trees, &v)? {
            if !idxs.contains(&i) {
                idxs.push(i);
            }
        }
    }
    commits_view(root, trees, &idxs, &rest, None)
}

/// Print the `dest..src` table for `merge --review`. See `commits_view`.
pub(crate) fn cmd_commits_review(
    root: &Path,
    trees: &[Worktree],
    rest: &[String],
    ctx: ReviewCtx,
) -> Result<(), String> {
    commits_view(root, trees, &[], rest, Some(ctx))
}

/// The two refs a `merge --review` table is about.
///
/// `merge` owns the argument order (dest first) and resolves both sides to
/// refs; this module only needs to know which is which. Labels are carried
/// alongside because the destination may be a worktree and the source a bare
/// branch name, so neither can be recovered from the ref alone.
pub(crate) struct ReviewCtx<'a> {
    pub(crate) dest_ref: &'a str,
    pub(crate) dest_label: &'a str,
    pub(crate) src_ref: &'a str,
    pub(crate) src_label: &'a str,
    /// The verdict line printed above the table.
    ///
    /// Carried in rather than printed by the caller so that it lands *after*
    /// the tail has parsed: `merge --review --dry-run` is a rejected flag, and
    /// a header claiming a clean merge above that error would be reporting on a
    /// command that never ran.
    pub(crate) header: &'a str,
}

/// `cmd_commits`, plus the review mode `merge --review` renders through.
///
/// A review differs in four places and nowhere else, which is why it is a
/// parameter rather than a second command: the rows are the range `dest..src`
/// instead of a branch's log, there is exactly one mark column (the
/// destination's), merge commits are kept by default, and `idxs` is empty
/// because a merge source need not be a worktree at all.
///
/// Everything between -- the filters, the file blocks, the `--md` writer, the
/// renderer -- is the same code the plain table runs. It has been rewritten
/// once already for being duplicated (`cmd_list_with_ref`, commit `9c3237e`),
/// and a third copy is not worth a flag.
fn commits_view(
    root: &Path,
    trees: &[Worktree],
    idxs: &[usize],
    rest: &[String],
    review: Option<ReviewCtx>,
) -> Result<(), String> {
    if idxs.is_empty() && review.is_none() {
        return Err("commits needs a worktree, e.g. 'git-wt 1,2,3 commits'".into());
    }
    for (i, a) in idxs.iter().enumerate() {
        if idxs[i + 1..].contains(a) {
            return Err(format!("worktree #{} listed twice", a + 1));
        }
    }
    // Merges are kept under --review and dropped elsewhere: a review range is
    // bounded by the merge about to happen, so a merge inside it is the cargo
    // rather than the noise it is on a long-lived branch.
    let mut args = parse_commits_args_with(rest, review.is_some())?;
    // Ten rows unless told otherwise: --all and --union both name "give me
    // everything" outright, so a silent cap under either would contradict the
    // flag just asked for. Named otherwise, `-n` already won this fight above.
    if args.limit.is_none() && !args.all && !args.union {
        args.limit = Some(10);
    }
    if let Some(r) = &review {
        eprintln!("{}", r.header);
    }

    // The rows' source. One ref under --review -- the merge source -- with the
    // destination supplied as a `--not` base further down, which is what makes
    // the rows `dest..src`.
    let refs: Vec<String> = match &review {
        Some(r) => vec![r.src_ref.to_string()],
        None => idxs
            .iter()
            .map(|&i| ref_of(&trees[i]))
            .collect::<Result<_, _>>()?,
    };

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
    // One worktree is a log, not a comparison: there is no other branch to be
    // ahead of, so the rows are that branch's whole history and there are no
    // mark columns to compute. A review names one ref too, but it is a
    // comparison -- see `mark_refs`.
    let solo = refs.len() == 1 && review.is_none();

    // The refs the mark columns answer for, which is not always the refs the
    // rows came from.
    //
    // A review has exactly one, the destination, and it carries the only
    // question worth a column: every row is in the source by construction, so a
    // source column would be a `✓` repeating the range's own definition, while
    // the destination's cannot be `✓` at all. What is left there is the split
    // that matters -- `·` for a commit that is genuinely new, `≈` for one whose
    // patch is already in the destination under another sha, which is what a
    // cherry-picked hotfix leaves behind.
    let mark_refs: Vec<String> = match &review {
        Some(r) => vec![r.dest_ref.to_string()],
        None if solo => Vec::new(),
        None => refs.clone(),
    };
    // The set whose earliest member is the default view's floor: commits the
    // first branch has that at least one other is missing. `None` under --union
    // or --all, where the whole log is the rows and nothing is trimmed.
    // A review needs no floor: `dest..src` is already exactly the cut the
    // default view approximates with a date threshold.
    let divergent = if solo || args.union || args.all || review.is_some() {
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
        // `--not dest`: the rows become what the merge would bring over.
        review.as_ref().map(|r| r.dest_ref),
        git_limit,
        order,
        args.fmt,
        !args.merges,
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
    let mut row_files: Vec<Vec<FileStat>> = if args.files || args.squash {
        rows.iter()
            .map(|r| commit_files(root, &r.sha))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };
    // --filename cuts the block to the paths it matched: a merge can carry a
    // hundred files and match on three, and the whole list buries the answer.
    // --all-files widens it back to everything the commit touched, which is the
    // only way the counts sum to the commit again.
    if !args.all_files {
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
            // Not under --review: its rows are `dest..src` exactly, so nothing
            // was trimmed by a floor and there is no wider source to suggest.
            if !solo && !args.all && !args.union && review.is_none() {
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
        } else if review.is_some() {
            // The range itself is empty -- no filter took these rows, there
            // were none. `merge --review` says so in its header, which has
            // already printed, so repeating it here would be the same fact
            // twice in two different wordings.
            return Ok(());
        } else if args.union {
            "no commits".to_string()
        } else if solo || args.all {
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
    //
    // Solo has none: a lone column would be a ✓ on every row, which is only the
    // table repeating that these are that branch's commits.
    let sets: Vec<HashSet<String>> = mark_refs
        .iter()
        .map(|r| ref_shas(root, r, None))
        .collect::<Result<_, _>>()?;

    // Patch equivalence is what tells "not merged yet" from "already there,
    // under a different sha" -- the difference between work to do and work
    // done, which a bare '·' reports as the same thing. It costs a patch-id
    // walk per ordered pair, so --no-cherry buys the old, cheaper answer back
    // on a repo whose branches have diverged enormously.
    // Nothing to be equivalent to with one branch, so solo skips the walk too.
    //
    // The pair the patch comparison runs over is not always the mark columns: a
    // review has one column but two refs to compare, so it walks `[dest, src]`
    // and keeps the destination's answer. `equivalents` indexes by upstream, so
    // that answer is entry 0 -- the source commits whose patch `dest` already
    // carries -- and truncating to `mark_refs` picks it out.
    let cherry_refs: Vec<String> = match &review {
        Some(r) => vec![r.dest_ref.to_string(), r.src_ref.to_string()],
        None => refs.clone(),
    };
    let equiv = if solo || args.no_cherry {
        vec![HashSet::new(); sets.len()]
    } else {
        let mut e = equivalents(root, &cherry_refs);
        e.truncate(mark_refs.len());
        e
    };

    // Which sha the '≈' is pointing at, asked only when the column will print
    // it: it is a second patch-id walk over the same divergence.
    let picks = (args.pick && !solo).then(|| pick_ids(root, &cherry_refs));

    // `-x` trailer detection: a branch whose commit message says it was picked
    // from a row sha gets the row marked `←`. Author fingerprint detection:
    // a branch with the same author/date/subject under a different sha gets
    // the row marked `~`. Both are bounded at the same merge-base as the
    // patch-id walk, and like `equiv` they are truncated to the mark columns
    // (one column under --review, all listed branches otherwise).
    let trailer = if solo {
        vec![HashSet::new(); sets.len()]
    } else {
        let mut t = trailer_sets(root, &cherry_refs);
        t.truncate(mark_refs.len());
        t
    };
    let author_match = if solo {
        vec![HashSet::new(); sets.len()]
    } else {
        let mut a = author_match_sets(root, &cherry_refs, &rows);
        a.truncate(mark_refs.len());
        a
    };

    // Two lists: `labels` is who the table is about, `names` is the mark
    // columns. They are the same until there is only one worktree, which has a
    // subject but nothing to compare it against -- and under --review, where
    // the table is about the source and the one column answers for the
    // destination.
    let labels: Vec<String> = match &review {
        Some(r) => vec![r.src_label.to_string()],
        None => idxs.iter().map(|&i| label(&trees[i])).collect(),
    };
    let names: Vec<String> = match &review {
        Some(r) => vec![r.dest_label.to_string()],
        None if solo => Vec::new(),
        None => labels.clone(),
    };

    if let Some(path) = &args.md {
        let file = path.clone().unwrap_or_else(md_filename);
        // The command as typed, so the file says how to regenerate itself. A
        // review was not spelled `commits`, and echoing it as one would name a
        // command that prints a different table.
        let cmd = match &review {
            Some(r) => format!(
                "git-wt <dest> merge --review{}{}   # {} -> {}",
                if rest.is_empty() { "" } else { " " },
                rest.join(" "),
                r.src_label,
                r.dest_label
            ),
            None => format!(
                "git-wt {} commits{}{}",
                idxs.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(","),
                if rest.is_empty() { "" } else { " " },
                rest.join(" ")
            ),
        };
        return write_md(
            Path::new(&file),
            &rows,
            &row_files,
            &row_bodies,
            &labels,
            &names,
            &sets,
            &equiv,
            &trailer,
            &author_match,
            picks.as_ref(),
            args.squash,
            &cmd,
            &if review.is_some() { MdHead::review() } else { MdHead::commits() },
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
        &trailer,
        &author_match,
        picks.as_ref(),
        args.squash,
        color_enabled(tty),
        term_width(tty),
        args.wrap,
        args.subjectw,
        args.branchw,
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
