pub(crate) mod args;

use std::io::IsTerminal;
use std::path::Path;

use crate::cmd::commits::{cmd_commits_review, ReviewCtx};
use crate::git::{git_cmd, git_quiet, git_run, git_run_no_editor, git_stdout};
use crate::ui::{color_enabled, confirm, paint, GREEN};
use crate::worktree::{label, leaf_of, ref_of, Worktree};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MergeOp {
    Start(String),
    Continue,
    Abort,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    Ours,
    Theirs,
}

impl Side {
    pub(crate) fn word(self) -> &'static str {
        match self {
            Side::Ours => "ours",
            Side::Theirs => "theirs",
        }
    }

    pub(crate) fn strategy_option(self) -> &'static str {
        self.word()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct MergeParsedArgs {
    pub(crate) op: MergeOp,
    pub(crate) message: Option<String>,
    pub(crate) no_ff: bool,
    pub(crate) ff_only: bool,
    pub(crate) squash: bool,
    pub(crate) force: bool,
    pub(crate) side: Option<Side>,
    pub(crate) dry_run: bool,
    pub(crate) review: Option<Vec<String>>,
}

pub(crate) fn set_side(slot: &mut Option<Side>, side: Side) -> Result<(), String> {
    match slot {
        Some(s) if *s == side => Ok(()),
        Some(_) => Err("ours and theirs conflict".into()),
        None => {
            *slot = Some(side);
            Ok(())
        }
    }
}

pub(crate) fn set_merge_op(slot: &mut Option<MergeOp>, op: MergeOp) -> Result<(), String> {
    match slot {
        Some(cur) if *cur == op => Ok(()),
        Some(_) => Err("continue and abort conflict".into()),
        None => {
            *slot = Some(op);
            Ok(())
        }
    }
}

pub(crate) fn start_only_flags(
    message: Option<&String>,
    no_ff: bool,
    ff_only: bool,
    squash: bool,
    force: bool,
) -> Vec<&'static str> {
    let mut v = Vec::new();
    if message.is_some() {
        v.push("-m");
    }
    if no_ff {
        v.push("--no-ff");
    }
    if ff_only {
        v.push("--ff-only");
    }
    if squash {
        v.push("--squash");
    }
    if force {
        v.push("-f");
    }
    v
}

pub(crate) fn parse_merge_args(args: &[String]) -> Result<MergeParsedArgs, String> {
    let mut source: Option<String> = None;
    let mut op: Option<MergeOp> = None;
    let mut message = None;
    let mut side: Option<Side> = None;
    let (mut no_ff, mut ff_only, mut squash, mut force, mut dry_run) =
        (false, false, false, false, false);
    let mut review: Option<Vec<String>> = None;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "review" | "--review" => {
                review = Some(it.by_ref().cloned().collect());
                break;
            }
            "continue" | "--continue" | "-c" => set_merge_op(&mut op, MergeOp::Continue)?,
            "abort" | "--abort" | "-a" => set_merge_op(&mut op, MergeOp::Abort)?,
            "ours" | "--ours" | "-o" => set_side(&mut side, Side::Ours)?,
            "theirs" | "--theirs" | "-t" => set_side(&mut side, Side::Theirs)?,
            "dry-run" | "--dry-run" | "-d" => dry_run = true,
            "-m" | "--message" => {
                message = Some(it.next().ok_or("--message needs a message")?.clone());
            }
            s if s.starts_with("--message=") => {
                message = Some(s["--message=".len()..].to_string())
            }
            "--no-ff" | "--nf" => no_ff = true,
            "--ff-only" | "--fo" => ff_only = true,
            "--squash" => squash = true,
            "-f" | "--force" => force = true,
            s if s.starts_with('-') && s != "-" => {
                return Err(format!("unknown option '{s}' for merge\nTry 'git-wt --help'"));
            }
            s => {
                if source.is_some() {
                    return Err("too many arguments\nTry 'git-wt --help'".into());
                }
                source = Some(s.to_string());
            }
        }
    }

    if review.is_some() {
        let mut bad = start_only_flags(message.as_ref(), no_ff, ff_only, squash, force);
        if dry_run {
            bad.push("dry-run");
        }
        if let Some(sd) = side {
            bad.push(sd.word());
        }
        if let Some(o) = &op {
            bad.push(if *o == MergeOp::Continue { "continue" } else { "abort" });
        }
        if !bad.is_empty() {
            return Err(format!(
                "review takes no merge options (got {})\n\
                 hint: '--review' ends merge's own flags -- anything meant for the \
                 commit table goes after it, e.g. 'merge --review -f'",
                bad.join(", ")
            ));
        }
    }

    if no_ff && ff_only {
        return Err("--no-ff and --ff-only conflict".into());
    }
    if squash && no_ff {
        return Err("--squash and --no-ff conflict".into());
    }

    if let Some(op) = op {
        let word = if op == MergeOp::Continue { "continue" } else { "abort" };
        if let Some(s) = source {
            return Err(format!("{word} takes no argument (got '{s}')"));
        }
        if let Some(sd) = side {
            return Err(format!(
                "{word} takes no merge options\n\
                 hint: '{w}' is applied when a merge starts, so it cannot join one already stopped\n\
                 hint: 'git-wt <N> merge abort', then re-run the merge with '{w}'",
                w = sd.word()
            ));
        }
        let mut bad = start_only_flags(message.as_ref(), no_ff, ff_only, squash, force);
        if dry_run {
            bad.push("dry-run");
        }
        if !bad.is_empty() {
            return Err(format!("{word} takes no merge options (got {})", bad.join(", ")));
        }
        return Ok(MergeParsedArgs {
            op,
            message: None,
            no_ff: false,
            ff_only: false,
            squash: false,
            force: false,
            side: None,
            dry_run: false,
            review: None,
        });
    }

    if dry_run {
        let bad = start_only_flags(message.as_ref(), no_ff, ff_only, squash, force);
        if !bad.is_empty() {
            return Err(format!("dry-run takes no merge options (got {})", bad.join(", ")));
        }
    }

    let source = source.ok_or(
        "merge needs a source: 'git-wt <N>,<M> merge' \
         (or 'git-wt <N> merge <BRANCH>', or continue/abort)",
    )?;
    Ok(MergeParsedArgs {
        op: MergeOp::Start(source),
        message,
        no_ff,
        ff_only,
        squash,
        force,
        side,
        dry_run,
        review,
    })
}

pub(crate) fn cmd_merge(
    root: &Path,
    trees: &[Worktree],
    idx: usize,
    args: &MergeParsedArgs,
) -> Result<(), String> {
    let dest = &trees[idx];
    if dest.bare {
        return Err("cannot merge into a bare worktree".into());
    }
    let dir = dest.path.as_path();
    let in_progress = git_quiet(dir, &["rev-parse", "--verify", "-q", "MERGE_HEAD"]);
    let color = std::io::stderr().is_terminal() && color_enabled(true);

    match &args.op {
        MergeOp::Abort => {
            if !in_progress {
                return Err(format!("no merge in progress in {}", dir.display()));
            }
            git_run(dir, &["merge", "--abort"])?;
            eprintln!("{} merge in {}", paint("Aborted", GREEN, color), leaf_of(dir));
            return Ok(());
        }
        MergeOp::Continue => {
            if !in_progress {
                return Err(format!("no merge in progress in {}", dir.display()));
            }
            let stuck = conflicted_files(dir);
            if !stuck.is_empty() {
                return Err(conflict_msg(dir, &stuck, idx));
            }
            git_run_no_editor(dir, &["merge", "--continue"])?;
            eprintln!("{} merge in {}", paint("Completed", GREEN, color), leaf_of(dir));
            return Ok(());
        }
        MergeOp::Start(_) => {}
    }

    let MergeOp::Start(source) = &args.op else { unreachable!() };
    let src_branch = resolve_merge_source(root, trees, source)?;

    if dest.branch.as_deref() == Some(src_branch.as_str()) {
        return Err(format!("'{src_branch}' is already checked out in worktree {}", idx + 1));
    }

    if args.dry_run {
        return merge_dry_run(dir, &src_branch, &label(dest), color);
    }

    if let Some(tail) = &args.review {
        return cmd_merge_review(root, trees, dest, dir, &src_branch, tail, color);
    }

    if in_progress {
        let Some(sd) = args.side else {
            return Err(format!(
                "a merge is already in progress in {}\n\
                 hint: 'git-wt {n} merge continue' or 'git-wt {n} merge abort'",
                dir.display(),
                n = idx + 1
            ));
        };
        eprintln!(
            "A merge is already in progress in {}, and '{}' only applies when a merge starts.",
            dir.display(),
            sd.word()
        );
        let at_risk = git_stdout(dir, &["status", "--porcelain"])
            .map(|p| has_tracked_changes(&p))
            .unwrap_or(true);
        let cost = if at_risk {
            "Uncommitted changes in that tree, including any conflict resolution \
             already done, are discarded"
        } else {
            "Any conflict resolution already done there is discarded"
        };
        if !confirm(&format!(
            "Abort it and re-merge '{src_branch}' with '{}'? {cost}. [y/N] ",
            sd.word()
        ))? {
            eprintln!("Aborted.");
            return Ok(());
        }
        git_run(dir, &["merge", "--abort"])?;
        eprintln!("{} the previous merge", paint("Abandoned", GREEN, color));
    }

    if !args.force {
        let porcelain = git_stdout(dir, &["status", "--porcelain"])?;
        if has_tracked_changes(&porcelain) {
            return Err(format!(
                "worktree {} has uncommitted changes\nhint: commit or stash them, or re-run with -f",
                idx + 1
            ));
        }
    }

    let mut argv: Vec<String> = vec!["merge".into()];
    if args.no_ff {
        argv.push("--no-ff".into());
    }
    if args.ff_only {
        argv.push("--ff-only".into());
    }
    if args.squash {
        argv.push("--squash".into());
    }
    if let Some(sd) = args.side {
        argv.extend(["-X".into(), sd.strategy_option().into()]);
    }
    if let Some(m) = &args.message {
        argv.extend(["-m".into(), m.clone()]);
    }
    argv.push(src_branch.clone());

    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    if let Err(e) = git_run_no_editor(dir, &refs) {
        let stuck = conflicted_files(dir);
        if stuck.is_empty() {
            return Err(e);
        }
        return Err(conflict_msg(dir, &stuck, idx));
    }

    let into = label(dest);
    let what = if args.squash { "Squashed" } else { "Merged" };
    let how = match args.side {
        Some(sd) => format!("{}, {} won conflicts", leaf_of(dir), sd.word()),
        None => leaf_of(dir),
    };
    eprintln!("{} {src_branch} into {into}  ({how})", paint(what, GREEN, color));
    if args.squash {
        eprintln!("hint: the merge is staged but not committed");
    }
    Ok(())
}

fn cmd_merge_review(
    root: &Path,
    trees: &[Worktree],
    dest: &Worktree,
    dir: &Path,
    src: &str,
    tail: &[String],
    color: bool,
) -> Result<(), String> {
    let dest_ref = ref_of(dest)?;
    let dest_label = label(dest);
    let verdict = merge_probe(dir, src)?;

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
    let header = if n == 0 {
        format!("{} {src} is already in {dest_label}", paint("Merged", GREEN, color))
    } else {
        format!("{src} -> {dest_label}   {n} {plural}, {how}")
    };

    cmd_commits_review(
        root,
        trees,
        tail,
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
        MergeVerdict::Conflict(files) => {
            let mut m = format!(
                "{} conflicting {}:\n",
                files.len(),
                if files.len() == 1 { "path" } else { "paths" }
            );
            for f in &files {
                m.push_str(&format!("  {f}\n"));
            }
            m.push_str("hint: nothing was changed — this was a review\n");
            m.push_str("hint: 'ours' or 'theirs' would settle these automatically");
            Err(m)
        }
    }
}

pub(crate) fn merge_dry_run(dir: &Path, src: &str, into: &str, color: bool) -> Result<(), String> {
    match merge_probe(dir, src)? {
        MergeVerdict::Clean => {
            eprintln!("{} {src} merges into {into} cleanly", paint("Clean", GREEN, color));
            Ok(())
        }
        MergeVerdict::Conflict(files) => {
            let mut m = format!("{src} does NOT merge into {into} cleanly\n");
            for f in &files {
                m.push_str(&format!("  {f}\n"));
            }
            m.push_str("hint: nothing was changed — this was a dry run\n");
            m.push_str("hint: 'ours' or 'theirs' would settle these automatically");
            Err(m)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MergeVerdict {
    Clean,
    Conflict(Vec<String>),
}

pub(crate) fn merge_probe(dir: &Path, src: &str) -> Result<MergeVerdict, String> {
    let out = git_cmd(dir, &["merge-tree", "--write-tree", "--name-only", "HEAD", src])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    match out.status.code() {
        Some(0) => Ok(MergeVerdict::Clean),
        Some(1) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            Ok(MergeVerdict::Conflict(
                stdout
                    .lines()
                    .skip(1)
                    .take_while(|l| !l.trim().is_empty())
                    .map(str::to_string)
                    .collect(),
            ))
        }
        _ => {
            let err = String::from_utf8_lossy(&out.stderr);
            if err.contains("unknown option") || err.contains("usage:") {
                return Err("dry-run needs git 2.38 or newer (git merge-tree --write-tree)".into());
            }
            Err(err.trim().to_string())
        }
    }
}

pub(crate) fn resolve_merge_source(
    root: &Path,
    trees: &[Worktree],
    source: &str,
) -> Result<String, String> {
    if let Ok(n) = source.parse::<usize>() {
        if n >= 1 && n <= trees.len() {
            let w = &trees[n - 1];
            return w.branch.clone().ok_or_else(|| {
                format!("worktree {n} is {} — no branch to merge", label(w))
            });
        }
    }
    if git_quiet(root, &["rev-parse", "--verify", "-q", &format!("{source}^{{commit}}")]) {
        return Ok(source.to_string());
    }
    Err(format!("no worktree or branch '{source}' (see 'git-wt list')"))
}

pub(crate) fn has_tracked_changes(porcelain: &str) -> bool {
    porcelain.lines().any(|l| !l.trim().is_empty() && !l.starts_with("??"))
}

pub(crate) fn conflicted_files(dir: &Path) -> Vec<String> {
    git_stdout(dir, &["diff", "--name-only", "--diff-filter=U"])
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

pub(crate) fn conflict_msg(dir: &Path, files: &[String], idx: usize) -> String {
    let mut m = format!("merge conflict in {}\n", dir.display());
    for f in files {
        m.push_str(&format!("  {f}\n"));
    }
    let n = idx + 1;
    m.push_str(&format!(
        "hint: resolve them there, 'git add' each, then 'git-wt {n} merge continue'\n\
         hint: or undo the merge with 'git-wt {n} merge abort'\n\
         hint: or redo it letting one side win: 'git-wt {n} merge abort', then \
         'git-wt {n},<M> merge theirs'"
    ));
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worktree::{classify_status, Status};

    fn merge_args(args: &[&str]) -> Result<MergeParsedArgs, String> {
        let v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse_merge_args(&v)
    }

    #[test]
    fn tracked_changes_ignore_untracked_only() {
        assert!(!has_tracked_changes(""));
        assert!(!has_tracked_changes("?? new.txt"));
        assert!(!has_tracked_changes("?? a\n?? b"));
        assert!(has_tracked_changes(" M src/main.rs"));
        assert!(has_tracked_changes("A  staged.rs"));
        assert!(has_tracked_changes("?? new.txt\n M src/main.rs"));
        assert!(has_tracked_changes(" M src/main.rs\n?? new.txt"));
        assert_eq!(classify_status(" M a\n?? b"), Status::Untracked);
    }

    #[test]
    fn merge_parses_source_and_options() {
        let a = merge_args(&["2"]).unwrap();
        assert_eq!(a.op, MergeOp::Start("2".into()));
        assert!(!a.no_ff && !a.squash && !a.force && a.message.is_none());

        let a = merge_args(&["feat/x", "--no-ff", "-m", "sync", "-f"]).unwrap();
        assert_eq!(a.op, MergeOp::Start("feat/x".into()));
        assert!(a.no_ff && a.force);
        assert_eq!(a.message.as_deref(), Some("sync"));

        assert_eq!(
            merge_args(&["2", "--message=hi"]).unwrap().message.as_deref(),
            Some("hi")
        );
    }

    #[test]
    fn short_aliases_set_the_same_field_as_their_long_form() {
        assert_eq!(
            merge_args(&["2", "--nf"]).unwrap().no_ff,
            merge_args(&["2", "--no-ff"]).unwrap().no_ff
        );
        assert_eq!(
            merge_args(&["2", "--fo"]).unwrap().ff_only,
            merge_args(&["2", "--ff-only"]).unwrap().ff_only
        );
    }

    #[test]
    fn merge_accepts_bare_and_dashed_resume_words() {
        assert_eq!(merge_args(&["continue"]).unwrap().op, MergeOp::Continue);
        assert_eq!(merge_args(&["--continue"]).unwrap().op, MergeOp::Continue);
        assert_eq!(merge_args(&["abort"]).unwrap().op, MergeOp::Abort);
        assert_eq!(merge_args(&["--abort"]).unwrap().op, MergeOp::Abort);
    }

    #[test]
    fn merge_words_take_optional_dashes_and_shorts() {
        for (bare, dashed, short) in [
            ("continue", "--continue", "-c"),
            ("abort", "--abort", "-a"),
        ] {
            let want = merge_args(&[bare]).unwrap().op;
            assert_eq!(merge_args(&[dashed]).unwrap().op, want, "{dashed}");
            assert_eq!(merge_args(&[short]).unwrap().op, want, "{short}");
        }
        for (bare, dashed, short, want) in [
            ("ours", "--ours", "-o", Side::Ours),
            ("theirs", "--theirs", "-t", Side::Theirs),
        ] {
            for w in [bare, dashed, short] {
                assert_eq!(merge_args(&["2", w]).unwrap().side, Some(want), "{w}");
            }
        }
        for w in ["dry-run", "--dry-run", "-d"] {
            assert!(merge_args(&["2", w]).unwrap().dry_run, "{w}");
        }
    }

    #[test]
    fn merge_side_maps_to_strategy_option() {
        assert_eq!(Side::Ours.strategy_option(), "ours");
        assert_eq!(Side::Theirs.strategy_option(), "theirs");
    }

    #[test]
    fn merge_rejects_both_ops_but_allows_repeats() {
        let e = merge_args(&["continue", "abort"]).unwrap_err();
        assert_eq!(e, "continue and abort conflict");
        assert!(merge_args(&["-c", "--abort"]).is_err());
        assert_eq!(merge_args(&["continue", "-c"]).unwrap().op, MergeOp::Continue);
    }

    #[test]
    fn merge_rejections_name_the_offending_flag() {
        let e = merge_args(&["abort", "-m", "x", "--squash"]).unwrap_err();
        assert!(e.contains("got -m, --squash"), "{e}");
        let e = merge_args(&["2", "dry-run", "--no-ff", "-f"]).unwrap_err();
        assert!(e.contains("got --no-ff, -f"), "{e}");
    }

    #[test]
    fn merge_rejects_both_sides_but_allows_repeats() {
        assert!(merge_args(&["2", "ours", "theirs"]).is_err());
        assert!(merge_args(&["2", "-o", "--theirs"]).is_err());
        assert_eq!(merge_args(&["2", "ours", "-o"]).unwrap().side, Some(Side::Ours));
    }

    #[test]
    fn merge_resume_rejects_a_side_with_a_pointed_hint() {
        let e = merge_args(&["theirs", "continue"]).unwrap_err();
        assert!(e.contains("applied when a merge starts"), "{e}");
        assert!(e.contains("merge abort"), "{e}");
    }

    #[test]
    fn merge_dry_run_rejects_start_only_flags() {
        assert!(merge_args(&["2", "dry-run", "--no-ff"]).is_err());
        assert!(merge_args(&["2", "dry-run", "-m", "x"]).is_err());
        assert!(merge_args(&["2", "dry-run", "-f"]).is_err());
        let e = merge_args(&["2", "dry-run", "--ff-only"]).unwrap_err();
        assert!(e.contains("got --ff-only"), "{e}");
        assert!(merge_args(&["2", "dry-run", "theirs"]).is_ok());
    }

    #[test]
    fn review_hands_the_tail_over_untouched() {
        let a = merge_args(&["2", "--review"]).unwrap();
        assert_eq!(a.review.as_deref(), Some(&[][..]));
        assert!(!a.force);

        let a = merge_args(&["2", "--review", "-f", "-n", "5", "-af", "-m", "x", "-d", "1"])
            .unwrap();
        assert!(!a.force && !a.dry_run && a.message.is_none());
        assert_eq!(a.review.unwrap(), ["-f", "-n", "5", "-af", "-m", "x", "-d", "1"]);

        assert!(merge_args(&["2", "review"]).unwrap().review.is_some());

        let a = merge_args(&["feat/x", "--review", "-f"]).unwrap();
        assert_eq!(a.op, MergeOp::Start("feat/x".into()));
        assert_eq!(a.review.unwrap(), ["-f"]);
        assert_eq!(
            merge_args(&["1", "2", "--review"]).unwrap_err(),
            "too many arguments\nTry 'git-wt --help'"
        );
    }

    #[test]
    fn review_rejects_merge_flags_typed_before_it() {
        for (args, want) in [
            (vec!["2", "-f", "--review"], "-f"),
            (vec!["2", "-m", "x", "--review"], "-m"),
            (vec!["2", "--squash", "--review"], "--squash"),
            (vec!["2", "dry-run", "--review"], "dry-run"),
            (vec!["2", "theirs", "--review"], "theirs"),
            (vec!["-a", "--review"], "abort"),
        ] {
            let e = merge_args(&args).unwrap_err();
            assert!(e.starts_with("review takes no merge options"), "{args:?}: {e}");
            assert!(e.contains(want), "{args:?}: {e}");
            assert!(e.contains("ends merge's own flags"), "{args:?}: {e}");
        }
    }

    #[test]
    fn merge_rejects_bad_combinations() {
        assert!(merge_args(&[]).is_err());
        assert!(merge_args(&["--continue", "2"]).is_err());
        assert!(merge_args(&["--continue", "--no-ff"]).is_err());
        assert!(merge_args(&["--continue", "--abort"]).is_err());
        assert!(merge_args(&["2", "--no-ff", "--ff-only"]).is_err());
        assert!(merge_args(&["2", "--squash", "--no-ff"]).is_err());
        assert!(merge_args(&["2", "3"]).is_err());
        assert!(merge_args(&["2", "--rebase"]).is_err());
        assert!(merge_args(&["-m"]).is_err());
    }
}
