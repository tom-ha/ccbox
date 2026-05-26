//! `BorderRenderer` — top / bottom / separator / seam / dim borders.
//!
//! Borders are 1-cell-tall rows that span the whole layout. Each renders the
//! corner glyphs, the horizontal fill, optional `down`/`up` junctions for
//! vertical separators above and below, and the active pill (top border) or
//! pill bottom (separator-with-pill-bottom). Output ends with a `RESET`.

use std::collections::HashSet;

use crate::glyphs::{self, RESET};
use crate::render::gradient::GradientEngine;
use crate::render::pill::{Edge, Pill};
use crate::theme::Theme;
use crate::width::visible_width;

pub struct BorderRenderer<'a> {
    pub gradient: GradientEngine<'a>,
    pub theme: &'a Theme,
}

const DIM_MIN: f64 = 0.6;
const DIM_RAMP: f64 = 5.0;

impl<'a> BorderRenderer<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self { gradient: GradientEngine::new(theme), theme }
    }

    fn dim_for_col(col: i32, elbows: &HashSet<i32>) -> f64 {
        let d = elbows.iter().map(|e| (col - e).abs()).min().unwrap_or(0);
        if d == 0 { 1.0 } else { (1.0 - (1.0 - DIM_MIN) * (d as f64 / DIM_RAMP)).max(DIM_MIN) }
    }

    /// Top border. `downs` are columns where a vertical separator descends
    /// into the content below. `right_chip` is fully-styled chip text painted
    /// flush against the right corner (or empty for a plain top border).
    /// `pill` is optional — render its top edge.
    pub fn border_top(
        &self,
        width: i32,
        right_chip: &str,
        downs: &[i32],
        fill: f64,
        pill: Option<&Pill>,
    ) -> String {
        let downs_set: HashSet<i32> = downs.iter().copied().collect();
        let empty_pill = Pill::default();
        let p = pill.unwrap_or(&empty_pill);

        let ch = |col: i32| -> String {
            let pc = p.border_char(col, Edge::Top);
            if !pc.is_empty() { return pc.to_string(); }
            if downs_set.contains(&col) { "┬".to_string() } else { "─".to_string() }
        };
        let clr = |col: i32, pos: i32| -> String {
            if p.active() && p.start <= col && col <= p.end {
                p.border_fg(col)
            } else {
                self.gradient.grad_at(pos, width, 1.0, fill)
            }
        };

        let mut parts = String::new();
        if p.active() && p.start <= 1 {
            parts.push_str(&p.border_fg(p.start));
            parts.push_str(glyphs::PILL_TL);
        } else {
            parts.push_str(&self.gradient.grad_at(0, width, 1.0, fill));
            parts.push('╭');
        }

        let chip_w = visible_width(right_chip) as i32;
        // Inner cells are columns 2..=width-1 (inclusive). Right-anchor the
        // chip across the trailing chip_w cells, with `dashes_w` dashes filling
        // the lead. If the chip would not fit, fall back to all-dashes.
        let inner_w = (width - 2).max(0);
        let chip_fits = !right_chip.is_empty() && chip_w > 0 && chip_w <= inner_w;
        let dashes_w = if chip_fits { inner_w - chip_w } else { inner_w };

        for i in 0..dashes_w {
            let col = i + 2;
            parts.push_str(&clr(col, i + 1));
            parts.push_str(&ch(col));
        }
        if chip_fits {
            parts.push_str(right_chip);
            parts.push_str(RESET);
        }

        if p.active() && p.start <= width && width <= p.end {
            parts.push_str(&p.border_fg(width));
            parts.push_str(p.border_char(width, Edge::Top));
        } else {
            parts.push_str(&self.gradient.grad_at(width - 1, width, 1.0, fill));
            parts.push('╮');
        }
        parts.push_str(RESET);
        parts
    }

    pub fn border_bottom(&self, width: i32, ups: &[i32], fill: f64) -> String {
        let ups_set: HashSet<i32> = ups.iter().copied().collect();
        let mut parts = String::new();
        parts.push_str(&self.gradient.grad_at(0, width, 1.0, fill));
        parts.push('╰');
        for i in 0..(width - 2) {
            let ch = if ups_set.contains(&(i + 2)) { '┴' } else { '─' };
            parts.push_str(&self.gradient.grad_at(i + 1, width, 1.0, fill));
            parts.push(ch);
        }
        parts.push_str(&self.gradient.grad_at(width - 1, width, 1.0, fill));
        parts.push('╯');
        parts.push_str(RESET);
        parts
    }

    pub fn border_separator(&self, width: i32, ups: &[i32], fill: f64) -> String {
        let ups_set: HashSet<i32> = ups.iter().copied().collect();
        let mut parts = String::new();
        parts.push_str(&self.gradient.grad_at(0, width, 1.0, fill));
        parts.push('├');
        for i in 0..(width - 2) {
            let ch = if ups_set.contains(&(i + 2)) { '┴' } else { '─' };
            parts.push_str(&self.gradient.grad_at(i + 1, width, 1.0, fill));
            parts.push(ch);
        }
        parts.push_str(&self.gradient.grad_at(width - 1, width, 1.0, fill));
        parts.push('┤');
        parts.push_str(RESET);
        parts
    }

    /// Dimmed separator. Used between two content rows that share a vertical
    /// separator (`downs` / `ups`). `pill` and `pill_edge` let the separator
    /// double as the bottom edge of a pill.
    pub fn border_separator_dim(
        &self,
        width: i32,
        downs: &[i32],
        ups: &[i32],
        fill: f64,
        pill: Option<&Pill>,
        pill_edge: Edge,
    ) -> String {
        let downs_set: HashSet<i32> = downs.iter().copied().collect();
        let ups_set: HashSet<i32> = ups.iter().copied().collect();
        let mut elbow_cols: HashSet<i32> = HashSet::new();
        elbow_cols.insert(1);
        elbow_cols.insert(width);
        elbow_cols.extend(downs_set.iter().copied());
        elbow_cols.extend(ups_set.iter().copied());
        let empty_pill = Pill::default();
        let p = pill.unwrap_or(&empty_pill);
        let edge = pill_edge;

        let mut parts = String::new();
        if p.active() && p.start <= 1 {
            parts.push_str(&p.border_fg(p.start));
            parts.push_str(p.border_char(p.start, edge));
        } else {
            parts.push_str(&self.gradient.grad_at(0, width, Self::dim_for_col(1, &elbow_cols), fill));
            parts.push('├');
        }
        for i in 0..(width - 2) {
            let col = i + 2;
            let pc = if p.active() { p.border_char(col, edge) } else { "" };
            if !pc.is_empty() {
                parts.push_str(&p.border_fg(col));
                parts.push_str(pc);
            } else {
                let ch = if downs_set.contains(&col) && ups_set.contains(&col) {
                    '┼'
                } else if downs_set.contains(&col) {
                    '┬'
                } else if ups_set.contains(&col) {
                    '┴'
                } else {
                    '┄'
                };
                parts.push_str(&self.gradient.grad_at(i + 1, width, Self::dim_for_col(col, &elbow_cols), fill));
                parts.push(ch);
            }
        }
        if p.active() && p.start <= width && width <= p.end {
            parts.push_str(&p.border_fg(width));
            parts.push_str(p.border_char(width, edge));
        } else {
            parts.push_str(&self.gradient.grad_at(width - 1, width, Self::dim_for_col(width, &elbow_cols), fill));
            parts.push('┤');
        }
        parts.push_str(RESET);
        parts
    }

    /// Single content row with side borders. Optional pill-flush, right-pill,
    /// background lead/trail spans for the model pill.
    pub fn border_line(
        &self,
        content: &str,
        width: i32,
        fill: f64,
        bg_lead: &str,
        bg_trail: &str,
        pill_flush: bool,
        right_pill: &str,
    ) -> String {
        let content_w = visible_width(content) as i32;
        if !right_pill.is_empty() {
            let pill_w = visible_width(right_pill) as i32;
            let pad = (width - 2 - content_w - pill_w).max(0) as usize;
            let left = self.gradient.grad_at(0, width, 1.0, fill);
            let lead = if !bg_lead.is_empty() {
                format!("{bg_lead} \x1b[49m")
            } else {
                " ".to_string()
            };
            return format!("{left}│{RESET}{lead}{content}{}{right_pill}{RESET}", " ".repeat(pad));
        }
        if pill_flush {
            let pad = (width - 1 - content_w).max(0) as usize;
            let right = self.gradient.grad_at(width - 1, width, 1.0, fill);
            return format!("{content}{}{right}│{RESET}", " ".repeat(pad));
        }
        let pad = (width - 3 - content_w).max(0);
        let left = self.gradient.grad_at(0, width, 1.0, fill);
        let right = self.gradient.grad_at(width - 1, width, 1.0, fill);
        let lead = if !bg_lead.is_empty() {
            format!("{bg_lead} \x1b[49m")
        } else {
            " ".to_string()
        };
        let pad_str = if !bg_trail.is_empty() && pad > 0 {
            format!("{}{bg_trail} \x1b[49m", " ".repeat((pad - 1) as usize))
        } else {
            " ".repeat(pad as usize)
        };
        format!("{left}│{RESET}{lead}{content}{pad_str}{right}│{RESET}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::builtin::CLAUDE_DARK;
    use crate::width::visible_width;

    fn br() -> BorderRenderer<'static> { BorderRenderer::new(&CLAUDE_DARK) }

    #[test]
    fn top_border_visible_width_eq_width() {
        for w in [40, 65, 80, 100, 130] {
            let s = br().border_top(w, "", &[], 1.0, None);
            assert_eq!(visible_width(&s), w as usize, "width={w}: {s:?}");
        }
    }

    #[test]
    fn bottom_border_visible_width_eq_width() {
        for w in [40, 65, 80, 100, 130] {
            let s = br().border_bottom(w, &[], 1.0);
            assert_eq!(visible_width(&s), w as usize);
        }
    }

    #[test]
    fn separator_visible_width_eq_width() {
        for w in [40, 65, 80, 100, 130] {
            let s = br().border_separator(w, &[], 1.0);
            assert_eq!(visible_width(&s), w as usize);
        }
    }

    #[test]
    fn separator_dim_visible_width_eq_width() {
        for w in [40, 65, 80, 100, 130] {
            let s = br().border_separator_dim(w, &[], &[], 1.0, None, Edge::Bottom);
            assert_eq!(visible_width(&s), w as usize);
        }
    }

    #[test]
    fn bottom_border_seam_at_ups_columns() {
        // ups column should render ┴ at that visible-column index.
        let s = br().border_bottom(20, &[5, 12], 1.0);
        // Strip ANSI and count to col 5.
        let stripped = crate::ansi::strip_ansi(&s);
        let chars: Vec<char> = stripped.chars().collect();
        assert_eq!(chars[4], '┴', "{stripped:?}");
        assert_eq!(chars[11], '┴', "{stripped:?}");
    }

    #[test]
    fn top_border_seam_at_downs_columns() {
        let s = br().border_top(20, "", &[5, 12], 1.0, None);
        let stripped = crate::ansi::strip_ansi(&s);
        let chars: Vec<char> = stripped.chars().collect();
        assert_eq!(chars[4], '┬');
        assert_eq!(chars[11], '┬');
    }

    #[test]
    fn top_border_chip_anchors_to_right_corner() {
        // chip is 5 plain cells; with width=20, dashes_w = 20 - 2 - 5 = 13.
        // Layout: ╭ <13 dashes> <5 chip> ╮
        let s = br().border_top(20, "abcde", &[], 1.0, None);
        let plain = crate::ansi::strip_ansi(&s);
        assert_eq!(visible_width(&s), 20);
        // The chip should appear flush with the right corner (cols 15..=19),
        // and the right corner is at col 20.
        assert!(plain.ends_with("abcde╮"), "expected chip flush right, got {plain:?}");
    }

    #[test]
    fn top_border_empty_chip_is_all_dashes() {
        let s = br().border_top(20, "", &[], 1.0, None);
        let plain = crate::ansi::strip_ansi(&s);
        assert_eq!(visible_width(&s), 20);
        let chars: Vec<char> = plain.chars().collect();
        assert_eq!(chars[0], '╭');
        assert_eq!(chars[19], '╮');
        for c in &chars[1..19] {
            assert_eq!(*c, '─', "expected only dashes, got {plain:?}");
        }
    }

    #[test]
    fn top_border_overlong_chip_falls_back_to_dashes() {
        // chip wider than inner_w → don't render it; full-dash border instead.
        let s = br().border_top(10, "this is way too long", &[], 1.0, None);
        let plain = crate::ansi::strip_ansi(&s);
        assert_eq!(visible_width(&s), 10);
        assert!(!plain.contains("this is"), "overlong chip must not render: {plain:?}");
    }

    #[test]
    fn border_line_pads_to_width() {
        for w in [40, 65, 100] {
            let line = br().border_line("hello", w, 1.0, "", "", false, "");
            assert_eq!(visible_width(&line), w as usize);
        }
    }
}
