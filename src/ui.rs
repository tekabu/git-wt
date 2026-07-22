use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::process::{Child, Command, Stdio};

// ---------------------------------------------------------------------------
// Color, status, and metadata (no dependencies; ANSI on a TTY only)
// ---------------------------------------------------------------------------

pub(crate) const RESET: &str = "\x1b[0m";
pub(crate) const GREEN: &str = "32";
pub(crate) const YELLOW: &str = "33";
pub(crate) const RED: &str = "31";
pub(crate) const DIM: &str = "2";
/// The cell a filter acted on: amber, bold.
///
/// Yellow is the right family -- it is the warmest thing on a dark terminal and
/// the first color the eye finds -- but plain yellow is spent on '≈'. So this is
/// a step over into amber (256-color 214), which reads as the same family
/// without being the same color, and is bold so it carries on a light
/// background too. Terminals that cannot do 256 colors fall back to their
/// nearest yellow, which is exactly the right failure.
pub(crate) const MATCH: &str = "1;38;5;214";

/// `list`'s search highlight: bold, 256-color 33, Material Blue 500 -- a
/// different family from `MATCH`'s amber so the two searches (this one over
/// worktrees, that one over commit text) never read as the same feature.
pub(crate) const SEARCH_MATCH: &str = "1;38;5;33";

/// Whether to emit ANSI for a stream that is (or isn't) a terminal. Honors the
/// `NO_COLOR` (any value disables) and `CLICOLOR_FORCE` (nonzero forces on)
/// conventions; otherwise follows the stream's TTY-ness.
pub(crate) fn color_enabled(is_tty: bool) -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if let Some(v) = std::env::var_os("CLICOLOR_FORCE") {
        if !v.is_empty() && v != "0" {
            return true;
        }
    }
    is_tty
}

/// Wrap `s` in an ANSI SGR code when `on`, else return it unchanged. The code
/// is a bare parameter string like "32" or "2".
pub(crate) fn paint(s: &str, code: &str, on: bool) -> String {
    if on {
        format!("\x1b[{code}m{s}{RESET}")
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
// Pager: `commits`/`log` page through `less -R` the same way `git diff` pages
// through git's own pager -- a table is exactly the kind of output that runs
// past a screen. Only bare libc calls, no crate: the fd swap is the same
// `dup`/`dup2` trick git itself uses to hand its own stdout to a child.
// ---------------------------------------------------------------------------

extern "C" {
    fn dup(fd: i32) -> i32;
    fn dup2(oldfd: i32, newfd: i32) -> i32;
    fn close(fd: i32) -> i32;
}

/// While alive, this process's stdout is a pipe into a spawned pager; dropping
/// it restores the original fd and waits for the pager to exit (i.e. for the
/// user to quit `less`), so the command doesn't return before the view closes.
pub(crate) struct Pager {
    child: Option<Child>,
    saved_stdout: Option<i32>,
}

impl Pager {
    /// Start the pager when `enabled` (stdout is a terminal, decided by the
    /// caller before this call, since the caller's own `is_terminal()` check
    /// stops being true the moment stdout is repointed at a pipe). `PAGER` or
    /// `GIT_WT_PAGER` (checked first, so it can differ from git's own pager)
    /// override the default; either set to `cat` (or empty) turns paging off,
    /// the same convention git honors.
    pub(crate) fn start(enabled: bool) -> Pager {
        let none = Pager { child: None, saved_stdout: None };
        if !enabled {
            return none;
        }
        let cmd = std::env::var("GIT_WT_PAGER")
            .or_else(|_| std::env::var("PAGER"))
            .unwrap_or_else(|_| "less".to_string());
        let mut parts = cmd.split_whitespace();
        let Some(prog) = parts.next() else { return none };
        if prog == "cat" {
            return none;
        }
        // `less`'s own defaults, unless the caller named a pager and its own
        // flags: -R shows our color codes instead of escaping them. No -F: a
        // table that fits one screen should still open the pager's alternate
        // screen, not print straight through. No -X: quitting restores the
        // prior screen instead of leaving the table in scrollback.
        let args: Vec<&str> = if parts.clone().next().is_some() {
            parts.collect()
        } else if prog == "less" {
            vec!["-R"]
        } else {
            vec![]
        };

        let Ok(mut child) = Command::new(prog).args(&args).stdin(Stdio::piped()).spawn() else {
            return none;
        };
        let Some(pager_stdin) = child.stdin.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return none;
        };

        let stdout_fd = std::io::stdout().as_raw_fd();
        let saved = unsafe { dup(stdout_fd) };
        if saved < 0 {
            drop(pager_stdin);
            let _ = child.wait();
            return none;
        }
        let pipe_fd = pager_stdin.as_raw_fd();
        unsafe { dup2(pipe_fd, stdout_fd) };
        // The pipe now lives at fd 1 too. Close `pager_stdin`'s own fd (not
        // via its Drop, which would also close fd 1's copy) so this process
        // holds the pipe's write end exactly once -- otherwise that extra
        // open fd keeps the pipe open after we dup2 stdout back, and `less`
        // never sees EOF, hanging on "Waiting for data...".
        unsafe { close(pipe_fd) };
        std::mem::forget(pager_stdin);

        Pager { child: Some(child), saved_stdout: Some(saved) }
    }
}

impl Drop for Pager {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else { return };
        std::io::stdout().flush().ok();
        if let Some(saved) = self.saved_stdout {
            let stdout_fd = std::io::stdout().as_raw_fd();
            unsafe {
                dup2(saved, stdout_fd);
                close(saved);
            }
        }
        let _ = child.wait();
    }
}

pub(crate) const CHECK: &str = "✓";
pub(crate) const MISS: &str = "·";
/// Not this commit, but this patch: a cherry-pick or a rebase's copy.
pub(crate) const EQUIV: &str = "≈";
/// A `-x` cherry-pick trailer on another branch names this commit as its source.
pub(crate) const TRAILER: &str = "←";
/// Another branch has a commit with the same author-email, author-date, and subject.
pub(crate) const FINGERPRINT: &str = "~";
pub(crate) const ELLIPSIS: char = '…';

/// ANSI blue.
pub(crate) const BLUE: &str = "34";
/// ANSI magenta.
pub(crate) const MAGENTA: &str = "35";

/// Material Design 500-weight colors (256-color approximations), cycled
/// across the commits table's header labels so each column name is its own
/// color and the eye can jump straight to the one it wants instead of
/// reading a flat dim row left to right.
pub(crate) const HEADER_COLORS: &[&str] = &[
    "1;38;5;203", // Red 500
    "1;38;5;127", // Purple 500
    "1;38;5;33",  // Blue 500
    "1;38;5;30",  // Teal 500
    "1;38;5;71",  // Green 500
    "1;38;5;208", // Orange 500
];

/// The header over `--pick-id`'s shas.
pub(crate) const PICK_HEAD: &str = "pick";

/// A full sha cut to `n`, the way git abbreviates one.
///
/// No uniqueness check: git's own `--short` picks a length for the repo, and
/// this borrows it rather than second-guessing it per sha.
pub(crate) fn abbrev(sha: &str, n: usize) -> String {
    sha.chars().take(n).collect()
}

/// The narrowest `list` will cut the branch column to, however tight the
/// terminal gets. Wide enough to hold the header and a name's distinguishing
/// head; past this the row is better off wrapping.
pub(crate) const BRANCH_MIN: usize = 12;

/// The default cut for a mark column's branch-name header in `commits`,
/// unbounded by a terminal the way the subject column is (there is no row
/// left of it competing for space). Wide enough for most branch names whole;
/// past it an issue-shaped one would rather give up its tail than push the
/// marks and subject off the edge. `--branch-width full` opts out.
pub(crate) const BRANCH_HEAD_MAX: usize = 24;

/// Below this, a truncated subject says nothing; let the line wrap instead.
pub(crate) const MIN_TEXTW: usize = 24;
/// Enough for a full name; past it, the subject has the better claim.
pub(crate) const AUTHOR_MAX: usize = 16;

/// The terminal's width, or None when stdout is not one.
///
/// No ioctl, so no libc: `tput` reads the same terminfo git's own pager does,
/// and COLUMNS wins when a shell exports it. A terminal that answers neither
/// gets the 80 every terminal is at least as wide as.
pub(crate) fn term_width(is_tty: bool) -> Option<usize> {
    if !is_tty {
        return None;
    }
    if let Some(n) = std::env::var("COLUMNS").ok().and_then(|v| v.parse().ok()) {
        return Some(n);
    }
    let out = Command::new("tput")
        .arg("cols")
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok());
    Some(out.unwrap_or(80))
}

/// An upper bound on the terminal columns `s` will occupy.
///
/// Deliberately not a width table: getting that right needs Unicode data this
/// crate has no dependency for, and a wrong *padding* silently breaks a column.
/// So no column is padded by this -- only the last one is budgeted by it, where
/// over-estimating costs a few characters of subject and under-estimating could
/// only ever wrap. Every non-ASCII char is assumed double-width, which is true
/// of the ones that actually turn up in subjects (emoji, CJK) and merely
/// pessimistic for the rest (accented Latin).
pub(crate) fn width_bound(s: &str) -> usize {
    s.chars()
        .map(|c| match c {
            // Our own marker, and known narrow: counting it wide would let a
            // budgeted string exceed the budget it was just cut to.
            ELLIPSIS => 1,
            c if c.is_ascii() => 1,
            _ => 2,
        })
        .sum()
}

/// Cut `s` to `max` characters, ending in an ellipsis when anything was lost.
pub(crate) fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(1);
    s.chars().take(keep).chain(std::iter::once(ELLIPSIS)).collect()
}

/// Cut `s` to fit `max` terminal columns by `width_bound`'s reckoning.
pub(crate) fn ellipsize_wide(s: &str, max: usize) -> String {
    if width_bound(s) <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    // The ellipsis needs a column of its own, so stop one short of the budget.
    for c in s.chars() {
        let w = if c.is_ascii() { 1 } else { 2 };
        if used + w > max.saturating_sub(1) {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push(ELLIPSIS);
    out
}

/// Split `s` at the last column that fits `max`, preferring a word boundary.
///
/// The tail keeps no leading space: it starts a line of its own, where a space
/// would push the text a column out of the subject column it is indented to.
pub(crate) fn split_at_width(s: &str, max: usize) -> (&str, &str) {
    let mut used = 0;
    let mut end = s.len();
    for (i, c) in s.char_indices() {
        let w = if c.is_ascii() { 1 } else { 2 };
        if used + w > max {
            end = i;
            break;
        }
        used += w;
    }
    // A word longer than the whole budget has no boundary to break at -- a sha
    // or a URL, usually -- so it is cut mid-word rather than left to overflow.
    match s[..end].rfind(' ').filter(|b| *b > 0) {
        Some(b) => (&s[..b], s[b + 1..].trim_start()),
        None => (&s[..end], s[end..].trim_start()),
    }
}

/// Break `s` into at most `lines` lines of `max` columns by `width_bound`.
///
/// Only the last line an allowance permits is ellipsized, and only when the
/// subject outruns it: the ellipsis means "there was more", so a line that
/// wrapped must not wear one.
pub(crate) fn wrap_wide(s: &str, max: usize, lines: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    loop {
        if width_bound(rest) <= max {
            out.push(rest.to_string());
            return out;
        }
        // The last line this allowance buys: what is left has to fit in it.
        if out.len() + 1 >= lines {
            out.push(ellipsize_wide(rest, max));
            return out;
        }
        let (head, tail) = split_at_width(rest, max);
        // No progress means no boundary and no room -- a budget under one
        // character's width. Cut the loop rather than spin on it.
        if head.is_empty() {
            out.push(ellipsize_wide(rest, max));
            return out;
        }
        out.push(head.to_string());
        rest = tail;
    }
}


// ---------------------------------------------------------------------------
// Prompt
// ---------------------------------------------------------------------------

/// Print a prompt to stderr and read a yes/no answer from stdin. Requires the
/// user to type and press Enter; empty or anything but y/yes is No. EOF / no
/// tty is No.
pub(crate) fn confirm(prompt: &str) -> Result<bool, String> {
    eprint!("{prompt}");
    std::io::stderr().flush().ok();
    let mut line = String::new();
    let n = std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Ok(false); // EOF / no tty -> treat as No
    }
    let a = line.trim().to_ascii_lowercase();
    Ok(a == "y" || a == "yes")
}

// ---------------------------------------------------------------------------
// Text matching
// ---------------------------------------------------------------------------

/// Paint every case-folded occurrence of `needle` in `s` with `code`, leaving
/// the rest of the string under `base`.
///
/// Widths are measured on the plain text and color applied after -- the rule
/// the table already follows -- so a highlight never shifts a column. `base`
/// exists because a file block is already dim: the RESET that ends a highlight
/// would otherwise drop the rest of the line out of dim, and the block would
/// brighten from its first match onward.
///
/// The search runs on a lowercased copy but the offsets are mapped back, so
/// what prints keeps the case it was written in -- lowercasing can change a
/// string's byte length, and assuming it does not would slice mid-character.
pub(crate) fn paint_matches(s: &str, needle: &str, code: &str, base: &str, on: bool) -> String {
    let tint = |t: &str| if base.is_empty() { t.to_string() } else { paint(t, base, on) };
    if !on || needle.is_empty() {
        return tint(s);
    }
    // Where each byte of the lowercased copy came from, plus the end, so both
    // bounds of a match always map back to a real offset in `s`.
    let mut lower = String::with_capacity(s.len());
    let mut origin: Vec<usize> = Vec::with_capacity(s.len() + 1);
    for (i, c) in s.char_indices() {
        for l in c.to_lowercase() {
            lower.push(l);
            origin.resize(lower.len(), i);
        }
    }
    origin.push(s.len());

    let needle = needle.to_lowercase();
    let mut out = String::new();
    let mut cut = 0;
    let mut from = 0;
    while let Some(rel) = lower[from..].find(&needle) {
        let (lo, hi) = (from + rel, from + rel + needle.len());
        let (start, end) = (origin[lo], origin[hi]);
        // A folding that changes length can put a bound inside a character.
        // Skip such a match rather than slice one in half.
        if start >= cut && s.is_char_boundary(start) && s.is_char_boundary(end) {
            out.push_str(&tint(&s[cut..start]));
            out.push_str(&paint(&s[start..end], code, on));
            cut = end;
        }
        from = hi.max(lo + 1);
    }
    if cut == 0 {
        return tint(s);
    }
    out.push_str(&tint(&s[cut..]));
    out
}

/// True when every char of `needle` appears in `hay`, in order.
pub(crate) fn is_subseq(hay: &str, needle: &str) -> bool {
    let mut chars = hay.chars();
    'outer: for nc in needle.chars() {
        for hc in chars.by_ref() {
            if hc == nc {
                continue 'outer;
            }
        }
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_wraps_only_when_on() {
        assert_eq!(paint("x", GREEN, false), "x");
        assert_eq!(paint("x", GREEN, true), "\x1b[32mx\x1b[0m");
    }


    #[test]
    fn ellipsize_only_cuts_what_overflows() {
        assert_eq!(ellipsize("short", 10), "short");
        // Exactly the budget is not an overflow: nothing is lost, so no marker.
        assert_eq!(ellipsize("abcde", 5), "abcde");
        assert_eq!(ellipsize("abcdef", 5), "abcd…");
        // The marker costs a character, so the result still fits the budget.
        assert_eq!(ellipsize("abcdef", 5).chars().count(), 5);
        // Counted in characters, not bytes: a multi-byte subject must not be
        // cut mid-codepoint, nor counted as if it were wider than it looks.
        assert_eq!(ellipsize("héllo wörld", 20), "héllo wörld");
        assert_eq!(ellipsize("héllo wörld", 7), "héllo …");
        assert_eq!(ellipsize("日本語のコミット", 4), "日本語…");
    }


    #[test]
    fn wrapping_a_subject_never_exceeds_its_budget() {
        let s = "fix(portal-sales): validate the uploaded masterfile rows";
        // Every line fits, and the words survive the break.
        for line in wrap_wide(s, 20, 3) {
            assert!(width_bound(&line) <= 20, "{line:?}");
        }
        assert_eq!(wrap_wide(s, 20, usize::MAX).join(" "), s, "full loses nothing");
        // One line is the old behavior exactly: cut, with an ellipsis.
        let one = wrap_wide(s, 20, 1);
        assert_eq!(one.len(), 1);
        assert!(one[0].ends_with(ELLIPSIS), "{one:?}");
        // Only the last line an allowance permits wears the ellipsis: the
        // others wrapped, and an ellipsis there would claim text was lost.
        let two = wrap_wide(s, 20, 2);
        assert_eq!(two.len(), 2);
        assert!(!two[0].ends_with(ELLIPSIS), "{two:?}");
        assert!(two[1].ends_with(ELLIPSIS), "{two:?}");
        // A subject that fits takes one line whatever it is allowed.
        assert_eq!(wrap_wide("short one", 20, 3), vec!["short one"]);
        // An emoji is two columns wide and one char: the budget counts columns.
        for line in wrap_wide("🚀🚀🚀🚀🚀🚀 ship it", 6, 4) {
            assert!(width_bound(&line) <= 6, "{line:?}");
        }
        // A word longer than the budget has no boundary to break at, so it is
        // cut rather than left to overflow -- and the wrap still terminates.
        let long = wrap_wide("aaaaaaaaaaaaaaaaaaaaaaaa tail", 8, usize::MAX);
        assert!(long.len() > 1, "{long:?}");
        assert!(long.iter().all(|l| width_bound(l) <= 8), "{long:?}");
        assert_eq!(long.last().unwrap(), "tail");
    }


    #[test]
    fn wrapped_lines_start_at_the_subject_column() {
        // A leading space would push the text one column past the indent the
        // continuation line is padded to -- the table failing to line up.
        let (head, tail) = split_at_width("feat: add the thing", 10);
        assert_eq!(head, "feat: add");
        assert_eq!(tail, "the thing");
    }


    #[test]
    fn width_bound_never_under_counts_a_subject() {
        // ASCII is exact.
        assert_eq!(width_bound("abc"), 3);
        // An emoji is two columns wide but one char: counting chars is what
        // shifted every column after an emoji subject.
        assert_eq!("🚀 fix".chars().count(), 5);
        assert_eq!(width_bound("🚀 fix"), 6);
        // CJK, likewise.
        assert_eq!(width_bound("日本語"), 6);
        // Pessimistic on accented Latin -- costs a character of subject, never
        // an overflow, which is the safe direction for a budget.
        assert_eq!(width_bound("é"), 2);
    }


    #[test]
    fn ellipsize_wide_budgets_in_columns_not_chars() {
        assert_eq!(ellipsize_wide("abcdef", 10), "abcdef");
        assert_eq!(ellipsize_wide("abcdef", 4), "abc…");
        // Two emoji = 4 columns, so a 4-column budget fits them whole: exactly
        // the budget is not an overflow.
        assert_eq!(ellipsize_wide("🚀🚀", 4), "🚀🚀");
        // Never cut mid-emoji: the char is atomic, so a budget that cannot fit
        // it drops it rather than splitting it.
        assert_eq!(ellipsize_wide("🚀🚀", 3), "🚀…");
        // The result always fits the budget it was given.
        for max in 2..12 {
            let out = ellipsize_wide("🚀 (ci): add validate stage", max);
            assert!(width_bound(&out) <= max, "{max}: {out:?}");
        }
    }


    #[test]
    fn paint_matches_lights_every_hit_and_nothing_else() {
        let amber = |s: &str, n: &str| paint_matches(s, n, MATCH, "", true);
        // Color off is the plain string, whatever matched.
        assert_eq!(paint_matches("fix: the thing", "the", MATCH, "", false), "fix: the thing");
        // No match, and an empty needle, both leave the string alone -- an
        // empty needle matches at every position, which would paint nothing
        // and everything at once.
        assert_eq!(amber("fix: the thing", "zzz"), "fix: the thing");
        assert_eq!(amber("fix: the thing", ""), "fix: the thing");
        // The match is wrapped and the rest is untouched.
        assert_eq!(amber("a fix b", "fix"), format!("a \x1b[{MATCH}mfix\x1b[0m b"));
        // Every occurrence, not just the first.
        assert_eq!(amber("fix fix", "fix").matches("\x1b[0m").count(), 2);
        // Case-folded, and the original case is what prints.
        assert_eq!(amber("FIX it", "fix"), format!("\x1b[{MATCH}mFIX\x1b[0m it"));
        // Either end of the string, where an off-by-one would panic or drop text.
        assert_eq!(amber("fix", "fix"), format!("\x1b[{MATCH}mfix\x1b[0m"));
        assert_eq!(amber("a fix", "fix"), format!("a \x1b[{MATCH}mfix\x1b[0m"));
        // Multi-byte text must not be sliced mid-character.
        assert!(amber("héllo wörld", "wörld").contains("wörld"));
        assert!(amber("日本語のコミット", "コミット").contains("コミット"));
        // Painting never loses a character: strip the escapes and it is the
        // string that went in.
        for (hay, needle) in [("fix: héllo", "héllo"), ("🚀 fix 🚀", "fix"), ("aaa", "a")] {
            let plain = amber(hay, needle)
                .replace(&format!("\x1b[{MATCH}m"), "")
                .replace("\x1b[0m", "");
            assert_eq!(plain, hay, "{hay:?} / {needle:?}");
        }
    }

    #[test]
    fn paint_matches_keeps_the_rest_of_a_dim_line_dim() {
        // A file block is dim before it is highlighted, so the text after a
        // match has to be put back under DIM -- the highlight's RESET ends the
        // dim as well as the amber.
        let out = paint_matches("\tM  src/ui.rs  +1  -0", "ui.rs", MATCH, DIM, true);
        let tail = &out[out.rfind("\x1b[0m").unwrap()..];
        assert!(out.matches(&format!("\x1b[{DIM}m")).count() >= 2, "{out:?}");
        assert!(tail.starts_with("\x1b[0m"), "{out:?}");
        // And with color off it is the bare line, no escapes at all.
        assert_eq!(
            paint_matches("\tM  src/ui.rs", "ui.rs", MATCH, DIM, false),
            "\tM  src/ui.rs"
        );
    }

    #[test]
    fn subseq_matches_in_order() {
        assert!(is_subseq("feature-login", "flogin"));
        assert!(is_subseq("feature-login", "feat"));
        assert!(!is_subseq("feature-login", "zzz"));
        assert!(!is_subseq("abc", "cba"));
    }

    #[test]
    fn a_piped_table_has_no_width_to_fit() {
        // Not a terminal: the subject is the payload for `| grep`, so it must
        // arrive whole however long it is.
        assert_eq!(term_width(false), None);
    }

}
