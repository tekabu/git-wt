use std::io::IsTerminal;
use std::path::Path;

use crate::cmd::commits::{cmd_commits_review, ReviewCtx};
use crate::ui::confirm;
use crate::git::{git_cmd, git_quiet, git_run, git_run_no_editor, git_stdout};
use crate::ui::{color_enabled, paint, GREEN};
use crate::worktree::{label, leaf_of, ref_of, Worktree};

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
    /// `--review` and everything typed after it, untouched.
    ///
    /// `Some(tail)` is the flag; the tail is handed to `commits`' parser
    /// verbatim, which is the whole point -- `merge` and `commits` claim the
    /// same short letters for different things (`-f` is force here and files
    /// there), so the only way both keep their meanings is for `merge` to stop
    /// looking. `Some(vec![])` is a bare `--review`.
    pub(crate) review: Option<Vec<String>>,
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

    let mut review: Option<Vec<String>> = None;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            // Ends merge's parsing. Everything left belongs to `commits`, which
            // is why nothing below this line ever sees it.
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

    // `--review` merges nothing, so every merge option is meaningless under it.
    // Saying so is not pedantry: they were *claimed* before `--review` was
    // reached -- `merge -f --review` has already set force, and `-a` an abort --
    // so accepting them silently would report on a merge shaped by flags that
    // never ran. The fix is always the same, and it is a keystroke: move them
    // after `--review`, where they read as the `commits` flags they look like.
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
            review: None,
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
    Ok(MergeArgs {
        op: MergeOp::Start(source),
        message, no_ff, ff_only, squash, force, side, dry_run, review,
    })
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

    // Same reasoning, and the same position: --review only reads. It reports
    // ahead of the in-progress guard too, because a stopped merge has not
    // committed -- HEAD is still where it was when the merge began, so
    // `dest..src` still names exactly the commits that have yet to land.
    if let Some(tail) = &args.review {
        return cmd_merge_review(root, trees, dest, dir, &src_branch, tail, color);
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

/// `merge --review`: what this merge would bring over, and whether it lands.
///
/// A header from the same `merge-tree` probe `--dry-run` uses, then the commit
/// table for `dest..src`. The order is the point -- a reviewer wants the verdict
/// before the list -- and it is why `merge_probe` exists separately from
/// `merge_dry_run`: a conflict has to be held as a value while the table
/// prints, where an `Err` would have ended the command before it.
///
/// Exit codes match `--dry-run`: 0 clean, 1 conflict, so
/// `if git-wt 1,2 merge --review; then` keeps its meaning.
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

    // The range's own size, not the table's: filters cut what is shown, and the
    // header is answering what the merge would carry.
    let range = format!("{dest_ref}..{src}");
    let n: usize = git_stdout(dir, &["rev-list", "--count", &range])?
        .trim()
        .parse()
        .unwrap_or(0);
    // Nothing to bring over means there is no merge to have a verdict about:
    // "0 commits, merges cleanly" is true only the way an empty statement is,
    // and reads as though a merge just ran. The one fact worth printing is
    // that the destination already has everything, in `merged`'s words.
    //
    // Still a header rather than an early return, so the tail is parsed first:
    // `merge --review --bogus` has to be an error whether or not the range
    // happens to be empty, and returning here would exit 0 on a rejected flag.
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
            let mut m = format!("{} conflicting {}:\n", files.len(), if files.len() == 1 { "path" } else { "paths" });
            for f in &files {
                m.push_str(&format!("  {f}\n"));
            }
            m.push_str("hint: nothing was changed — this was a review\n");
            m.push_str("hint: 'ours' or 'theirs' would settle these automatically");
            Err(m)
        }
    }
}

/// Report whether `src` would merge into the worktree's HEAD, touching nothing.
///
/// `git merge-tree --write-tree` does the whole job: it resolves the merge into
/// a tree object and exits 1 when a path conflicts, with no index, no checkout
/// and nothing to clean up afterwards. It needs git 2.38+, which is checked by
/// running it rather than by parsing `git --version`.
pub(crate) fn merge_dry_run(dir: &Path, src: &str, into: &str, color: bool) -> Result<(), String> {
    match merge_probe(dir, src)? {
        MergeVerdict::Clean => {
            eprintln!(
                "{} {src} merges into {into} cleanly",
                paint("Clean", GREEN, color)
            );
            Ok(())
        }
        // Exiting nonzero keeps `if git-wt 1,2 merge dry-run; then` meaningful,
        // and mirrors what a real merge does on a conflict.
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

/// What the probe found: a merge that resolves, or the paths that collide.
///
/// Separate from any wording because two callers need the same answer in
/// opposite shapes: `merge_dry_run` turns a conflict into an `Err` and lets the
/// caller print it, while `--review` has to print its header and table *before*
/// deciding the exit code. An `Err` cannot be held open like that.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MergeVerdict {
    Clean,
    Conflict(Vec<String>),
}

/// Would `src` merge into the worktree's HEAD? Touches nothing.
///
/// `git merge-tree --write-tree` does the whole job: it resolves the merge into
/// a tree object and exits 1 when a path conflicts, with no index, no checkout
/// and nothing to clean up afterwards. It needs git 2.38+, which is checked by
/// running it rather than by parsing `git --version`.
pub(crate) fn merge_probe(dir: &Path, src: &str) -> Result<MergeVerdict, String> {
    let out = git_cmd(dir, &["merge-tree", "--write-tree", "--name-only", "HEAD", src])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    match out.status.code() {
        Some(0) => Ok(MergeVerdict::Clean),
        // stdout is the resulting tree's oid, then the paths that collided,
        // then a blank line and git's own commentary ("Auto-merging f",
        // "CONFLICT (content): ..."). The blank line is the only boundary, so
        // the list has to stop at it -- taking every non-empty line instead
        // reported those messages as though they were paths.
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
    fn short_aliases_set_the_same_field_as_their_long_form() {
        assert_eq!(merge_args(&["2", "--nf"]).unwrap().no_ff, merge_args(&["2", "--no-ff"]).unwrap().no_ff);
        assert_eq!(merge_args(&["2", "--fo"]).unwrap().ff_only, merge_args(&["2", "--ff-only"]).unwrap().ff_only);
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


    /// The handoff: `--review` ends merge's parsing, so the tail keeps its
    /// `commits` meanings. `-f` is the one that matters -- it is force here and
    /// files there, and getting it wrong runs a merge instead of printing a
    /// table.
    #[test]
    fn review_hands_the_tail_over_untouched() {
        let a = merge_args(&["2", "--review"]).unwrap();
        assert_eq!(a.review.as_deref(), Some(&[][..]));
        assert!(!a.force);

        // Every one of these is a merge flag before --review and a commits flag
        // after it. None of them is claimed.
        let a = merge_args(&["2", "--review", "-f", "-n", "5", "-af", "-m", "x", "-d", "1"])
            .unwrap();
        assert!(!a.force && !a.dry_run && a.message.is_none());
        assert_eq!(
            a.review.unwrap(),
            ["-f", "-n", "5", "-af", "-m", "x", "-d", "1"]
        );

        // The bare word, like every other verb-ish word in this grammar.
        assert!(merge_args(&["2", "review"]).unwrap().review.is_some());

        // The positional source is exempt: it is not a flag, and merge still
        // needs it. Only one, as ever.
        let a = merge_args(&["feat/x", "--review", "-f"]).unwrap();
        assert_eq!(a.op, MergeOp::Start("feat/x".into()));
        assert_eq!(a.review.unwrap(), ["-f"]);
        // A second positional after --review is the commits parser's problem,
        // not a 'too many arguments' here -- which is the handoff working.
        assert_eq!(merge_args(&["1", "2", "--review"]).unwrap_err(), "too many arguments\nTry 'git-wt --help'");
    }

    /// A merge flag *before* `--review` was already claimed by the time the
    /// handoff was reached, so accepting it would report on a merge shaped by
    /// options that never ran.
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
