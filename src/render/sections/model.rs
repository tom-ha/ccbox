//! `model_right_section` — model identity cluster on the top row.

use crate::ansi::Rgb;
use crate::glyphs::{
    BOLD, GLYPH_BURN_FAST, GLYPH_MODEL, GLYPH_THINKING, ITALIC, PILL_LEFT, PILL_RIGHT, RESET,
};
use crate::render::palette::{model_key, LevelPct};
use crate::render::pill::{paint_bg_span, pill_gradient_fg, Cell};
use crate::render::{BgShift, Renderer};
use crate::width::visible_width;

impl Renderer {
    pub fn model_colour(&self, model_name: &str) -> &'static str {
        self.theme.model_colors(model_key(model_name)).label
    }

    fn model_anchor_pair(&self, model_name: &str) -> (Rgb, Rgb) {
        let mc = self.theme.model_colors(model_key(model_name));
        let shift = if self.bg_shift == BgShift::Warm {
            mc.warm_shift
        } else {
            mc.cool_shift
        };
        (mc.anchor, shift)
    }

    fn model_bg_pct(&self, effort_level: &str) -> u16 {
        LevelPct::lookup(effort_level)
    }

    /// Background lead-cell escape used by the layout to paint the gradient
    /// stripe under the model name.
    pub fn model_bg_lead(&self, model_name: &str, effort_level: &str) -> String {
        let pct = self.model_bg_pct(effort_level);
        if pct == 0 {
            return String::new();
        }
        let (anchor, _) = self.model_anchor_pair(model_name);
        let scaled = scale(anchor, pct as i32);
        format!("\x1b[48;2;{};{};{}m", scaled.0, scaled.1, scaled.2)
    }

    /// Background trail-cell escape.
    pub fn model_bg_trail(&self, model_name: &str, effort_level: &str) -> String {
        let pct = self.model_bg_pct(effort_level);
        if pct == 0 {
            return String::new();
        }
        let (_, shift) = self.model_anchor_pair(model_name);
        let scaled = scale(shift, pct as i32);
        format!("\x1b[48;2;{};{};{}m", scaled.0, scaled.1, scaled.2)
    }

    /// `(right_text, right_text_visible_width)` — the model-identity cluster
    /// for the top row. Either a plain `<glyph> <name>` text run, a name plus
    /// inline thinking effort label, or a coloured pill (when the thinking
    /// effort has a non-zero percent).
    pub fn model_right_section(
        &self,
        model_name: &str,
        model_thinking: &str,
        effort_level: &str,
        fast_mode: bool,
    ) -> (String, usize) {
        let model_clr = self.model_colour(model_name);
        let pct = self.model_bg_pct(effort_level) as i32;
        let glyph = if fast_mode {
            GLYPH_BURN_FAST
        } else {
            GLYPH_THINKING
        };

        let right_text = if pct > 0 {
            let (anchor, shift) = self.model_anchor_pair(model_name);
            let mut cells: Vec<Cell> = Vec::new();
            cells.push(Cell {
                ch: GLYPH_MODEL.into(),
                fg: None,
                bold: false,
                italic: false,
            });
            cells.push(Cell {
                ch: " ".into(),
                fg: None,
                bold: false,
                italic: false,
            });
            cells.push(Cell {
                ch: " ".into(),
                fg: None,
                bold: false,
                italic: false,
            });
            for ch in model_name.chars() {
                cells.push(Cell {
                    ch: ch.to_string(),
                    fg: None,
                    bold: false,
                    italic: false,
                });
            }
            cells.push(Cell {
                ch: " ".into(),
                fg: None,
                bold: false,
                italic: false,
            });
            cells.push(Cell {
                ch: glyph.into(),
                fg: None,
                bold: true,
                italic: false,
            });
            cells.push(Cell {
                ch: " ".into(),
                fg: None,
                bold: true,
                italic: false,
            });
            cells.push(Cell {
                ch: " ".into(),
                fg: None,
                bold: true,
                italic: false,
            });
            for ch in model_thinking.chars() {
                cells.push(Cell {
                    ch: ch.to_string(),
                    fg: None,
                    bold: false,
                    italic: true,
                });
            }
            cells.push(Cell {
                ch: " ".into(),
                fg: None,
                bold: false,
                italic: false,
            });
            let n = cells.len() as i32;
            let pill_l = format!(
                "{}{PILL_LEFT}",
                pill_gradient_fg(0, 0, n, anchor, shift, pct)
            );
            let pill_r = format!(
                "{}{PILL_RIGHT}",
                pill_gradient_fg(n, 0, n, anchor, shift, pct)
            );
            let painted = paint_bg_span(
                &cells,
                anchor,
                shift,
                pct,
                self.theme.pill_fg_dark,
                Some(self.theme.pill_fg_light),
            );
            format!("{pill_l}{painted}{pill_r}{RESET}")
        } else if !model_thinking.is_empty() {
            format!(
                "{model_clr}{GLYPH_MODEL}  {model_name}{RESET} {BOLD}{glyph}  {RESET}{model_clr}{ITALIC}{model_thinking}{RESET}",
            )
        } else {
            format!("{model_clr}{GLYPH_MODEL}  {model_name}{RESET}")
        };
        let right_w = visible_width(&right_text);
        (right_text, right_w)
    }
}

fn scale(rgb: Rgb, pct: i32) -> Rgb {
    let f = |c: u8| -> u8 {
        let v = (c as i32) * pct / 100;
        v.clamp(0, 255) as u8
    };
    (f(rgb.0), f(rgb.1), f(rgb.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_colour_matches_kind() {
        let r = Renderer::default();
        assert_eq!(
            r.model_colour("claude-opus-4-7"),
            r.theme
                .model_colors(crate::render::palette::ModelKind::Opus)
                .label
        );
        assert_eq!(
            r.model_colour("claude-sonnet-4-6"),
            r.theme
                .model_colors(crate::render::palette::ModelKind::Sonnet)
                .label
        );
    }

    #[test]
    fn model_right_section_plain_returns_name_only() {
        let r = Renderer::default();
        let (text, w) = r.model_right_section("Sonnet 4.6", "", "", false);
        let plain = crate::ansi::strip_ansi(&text);
        assert!(plain.contains("Sonnet 4.6"), "{plain}");
        assert_eq!(w, visible_width(&text));
        assert!(!plain.contains("T-"));
        assert!(!plain.contains('%'));
    }

    #[test]
    fn model_right_section_with_thinking_label() {
        let r = Renderer::default();
        let (text, _w) = r.model_right_section("Sonnet 4.6", "thinking", "", false);
        let plain = crate::ansi::strip_ansi(&text);
        assert!(plain.contains("Sonnet 4.6"));
        assert!(plain.contains("thinking"));
        assert!(!plain.contains("T-"));
        assert!(!plain.contains('%'));
    }

    #[test]
    fn model_right_section_fast_mode_uses_zap_glyph() {
        let r = Renderer::default();
        let (text, _w) = r.model_right_section("Sonnet 4.6", "fast", "", true);
        assert!(text.contains(GLYPH_BURN_FAST));
    }
}
