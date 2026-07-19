use std::io::IsTerminal;
use std::path::Path;

use crate::ui::confirm;
use crate::git::{git_cmd, git_quiet, git_run, git_run_no_editor, git_stdout};
use crate::ui::{color_enabled, paint, GREEN};
use crate::worktree::{label, leaf_of, Worktree};

// ---------------------------------------------------------------------------
// Merge: git-wt <N>,<M> merge | --continue | --abort
// ---------------------------------------------------------------------------

/// What a `merge` invocation asks for. `continue`/`abort` resume or undo a
/// merge that is already in progress, so they carry no source and no options.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MergeOp {
    Start(String),
    Continue,
    Abort,
}

/// Which side wins a hunk that both branches touched. Maps to git's
/// `-X ours` / `-X theirs`, never `-s ours`: the strategy *option* picks a side
/// only where hunks actually collide, while the whole-tree *strategy* would drop
/// the source's changes entirely and still record a merge — silent data loss.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    Ours,
    Theirs,
}

impl Side {
    /// The word the user typed, for echoing back in messages.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Side::Ours => "ours",
            Side::Theirs => "theirs",
        }
    }

    /// git's `-X` argument.
    pub(crate) fn strategy_option(self) -> &'static str {
        self.word()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct MergeArgs {
    pub(crate) op: MergeOp,
    pub(crate) message: Option<String>,
    pub(crate) no_ff: bool,
    pub(crate) ff_only: bool,
    pub(crate) squash: bool,
    pub(crate) force: bool,
    pub(crate) side: Option<Side>,
    pub(crate) dry_run: bool,
}

/// Parse the words after a `merge` verb, in either target form.
///
/// The list form hands its source in as a positional, so both spellings land
/// here with the same shape. A worktree-number source is only legal via the
/// list — `dispatch_target` rejects `git-wt <N> merge <M>` before this parser's
/// result is used — but a branch source and the resume words (`continue`,
/// `abort`, whose source is implicit in the in-progress merge) still arrive
/// from the single-target form.
///
/// The verb-ish words — `continue`, `abort`, `ours`, `theirs`, `dry-run` — each
/// take an optional `--` plus a short form, so `abort`, `--abort` and `-a` are
/// one thing: they read as words in this grammar, and the dashes are noise. The
/// flags that mirror git's own spelling (`--no-ff`, `--squash`, ...) keep their
/// dashes so muscle memory carries over.
///
/// Keywords are matched ahead of the positional source, so a branch actually
/// named `ours` or `abort` must be spelled `heads/ours` to be merged.
pub(crate) fn parse_merge_args(args: &[String]) -> Result<MergeArgs, String> {
    let mut source: Option<String> = None;
    let mut op: Option<MergeOp> = None;
    let mut message = None;
    let mut side: Option<Side> = None;
    let (mut no_ff, mut ff_only, mut squash, mut force, mut dry_run) =
        (false, false, false, false, false);

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
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
            "--no-ff" => no_ff = true,
            "--ff-only" => ff_only = true,
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

    if no_ff && ff_only {
        return Err("--no-ff and --ff-only conflict".into());
    }
    if squash && no_ff {
        return Err("--squash and --no-ff conflict".into());
    }

    // A resume takes no source and no merge options: those were settled when
    // the merge started, so silently accepting them would be a lie.
    if let Some(op) = op {
        let word = if op == MergeOp::Continue { "continue" } else { "abort" };
        if let Some(s) = source {
            return Err(format!("{word} takes no argument (got '{s}')"));
        }
        // The tempting one: `merge theirs continue` reads as "finish this
        // conflict by taking theirs", but a side is chosen when the merge is
        // computed. Name the real path instead of ignoring the word.
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
        return Ok(MergeArgs {
            op,
            message: None,
            no_ff: false,
            ff_only: false,
            squash: false,
            force: false,
            side: None,
            dry_run: false,
        });
    }

    // A dry run computes the merge in memory and writes nothing, so the flags
    // that shape a real merge commit have nothing to act on.
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
    Ok(MergeArgs { op: MergeOp::Start(source), message, no_ff, ff_only, squash, force, side, dry_run })
}

/// Record `ours`/`theirs`. They are opposite answers to one question, so asking
/// for both is a mistake worth naming; asking twice for the same side is not.
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

/// Record `continue`/`abort`. Like `set_side`, they are opposite answers to one
/// question, so asking for both is a mistake; asking twice for the same one is
/// only redundant.
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

/// The flags that only mean something when a real merge runs, as the user would
/// type them. Some shape the resulting commit (`-m`, `--no-ff`, `--squash`),
/// others gate whether it may run at all (`--ff-only`, `-f`) — either way they
/// need a merge that is about to start, which is exactly what `continue`/
/// `abort` (one already stopped) and `dry-run` (none at all) do not have.
///
/// Shared so all three can name what they are rejecting rather than just
/// saying "no merge options".
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

pub(crate) fn cmd_merge(
    root: &Path,
    trees: &[Worktree],
    idx: usize,
    args: &MergeArgs,
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
            // Unresolved paths make `git merge --continue` fail with a terse
            // message; naming the files is what the user actually needs.
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

    // A dry run only reads, so it answers before any of the guards below: it
    // never prompts, never writes, and is happy to report on a merge that is
    // currently stopped.
    if args.dry_run {
        return merge_dry_run(dir, &src_branch, &label(dest), color);
    }

    if in_progress {
        // `ours`/`theirs` are applied while the merge is computed, so they can't
        // join one that has already stopped. Redoing it from a clean state is
        // the only way to honor the word — and it costs whatever resolution has
        // been done in that tree, so it is the user's call, not ours.
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
        // Mid-merge, tracked changes are the half-resolved conflict itself —
        // but a merge started with -f over a dirty tree buried the user's own
        // edits in there too, and `merge --abort` unwinds the lot. Say so only
        // when there is actually something to lose.
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

    // A merge into uncommitted work can end with the user's own edits tangled in
    // conflict markers, so it takes an explicit --force. A status git can't read
    // is not a green light, so the error propagates rather than merging blind.
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
    // Name the side when one was forced: it silently decided every collision,
    // so it belongs in the record of what just happened.
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

/// Report whether `src` would merge into the worktree's HEAD, touching nothing.
///
/// `git merge-tree --write-tree` does the whole job: it resolves the merge into
/// a tree object and exits 1 when a path conflicts, with no index, no checkout
/// and nothing to clean up afterwards. It needs git 2.38+, which is checked by
/// running it rather than by parsing `git --version`.
pub(crate) fn merge_dry_run(dir: &Path, src: &str, into: &str, color: bool) -> Result<(), String> {
    let out = git_cmd(dir, &["merge-tree", "--write-tree", "--name-only", "HEAD", src])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    match out.status.code() {
        Some(0) => {
            eprintln!(
                "{} {src} merges into {into} cleanly",
                paint("Clean", GREEN, color)
            );
            Ok(())
        }
        // Conflicts. stdout is the resulting tree's oid, then the paths that
        // collided. Exiting nonzero keeps `if git-wt 1,2 merge dry-run; then`
        // meaningful, and mirrors what a real merge does on a conflict.
        Some(1) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let files: Vec<String> = stdout
                .lines()
                .skip(1)
                .filter(|l| !l.trim().is_empty())
                .map(str::to_string)
                .collect();
            let mut m = format!("{src} does NOT merge into {into} cleanly\n");
            for f in &files {
                m.push_str(&format!("  {f}\n"));
            }
            m.push_str("hint: nothing was changed — this was a dry run\n");
            m.push_str("hint: 'ours' or 'theirs' would settle these automatically");
            Err(m)
        }
        _ => {
            let err = String::from_utf8_lossy(&out.stderr);
            // Older git has merge-tree, but not --write-tree.
            if err.contains("unknown option") || err.contains("usage:") {
                return Err("dry-run needs git 2.38 or newer (git merge-tree --write-tree)".into());
            }
            Err(err.trim().to_string())
        }
    }
}

/// Resolve a merge source word to a branch name. A number that names a
/// worktree wins over a same-named branch: numbers are this tool's grammar.
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
    Err(format!(
        "no worktree or branch '{source}' (see 'git-wt list')"
    ))
}

/// Whether `git status --porcelain` reports changes to *tracked* files, staged
/// or not. Untracked files don't count: a merge that would overwrite one makes
/// git refuse on its own, so they are no reason to demand `-f`. Deliberately
/// not `classify_status`, which collapses a tree holding both kinds to
/// `Untracked` and would wave those tracked edits through.
pub(crate) fn has_tracked_changes(porcelain: &str) -> bool {
    porcelain
        .lines()
        .any(|l| !l.trim().is_empty() && !l.starts_with("??"))
}

/// Paths with unresolved conflicts in a worktree, one per line.
pub(crate) fn conflicted_files(dir: &Path) -> Vec<String> {
    git_stdout(dir, &["diff", "--name-only", "--diff-filter=U"])
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// The message shown when a merge stops on conflicts: where it stopped, what
/// is conflicted, and the two ways out.
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

    fn merge_args(args: &[&str]) -> Result<MergeArgs, String> {
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
        // The case classify_status collapses to Untracked: tracked edits are
        // still present, so a merge here needs -f.
        assert!(has_tracked_changes("?? new.txt\n M src/main.rs"));
        assert!(has_tracked_changes(" M src/main.rs\n?? new.txt"));
        assert_eq!(classify_status(" M a\n?? b"), Status::Untracked); // why not classify_status
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

        assert_eq!(merge_args(&["2", "--message=hi"]).unwrap().message.as_deref(), Some("hi"));
    }


    #[test]
    fn merge_accepts_bare_and_dashed_resume_words() {
        assert_eq!(merge_args(&["continue"]).unwrap().op, MergeOp::Continue);
        assert_eq!(merge_args(&["--continue"]).unwrap().op, MergeOp::Continue);
        assert_eq!(merge_args(&["abort"]).unwrap().op, MergeOp::Abort);
        assert_eq!(merge_args(&["--abort"]).unwrap().op, MergeOp::Abort);
    }


    /// Every keyword means the same thing bare, dashed, or short.
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
        // -X ours / -X theirs, never -s ours: the whole-tree strategy would
        // drop the source's changes and still record a merge.
        assert_eq!(Side::Ours.strategy_option(), "ours");
        assert_eq!(Side::Theirs.strategy_option(), "theirs");
    }


    #[test]
    fn merge_rejects_both_ops_but_allows_repeats() {
        let e = merge_args(&["continue", "abort"]).unwrap_err();
        assert_eq!(e, "continue and abort conflict");
        assert!(merge_args(&["-c", "--abort"]).is_err());
        // Saying the same word twice is redundant, not wrong — same rule as
        // ours/theirs.
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
        // Saying the same side twice is redundant, not wrong.
        assert_eq!(merge_args(&["2", "ours", "-o"]).unwrap().side, Some(Side::Ours));
    }


    #[test]
    fn merge_resume_rejects_a_side_with_a_pointed_hint() {
        // 'theirs continue' reads as "finish this by taking theirs", which git
        // cannot do — the error has to say so rather than ignore the word.
        let e = merge_args(&["theirs", "continue"]).unwrap_err();
        assert!(e.contains("applied when a merge starts"), "{e}");
        assert!(e.contains("merge abort"), "{e}");
    }


    #[test]
    fn merge_dry_run_rejects_start_only_flags() {
        assert!(merge_args(&["2", "dry-run", "--no-ff"]).is_err());
        assert!(merge_args(&["2", "dry-run", "-m", "x"]).is_err());
        assert!(merge_args(&["2", "dry-run", "-f"]).is_err());
        // --ff-only gates the merge rather than shaping its commit, but a dry
        // run has no merge to gate: merge-tree resolves in memory and never
        // fast-forwards anything, so honoring it is impossible.
        let e = merge_args(&["2", "dry-run", "--ff-only"]).unwrap_err();
        assert!(e.contains("got --ff-only"), "{e}");
        // A side is fine: it changes what the dry run would report.
        assert!(merge_args(&["2", "dry-run", "theirs"]).is_ok());
    }


    #[test]
    fn merge_rejects_bad_combinations() {
        assert!(merge_args(&[]).is_err()); // no source
        assert!(merge_args(&["--continue", "2"]).is_err()); // resume takes no source
        assert!(merge_args(&["--continue", "--no-ff"]).is_err()); // nor options
        assert!(merge_args(&["--continue", "--abort"]).is_err());
        assert!(merge_args(&["2", "--no-ff", "--ff-only"]).is_err());
        assert!(merge_args(&["2", "--squash", "--no-ff"]).is_err());
        assert!(merge_args(&["2", "3"]).is_err()); // too many
        assert!(merge_args(&["2", "--rebase"]).is_err()); // unknown option
        assert!(merge_args(&["-m"]).is_err()); // -m needs a value
    }

}
