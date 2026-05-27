//! `Pill` value type, plus `paint_bg_span` and `pill_gradient_fg` helpers.
//!
//! `start` and `end` use `i32` so an inactive pill can encode itself with
//! `start = end = -1`.

use crate::ansi::Rgb;
use crate::glyphs;

#[derive(Debug, Clone, Copy)]
pub struct Pill {
    pub start: i32,
    pub end: i32,
    pub anchor: Rgb,
    pub shift: Rgb,
    pub pct: i32,
}

impl Default for Pill {
    fn default() -> Self {
        Self {
            start: -1,
            end: -1,
            anchor: (0, 0, 0),
            shift: (0, 0, 0),
            pct: 0,
        }
    }
}

impl Pill {
    pub fn active(&self) -> bool {
        self.pct > 0
    }

    pub fn border_char(&self, col: i32, edge: Edge) -> &'static str {
        if !self.active() || !(self.start..=self.end).contains(&col) {
            return "";
        }
        match edge {
            Edge::Top => {
                if col == self.start {
                    glyphs::PILL_TL
                } else if col == self.end {
                    glyphs::PILL_TR
                } else {
                    glyphs::PILL_TOP
                }
            }
            Edge::Bottom => {
                if col == self.start {
                    glyphs::PILL_BL
                } else if col == self.end {
                    glyphs::PILL_BR
                } else {
                    glyphs::PILL_BOT
                }
            }
        }
    }

    pub fn border_fg(&self, col: i32) -> String {
        pill_gradient_fg(
            col - self.start,
            0,
            self.end - self.start,
            self.anchor,
            self.shift,
            self.pct,
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Edge {
    Top,
    Bottom,
}

fn scale(rgb: Rgb, pct: i32) -> Rgb {
    let f = |c: u8| -> u8 {
        let v = (c as i32) * pct / 100;
        v.clamp(0, 255) as u8
    };
    (f(rgb.0), f(rgb.1), f(rgb.2))
}

/// Build a foreground ANSI escape for the `col`-th cell of a pill that runs
/// from `pill_start` to `pill_end`.
pub fn pill_gradient_fg(
    col: i32,
    pill_start: i32,
    pill_end: i32,
    anchor: Rgb,
    shift: Rgb,
    pct: i32,
) -> String {
    let c0 = scale(anchor, pct);
    let c1 = scale(shift, pct);
    let span = (pill_end - pill_start).max(1) as f64;
    let mut t = (col - pill_start) as f64 / span;
    if t < 0.0 {
        t = 0.0;
    }
    if t > 1.0 {
        t = 1.0;
    }
    let r = (c0.0 as f64 + (c1.0 as f64 - c0.0 as f64) * t).trunc() as i32;
    let g = (c0.1 as f64 + (c1.1 as f64 - c0.1 as f64) * t).trunc() as i32;
    let b = (c0.2 as f64 + (c1.2 as f64 - c0.2 as f64) * t).trunc() as i32;
    format!("\x1b[38;2;{r};{g};{b}m")
}

/// One cell of `paint_bg_span` input: `(char, optional fg, bold, italic)`.
pub struct Cell {
    pub ch: String,
    pub fg: Option<Rgb>,
    pub bold: bool,
    pub italic: bool,
}

/// Paint a horizontal background gradient across `cells`, flipping the
/// foreground per-cell based on background luminance.
pub fn paint_bg_span(
    cells: &[Cell],
    anchor: Rgb,
    shift: Rgb,
    pct: i32,
    pill_fg_dark: Rgb,
    pill_fg_light: Option<Rgb>,
) -> String {
    let c0 = scale(anchor, pct);
    let c1 = scale(shift, pct);
    let n = (cells.len() as i64 - 1).max(1);
    let mut prev_bg: Option<Rgb> = None;
    let mut prev_fg: Option<Option<Rgb>> = None;
    let mut prev_bold = false;
    let mut prev_italic = false;
    let mut out = String::new();
    for (i, cell) in cells.iter().enumerate() {
        let t = i as f64 / n as f64;
        let r = (c0.0 as f64 + (c1.0 as f64 - c0.0 as f64) * t).trunc() as i32;
        let g = (c0.1 as f64 + (c1.1 as f64 - c0.1 as f64) * t).trunc() as i32;
        let b = (c0.2 as f64 + (c1.2 as f64 - c0.2 as f64) * t).trunc() as i32;
        let lum = (r * 299 + g * 587 + b * 114) / 1000;
        let fg_rgb: Option<Rgb> = if lum as u16 >= crate::render::palette::BG_LUM_THRESHOLD {
            Some(pill_fg_dark)
        } else if let Some(l) = pill_fg_light {
            Some(l)
        } else {
            cell.fg
        };
        let cur_bg = (r as u8, g as u8, b as u8);
        if prev_bg != Some(cur_bg) {
            out.push_str(&format!("\x1b[48;2;{r};{g};{b}m"));
            prev_bg = Some(cur_bg);
        }
        if prev_fg != Some(fg_rgb) {
            match fg_rgb {
                None => out.push_str("\x1b[39m"),
                Some((fr, fg, fb)) => out.push_str(&format!("\x1b[38;2;{fr};{fg};{fb}m")),
            }
            prev_fg = Some(fg_rgb);
        }
        if cell.bold != prev_bold {
            out.push_str(if cell.bold { "\x1b[1m" } else { "\x1b[22m" });
            prev_bold = cell.bold;
        }
        if cell.italic != prev_italic {
            out.push_str(if cell.italic { "\x1b[3m" } else { "\x1b[23m" });
            prev_italic = cell.italic;
        }
        out.push_str(&cell.ch);
    }
    out.push_str("\x1b[49m");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_pill_is_inactive() {
        let p = Pill::default();
        assert_eq!(p.start, -1);
        assert_eq!(p.end, -1);
        assert!(!p.active());
    }

    #[test]
    fn inactive_pill_border_char_empty() {
        let p = Pill::default();
        assert_eq!(p.border_char(3, Edge::Top), "");
    }

    #[test]
    fn active_pill_borders_at_ends() {
        let p = Pill {
            start: 5,
            end: 10,
            anchor: (255, 0, 0),
            shift: (0, 0, 255),
            pct: 100,
        };
        assert_eq!(p.border_char(5, Edge::Top), glyphs::PILL_TL);
        assert_eq!(p.border_char(10, Edge::Top), glyphs::PILL_TR);
        assert_eq!(p.border_char(7, Edge::Top), glyphs::PILL_TOP);
        assert_eq!(p.border_char(5, Edge::Bottom), glyphs::PILL_BL);
        assert_eq!(p.border_char(10, Edge::Bottom), glyphs::PILL_BR);
        assert_eq!(p.border_char(7, Edge::Bottom), glyphs::PILL_BOT);
    }

    #[test]
    fn pill_gradient_fg_starts_and_ends_at_endpoints() {
        let anchor = (100, 0, 0);
        let shift = (0, 100, 0);
        // pct=100 (no scaling). t=0 → anchor, t=1 → shift.
        assert_eq!(
            pill_gradient_fg(0, 0, 10, anchor, shift, 100),
            "\x1b[38;2;100;0;0m"
        );
        assert_eq!(
            pill_gradient_fg(10, 0, 10, anchor, shift, 100),
            "\x1b[38;2;0;100;0m"
        );
    }

    #[test]
    fn pill_gradient_fg_clamps_outside_range() {
        let anchor = (50, 0, 0);
        let shift = (0, 0, 50);
        assert_eq!(
            pill_gradient_fg(-5, 0, 10, anchor, shift, 100),
            "\x1b[38;2;50;0;0m"
        );
        assert_eq!(
            pill_gradient_fg(99, 0, 10, anchor, shift, 100),
            "\x1b[38;2;0;0;50m"
        );
    }

    #[test]
    fn paint_bg_span_emits_background_for_each_distinct_cell() {
        let cells = vec![
            Cell {
                ch: "a".into(),
                fg: None,
                bold: false,
                italic: false,
            },
            Cell {
                ch: "b".into(),
                fg: None,
                bold: false,
                italic: false,
            },
            Cell {
                ch: "c".into(),
                fg: None,
                bold: false,
                italic: false,
            },
        ];
        let out = paint_bg_span(&cells, (255, 255, 255), (0, 0, 0), 100, (15, 15, 15), None);
        assert!(out.contains("\x1b[48;2;255;255;255m"));
        assert!(out.contains("\x1b[49m"));
        assert!(out.contains('a'));
        assert!(out.contains('b'));
        assert!(out.contains('c'));
    }
}
