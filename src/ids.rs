//! Stable worktree numbers.
//!
//! The numbers users type (`git-wt 2 switch`) used to be positions in `git
//! worktree list` output, so removing one renumbered every worktree after it.
//! Here each worktree keeps the number it was first given, for as long as it
//! exists; a number only comes back into circulation once its worktree is gone,
//! and then it goes to the next worktree added (lowest free number wins).
//!
//! State lives next to the repo's shared git data, one line per worktree:
//!
//! ```text
//! 1\t/path/to/main
//! 4\t/path/to/feat-x
//! ```
//!
//! It is a cache, not a source of truth: a missing, unreadable, or corrupt file
//! only costs the numbering history, never a command. `git worktree list` still
//! decides which worktrees exist.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::git::git_stdout;
use crate::worktree::{Worktree, canon};

/// Where the id map is kept: the repo's common git dir, so every linked
/// worktree reads and writes the same file.
fn id_file(root: &Path) -> Option<PathBuf> {
    let common = git_stdout(root, &["rev-parse", "--path-format=absolute", "--git-common-dir"]).ok()?;
    let common = common.trim();
    if common.is_empty() {
        return None;
    }
    Some(PathBuf::from(common).join("git-wt-ids"))
}

/// Parse the id file into path -> id. Unparsable lines are dropped rather than
/// failing the read: a half-written file should cost one worktree its number,
/// not the whole map.
fn load(file: &Path) -> HashMap<PathBuf, u32> {
    let mut map = HashMap::new();
    let Ok(text) = std::fs::read_to_string(file) else {
        return map;
    };
    for line in text.lines() {
        let Some((id, path)) = line.split_once('\t') else {
            continue;
        };
        let Ok(id) = id.trim().parse::<u32>() else {
            continue;
        };
        if id == 0 || path.is_empty() {
            continue;
        }
        map.insert(PathBuf::from(path), id);
    }
    map
}

/// Write the map back, sorted by id so the file stays readable and diffable.
/// Written to a sibling temp file and renamed, so a crash mid-write leaves the
/// old map intact instead of a truncated one. A failure here only costs future
/// numbering stability, never the current command, so it's a warning, not an
/// error return.
fn save(file: &Path, entries: &[(u32, PathBuf)]) {
    let mut text = String::new();
    for (id, path) in entries {
        text.push_str(&format!("{id}\t{}\n", path.display()));
    }
    let tmp = file.with_extension("tmp");
    if let Err(e) = std::fs::write(&tmp, text) {
        eprintln!("warning: could not save worktree numbers: {e}");
        return;
    }
    if let Err(e) = std::fs::rename(&tmp, file) {
        eprintln!("warning: could not save worktree numbers: {e}");
    }
}

/// The lowest id not in `used`, starting at 1.
fn lowest_free(used: &[u32]) -> u32 {
    let mut sorted = used.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut n = 1;
    for id in sorted {
        if id != n {
            break;
        }
        n += 1;
    }
    n
}

/// Give every worktree its stable number, in place. Order is left exactly as
/// `trees` came in (git's own listing order, main worktree first) -- callers
/// that key off position, like "remove" printing `trees[0]`'s path as the
/// worktree left behind, depend on that. Sort a copy if display wants id order.
///
/// Worktrees already in the map keep their number. New ones take the lowest
/// number no live worktree is using -- which is how a removed worktree's number
/// gets reused, and only then. Entries for worktrees that no longer exist are
/// dropped, which is what frees their numbers.
pub(crate) fn assign(root: &Path, trees: &mut [Worktree]) {
    let file = id_file(root);
    let known = file.as_deref().map(load).unwrap_or_default();
    let known = assign_from(&known, trees);

    if let Some(file) = file {
        let entries: Vec<(u32, PathBuf)> =
            trees.iter().map(|w| (w.id, w.path.clone())).collect();
        // Only touch the file when the mapping actually moved: `list` runs
        // often, and a no-op write would churn mtime on every invocation.
        let same = entries.len() == known.len()
            && entries
                .iter()
                .all(|(id, p)| known.get(&canon(p)) == Some(id));
        if !same {
            save(&file, &entries);
        }
    }
}

/// The pure half of `assign`: no filesystem, no git -- just the id-picking
/// rules, so it can be tested without a repo. Returns `known` re-keyed by
/// canonical path, which the caller reuses to decide whether the file needs
/// rewriting.
fn assign_from(known: &HashMap<PathBuf, u32>, trees: &mut [Worktree]) -> HashMap<PathBuf, u32> {
    // Keys are canonical paths so a worktree keeps its number when git reports
    // it differently than when it was first seen (symlinks, /private on macOS).
    let known: HashMap<PathBuf, u32> = known
        .iter()
        .map(|(p, id)| (canon(p), *id))
        .collect();

    let mut used: Vec<u32> = Vec::new();
    let mut missing: Vec<usize> = Vec::new();
    for (i, w) in trees.iter_mut().enumerate() {
        match known.get(&canon(&w.path)) {
            // A duplicate id in the file (hand-edited, or two repos sharing a
            // path) is treated as unknown for the later worktree, so no two
            // live worktrees can answer to the same number.
            Some(id) if !used.contains(id) => {
                w.id = *id;
                used.push(*id);
            }
            _ => missing.push(i),
        }
    }
    for i in missing {
        let id = lowest_free(&used);
        trees[i].id = id;
        used.push(id);
    }

    known
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowest_free_fills_gaps() {
        assert_eq!(lowest_free(&[]), 1);
        assert_eq!(lowest_free(&[1, 2, 3]), 4);
        assert_eq!(lowest_free(&[1, 3, 4]), 2);
        assert_eq!(lowest_free(&[2, 3]), 1);
    }

    fn wt(path: &str) -> Worktree {
        Worktree {
            id: 0,
            path: PathBuf::from(path),
            branch: None,
            detached: false,
            bare: false,
            locked: None,
            prunable: None,
        }
    }

    #[test]
    fn assign_from_numbers_fresh_worktrees_in_order() {
        let mut trees = vec![wt("/a"), wt("/b"), wt("/c")];
        assign_from(&HashMap::new(), &mut trees);
        assert_eq!(trees.iter().map(|w| w.id).collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn assign_from_keeps_a_known_worktrees_number() {
        let known = HashMap::from([(PathBuf::from("/a"), 4), (PathBuf::from("/b"), 2)]);
        let mut trees = vec![wt("/a"), wt("/b")];
        assign_from(&known, &mut trees);
        assert_eq!(trees[0].id, 4);
        assert_eq!(trees[1].id, 2);
    }

    #[test]
    fn assign_from_gives_a_new_worktree_the_lowest_free_number() {
        // "/b" (id 2) is gone; "/c" is new and should take 2, not 3.
        let known = HashMap::from([(PathBuf::from("/a"), 1), (PathBuf::from("/b"), 2)]);
        let mut trees = vec![wt("/a"), wt("/c")];
        assign_from(&known, &mut trees);
        assert_eq!(trees[0].id, 1);
        assert_eq!(trees[1].id, 2);
    }

    #[test]
    fn assign_from_heals_a_duplicate_id_in_the_map() {
        // Hand-edited (or stale) file gives two live worktrees the same id;
        // the second one loses the tie and is renumbered instead.
        let known = HashMap::from([(PathBuf::from("/a"), 1), (PathBuf::from("/b"), 1)]);
        let mut trees = vec![wt("/a"), wt("/b")];
        assign_from(&known, &mut trees);
        assert_eq!(trees[0].id, 1);
        assert_eq!(trees[1].id, 2);
    }

    #[test]
    fn load_skips_bad_lines() {
        let dir = std::env::temp_dir().join(format!("git-wt-ids-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("ids");
        std::fs::write(&f, "1\t/a\nnope\n0\t/b\nx\t/c\n7\t/d\n").unwrap();
        let map = load(&f);
        assert_eq!(map.get(Path::new("/a")), Some(&1));
        assert_eq!(map.get(Path::new("/d")), Some(&7));
        assert_eq!(map.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
