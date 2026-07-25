pub(crate) mod args;

use std::io::IsTerminal;
use std::path::Path;

use crate::cmd::commits::args::ReviewFlags;
use crate::cmd::commits::rows::commit_files;
use crate::cmd::commits::{cmd_commits_review, ReviewCtx};
use crate::cmd::meld::{extract_files, require_meld, temp_meld_dir};
use crate::cmd::merge::{merge_probe, MergeVerdict};
use crate::git::git_stdout;
use crate::ui::{color_enabled, paint, GREEN};
use crate::worktree::{label, ref_of, Worktree};

/// `git-wt review <SOURCE> [-t <DEST>]`: the commits `dest..src` would bring
/// over, under a verdict line saying whether the merge would be clean.
///
/// Answers a question and changes nothing, so the exit code carries the
/// verdict rather than success: 0 when the merge would be clean, 1 when it
/// would conflict -- the same contract `merge --dry-run` has, which this
/// shares a probe with.
pub(crate) fn cmd_review(
    root: &Path,
    trees: &[Worktree],
    idx: usize,
    src: &str,
    meld: bool,
    flags: ReviewFlags,
) -> Result<(), String> {
    let dest = &trees[idx];
    if dest.bare {
        return Err("cannot review a merge into a bare worktree".into());
    }
    let dir = dest.path.as_path();
    let color = std::io::stderr().is_terminal() && color_enabled(true);

    let dest_ref = ref_of(dest)?;
    let dest_label = label(dest);
    let verdict = merge_probe(dir, src)?;

    if meld {
        review_meld(root, &dest_ref, &dest_label, src)?;
        return match verdict {
            MergeVerdict::Clean => Ok(()),
            MergeVerdict::Conflict(files) => Err(review_conflict_msg(&files)),
        };
    }

    let range = format!("{dest_ref}..{src}");
    let n: usize = git_stdout(dir, &["rev-list", "--count", &range])?
        .trim()
        .parse()
        .unwrap_or(0);

    let plural = if n == 1 { "commit" } else { "commits" };
    let how = match &verdict {
        MergeVerdict::Clean => paint("merges cleanly", GREEN, color),
        MergeVerdict::Conflict(_) => "does NOT merge cleanly".to_string(),
    };
    // Nothing to bring over means there is no merge to have a verdict about,
    // so it says the one true thing rather than "0 commits, merges cleanly"
    // above an empty table -- which reads as though a merge just ran.
    let header = if n == 0 {
        format!("{} {src} is already in {dest_label}", paint("Merged", GREEN, color))
    } else {
        format!("{src} -> {dest_label}   {n} {plural}, {how}")
    };

    cmd_commits_review(
        root,
        trees,
        flags,
        ReviewCtx {
            dest_ref: &dest_ref,
            dest_label: &dest_label,
            src_ref: src,
            src_label: src,
            header: &header,
        },
    )?;

    match verdict {
        MergeVerdict::Clean => Ok(()),
        MergeVerdict::Conflict(files) => Err(review_conflict_msg(&files)),
    }
}

fn review_conflict_msg(files: &[String]) -> String {
    let mut m = format!(
        "{} conflicting path{}:\n",
        files.len(),
        if files.len() == 1 { "" } else { "s" }
    );
    for f in files {
        m.push_str(&format!("  {f}\n"));
    }
    m.push_str("nothing was changed — this was a review");
    m
}

/// `review <SOURCE> --meld`: open meld on the files `dest_ref..src` touches,
/// each side extracted from git (not the worktree's on-disk state), same as
/// `meld --diff` does for two worktrees.
///
/// The path set is the union of each reviewed commit's first-parent diff --
/// exactly the set the `--squash` "consolidated files" block lists, computed
/// through the same `commit_files`. A plain `merge-base..src` tree diff
/// instead nets merges against neither parent, so a merge inside the range
/// drops its whole second-parent import into the set even though the table
/// (first-parent) never shows it: that is what made meld's file list disagree
/// with the consolidated one printed beside it.
fn review_meld(root: &Path, dest_ref: &str, dest_label: &str, src: &str) -> Result<(), String> {
    require_meld()?;

    let mut paths = review_paths(root, dest_ref, src)?;
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        eprintln!("no files differ between {dest_label} and {src}");
        return Ok(());
    }

    let tmp = temp_meld_dir()?;
    let dir_dest = tmp.join("a");
    let dir_src = tmp.join("b");
    let extract_all = || -> Result<(), String> {
        extract_files(root, dest_ref, &paths, &dir_dest)?;
        extract_files(root, src, &paths, &dir_src)
    };
    if let Err(e) = extract_all() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }

    let on = color_enabled(std::io::stderr().is_terminal());
    eprintln!("{} {dest_label} ↔ {src}", paint("meld", GREEN, on));
    eprintln!("  {dest_label}: {}", dir_dest.display());
    eprintln!("  {src}: {}", dir_src.display());

    let status = std::process::Command::new("meld").arg(&dir_dest).arg(&dir_src).status();
    let _ = std::fs::remove_dir_all(&tmp);
    let status = status.map_err(|e| format!("failed to run meld: {e}"))?;
    if !status.success() {
        return Err("meld exited with an error".into());
    }
    Ok(())
}

/// The files the reviewed commits touch, defined exactly as the review
/// table's consolidated block defines them: the union of each commit's
/// first-parent diff over `dest_ref..src`, via the same `commit_files`. So
/// the meld tree and the `--squash` "consolidated files" list name the same
/// paths rather than two subtly different sets.
///
/// A rename's `commit_files` path is `old => new`; both halves are kept, so
/// meld can extract the file under whichever name each side holds it by.
fn review_paths(root: &Path, dest_ref: &str, src: &str) -> Result<Vec<String>, String> {
    let range = format!("{dest_ref}..{src}");
    let shas = git_stdout(root, &["rev-list", &range])?;
    let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for sha in shas.lines().map(str::trim).filter(|s| !s.is_empty()) {
        for f in commit_files(root, sha)? {
            match f.path.split_once(" => ") {
                Some((old, new)) => {
                    set.insert(old.to_string());
                    set.insert(new.to_string());
                }
                None => {
                    set.insert(f.path);
                }
            }
        }
    }
    Ok(set.into_iter().collect())
}
