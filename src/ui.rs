use std::process::{Command, Stdio};

// ---------------------------------------------------------------------------
// Color, status, and metadata (no dependencies; ANSI on a TTY only)
// ---------------------------------------------------------------------------

pub(crate) const RESET: &str = "\x1b[0m";
pub(crate) const GREEN: &str = "32";
pub(crate) const YELLOW: &str = "33";
pub(crate) const RED: &str = "31";
pub(crate) const DIM: &str = "2";

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

pub(crate) const CHECK: &str = "✓";
pub(crate) const MISS: &str = "·";
/// Not this commit, but this patch: a cherry-pick or a rebase's copy.
pub(crate) const EQUIV: &str = "≈";
pub(crate) const ELLIPSIS: char = '…';

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
    fn a_piped_table_has_no_width_to_fit() {
        // Not a terminal: the subject is the payload for `| grep`, so it must
        // arrive whole however long it is.
        assert_eq!(term_width(false), None);
    }

}
