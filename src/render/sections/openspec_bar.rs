//! `openspec_bar` + `spec_gradient_bar` — coloured progress bar for each
//! OpenSpec change directory.

use crate::ansi::Rgb;
use crate::glyphs::{bar, BOLD, CLR_WHITE_BRT, ITALIC, RESET};
use crate::render::Renderer;

pub const SPEC_MID_MIN_WIDTH: i32 = 20;

impl Renderer {
    /// Interpolate inside the `idx`-th spec gradient at parameter `t ∈ [0, 1]`.
    pub fn spec_rgb_at(&self, t: f64, idx: usize, three_stops: bool) -> Rgb {
        let palette = self.theme.spec_gradients;
        let entry = palette[idx % palette.len()];
        let stops: &[Rgb] = if three_stops {
            &[entry.0, entry.1, entry.2]
        } else {
            &[entry.0, entry.2]
        };
        let n = stops.len();
        let seg = t.clamp(0.0, 1.0) * (n as f64 - 1.0);
        let s0 = (seg as usize).min(n - 2);
        let s1 = s0 + 1;
        let u = seg - s0 as f64;
        let c0 = stops[s0];
        let c1 = stops[s1];
        let lerp = |a: u8, b: u8| -> i32 {
            (a as f64 + (b as f64 - a as f64) * u).trunc() as i32
        };
        (lerp(c0.0, c1.0).clamp(0, 255) as u8, lerp(c0.1, c1.1).clamp(0, 255) as u8, lerp(c0.2, c1.2).clamp(0, 255) as u8)
    }

    pub fn spec_gradient_bar(&self, filled: i32, bar_w: i32, idx: usize) -> String {
        if filled <= 0 || bar_w <= 0 {
            return String::new();
        }
        let denom = (bar_w - 1).max(1) as f64;
        let three_stops = bar_w >= SPEC_MID_MIN_WIDTH;
        let mut out = String::new();
        for i in 0..filled {
            let (r, g, b) = self.spec_rgb_at(i as f64 / denom, idx, three_stops);
            out.push_str(&format!("\x1b[38;2;{r};{g};{b}m{}", bar::HEAVY));
        }
        out
    }

    pub fn openspec_bar(&self, name: &str, done: i32, total: i32, box_width: i32, title_w: i32, idx: usize) -> String {
        let pct = if total > 0 { done * 100 / total } else { 0 };
        let title = if (name.chars().count() as i32) > title_w {
            let take = (title_w - 3).max(1) as usize;
            let trimmed: String = name.chars().take(take).collect();
            format!("{trimmed}...")
        } else {
            let n = (title_w as usize).saturating_sub(name.chars().count());
            format!("{name}{}", " ".repeat(n))
        };
        let suffix_visible = 7 + done.to_string().len() as i32 + total.to_string().len() as i32;
        let bar_w = ((box_width - 3) - (title_w + 1) - suffix_visible).max(4);
        let filled = if total > 0 { done * bar_w / total } else { 0 };
        let mut empty = bar_w - filled;

        let mut bar_filled = self.spec_gradient_bar(filled, bar_w, idx);
        if filled > 0 && empty > 0 {
            let denom = (bar_w - 1).max(1) as f64;
            let three_stops = bar_w >= SPEC_MID_MIN_WIDTH;
            let (cr, cg, cb) = self.spec_rgb_at(filled as f64 / denom, idx, three_stops);
            let r = (cr as f64 * 0.45).trunc() as i32;
            let g = (cg as f64 * 0.45).trunc() as i32;
            let b = (cb as f64 * 0.45).trunc() as i32;
            bar_filled.push_str(&format!("\x1b[38;2;{r};{g};{b}m{}", bar::HEAVY));
            empty -= 1;
        }
        let bar_empty = format!("{}{}\x1b[0m", self.theme.spec_empty_ansi, bar::HEAVY.repeat(empty as usize));

        format!(
            "{CLR_WHITE_BRT}{ITALIC}{title}{RESET}{RESET} {bar_filled}{RESET}{bar_empty} {}{done}/{total}{RESET} {BOLD}{:>3}%{RESET}",
            self.theme.label,
            pct,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::width::visible_width;

    #[test]
    fn spec_gradient_bar_filled_chars_match_filled_count() {
        let r = Renderer::default();
        let s = r.spec_gradient_bar(5, 20, 0);
        // each glyph is 1 cell, no other content.
        assert_eq!(visible_width(&s), 5);
    }

    #[test]
    fn spec_gradient_bar_empty_returns_empty_string() {
        let r = Renderer::default();
        assert!(r.spec_gradient_bar(0, 20, 0).is_empty());
        assert!(r.spec_gradient_bar(5, 0, 0).is_empty());
    }

    #[test]
    fn openspec_bar_shows_pct_and_counts() {
        let r = Renderer::default();
        let s = r.openspec_bar("some-example-change", 3, 5, 80, 25, 0);
        let plain = crate::ansi::strip_ansi(&s);
        assert!(plain.contains("3/5"), "{plain}");
        assert!(plain.contains("60%"), "{plain}");
    }

    #[test]
    fn openspec_bar_truncates_title_with_ellipsis() {
        let r = Renderer::default();
        let s = r.openspec_bar("a-very-very-very-long-name-for-a-change", 1, 2, 80, 12, 0);
        let plain = crate::ansi::strip_ansi(&s);
        assert!(plain.contains("..."), "{plain}");
    }
}
