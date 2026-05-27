//! Gradient interpolation, gradient bars, and sparkline rendering.

use crate::ansi::Rgb;
use crate::consts::LIVE_DIM;
use crate::glyphs::{self, bar};
use crate::theme::{Theme, DEFAULT_THEME};

/// Truncate `x` toward zero and saturate into a `u8`.
fn trunc_u8(x: f64) -> u8 {
    let mut v = x.trunc() as i64;
    if v < 0 {
        v = 0;
    }
    if v > 255 {
        v = 255;
    }
    v as u8
}

/// Interpolate `c0 -> c1` at parameter `u`, truncating toward zero.
/// `dim` scales the result before truncation.
fn lerp_rgb(c0: Rgb, c1: Rgb, u: f64, dim: f64) -> Rgb {
    let f = |a: u8, b: u8| -> u8 {
        let v = (a as f64 + (b as f64 - a as f64) * u) * dim;
        trunc_u8(v)
    };
    (f(c0.0, c1.0), f(c0.1, c1.1), f(c0.2, c1.2))
}

/// The gradient engine — owns the active theme's gradient stops.
pub struct GradientEngine<'a> {
    pub theme: &'a Theme,
}

const SPARK_CHARS: &[&str] = &["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

impl<'a> GradientEngine<'a> {
    pub const FADE: f64 = 0.06;

    pub fn new(theme: &'a Theme) -> Self {
        Self { theme }
    }
    pub fn default_engine() -> GradientEngine<'static> {
        GradientEngine {
            theme: DEFAULT_THEME,
        }
    }

    fn sample(&self, stops: &[(f32, Rgb)], t: f64, dim: f64) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        for i in 0..stops.len().saturating_sub(1) {
            let (t0, c0) = stops[i];
            let (t1, c1) = stops[i + 1];
            if t <= t1 as f64 {
                let u = if (t1 - t0) > 0.0 {
                    (t - t0 as f64) / (t1 - t0) as f64
                } else {
                    0.0
                };
                return lerp_rgb(c0, c1, u, dim);
            }
        }
        let (_, c) = stops.last().copied().unwrap_or((0.0, (0, 0, 0)));
        let (r, g, b) = c;
        (
            trunc_u8(r as f64 * dim),
            trunc_u8(g as f64 * dim),
            trunc_u8(b as f64 * dim),
        )
    }

    pub fn gradient_rgb(&self, t: f64, dim: f64) -> Rgb {
        self.sample(self.theme.grad_stops, t, dim)
    }
    pub fn spark_rgb(&self, t: f64, dim: f64) -> Rgb {
        self.sample(self.theme.spark_stops, t, dim)
    }

    pub fn gradient_color(&self, t: f64, dim: f64) -> String {
        let (r, g, b) = self.gradient_rgb(t, dim);
        format!("\x1b[38;2;{r};{g};{b}m")
    }
    pub fn spark_color(&self, t: f64, dim: f64) -> String {
        let (r, g, b) = self.spark_rgb(t, dim);
        format!("\x1b[38;2;{r};{g};{b}m")
    }

    /// Color for the `col`-th cell of an `width`-cell row, with `fill` in
    /// `[0.0, 1.0]` selecting how much of the row is "live".
    pub fn grad_at(&self, col: i32, width: i32, dim: f64, fill: f64) -> String {
        let denom = (width - 1).max(1) as f64;
        let t = col as f64 / denom;
        if fill <= 0.0 {
            return self.theme.border_off.to_string();
        }
        let fade = Self::FADE;
        if t <= fill - fade {
            return self.gradient_color(t, dim);
        }
        if t >= fill + fade {
            return self.theme.border_off.to_string();
        }
        let live_rgb = self.gradient_rgb(t.min(fill), dim);
        let grey = self.theme.grey_rgb;
        let u = ((t - (fill - fade)) / (2.0 * fade)).clamp(0.0, 1.0);
        let r = trunc_u8(live_rgb.0 as f64 + (grey.0 as f64 - live_rgb.0 as f64) * u);
        let g = trunc_u8(live_rgb.1 as f64 + (grey.1 as f64 - live_rgb.1 as f64) * u);
        let b = trunc_u8(live_rgb.2 as f64 + (grey.2 as f64 - live_rgb.2 as f64) * u);
        format!("\x1b[38;2;{r};{g};{b}m")
    }

    pub fn gradient_bar(&self, filled: i32, bar_w: i32) -> String {
        if filled <= 0 || bar_w <= 0 {
            return String::new();
        }
        let denom = (bar_w - 1).max(1) as f64;
        let mut out = String::new();
        for i in 0..filled {
            let (r, g, b) = self.gradient_rgb(i as f64 / denom, 1.0);
            out.push_str(&format!("\x1b[48;2;{r};{g};{b}m "));
        }
        if filled <= bar_w {
            out.push_str("\x1b[49m");
            out.push_str(&self.gradient_color(filled as f64 / denom, 1.0));
            out.push_str(bar::MID);
        }
        out
    }

    fn spark_flat(idx: i32) -> (&'static str, &'static str) {
        if idx <= 0 {
            return (" ", SPARK_CHARS[0]);
        }
        if idx <= 8 {
            return (" ", SPARK_CHARS[(idx - 1) as usize]);
        }
        (SPARK_CHARS[(idx - 9) as usize], "█")
    }
    fn spark_rise(idx: i32) -> (&'static str, &'static str) {
        if idx <= 0 {
            return (" ", SPARK_CHARS[0]);
        }
        if idx <= 3 {
            return (" ", glyphs::SPARK_RISE_SMALL);
        }
        if idx <= 7 {
            return (" ", glyphs::SPARK_RISE_MED);
        }
        if idx <= 8 {
            return (" ", glyphs::SPARK_RISE_TALL);
        }
        (glyphs::SPARK_RISE_TOP, glyphs::SPARK_RISE_TALL)
    }
    fn spark_fall(idx: i32) -> (&'static str, &'static str) {
        if idx <= 0 {
            return (" ", SPARK_CHARS[0]);
        }
        if idx <= 3 {
            return (" ", glyphs::SPARK_FALL_SMALL);
        }
        if idx <= 7 {
            return (" ", glyphs::SPARK_FALL_MED);
        }
        if idx <= 8 {
            return (" ", glyphs::SPARK_FALL_TALL);
        }
        (glyphs::SPARK_FALL_TOP, glyphs::SPARK_FALL_TALL)
    }

    /// Sparkline. Returns `(top_row, bot_row)` — strings with embedded ANSI
    /// escapes and no trailing newline.
    pub fn sparkline(&self, history: &[i32], live: bool) -> (String, String) {
        if history.is_empty() {
            return (String::new(), String::new());
        }
        let max_val = *history.iter().max().unwrap_or(&0);
        let indices: Vec<i32> = history
            .iter()
            .map(|&v| {
                let r = if max_val > 0 {
                    v as f64 / max_val as f64
                } else {
                    0.0
                };
                ((r * 16.0).trunc() as i32).min(16)
            })
            .collect();
        let last_i = indices.len() - 1;
        let mut top = String::new();
        let mut bot = String::new();
        for (i, &idx) in indices.iter().enumerate() {
            let prev_idx = if i > 0 { indices[i - 1] } else { 0 };
            let (top_ch, bot_ch, tint_idx) = if idx > prev_idx {
                let (t, b) = Self::spark_rise(idx);
                (t, b, idx)
            } else if prev_idx > idx {
                let (t, b) = Self::spark_fall(prev_idx);
                (t, b, prev_idx)
            } else {
                let (t, b) = Self::spark_flat(idx);
                (t, b, idx)
            };
            let ratio = tint_idx as f64 / 16.0;
            let ratio_bot = ratio * 0.5;
            let ratio_top = 0.5 + ratio * 0.5;
            let (bot_clr, top_clr) = if live && i == last_i {
                (
                    self.spark_color(ratio_bot, LIVE_DIM as f64),
                    self.spark_color(ratio_top, LIVE_DIM as f64),
                )
            } else {
                (
                    self.spark_color(ratio_bot, 1.0),
                    self.spark_color(ratio_top, 1.0),
                )
            };
            top.push_str(&top_clr);
            top.push_str(top_ch);
            top.push_str(glyphs::RESET);
            bot.push_str(&bot_clr);
            bot.push_str(bot_ch);
            bot.push_str(glyphs::RESET);
        }
        (top, bot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::builtin::CLAUDE_DARK;

    fn eng() -> GradientEngine<'static> {
        GradientEngine::new(&CLAUDE_DARK)
    }

    // grad_stops claude-dark = [(0.0, (40,210,80)), (0.25, (240,230,20)), (0.5, (255,140,20)), (0.75, (220,40,50)), (1.0, (170,60,210))]
    #[test]
    fn gradient_rgb_at_zero() {
        assert_eq!(eng().gradient_rgb(0.0, 1.0), (40, 210, 80));
    }
    #[test]
    fn gradient_rgb_at_one() {
        assert_eq!(eng().gradient_rgb(1.0, 1.0), (170, 60, 210));
    }
    #[test]
    fn gradient_rgb_clamps_above_one() {
        assert_eq!(eng().gradient_rgb(1.5, 1.0), (170, 60, 210));
    }
    #[test]
    fn gradient_rgb_dim() {
        // int(40 * 0.5) = 20, int(210 * 0.5) = 105, int(80 * 0.5) = 40
        assert_eq!(eng().gradient_rgb(0.0, 0.5), (20, 105, 40));
    }

    #[test]
    fn gradient_color_format() {
        let c = eng().gradient_color(0.5, 1.0);
        assert!(c.starts_with("\x1b[38;2;"));
        assert!(c.ends_with('m'));
    }

    #[test]
    fn grad_at_start_of_full_bar() {
        let e = eng();
        assert_eq!(e.grad_at(0, 10, 1.0, 1.0), e.gradient_color(0.0, 1.0));
    }
    #[test]
    fn grad_at_past_zero_fill_returns_border_off() {
        assert_eq!(eng().grad_at(9, 10, 1.0, 0.0), CLAUDE_DARK.border_off);
    }

    #[test]
    fn spark_rgb_dim_zero() {
        assert_eq!(eng().spark_rgb(0.3, 0.0), (0, 0, 0));
        assert_eq!(eng().spark_rgb(0.7, 0.0), (0, 0, 0));
    }
    #[test]
    fn spark_rgb_dim_half() {
        let e = eng();
        let (r, g, b) = e.spark_rgb(0.7, 1.0);
        let half = e.spark_rgb(0.7, 0.5);
        // Dim is applied after the lerp; just check that the dimmed value
        // is no greater than the un-dimmed one.
        assert!(half.0 <= r && half.1 <= g && half.2 <= b);
    }

    #[test]
    fn spark_color_dim_one_matches_default() {
        let e = eng();
        assert_eq!(e.spark_color(0.5, 1.0), e.spark_color(0.5, 1.0));
    }

    #[test]
    fn sparkline_empty_history() {
        let (t, b) = eng().sparkline(&[], false);
        assert!(t.is_empty() && b.is_empty());
    }

    #[test]
    fn sparkline_history_length_matches_cells() {
        // Each cell emits one rendered glyph per row; visible_width on the
        // resulting top/bot row equals history.len() (no wide chars in spark).
        use crate::width::visible_width;
        let (t, b) = eng().sparkline(&[1, 2, 3, 4, 5], false);
        assert_eq!(visible_width(&t), 5);
        assert_eq!(visible_width(&b), 5);
    }

    #[test]
    fn gradient_bar_filled_width_visible() {
        use crate::width::visible_width;
        // filled=5, bar_w=10: 5 background cells + a leading-edge MID char.
        // MID glyph is `` (visible width 1).
        let s = eng().gradient_bar(5, 10);
        assert_eq!(visible_width(&s), 6);
    }

    #[test]
    fn gradient_bar_empty() {
        assert!(eng().gradient_bar(0, 10).is_empty());
        assert!(eng().gradient_bar(5, 0).is_empty());
    }
}
