pub(crate) mod args;

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::cmd::diff::args::DiffArgs;
use crate::git::{git_cmd, git_stdout};
use crate::ui::{color_enabled, paint, DIM, GREEN, RED, RESET, YELLOW};
use crate::worktree::{is_dirty, label, ref_of, Worktree};

pub(crate) fn cmd_diff(root: &Path, trees: &[Worktree], idxs: &[usize], args: &DiffArgs) -> Result<(), String> {
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
        return Err(format!("worktree #{} against itself is always empty", idx + 1));
    }

    let a = ref_of(&trees[idx])?;
    let b = ref_of(&trees[other])?;
    let rest = &args.rest;

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
            "live" | "--live" => {}
            "hunks" | "--hunks" => hunks = true,
            "--" => {
                paths.extend(it.cloned());
                break;
            }
            "--name-only" | "--name-status" | "--stat" => listing = Some(arg.clone()),
            unknown => {
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

    let dots = dots.unwrap_or("...");
    if live {
        let files = live_diff(
            root,
            &trees[idx].path,
            &trees[other].path,
            &paths,
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

pub(crate) struct Hunk {
    pub(crate) line: usize,
    pub(crate) kind: &'static str,
    pub(crate) count: usize,
}

pub(crate) struct FileDiff {
    pub(crate) path: String,
    pub(crate) status: char,
    pub(crate) plus: usize,
    pub(crate) minus: usize,
    pub(crate) binary: bool,
    pub(crate) hunks: Vec<Hunk>,
}

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

pub(crate) fn ref_diff(root: &Path, range: &str, paths: &[String]) -> Result<Vec<FileDiff>, String> {
    let mut args: Vec<&str> = vec!["diff", "-U0", "--no-color", "--no-renames", range];
    if !paths.is_empty() {
        args.push("--");
        args.extend(paths.iter().map(String::as_str));
    }
    let text = git_stdout(root, &args)?;
    Ok(split_patch(&text))
}

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

pub(crate) fn parse_patch_into(text: &str, fd: &mut FileDiff) {
    let mut in_hunks = false;
    for line in text.lines() {
        if line.starts_with("@@") {
            in_hunks = true;
        }
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
    Some(Hunk { line: new_start, kind, count })
}

pub(crate) fn parse_range(tok: &str) -> Option<(usize, usize)> {
    let body = tok.strip_prefix('-').or_else(|| tok.strip_prefix('+'))?;
    match body.split_once(',') {
        Some((s, c)) => Some((s.parse().ok()?, c.parse().ok()?)),
        None => Some((body.parse().ok()?, 1)),
    }
}

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
    let w = files.iter().map(|f| f.path.chars().count()).max().unwrap_or(0);
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

pub(crate) fn render_stat(files: &[FileDiff], on: bool) -> Result<(), String> {
    const BAR: usize = 40;
    let w = files.iter().map(|f| f.path.chars().count()).max().unwrap_or(0);
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
        let cell = |n: usize| -> usize {
            if max <= BAR {
                n
            } else if n == 0 {
                0
            } else {
                (n * BAR / max).max(1)
            }
        };
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

pub(crate) fn summary(files: &[FileDiff]) -> String {
    let p: usize = files.iter().map(|f| f.plus).sum();
    let m: usize = files.iter().map(|f| f.minus).sum();
    let mut s = format!("{} file{} changed", files.len(), if files.len() == 1 { "" } else { "s" });
    if p > 0 {
        s += &format!(", {p} insertion{}(+)", if p == 1 { "" } else { "s" });
    }
    if m > 0 {
        s += &format!(", {m} deletion{}(-)", if m == 1 { "" } else { "s" });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hunk(line: &str) -> (usize, &'static str, usize) {
        let h = parse_hunk_header(line).expect("header should parse");
        (h.line, h.kind, h.count)
    }

    #[test]
    fn omitted_hunk_count_means_one() {
        assert_eq!(hunk("@@ -119 +119 @@"), (119, "modified", 1));
        assert_eq!(parse_range("-119"), Some((119, 1)));
        assert_eq!(parse_range("+42,7"), Some((42, 7)));
    }

    #[test]
    fn zero_hunk_count_is_not_an_edit() {
        assert_eq!(hunk("@@ -0,0 +290,2 @@"), (290, "added", 2));
        assert_eq!(hunk("@@ -5,3 +4,0 @@"), (4, "deleted", 3));
        assert_eq!(hunk("@@ -119,3 +119,5 @@ fn x() {"), (119, "modified", 5));
    }

    #[test]
    fn patch_counts_skip_the_file_headers() {
        let patch = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1,2 @@\n-old\n+new\n+extra\n";
        let mut fd = FileDiff {
            path: "x".into(),
            status: 'M',
            plus: 0,
            minus: 0,
            binary: false,
            hunks: Vec::new(),
        };
        parse_patch_into(patch, &mut fd);
        assert_eq!((fd.plus, fd.minus), (2, 1));
        assert_eq!(fd.hunks.len(), 1);
    }

    #[test]
    fn patch_splits_by_file_and_reads_status_from_dev_null() {
        let patch = "\
diff --git a/add.txt b/add.txt
--- /dev/null
+++ b/add.txt
@@ -0,0 +1 @@
+hi
diff --git a/gone.txt b/gone.txt
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
";
        let files = split_patch(patch);
        assert_eq!(files.len(), 2);
        assert_eq!((files[0].path.as_str(), files[0].status), ("add.txt", 'A'));
        assert_eq!((files[0].plus, files[0].minus), (1, 0));
        assert_eq!((files[1].path.as_str(), files[1].status), ("gone.txt", 'D'));
        assert_eq!((files[1].plus, files[1].minus), (0, 1));
    }

    #[test]
    fn binary_patch_reports_no_counts() {
        let mut fd = FileDiff {
            path: "i.png".into(),
            status: 'M',
            plus: 0,
            minus: 0,
            binary: false,
            hunks: Vec::new(),
        };
        parse_patch_into("Binary files a/i.png and b/i.png differ\n", &mut fd);
        assert!(fd.binary);
        assert_eq!((fd.plus, fd.minus), (0, 0));
    }

    #[test]
    fn summary_matches_gits_phrasing() {
        let f = |p, m| FileDiff {
            path: "x".into(),
            status: 'M',
            plus: p,
            minus: m,
            binary: false,
            hunks: Vec::new(),
        };
        assert_eq!(
            summary(&[f(90, 10), f(345, 38), f(73, 4)]),
            "3 files changed, 508 insertions(+), 52 deletions(-)"
        );
        assert_eq!(summary(&[f(1, 1)]), "1 file changed, 1 insertion(+), 1 deletion(-)");
        assert_eq!(summary(&[f(0, 2)]), "1 file changed, 2 deletions(-)");
    }
}
