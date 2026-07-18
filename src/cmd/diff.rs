use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::git::{git_cmd, git_stdout};
use crate::ui::{color_enabled, paint, DIM, GREEN, RED, RESET, YELLOW};
use crate::worktree::{is_dirty, label, ref_of, Worktree};

// ---------------------------------------------------------------------------
// Diff: git-wt <N>,<M> diff [..|...] [flags] [-- PATH...]
// ---------------------------------------------------------------------------

/// Diff two worktrees, as `git diff <ref1><dots><ref2>`.
///
/// Refs, not directories: a directory diff would drag in build output and
/// everything else .gitignore exists to hide. That also means uncommitted work
/// is invisible here, so warn when either side is dirty and point at meld.
pub(crate) fn cmd_diff(
    root: &Path,
    trees: &[Worktree],
    idxs: &[usize],
    rest: &[String],
) -> Result<(), String> {
    let (idx, other) = match idxs {
        [a, b] => (*a, *b),
        _ => {
            return Err(format!(
                "diff takes exactly two worktrees, got {}\nhint: 'git-wt 1,2,3 meld' compares three",
                idxs.len()
            ));
        }
    };
    if other == idx {
        return Err(format!(
            "worktree #{} against itself is always empty",
            idx + 1
        ));
    }

    let a = ref_of(&trees[idx])?;
    let b = ref_of(&trees[other])?;

    // rather than becoming a flag with a new name to learn. `live`/`hunks` are
    // bare words for the same reason `..` is: they read as part of the sentence.
    // A pathspec can never be mistaken for one, since pathspecs follow `--`.
    // Settled before the main pass, so the unknown-argument hint below is right
    // whatever the word order: '1,2 diff -w live' must not be told to go run a
    // ref diff. Stops at `--`, where a *pathspec* named 'live' could begin.
    let live = rest
        .iter()
        .take_while(|a| a.as_str() != "--")
        .any(|a| a == "live" || a == "--live");

    let mut dots: Option<&str> = None;
    let mut hunks = false;
    let mut listing: Option<String> = None;
    let mut paths: Vec<String> = Vec::new();
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            ".." => dots = Some(".."),
            "..." => dots = Some("..."),
            // Already counted by the pre-scan.
            "live" | "--live" => {}
            "hunks" | "--hunks" => hunks = true,
            // Everything past `--` is a pathspec; git validates it, not us.
            "--" => {
                paths.extend(it.cloned());
                break;
            }
            "--name-only" | "--name-status" | "--stat" => listing = Some(arg.clone()),
            unknown => {
                // Under `live` there is no single git command to hand off to --
                // that is the whole reason `live` exists -- so pointing at one
                // would contradict the mode the user is already in.
                let hint = if live {
                    "hint: live has no git equivalent to defer to; \
                     'git diff --no-index <dir A>/<file> <dir B>/<file>' is the \
                     closest, one file at a time"
                        .to_string()
                } else {
                    let d = dots.unwrap_or("...");
                    format!(
                        "hint: for any other git flag, run git itself: \
                         git diff {a}{d}{b} {unknown}"
                    )
                };
                return Err(format!(
                    "unexpected argument '{unknown}' for diff\n\
                     diff takes live, hunks, .., ..., --name-only, --name-status, \
                     --stat, -- PATH...\n\
                     {hint}"
                ));
            }
        }
    }

    // A range is a statement about refs. `live` never looks at a ref, so the
    // two cannot both be honored -- silently dropping one would be worse.
    if live {
        if let Some(d) = dots {
            return Err(format!(
                "'live' and '{d}' cannot combine: a range compares commits, \
                 live compares the files on disk\n\
                 hint: drop '{d}' for live contents, or drop 'live' for the range"
            ));
        }
    }
    if let (true, Some(l)) = (hunks, listing.as_deref()) {
        return Err(format!(
            "'hunks' and '{l}' cannot combine: hunks prints line numbers per file, \
             {l} prints a listing"
        ));
    }

    let on_err = color_enabled(std::io::stderr().is_terminal());
    // `live` is the answer to the dirty warning, so it does not get warned at.
    if !live {
        for &i in &[idx, other] {
            if is_dirty(&trees[i].path) {
                eprintln!(
                    "{} #{} {} has uncommitted changes; this diff is committed state only \
                     (try 'git-wt {},{} diff live')",
                    paint("warning:", YELLOW, on_err),
                    i + 1,
                    label(&trees[i]),
                    idx + 1,
                    other + 1
                );
            }
        }
    }

    // '...' by default so a bare '1,2 diff' previews '1,2 merge': the range
    // holds M's commits since the fork and nothing of N's, which is what the
    // merge brings in. '..' answers a different question -- tip vs tip -- and
    // reports N's own commits as deletions, which reads as a huge phantom diff
    // on branches that have diverged at all.
    let dots = dots.unwrap_or("...");
    if live {
        let files = live_diff(
            root,
            &trees[idx].path,
            &trees[other].path,
            &paths,
            // --name-only/--name-status answer "which files", which the byte
            // compare already knows -- no per-file git process needed. Every
            // other view prints counts, which only the patch can supply.
            !matches!(listing.as_deref(), Some("--name-only") | Some("--name-status")),
        )?;
        let head = format!("diff {a} ↔ {b}   live — literal contents, .gitignore honored");
        return render(&files, &head, listing.as_deref(), hunks);
    }
    if hunks {
        let files = ref_diff(root, &format!("{a}{dots}{b}"), &paths)?;
        let head = format!("diff {a} ↔ {b}   {a}{dots}{b} — committed state");
        return render(&files, &head, None, true);
    }

    let mut argv: Vec<String> = Vec::new();
    if let Some(l) = &listing {
        argv.push(l.clone());
    }
    if !paths.is_empty() {
        argv.push("--".into());
        argv.extend(paths);
    }

    // Inherit stdio so git's own pager and color logic apply, exactly as a
    // hand-typed `git diff` would.
    let status = git_cmd(root, &[])
        .arg("diff")
        .arg(format!("{a}{dots}{b}"))
        .args(&argv)
        .status()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !status.success() {
        return Err("git diff exited with an error".into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// live: compare worktrees by file content instead of by commit
// ---------------------------------------------------------------------------

/// One hunk, reduced to what the `hunks` view prints: where it lands on the
/// `+` side, and what kind of change it is.
pub(crate) struct Hunk {
    pub(crate) line: usize,
    pub(crate) kind: &'static str,
    pub(crate) count: usize,
}

/// One differing path. `status` is A/M/D from the union of both sides, so a
/// file that is untracked-and-new on the `+` side is genuinely an add.
pub(crate) struct FileDiff {
    pub(crate) path: String,
    pub(crate) status: char,
    pub(crate) plus: usize,
    pub(crate) minus: usize,
    pub(crate) binary: bool,
    pub(crate) hunks: Vec<Hunk>,
}

/// Paths worth considering in a worktree: tracked, plus untracked that
/// `.gitignore` does not hide. Only git knows this set -- `diff -rq` would
/// drown in `target/`. `-z` because a path may contain anything but NUL.
pub(crate) fn live_files(dir: &Path, paths: &[String]) -> Result<Vec<String>, String> {
    let mut args: Vec<&str> = vec!["ls-files", "-z", "--cached", "--others", "--exclude-standard"];
    if !paths.is_empty() {
        args.push("--");
        args.extend(paths.iter().map(String::as_str));
    }
    let out = git_stdout(dir, &args)?;
    Ok(out
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}

/// Byte-for-byte equality. Length first, so unequal files usually cost one
/// `stat` each. An unreadable file counts as differing: better to show it and
/// let the diff report why than to silently call it unchanged.
pub(crate) fn same_bytes(a: &Path, b: &Path) -> bool {
    match (a.metadata(), b.metadata()) {
        (Ok(ma), Ok(mb)) if ma.len() != mb.len() => return false,
        (Ok(_), Ok(_)) => {}
        _ => return false,
    }
    match (std::fs::read(a), std::fs::read(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// The union of both worktrees' candidate paths, filtered to those that
/// actually differ on disk. With `content`, each survivor also gets a
/// `git diff --no-index` run for its counts and hunks.
pub(crate) fn live_diff(
    root: &Path,
    a_dir: &Path,
    b_dir: &Path,
    paths: &[String],
    content: bool,
) -> Result<Vec<FileDiff>, String> {
    let mut union: Vec<String> = live_files(a_dir, paths)?;
    union.extend(live_files(b_dir, paths)?);
    union.sort();
    union.dedup();

    let mut out = Vec::new();
    for p in union {
        let pa = a_dir.join(&p);
        let pb = b_dir.join(&p);
        // `--cached` lists index entries, so a path can be listed on a side
        // where the file is gone. Absent from both is nothing to report.
        let (ea, eb) = (pa.is_file(), pb.is_file());
        let status = match (ea, eb) {
            (false, false) => continue,
            (false, true) => 'A',
            (true, false) => 'D',
            (true, true) => {
                if same_bytes(&pa, &pb) {
                    continue;
                }
                'M'
            }
        };
        let mut fd = FileDiff {
            path: p,
            status,
            plus: 0,
            minus: 0,
            binary: false,
            hunks: Vec::new(),
        };
        if content {
            // Substituting /dev/null for the missing side turns a one-sided
            // file into real hunks instead of an error.
            let null = PathBuf::from("/dev/null");
            let text = no_index_diff(
                root,
                if ea { &pa } else { &null },
                if eb { &pb } else { &null },
                &fd.path,
            )?;
            parse_patch_into(&text, &mut fd);
        }
        out.push(fd);
    }
    Ok(out)
}

/// `git diff --no-index` on two literal paths: git ignoring that it is git.
/// It exits 1 to mean "they differ", which is the expected case here, so only
/// a code above 1 is a real failure. `show` names the path for errors, since
/// `a`/`b` may be absolute, or /dev/null for a one-sided file.
///
/// `root` is only the process's cwd; `--no-index` resolves the two paths
/// itself and never consults a repo, so any existing directory would do.
pub(crate) fn no_index_diff(root: &Path, a: &Path, b: &Path, show: &str) -> Result<String, String> {
    let out = git_cmd(root, &["diff", "--no-index", "-U0", "--no-color"])
        .arg(a)
        .arg(b)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    match out.status.code() {
        Some(0) | Some(1) => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
        _ => Err(format!(
            "git diff --no-index failed on '{show}': {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
    }
}

/// The `hunks` view over a ref diff. Line numbers are just as useful against
/// commits, so `hunks` does not require `live`.
pub(crate) fn ref_diff(root: &Path, range: &str, paths: &[String]) -> Result<Vec<FileDiff>, String> {
    // A rename reported as one entry would have no single `+` side to number;
    // --no-renames splits it back into the add and the delete `live` would
    // have seen anyway, so the two views agree.
    let mut args: Vec<&str> = vec!["diff", "-U0", "--no-color", "--no-renames", range];
    if !paths.is_empty() {
        args.push("--");
        args.extend(paths.iter().map(String::as_str));
    }
    let text = git_stdout(root, &args)?;
    Ok(split_patch(&text))
}

/// Split a multi-file patch on its `diff --git` headers. The path comes from
/// the `+++ b/` line, falling back to `--- a/` for a deletion, where the `+`
/// side is /dev/null.
pub(crate) fn split_patch(text: &str) -> Vec<FileDiff> {
    let mut out: Vec<FileDiff> = Vec::new();
    let mut cur: Option<FileDiff> = None;
    let mut in_hunks = false;
    for line in text.lines() {
        if line.starts_with("diff --git ") {
            if let Some(f) = cur.take() {
                out.push(f);
            }
            cur = Some(FileDiff {
                path: String::new(),
                status: 'M',
                plus: 0,
                minus: 0,
                binary: false,
                hunks: Vec::new(),
            });
            in_hunks = false;
            continue;
        }
        let Some(f) = cur.as_mut() else { continue };
        if !in_hunks {
            if let Some(p) = line.strip_prefix("--- ") {
                if p == "/dev/null" {
                    f.status = 'A';
                } else if f.path.is_empty() {
                    f.path = p.strip_prefix("a/").unwrap_or(p).to_string();
                }
                continue;
            }
            if let Some(p) = line.strip_prefix("+++ ") {
                if p == "/dev/null" {
                    f.status = 'D';
                } else {
                    f.path = p.strip_prefix("b/").unwrap_or(p).to_string();
                }
                continue;
            }
        }
        if line.starts_with("@@") {
            in_hunks = true;
        }
        eat_patch_line(line, f);
    }
    if let Some(f) = cur.take() {
        out.push(f);
    }
    out
}

/// Fold one file's `-U0` patch into `fd`'s counts and hunks.
pub(crate) fn parse_patch_into(text: &str, fd: &mut FileDiff) {
    let mut in_hunks = false;
    for line in text.lines() {
        if line.starts_with("@@") {
            in_hunks = true;
        }
        // The `---`/`+++` headers are +/- lines to a naive counter; skipping
        // everything before the first `@@` keeps them out of the totals.
        if !in_hunks && !line.starts_with("Binary files ") {
            continue;
        }
        eat_patch_line(line, fd);
    }
}

pub(crate) fn eat_patch_line(line: &str, fd: &mut FileDiff) {
    if line.starts_with("Binary files ") {
        fd.binary = true;
        return;
    }
    if line.starts_with("@@") {
        if let Some(h) = parse_hunk_header(line) {
            fd.hunks.push(h);
        }
        return;
    }
    if line.starts_with('+') {
        fd.plus += 1;
    } else if line.starts_with('-') {
        fd.minus += 1;
    }
}

/// `@@ -oldStart,oldCount +newStart,newCount @@`. Two traps live here: an
/// omitted count means 1, and a zero count is not an edit -- `old == 0` is a
/// pure insertion, `new == 0` a pure deletion. Labeling off the new-side
/// number alone would report every deletion as `+0`.
pub(crate) fn parse_hunk_header(line: &str) -> Option<Hunk> {
    let mut it = line.split_whitespace();
    it.next()?; // @@
    let (_, old_count) = parse_range(it.next()?)?;
    let (new_start, new_count) = parse_range(it.next()?)?;
    let (kind, count) = match (old_count, new_count) {
        (0, n) => ("added", n),
        (o, 0) => ("deleted", o),
        (_, n) => ("modified", n),
    };
    Some(Hunk {
        line: new_start,
        kind,
        count,
    })
}

/// `-119,3` / `+119` -> (start, count). No comma means a count of 1.
pub(crate) fn parse_range(tok: &str) -> Option<(usize, usize)> {
    let body = tok.strip_prefix('-').or_else(|| tok.strip_prefix('+'))?;
    match body.split_once(',') {
        Some((s, c)) => Some((s.parse().ok()?, c.parse().ok()?)),
        None => Some((body.parse().ok()?, 1)),
    }
}

// ---------------------------------------------------------------------------
// live: output
// ---------------------------------------------------------------------------

pub(crate) fn status_paint(s: char) -> &'static str {
    match s {
        'A' => GREEN,
        'D' => RED,
        _ => YELLOW,
    }
}

pub(crate) fn render(
    files: &[FileDiff],
    head: &str,
    listing: Option<&str>,
    hunks: bool,
) -> Result<(), String> {
    let on = color_enabled(std::io::stdout().is_terminal());

    // Silence is the right answer for "nothing differs", but on stdout it is
    // indistinguishable from the empty ref diff `live` exists to fix. Say so
    // on stderr, where it cannot corrupt a pipe -- from every view, so that
    // "no output" never means two different things depending on the flags.
    if files.is_empty() {
        eprintln!("no differences");
        return Ok(());
    }

    match listing {
        Some("--name-only") => {
            for f in files {
                println!("{}", f.path);
            }
            return Ok(());
        }
        Some("--name-status") => {
            for f in files {
                println!("{}\t{}", f.status, f.path);
            }
            return Ok(());
        }
        Some("--stat") => return render_stat(files, on),
        _ => {}
    }

    println!("{}\n", paint(head, DIM, on));
    let w = files.iter().map(|f| f.path.len()).max().unwrap_or(0);
    let pw = files
        .iter()
        .map(|f| format!("+{}", f.plus).len())
        .max()
        .unwrap_or(1);
    for f in files {
        let counts = if f.binary {
            "binary".to_string()
        } else {
            format!(
                "{:<pw$} {}",
                paint(&format!("+{}", f.plus), GREEN, on),
                paint(&format!("−{}", f.minus), RED, on),
                // `{:<n}` pads to a byte count, and paint() added bytes that
                // occupy no columns: "\x1b[" + GREEN + "m" ... RESET. Hence
                // +3 -- the two bytes of "\x1b[" plus the "m".
                pw = pw + if on { GREEN.len() + RESET.len() + 3 } else { 0 }
            )
        };
        println!(
            "{} {:<w$}  {}",
            paint(&f.status.to_string(), status_paint(f.status), on),
            f.path,
            counts,
            w = w
        );
        if hunks {
            // Right-align to this file's widest line number so the numbers
            // form a column, without padding every file out to a fixed width.
            let lw = f
                .hunks
                .iter()
                .map(|h| h.line.to_string().len())
                .max()
                .unwrap_or(1);
            for h in &f.hunks {
                println!("      {:>lw$}  {} {}", h.line, h.kind, h.count, lw = lw);
            }
        }
    }
    println!("\n{}", paint(&summary(files), DIM, on));
    Ok(())
}

/// `git diff --stat`'s shape: a churn bar per file, scaled so the widest row
/// fits, then the same summary line.
pub(crate) fn render_stat(files: &[FileDiff], on: bool) -> Result<(), String> {
    const BAR: usize = 40;
    let w = files.iter().map(|f| f.path.len()).max().unwrap_or(0);
    let max = files
        .iter()
        .map(|f| f.plus + f.minus)
        .max()
        .unwrap_or(0)
        .max(1);
    let nw = files
        .iter()
        .map(|f| (f.plus + f.minus).to_string().len())
        .max()
        .unwrap_or(1);
    for f in files {
        if f.binary {
            println!(" {:<w$} | Bin", f.path, w = w);
            continue;
        }
        let total = f.plus + f.minus;
        // Scale only when the widest row would overflow, so small diffs show
        // their exact churn one character per line, as git does.
        let cell = |n: usize| -> usize {
            if max <= BAR {
                n
            } else if n == 0 {
                0
            } else {
                (n * BAR / max).max(1)
            }
        };
        // An empty run must stay empty: painting "" would emit a colour code
        // wrapping nothing, which is invisible but real bytes on the pipe.
        let run = |n: usize, ch: &str, col: &str| match n {
            0 => String::new(),
            _ => paint(&ch.repeat(n), col, on),
        };
        let bar = format!(
            "{}{}",
            run(cell(f.plus), "+", GREEN),
            run(cell(f.minus), "-", RED)
        );
        println!(" {:<w$} | {:>nw$} {}", f.path, total, bar, w = w, nw = nw);
    }
    println!("{}", paint(&summary(files), DIM, on));
    Ok(())
}

/// git's own phrasing, singulars and all, so the line reads the same as the
/// `--stat` a user would get once the work is committed.
pub(crate) fn summary(files: &[FileDiff]) -> String {
    let p: usize = files.iter().map(|f| f.plus).sum();
    let m: usize = files.iter().map(|f| f.minus).sum();
    let mut s = format!(
        "{} file{} changed",
        files.len(),
        if files.len() == 1 { "" } else { "s" }
    );
    if p > 0 {
        s += &format!(", {p} insertion{}(+)", if p == 1 { "" } else { "s" });
    }
    if m > 0 {
        s += &format!(", {m} deletion{}(-)", if m == 1 { "" } else { "s" });
    }
    s
}
