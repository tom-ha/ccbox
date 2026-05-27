//! ANSI escape construction and ANSI-stripping utilities.

use once_cell::sync::Lazy;
use regex::Regex;

pub type Rgb = (u8, u8, u8);

/// Matches CSI SGR-style sequences (`\x1b[<numbers and semicolons>m`).
pub static ANSI_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap());

pub fn fg_rgb(rgb: Rgb) -> String {
    let (r, g, b) = rgb;
    format!("\x1b[38;2;{r};{g};{b}m")
}

pub fn bg_rgb(rgb: Rgb) -> String {
    let (r, g, b) = rgb;
    format!("\x1b[48;2;{r};{g};{b}m")
}

pub fn fg_256(idx: u8) -> String {
    format!("\x1b[38;5;{idx}m")
}

pub fn bg_256(idx: u8) -> String {
    format!("\x1b[48;5;{idx}m")
}

/// Strip all CSI SGR escapes from a string, returning a borrowed `Cow` when
/// the input contains no escapes.
pub fn strip_ansi(s: &str) -> std::borrow::Cow<'_, str> {
    ANSI_RE.replace_all(s, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fg_rgb_builds_expected_escape() {
        assert_eq!(fg_rgb((10, 20, 30)), "\x1b[38;2;10;20;30m");
    }

    #[test]
    fn fg_256_builds_expected_escape() {
        assert_eq!(fg_256(75), "\x1b[38;5;75m");
    }

    #[test]
    fn strip_ansi_removes_escapes_only() {
        assert_eq!(strip_ansi("\x1b[31mhi\x1b[0m"), "hi");
        assert_eq!(strip_ansi("hi"), "hi");
        assert_eq!(strip_ansi("\x1b[1;38;5;75mok\x1b[0m!"), "ok!");
    }
}
