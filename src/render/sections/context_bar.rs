//! `context_bar`, `context_line(_compact)`, `_empty_section` blend helper.

use once_cell::sync::Lazy;
use regex::Regex;

use crate::consts::AUTOCOMPACT_RATIO;
use crate::glyphs::{bar, BOLD, RESET};
use crate::input::session::ContextWindow;
use crate::render::format::fmt_tok;
use crate::render::Renderer;
use crate::width::visible_width;

static EMPTY_FADE_256: Lazy<Regex> = Lazy::new(|| Regex::new(r"\x1b\[38;5;(\d+)m").unwrap());
static EMPTY_FADE_RGB: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\x1b\[38;2;(\d+);(\d+);(\d+)m").unwrap());

impl Renderer {
    /// 30-cell coloured/empty bar reflecting `fill_ratio` ∈ [0, 1].
    pub fn context_bar(&self, fill_ratio: f64) -> String {
        let ratio = fill_ratio.clamp(0.0, 1.0);
        let filled = (ratio * 30.0).trunc() as usize;
        let bar_filled: String = bar::FILLED.repeat(filled);
        let bar_empty: String = bar::EMPTY.repeat(30 - filled);
        let color = if ratio >= 0.9 {
            self.theme.alert
        } else if ratio >= 0.7 {
            self.theme.warn
        } else {
            self.theme.safe
        };
        format!(
            "{color}{bar_filled}{RESET}{}{bar_empty}{RESET}",
            self.theme.bar_empty
        )
    }

    /// 3-step fade ramp from a darker shade to `bar_empty`, so the seam
    /// between the filled and empty halves looks smooth.
    fn empty_fade_colors(&self) -> Vec<String> {
        if let Some(caps) = EMPTY_FADE_256.captures(self.theme.bar_empty) {
            let n: i32 = caps[1].parse().unwrap_or(238);
            return [6, 4, 2]
                .into_iter()
                .map(|k| {
                    let v = (n - k).max(232);
                    format!("\x1b[38;5;{v}m")
                })
                .collect();
        }
        if let Some(caps) = EMPTY_FADE_RGB.captures(self.theme.bar_empty) {
            let r: f64 = caps[1].parse().unwrap_or(0.0);
            let g: f64 = caps[2].parse().unwrap_or(0.0);
            let b: f64 = caps[3].parse().unwrap_or(0.0);
            return [0.3, 0.5, 0.7]
                .into_iter()
                .map(|k| {
                    let r = (r * k).trunc() as i32;
                    let g = (g * k).trunc() as i32;
                    let b = (b * k).trunc() as i32;
                    format!("\x1b[38;2;{r};{g};{b}m")
                })
                .collect();
        }
        vec![self.theme.bar_empty.to_string(); 3]
    }

    pub fn empty_section(&self, empty: i32, blend: bool) -> String {
        if empty <= 0 {
            return String::new();
        }
        let empty = empty as usize;
        if !blend {
            return format!("{}{}", self.theme.bar_empty, bar::EMPTY.repeat(empty));
        }
        let fade = self.empty_fade_colors();
        let n = fade.len().min(empty);
        let mut out = String::new();
        for i in 0..n {
            out.push_str(&fade[i]);
            out.push_str(bar::EMPTY);
        }
        if empty > n {
            out.push_str(self.theme.bar_empty);
            out.push_str(&bar::EMPTY.repeat(empty - n));
        }
        out
    }

    /// Full-width context line: `ctx  <tokens> of <window> (<pct>%)  <bar>`.
    pub fn context_line(&self, ctx: &ContextWindow, available: i32) -> String {
        let total_tokens = ctx.used_tokens();
        let pct = ctx.used_pct().unwrap_or(0.0);
        let fill_ratio = pct / 100.0;
        let t = self.theme;
        let clr = self.risk_zone_color_for_ratio(fill_ratio / AUTOCOMPACT_RATIO);

        let window_label = if ctx.context_window_size > 0 {
            fmt_tok(ctx.context_window_size)
        } else {
            "?".to_string()
        };
        let prefix = format!(
            " {}ctx{RESET}  {}{}{RESET}{} of {}{}{RESET} {clr}{BOLD}({:.0}%){RESET}  ",
            t.label,
            t.dim_green,
            fmt_tok(total_tokens),
            t.label,
            t.dim_green,
            window_label,
            pct,
        );
        let bar_slot = (available - (visible_width(&prefix) as i32)).max(4);
        let bar = build_bar(self, fill_ratio, bar_slot);
        format!("{prefix}{bar}")
    }

    /// Compact context line for narrow layouts: `ctx <tokens>/<window> <pct>% <bar>`.
    pub fn context_line_compact(&self, ctx: &ContextWindow, available: i32) -> String {
        let total_tokens = ctx.used_tokens();
        let pct = ctx.used_pct().unwrap_or(0.0);
        let fill_ratio = pct / 100.0;
        let t = self.theme;
        let clr = self.risk_zone_color_for_ratio(fill_ratio / AUTOCOMPACT_RATIO);

        let window_label = if ctx.context_window_size > 0 {
            fmt_tok(ctx.context_window_size)
        } else {
            "?".to_string()
        };
        let prefix = format!(
            " {}ctx{RESET} {}{}/{}{RESET} {clr}{BOLD}{:.0}%{RESET} ",
            t.label,
            t.dim_green,
            fmt_tok(total_tokens),
            window_label,
            pct,
        );
        let bar_slot = (available - (visible_width(&prefix) as i32)).max(4);
        let bar = build_bar(self, fill_ratio, bar_slot);
        format!("{prefix}{bar}")
    }
}

/// Build a bar of visible width `bar_slot` cells with the filled portion
/// taking `fill_ratio` of the slot. `gradient_bar` emits `filled` colored
/// cells plus one trailing MID cap glyph, so we reserve 1 cell for the cap
/// and let `empty_section` fill the rest.
fn build_bar(r: &Renderer, fill_ratio: f64, bar_slot: i32) -> String {
    if bar_slot <= 0 {
        return String::new();
    }
    let raw_filled = (fill_ratio * bar_slot as f64).trunc() as i32;
    let filled = raw_filled.clamp(0, bar_slot - 1);
    let cap_w = if filled > 0 { 1 } else { 0 };
    let empty = (bar_slot - filled - cap_w).max(0);
    let g = r.gradient();
    format!(
        "{}{RESET}{}{RESET}",
        g.gradient_bar(filled, bar_slot),
        r.empty_section(empty, filled > 0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_bar_low_uses_safe() {
        let r = Renderer::default();
        let b = r.context_bar(0.1);
        assert!(b.contains(r.theme.safe));
    }
    #[test]
    fn context_bar_high_uses_alert() {
        let r = Renderer::default();
        let b = r.context_bar(0.95);
        assert!(b.contains(r.theme.alert));
    }
    #[test]
    fn context_bar_visible_width_is_30() {
        let r = Renderer::default();
        for f in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert_eq!(visible_width(&r.context_bar(f)), 30);
        }
    }
    #[test]
    fn context_line_contains_token_count() {
        let r = Renderer::default();
        let mut ctx = ContextWindow::default();
        ctx.current_usage.cache_read_input_tokens = 50_000;
        ctx.context_window_size = 200_000;
        let s = r.context_line(&ctx, 76);
        let plain = crate::ansi::strip_ansi(&s);
        assert!(plain.contains("50.0K"), "{plain}");
    }

    #[test]
    fn context_line_uses_of_word_and_window() {
        let r = Renderer::default();
        let mut ctx = ContextWindow::default();
        ctx.current_usage.cache_read_input_tokens = 120_000;
        ctx.context_window_size = 200_000;
        let s = r.context_line(&ctx, 100);
        let plain = crate::ansi::strip_ansi(&s);
        assert!(plain.contains("ctx"), "{plain}");
        assert!(
            plain.contains("120.0K of 200.0K"),
            "expected 'X of Y' form: {plain}"
        );
        assert!(plain.contains("(60%)"), "expected (pct%) form: {plain}");
    }

    #[test]
    fn context_line_unknown_window_renders_question_mark() {
        let r = Renderer::default();
        let mut ctx = ContextWindow::default();
        ctx.current_usage.cache_read_input_tokens = 75_000;
        ctx.context_window_size = 0;
        let s = r.context_line(&ctx, 100);
        let plain = crate::ansi::strip_ansi(&s);
        assert!(
            plain.contains("of ?"),
            "expected '?' for unknown window: {plain}"
        );
    }

    #[test]
    fn context_line_no_secondary_window_percentage() {
        // Previous design rendered both (pct_against_effective_limit) and a
        // secondary "% against window". The new design has only one.
        let r = Renderer::default();
        let mut ctx = ContextWindow::default();
        ctx.current_usage.cache_read_input_tokens = 50_000;
        ctx.context_window_size = 200_000;
        let s = r.context_line(&ctx, 100);
        let plain = crate::ansi::strip_ansi(&s);
        // Count occurrences of `%`. Should be exactly one (inside `(pct%)`).
        let count = plain.matches('%').count();
        assert_eq!(count, 1, "expected exactly one percentage: {plain}");
    }

    #[test]
    fn context_line_compact_uses_slash_form() {
        let r = Renderer::default();
        let mut ctx = ContextWindow::default();
        ctx.current_usage.cache_read_input_tokens = 30_000;
        ctx.context_window_size = 200_000;
        let s = r.context_line_compact(&ctx, 40);
        let plain = crate::ansi::strip_ansi(&s);
        assert!(plain.contains("ctx"), "{plain}");
        assert!(
            plain.contains("30.0K/200.0K"),
            "expected 'X/Y' compact form: {plain}"
        );
        assert!(plain.contains('%'), "{plain}");
        // Compact form should NOT include the word " of ".
        assert!(!plain.contains(" of "), "compact should omit 'of': {plain}");
    }

    #[test]
    fn context_line_compact_includes_pct() {
        let r = Renderer::default();
        let mut ctx = ContextWindow::default();
        ctx.current_usage.cache_read_input_tokens = 30_000;
        let s = r.context_line_compact(&ctx, 30);
        let plain = crate::ansi::strip_ansi(&s);
        assert!(plain.contains("%"), "{plain}");
    }

    #[test]
    fn context_line_visible_width_matches_available_across_ratios() {
        // Regression: the new context_line was 1 cell wider than `available`
        // at any non-zero fill ratio because it didn't compensate for the
        // bar::MID cap glyph emitted by gradient_bar.
        let r = Renderer::default();
        for &available in &[60i32, 80, 100, 137, 200] {
            for &total in &[0u64, 1_000, 75_000, 120_000, 160_000, 200_000, 500_000] {
                let mut ctx = ContextWindow::default();
                ctx.current_usage.cache_read_input_tokens = total;
                ctx.context_window_size = 200_000;
                let s = r.context_line(&ctx, available);
                assert_eq!(
                    visible_width(&s) as i32,
                    available,
                    "context_line width mismatch at available={available} total={total}",
                );
            }
        }
    }

    #[test]
    fn context_line_compact_visible_width_matches_available_across_ratios() {
        let r = Renderer::default();
        for &available in &[30i32, 40, 55, 80] {
            for &total in &[0u64, 1_000, 75_000, 120_000, 160_000, 200_000] {
                let mut ctx = ContextWindow::default();
                ctx.current_usage.cache_read_input_tokens = total;
                ctx.context_window_size = 200_000;
                let s = r.context_line_compact(&ctx, available);
                assert_eq!(
                    visible_width(&s) as i32,
                    available,
                    "context_line_compact width mismatch at available={available} total={total}",
                );
            }
        }
    }

    fn plain_line(ctx: &ContextWindow) -> String {
        crate::ansi::strip_ansi(&Renderer::default().context_line(ctx, 100)).into_owned()
    }

    #[test]
    fn context_line_1m_700k_renders_70_percent() {
        let mut ctx = ContextWindow::default();
        ctx.current_usage.cache_read_input_tokens = 700_000;
        ctx.context_window_size = 1_000_000;
        let plain = plain_line(&ctx);
        assert!(plain.contains("700.0K of 1.0M (70%)"), "{plain}");
    }

    #[test]
    fn context_line_sums_all_input_kinds() {
        let mut ctx = ContextWindow::default();
        ctx.current_usage.input_tokens = 3;
        ctx.current_usage.cache_creation_input_tokens = 17;
        ctx.current_usage.cache_read_input_tokens = 16_476;
        ctx.current_usage.output_tokens = 999;
        ctx.context_window_size = 200_000;
        let plain = plain_line(&ctx);
        assert!(plain.contains("16.5K of 200.0K (8%)"), "{plain}");
    }

    #[test]
    fn context_line_ignores_cumulative_session_totals() {
        let mut ctx = ContextWindow {
            total_input_tokens: 5_000_000,
            total_output_tokens: 400_000,
            context_window_size: 200_000,
            ..Default::default()
        };
        ctx.current_usage.cache_read_input_tokens = 50_000;
        let plain = plain_line(&ctx);
        assert!(plain.contains("50.0K of 200.0K (25%)"), "{plain}");
    }

    #[test]
    fn context_line_prefers_reported_used_percentage() {
        let mut ctx = ContextWindow::default();
        ctx.current_usage.cache_read_input_tokens = 50_000;
        ctx.context_window_size = 200_000;
        ctx.used_percentage = Some(31.0);
        let plain = plain_line(&ctx);
        assert!(plain.contains("(31%)"), "{plain}");
    }

    #[test]
    fn context_line_unknown_window_renders_zero_percent() {
        let mut ctx = ContextWindow::default();
        ctx.current_usage.cache_read_input_tokens = 75_000;
        let plain = plain_line(&ctx);
        assert!(plain.contains("75.0K of ? (0%)"), "{plain}");
    }

    #[test]
    fn context_line_near_autocompact_uses_warn_or_alert() {
        let r = Renderer::default();
        let mut ctx = ContextWindow::default();
        ctx.current_usage.cache_read_input_tokens = 700_000;
        ctx.context_window_size = 1_000_000;
        let s = r.context_line(&ctx, 100);
        assert!(
            s.contains(r.theme.warn) || s.contains(r.theme.alert),
            "expected warn/alert colour at 700K/1M, got: {s:?}"
        );
    }
}
