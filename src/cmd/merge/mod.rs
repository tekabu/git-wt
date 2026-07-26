pub(crate) mod args;

use std::io::IsTerminal;
use std::path::Path;

use clap::Parser;

use crate::cmd::merge::args::MergeOptions;
use crate::git::{git_cmd, git_quiet, git_run, git_run_no_editor, git_stdout};
use crate::ui::{color_enabled, confirm, paint, GREEN};
use crate::worktree::{label, leaf_of, Worktree};

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
        v.push("-F");
    }
    v
}

const NEEDS_SOURCE: &str = "merge needs a source: 'git-wt merge <SOURCE>' \
     (or '-b <SOURCE>', or --continue/--abort)";

/// `main`'s top-level error path prints its own leading `error: `, unlike
/// clap's own `Cli::parse()`, which prints the full render itself and exits
/// before that ever runs. A nested `try_parse_from` like this one instead
/// flows its result through the same `Result<_, String>` pipe as everything
/// else, so its error needs the same bare-one-liner shape: just clap's
/// reason, not its own repeated `error: ` prefix or the multi-line Usage
/// block underneath it.
#[cfg(test)]
fn clap_err_line(e: clap::error::Error) -> String {
    let s = e.to_string();
    // A plain clap error is one line, but `conflicts_with_all`'s "cannot be
    // used with:" renders the offending flags as an indented list on the
    // lines after -- stopping at line 1 (as this used to) silently dropped
    // all of them, leaving just the bare "...cannot be used with:" preamble.
    // Everything through the first blank line (before the Usage: block) is
    // the actual message; join it into one line so a substring check still
    // works the same way it does for the single-line case.
    s.lines()
        .take_while(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .trim_start_matches("error: ")
        .to_string()
}

#[derive(Parser)]
struct MergeOptionsWrap {
    #[command(flatten)]
    o: MergeOptions,
}

/// The test-only front door: parses raw argv through `MergeOptions` via clap
/// (same struct `Cli::parse()` already filled `MergeArgs.options` with in
/// production, wrapped here only so tests can hand this function strings) and
/// turns the typed result into `MergeParsedArgs`.
#[cfg(test)]
fn parse_merge_args(args: &[String]) -> Result<MergeParsedArgs, String> {
    let o = MergeOptionsWrap::try_parse_from(std::iter::once("merge".to_string()).chain(args.iter().cloned()))
        .map_err(clap_err_line)?
        .o;
    build_merge_parsed_args(o)
}

/// Turn a clap-parsed `MergeOptions` into `MergeParsedArgs`.
/// `--continue`/`--abort` rule out every other merge option declaratively
/// (see `MergeOptions`' doc comment), so nothing here can collide with what
/// clap has already refused.
pub(crate) fn build_merge_parsed_args(o: MergeOptions) -> Result<MergeParsedArgs, String> {
    let side = match (o.side.ours, o.side.theirs) {
        (true, false) => Some(Side::Ours),
        (false, true) => Some(Side::Theirs),
        _ => None,
    };

    if o.resume.r#continue || o.resume.abort {
        // `source`'s own `conflicts_with_all` covers the positional spelling,
        // but merge has a second one: `-s/--source` is a source here, not the
        // `-b` extra target it is elsewhere, and it lives on `MergeArgs` rather
        // than in this struct -- so clap never sees it as the thing a resume
        // word rules out. Dispatch folds it into `source` before calling us,
        // which is where both spellings finally meet.
        if o.source.is_some() {
            let word = if o.resume.r#continue { "--continue" } else { "--abort" };
            return Err(format!("'{word}' cannot be used with '[SOURCE]'"));
        }
        let op = if o.resume.r#continue { MergeOp::Continue } else { MergeOp::Abort };
        return Ok(MergeParsedArgs {
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

    if o.dry_run.dry_run {
        let bad = start_only_flags(o.message.as_ref(), o.no_ff, o.ff_only, o.squash.squash, o.force.force);
        if !bad.is_empty() {
            return Err(format!("--dry-run takes no merge options (got {})", bad.join(", ")));
        }
    }

    let source = o.source.ok_or(NEEDS_SOURCE)?;
    Ok(MergeParsedArgs {
        op: MergeOp::Start(source),
        message: o.message,
        no_ff: o.no_ff,
        ff_only: o.ff_only,
        squash: o.squash.squash,
        force: o.force.force,
        side,
        dry_run: o.dry_run.dry_run,
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
                return Err(conflict_msg(dir, &stuck));
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

    if in_progress {
        let Some(sd) = args.side else {
            return Err(format!(
                "a merge is already in progress in {}",
                dir.display()
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
                "worktree {} has uncommitted changes",
                idx + 1
            ));
        }
    }

    if !confirm(&format!("Merge {src_branch} into {}? [y/N] ", label(dest)))? {
        eprintln!("Aborted.");
        return Ok(());
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
        return Err(conflict_msg(dir, &stuck));
    }

    let into = label(dest);
    let what = if args.squash { "Squashed" } else { "Merged" };
    let how = match args.side {
        Some(sd) => format!("{}, {} won conflicts", leaf_of(dir), sd.word()),
        None => leaf_of(dir),
    };
    eprintln!("{} {src_branch} into {into}  ({how})", paint(what, GREEN, color));
    if args.squash {
        eprintln!("the merge is staged but not committed");
    }
    Ok(())
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
            m.push_str("nothing was changed — this was a dry run");
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

pub(crate) fn conflict_msg(dir: &Path, files: &[String]) -> String {
    let mut m = format!("merge conflict in {}\n", dir.display());
    for f in files {
        m.push_str(&format!("  {f}\n"));
    }
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

        let a = merge_args(&["feat/x", "--no-ff", "-m", "sync", "-F"]).unwrap();
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
    fn merge_resume_words_take_dashed_or_short_only() {
        assert_eq!(merge_args(&["--continue"]).unwrap().op, MergeOp::Continue);
        assert_eq!(merge_args(&["-c"]).unwrap().op, MergeOp::Continue);
        assert_eq!(merge_args(&["--abort"]).unwrap().op, MergeOp::Abort);
        assert_eq!(merge_args(&["-a"]).unwrap().op, MergeOp::Abort);
    }

    #[test]
    fn merge_words_take_dashed_or_short_only() {
        for (dashed, short, want) in [
            ("--continue", "-c", MergeOp::Continue),
            ("--abort", "-a", MergeOp::Abort),
        ] {
            assert_eq!(merge_args(&[dashed]).unwrap().op, want, "{dashed}");
            assert_eq!(merge_args(&[short]).unwrap().op, want, "{short}");
        }
        assert_eq!(merge_args(&["2", "--ours"]).unwrap().side, Some(Side::Ours));
        assert_eq!(merge_args(&["2", "--theirs"]).unwrap().side, Some(Side::Theirs));
        assert!(merge_args(&["2", "--dry-run"]).unwrap().dry_run);
    }

    #[test]
    fn merge_side_maps_to_strategy_option() {
        assert_eq!(Side::Ours.strategy_option(), "ours");
        assert_eq!(Side::Theirs.strategy_option(), "theirs");
    }

    #[test]
    fn merge_rejects_both_ops_but_allows_repeats() {
        // clap's own `conflicts_with_all` on `--continue`/`--abort` now
        // enforces this at parse time; the message is clap's, not ours.
        let e = merge_args(&["--continue", "--abort"]).unwrap_err();
        assert!(e.contains("--continue") && e.contains("--abort"), "{e}");
        assert!(merge_args(&["-c", "--abort"]).is_err());
        assert_eq!(merge_args(&["--continue", "-c"]).unwrap().op, MergeOp::Continue);
    }

    #[test]
    fn merge_rejections_name_the_offending_flag() {
        // `conflicts_with_all` reports every conflicting flag actually given,
        // same as the accumulator it replaced -- just in clap's own wording.
        let e = merge_args(&["--abort", "-m", "x", "--squash"]).unwrap_err();
        assert!(e.contains("--message") && e.contains("--squash"), "{e}");
        // '--dry-run' isn't in that declarative set (it's compatible with
        // ours/theirs, unlike message/no-ff/ff-only/squash/force), so its
        // "takes no merge options" check is still the hand-written
        // accumulator, unchanged.
        let e = merge_args(&["2", "--dry-run", "--no-ff", "-F"]).unwrap_err();
        assert!(e.contains("got --no-ff, -F"), "{e}");
    }

    #[test]
    fn merge_rejects_both_sides_but_allows_repeats() {
        assert!(merge_args(&["2", "--ours", "--theirs"]).is_err());
        assert_eq!(merge_args(&["2", "--ours", "--ours"]).unwrap().side, Some(Side::Ours));
    }

    #[test]
    fn merge_resume_rejects_a_side() {
        let e = merge_args(&["--theirs", "--continue"]).unwrap_err();
        assert!(e.contains("--theirs") && e.contains("--continue"), "{e}");
    }

    #[test]
    fn merge_dry_run_rejects_start_only_flags() {
        assert!(merge_args(&["2", "--dry-run", "--no-ff"]).is_err());
        assert!(merge_args(&["2", "--dry-run", "-m", "x"]).is_err());
        assert!(merge_args(&["2", "--dry-run", "-F"]).is_err());
        let e = merge_args(&["2", "--dry-run", "--ff-only"]).unwrap_err();
        assert!(e.contains("got --ff-only"), "{e}");
        assert!(merge_args(&["2", "--dry-run", "--theirs"]).is_ok());
    }

    #[test]
    fn review_and_meld_are_not_merge_words() {
        // `review` is its own subcommand with its own vocabulary, so merge
        // knows neither word: they reach clap as unknown arguments.
        for a in [
            vec!["2", "--review"],
            vec!["2", "--meld"],
            vec!["2", "--review", "--meld"],
        ] {
            let e = merge_args(&a).unwrap_err();
            assert!(e.contains("unexpected argument"), "{a:?}: {e}");
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
