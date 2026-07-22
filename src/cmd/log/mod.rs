pub(crate) mod paths;
pub(crate) mod render;

use std::collections::HashSet;
use std::io::IsTerminal;
use std::path::Path;

use crate::cli::{branch_targets, extract_branch_flag};
use crate::cmd::commits::args::{parse_commits_args_with, DateFilter, DateOp, Mode, Order};
use crate::cmd::commits::md::{md_filename, write_md, MdHead};
use crate::cmd::commits::render::Highlight;
use crate::cmd::commits::rows::{
    author_match_sets, body_hits, commit_day, commit_files, commit_of, commit_rows, equivalents,
    path_row_stat, pick_ids, ref_shas, trailer_sets, CommitRow, FileStat,
};
use crate::cmd::log::paths::resolve_pathspec;
use crate::cmd::log::render::{render_log, stat_cell};
use crate::ui::{color_enabled, is_subseq, term_width};
use crate::worktree::{label, ref_of, Worktree};

/// Print one file's history, across the listed branches: the same table
/// `commits` renders, with a pathspec selecting the rows instead of a branch
/// range. See `docs/PLAN-file-log.md` for the design this follows.
pub(crate) fn cmd_log(
    root: &Path,
    trees: &[Worktree],
    idxs: &[usize],
    rest: &[String],
) -> Result<(), String> {
    // `-b`/`--branch` rides anywhere in the line, same as `commits`.
    let (rest, branch) = extract_branch_flag(rest)?;
    let mut idxs = idxs.to_vec();
    if let Some(v) = branch {
        for i in branch_targets(trees, &v)? {
            if !idxs.contains(&i) {
                idxs.push(i);
            }
        }
    }
    if idxs.is_empty() {
        return Err("log needs a worktree, e.g. 'git-wt 1,2 log src/ui.rs'".into());
    }
    for (i, a) in idxs.iter().enumerate() {
        if idxs[i + 1..].contains(a) {
            return Err(format!("worktree #{} listed twice", a + 1));
        }
    }

    // PATH... is the leading run of non-flag tokens; everything after is a
    // `commits` option. A path is never in the target slot -- see the plan's
    // "Grammar" section for why -- so it always comes after the verb, and
    // never interleaved with flags.
    let split = rest.iter().take_while(|a| !a.starts_with('-')).count();
    let (path_args, opt_args) = rest.split_at(split);

    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let mut paths: Vec<String> = Vec::new();
    if path_args.is_empty() {
        // PATH omitted = the current directory, repo-relative -- never an
        // empty pathspec, which `resolve_pathspec` already turns into '.'.
        paths.push(resolve_pathspec(root, trees, &cwd, ".")?);
    } else {
        for p in path_args {
            let resolved = resolve_pathspec(root, trees, &cwd, p)?;
            if !paths.contains(&resolved) {
                paths.push(resolved);
            }
        }
    }

    let mut args = parse_commits_args_with(opt_args, Mode::Log)?;
    // Only --union names "give me everything" in `log`: there is no --all to
    // lift the cap the other way, since there is no divergence floor to lift.
    if args.limit.is_none() && !args.union {
        args.limit = Some(10);
    }

    let refs: Vec<String> = idxs
        .iter()
        .map(|&i| ref_of(&trees[i]))
        .collect::<Result<_, _>>()?;
    // Default rows: the first branch's history of the path. --union: every
    // listed branch's, unioned -- and a single `git log refA refB -- path`
    // already gives that union, the same way it does for `commits --union`.
    let row_refs: &[String] = if args.union { &refs } else { &refs[..1] };
    let solo = refs.len() == 1;
    let mark_refs: Vec<String> = if solo { Vec::new() } else { refs.clone() };

    // `--follow` takes exactly one pathspec; with several, --no-follow
    // behavior is what git gives, and that is simply what happens when this
    // stays false.
    let follow = paths.len() == 1 && !args.no_follow;

    let filtered = !args.dates.is_empty()
        || args.commit_since.is_some()
        || args.commit_until.is_some()
        || !args.commits.is_empty()
        || args.author.is_some()
        || args.message.is_some();
    let git_limit = if filtered { None } else { args.limit };
    let order = if args.topo { Order::Topo } else { Order::Date };

    // `--merges` implies the full-history/first-parent-diff walk inside
    // `commit_rows` itself -- the same trap `path_shas` documents, dodged the
    // same way, so a merge that carried the whole file over is not silently
    // pruned out from under a flag that asked to keep it.
    let all_rows = commit_rows(
        root,
        row_refs,
        None,
        git_limit,
        order,
        args.fmt,
        !args.merges,
        args.message.is_some(),
        &paths,
        follow,
    )?;
    let unfiltered = all_rows.len();

    let mut dates: Vec<DateFilter> = Vec::new();
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
    let mut wanted: HashSet<String> = HashSet::new();
    for id in &args.commits {
        let c = commit_of(root, id, "--commits")?;
        wanted.insert(c.clone());
        anchors.insert(c);
    }
    let needle = args.author.as_ref().map(|a| a.to_lowercase());
    let msg = args.message.as_ref().map(|s| s.to_lowercase());

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
        .filter(|r| {
            msg.as_ref().is_none_or(|n| {
                r.text.to_lowercase().contains(n) || r.body.to_lowercase().contains(n)
            })
        })
        .collect();
    if let Some(n) = args.limit {
        rows.truncate(n);
    }
    if args.reverse {
        rows.reverse();
    }

    if rows.is_empty() {
        let path_label = paths.join(", ");
        let msg = if filtered && unfiltered > 0 {
            format!("no commits match those filters: {unfiltered} commits, none kept")
        } else {
            // Not an error: a file deleted on one branch and alive on another
            // is exactly the case this table is for. The rename hint applies
            // regardless of how many branches were listed; --follow already
            // ran (paths.len() == 1) unless --no-follow said not to, so that
            // is the one case the hint has nothing left to offer.
            let on: Vec<String> = idxs.iter().map(|&i| label(&trees[i])).collect();
            let mut m = format!("no commits touched '{path_label}' on {}", on.join(", "));
            if !args.no_follow {
                m.push_str(
                    "\nhint: it may live under another name; --no-follow shows the literal path only",
                );
            }
            m
        };
        eprintln!("{msg}");
        return Ok(());
    }

    // The `±` cell and the name(s) each row's own diff actually touched --
    // the latter is the `path` column's source, on a rename or a multi-path
    // pathspec.
    let mut stats: Vec<(Option<usize>, Option<usize>)> = Vec::with_capacity(rows.len());
    let mut row_path_labels: Vec<String> = Vec::with_capacity(rows.len());
    for r in &rows {
        let (added, removed, names) = path_row_stat(root, &r.sha, &paths)?;
        stats.push((added, removed));
        row_path_labels.push(if names.is_empty() { paths.join(", ") } else { names.join(", ") });
    }
    // Printed only when it varies: otherwise it is the header's job, not a
    // column's.
    let varies = row_path_labels.iter().any(|p| p != &row_path_labels[0]);
    let row_paths: Option<&[String]> = if varies { Some(&row_path_labels) } else { None };

    // -f: the *other* files each of these commits touched -- the blast radius
    // of every change to this path, since the row already carries the path's
    // own `±`.
    let row_files: Vec<Vec<FileStat>> = if args.files {
        rows.iter()
            .map(|r| commit_files(root, &r.sha))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };

    let row_bodies: Vec<(Vec<String>, usize)> = match &msg {
        Some(n) => rows.iter().map(|r| body_hits(&r.body, n)).collect(),
        None => Vec::new(),
    };

    let sets: Vec<HashSet<String>> = mark_refs
        .iter()
        .map(|r| ref_shas(root, r, None))
        .collect::<Result<_, _>>()?;
    let cherry_refs: Vec<String> = refs.clone();
    let equiv = if solo || args.no_cherry {
        vec![HashSet::new(); sets.len()]
    } else {
        let mut e = equivalents(root, &cherry_refs);
        e.truncate(mark_refs.len());
        e
    };
    let picks = (args.pick && !solo).then(|| pick_ids(root, &cherry_refs));
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

    let labels: Vec<String> = idxs.iter().map(|&i| label(&trees[i])).collect();
    let names: Vec<String> = if solo { Vec::new() } else { labels.clone() };

    // The header: path, branches, and the shown rows' totals. `--squash`
    // promotes it to the path's lifetime totals by adding first/last touch --
    // the same rows, just the fuller reading of them.
    let mut authors: HashSet<&str> = HashSet::new();
    let mut added_total = Some(0usize);
    let mut removed_total = Some(0usize);
    for (r, (a, rm)) in rows.iter().zip(stats.iter()) {
        authors.insert(r.author.as_str());
        added_total = match (added_total, a) {
            (Some(x), Some(y)) => Some(x + y),
            _ => None,
        };
        removed_total = match (removed_total, rm) {
            (Some(x), Some(y)) => Some(x + y),
            _ => None,
        };
    }
    let fmt_total = |n: Option<usize>, sign: char| n.map(|n| format!("{sign}{n}")).unwrap_or_else(|| "-".to_string());
    let mut header = format!(
        "{}   {}   {} commit{}, {} {}, {} author{}",
        paths.join(", "),
        labels.join(", "),
        rows.len(),
        if rows.len() == 1 { "" } else { "s" },
        fmt_total(added_total, '+'),
        fmt_total(removed_total, '-'),
        authors.len(),
        if authors.len() == 1 { "" } else { "s" },
    );
    if args.squash {
        // Newest-first is the rows' own order, whatever --reverse asked for
        // was applied to; "first"/"last" name the calendar, not the row order.
        let (first, last) = rows
            .iter()
            .map(|r| r.key.as_str())
            .fold((None, None), |(f, l): (Option<&str>, Option<&str>), k| {
                (Some(f.map_or(k, |f| f.min(k))), Some(l.map_or(k, |l| l.max(k))))
            });
        header.push_str(&format!(
            ", first {}, last {}",
            first.unwrap_or(""),
            last.unwrap_or("")
        ));
    }
    eprintln!("{header}");

    if let Some(path) = &args.md {
        let file = path.clone().unwrap_or_else(md_filename);
        let cmd = format!(
            "git-wt {} log {}{}{}",
            idxs.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(","),
            paths.join(" "),
            if opt_args.is_empty() { "" } else { " " },
            opt_args.join(" ")
        );
        // The `±` cell and (when it varies) the path, folded into the subject
        // text: `write_md` renders whatever `CommitRow.text` holds, and a
        // second column shape for `log` alone is not worth a second writer.
        let decorated: Vec<CommitRow> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let mut d = r.clone();
                let mut prefix = format!("`{}` ", stat_cell(stats[i].0, stats[i].1));
                if let Some(rp) = row_paths {
                    prefix.push_str(&format!("`{}` ", rp[i]));
                }
                d.text = format!("{prefix}{}", d.text);
                d
            })
            .collect();
        return write_md(
            Path::new(&file),
            &decorated,
            &row_files,
            &row_bodies,
            &labels,
            &names,
            &sets,
            &equiv,
            &trailer,
            &author_match,
            picks.as_ref(),
            false,
            &cmd,
            &MdHead::log(),
        );
    }

    let tty = std::io::stdout().is_terminal();
    render_log(
        &rows,
        &stats,
        row_paths,
        &row_files,
        &row_bodies,
        &names,
        &sets,
        &equiv,
        &trailer,
        &author_match,
        picks.as_ref(),
        color_enabled(tty),
        term_width(tty),
        args.wrap,
        args.subjectw,
        args.branchw,
        &Highlight {
            date: !args.dates.is_empty(),
            author: args.author.is_some(),
            shas: anchors,
            message: msg.clone(),
            file: None,
        },
    );
    Ok(())
}
