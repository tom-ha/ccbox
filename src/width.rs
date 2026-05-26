//! Visible-width and ANSI-preserving middle ellipsis.
//!
//! Wide-char rule: U+1F300..U+1FAFF except the Supplemental Arrows-C block
//! U+1F800..U+1F8FF. CJK is treated as 1 column. This deliberately diverges
//! from `unicode-width` so the rendered statusline matches the column budget
//! the fixtures were captured against.

use crate::ansi::ANSI_RE;

/// Returns `true` for codepoints treated as 2-column.
pub fn is_wide(ch: char) -> bool {
    let cp = ch as u32;
    if (0x1F800..=0x1F8FF).contains(&cp) {
        return false;
    }
    (0x1F300..=0x1FAFF).contains(&cp)
}

/// Visible column count after stripping ANSI SGR escapes.
pub fn visible_width(s: &str) -> usize {
    let plain = ANSI_RE.replace_all(s, "");
    plain.chars().map(|c| if is_wide(c) { 2 } else { 1 }).sum()
}

/// Token used while scanning a string for middle-ellipsis: either an ANSI escape
/// or a single visible character.
enum Tok<'a> {
    Esc(&'a str),
    Ch(&'a str),
}

fn tokenize(text: &str) -> Vec<Tok<'_>> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if let Some(m) = ANSI_RE.find_at(text, i) {
            if m.start() == i {
                out.push(Tok::Esc(&text[m.start()..m.end()]));
                i = m.end();
                continue;
            }
        }
        // Advance one UTF-8 char.
        let rest = &text[i..];
        let ch_len = rest
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(1);
        out.push(Tok::Ch(&text[i..i + ch_len]));
        i += ch_len;
    }
    out
}

/// Concatenate a slice of tokens back into a string.
fn join(toks: &[&Tok<'_>]) -> String {
    let mut out = String::new();
    for t in toks {
        match t {
            Tok::Esc(s) | Tok::Ch(s) => out.push_str(s),
        }
    }
    out
}

/// Take a prefix consuming `n` visible columns, but always passing ANSI escapes
/// through (they have zero visible width).
fn take_prefix<'a>(toks: &'a [Tok<'a>], n_cols: usize) -> Vec<&'a Tok<'a>> {
    let mut out: Vec<&Tok> = Vec::new();
    let mut seen = 0usize;
    for t in toks {
        match t {
            Tok::Esc(_) => out.push(t),
            Tok::Ch(s) => {
                if seen >= n_cols {
                    break;
                }
                let w = s.chars().next().map(|c| if is_wide(c) { 2 } else { 1 }).unwrap_or(1);
                if seen + w > n_cols {
                    break;
                }
                seen += w;
                out.push(t);
            }
        }
    }
    out
}

/// Take a suffix consuming `n` visible columns (scanned right-to-left), keeping
/// all ANSI escapes encountered.
fn take_suffix<'a>(toks: &'a [Tok<'a>], n_cols: usize) -> Vec<&'a Tok<'a>> {
    let mut rev: Vec<&Tok> = Vec::new();
    let mut seen = 0usize;
    for t in toks.iter().rev() {
        match t {
            Tok::Esc(_) => rev.push(t),
            Tok::Ch(s) => {
                if seen >= n_cols {
                    break;
                }
                let w = s.chars().next().map(|c| if is_wide(c) { 2 } else { 1 }).unwrap_or(1);
                if seen + w > n_cols {
                    break;
                }
                seen += w;
                rev.push(t);
            }
        }
    }
    rev.reverse();
    rev
}

/// Truncate the middle of `text` with `…` so the visible width is at most
/// `max_w`. ANSI escapes are preserved as zero-width passthrough.
pub fn middle_ellipsis(text: &str, max_w: usize) -> String {
    if max_w <= 1 {
        return "…".to_string();
    }
    if visible_width(text) <= max_w {
        return text.to_string();
    }

    let toks = tokenize(text);
    let left_vis = (max_w - 1) / 2;
    let right_vis = max_w - 1 - left_vis;

    let mut prefix = take_prefix(&toks, left_vis);
    let suffix = take_suffix(&toks, right_vis);

    let mut result = String::new();
    result.push_str(&join(&prefix));
    result.push('…');
    result.push_str(&join(&suffix));

    if visible_width(&result) <= max_w {
        return result;
    }

    // Wide-char overshoot: drop one visible char from the prefix end.
    for j in (0..prefix.len()).rev() {
        if !matches!(prefix[j], Tok::Esc(_)) {
            prefix.remove(j);
            break;
        }
    }
    let mut result = String::new();
    result.push_str(&join(&prefix));
    result.push('…');
    result.push_str(&join(&suffix));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- is_wide ------------------------------------------------------------
    #[test]
    fn ascii_not_wide() {
        assert!(!is_wide('a'));
    }

    #[test]
    fn cjk_not_wide() {
        assert!(!is_wide('中'));
    }

    #[test]
    fn emoji_is_wide() {
        assert!(is_wide('🎨'));
    }

    #[test]
    fn is_wide_lower_boundary() {
        assert!(is_wide('\u{1F300}'));
    }

    #[test]
    fn is_wide_upper_boundary() {
        assert!(is_wide('\u{1FAFF}'));
    }

    #[test]
    fn is_wide_just_below_range() {
        assert!(!is_wide('\u{1F2FF}'));
    }

    #[test]
    fn is_wide_just_above_range() {
        assert!(!is_wide('\u{1FB00}'));
    }

    #[test]
    fn is_wide_excludes_arrows_c_block() {
        assert!(!is_wide('\u{1F800}'));
        assert!(!is_wide('\u{1F8FF}'));
    }

    // --- visible_width ------------------------------------------------------
    #[test]
    fn visible_width_empty() {
        assert_eq!(visible_width(""), 0);
    }

    #[test]
    fn visible_width_plain_text() {
        assert_eq!(visible_width("hello"), 5);
    }

    #[test]
    fn visible_width_ansi_wrapped() {
        assert_eq!(visible_width("\x1b[31mhi\x1b[0m"), 2);
    }

    #[test]
    fn visible_width_emoji_counts_as_two() {
        assert_eq!(visible_width("a🎨b"), 4);
    }

    #[test]
    fn visible_width_ansi_plus_emoji() {
        assert_eq!(visible_width("\x1b[38;5;75m🎨\x1b[0m"), 2);
    }

    // --- middle_ellipsis ----------------------------------------------------
    #[test]
    fn me_fits_no_truncation() {
        assert_eq!(middle_ellipsis("hello", 10), "hello");
    }

    #[test]
    fn me_exact_fit() {
        assert_eq!(middle_ellipsis("hello", 5), "hello");
    }

    #[test]
    fn me_ascii_truncates_width_respected() {
        let r = middle_ellipsis("abcdefghij", 7);
        assert!(visible_width(&r) <= 7);
        assert!(r.contains('…'));
    }

    #[test]
    fn me_ascii_truncates_contains_both_ends() {
        let r = middle_ellipsis("abcdefghij", 7);
        assert!(r.starts_with("abc"), "{r}");
        assert!(r.ends_with("ij"), "{r}");
    }

    #[test]
    fn me_edge_zero_width() {
        assert_eq!(middle_ellipsis("hello", 0), "…");
    }

    #[test]
    fn me_edge_one_width() {
        assert_eq!(middle_ellipsis("hello", 1), "…");
    }

    #[test]
    fn me_edge_two_width() {
        let r = middle_ellipsis("hello", 2);
        assert!(visible_width(&r) <= 2);
        assert!(r.contains('…'));
    }

    #[test]
    fn me_ansi_wrapped_visible_width_respected() {
        let colored = format!("\x1b[31m{}\x1b[0m", "abcdefghij");
        let r = middle_ellipsis(&colored, 7);
        assert!(visible_width(&r) <= 7);
        assert!(r.contains('…'));
    }

    #[test]
    fn me_ansi_wrapped_escapes_preserved() {
        let colored = format!("\x1b[31m{}\x1b[0m", "abcdefghij");
        let r = middle_ellipsis(&colored, 7);
        assert!(r.contains("\x1b["));
    }

    #[test]
    fn me_ansi_escapes_not_split() {
        let colored = "\x1b[31mLEFT_PART_THAT_IS_LONG_MIDDLE_RIGHT_PART\x1b[0m";
        let r = middle_ellipsis(colored, 20);
        // No truncated escape like `\x1b[3` (without `m`) should appear.
        let bytes = r.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == 0x1b {
                // Find next 'm' within a short window.
                let end = (i..bytes.len()).find(|&k| bytes[k] == b'm');
                assert!(end.is_some(), "orphan escape in {r:?}");
                i = end.unwrap() + 1;
            } else {
                i += 1;
            }
        }
    }
}
