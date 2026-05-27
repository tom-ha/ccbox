//! `tokens_cost` — single-line tokens / cost / rate block.

use crate::glyphs::{ICON_COST, ICON_TOK_RATE, RESET};
use crate::render::format::fmt_tok;
use crate::render::Renderer;
use crate::width::visible_width;

pub const IN_W: usize = 6;
pub const OUT_W: usize = 6;

/// Spark history + recent-active flags piped in from the data layer. Keeps
/// `tokens_cost` pure — it doesn't read or write the rate log.
#[derive(Debug, Clone, Default)]
pub struct TokensCostExtras {
    pub spark_history: Vec<i32>,
    pub in_active: bool,
    pub out_active: bool,
    /// When `false`, render whitespace in place of the cost cluster so other
    /// columns remain visually stable. Defaults to `true`.
    pub show_cost: bool,
}

impl TokensCostExtras {
    pub fn new() -> Self {
        Self {
            show_cost: true,
            ..Default::default()
        }
    }
}

/// Single-line result. `mark_col` is the 1-indexed column where the 60s tick
/// inside the sparkline lives (or `0` if the sparkline didn't fit).
pub struct TokensCost {
    pub line: String,
    pub mark_col: i32,
}

fn fmt_money(v: f64) -> String {
    let abs = v.abs();
    let int_part = abs as u64;
    let int_str = {
        let s = int_part.to_string();
        let bytes = s.as_bytes();
        let mut out = String::with_capacity(s.len() + s.len() / 3);
        for (i, &c) in bytes.iter().enumerate() {
            if i > 0 && (bytes.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c as char);
        }
        out
    };
    let cents = ((abs - int_part as f64) * 100.0).round() as u64;
    let sign = if v < 0.0 { "-" } else { "" };
    format!("{sign}${int_str}.{:02}", cents)
}

fn rjust(s: &str, w: usize) -> String {
    let cur = s.chars().count();
    if cur >= w {
        return s.to_string();
    }
    let mut out = " ".repeat(w - cur);
    out.push_str(s);
    out
}

fn in_cluster_width() -> i32 {
    // "↓ in " (5) + rjust IN_W
    5 + IN_W as i32
}
fn out_cluster_width() -> i32 {
    // "↑ out " (6) + rjust OUT_W
    6 + OUT_W as i32
}
fn cost_cluster_width(sess: f64, day: f64) -> i32 {
    let c1 = fmt_money(sess);
    let c2 = fmt_money(day);
    // "<ICON_COST><c1> sess · <c2> today" — the cost glyph is 1 cell wide.
    (1 + c1.chars().count()
        + " sess · ".chars().count()
        + c2.chars().count()
        + " today".chars().count()) as i32
}
fn rate_cluster_width(tok_rate: u64) -> i32 {
    // ICON_TOK_RATE (1 cell) + " " (1) + fmt_tok (≤6) + " t/m" (4)
    1 + 1 + fmt_tok(tok_rate).chars().count() as i32 + 4
}

const GAP_BETWEEN: i32 = 3; // "   " between clusters
const GAP_RATE_SPARK: i32 = 2; // "  " between rate label and sparkline

/// Compute how many cells the sparkline takes. Used by the layout to size the
/// rate-log history exactly to the rendered sparkline. Mirrors the math inside
/// `tokens_cost`.
pub fn sparkline_bar_w(
    box_width: i32,
    sess_cost: f64,
    day_cost: f64,
    tok_rate: u64,
    show_cost: bool,
) -> i32 {
    // content_w = box_width - 3 (one cell for left border, one space pad, one
    // cell for right border).
    let content_w = box_width - 3;
    let cost_w = if show_cost {
        cost_cluster_width(sess_cost, day_cost)
    } else {
        0
    };
    let cost_gap = if show_cost { GAP_BETWEEN } else { 0 };
    let fixed = in_cluster_width()
        + GAP_BETWEEN
        + out_cluster_width()
        + cost_gap
        + cost_w
        + GAP_BETWEEN
        + rate_cluster_width(tok_rate)
        + GAP_RATE_SPARK;
    (content_w - fixed).max(0)
}

impl Renderer {
    pub fn tokens_cost(
        &self,
        sess_in: u64,
        sess_out: u64,
        sess_cost: f64,
        day_cost: f64,
        tok_rate: u64,
        extras: &TokensCostExtras,
        box_width: i32,
    ) -> TokensCost {
        let t = self.theme;
        let in_icon = if extras.in_active {
            "\x1b[1m↓\x1b[0m"
        } else {
            "↓"
        };
        let out_icon = if extras.out_active {
            "\x1b[1m↑\x1b[0m"
        } else {
            "↑"
        };

        let sess_in_s = rjust(&fmt_tok(sess_in), IN_W);
        let sess_out_s = rjust(&fmt_tok(sess_out), OUT_W);

        let in_cluster = format!(
            "{}{}{RESET} {}in{RESET} {}{sess_in_s}{RESET}",
            t.tok_arrow, in_icon, t.label, t.tok,
        );
        let out_cluster = format!(
            "{}{}{RESET} {}out{RESET} {}{sess_out_s}{RESET}",
            t.tok_arrow, out_icon, t.label, t.tok,
        );

        let cost_cluster = if extras.show_cost {
            let day_clr = self.day_cost_colour(day_cost);
            let cost1 = fmt_money(sess_cost);
            let cost2 = fmt_money(day_cost);
            format!(
                "{}{ICON_COST}{RESET}{}{cost1}{RESET} {}sess{RESET} {}·{RESET} {day_clr}{cost2}{RESET} {}today{RESET}",
                t.safe, t.cost, t.label, t.label, t.label,
            )
        } else {
            String::new()
        };

        let rate_label = format!(
            "{}{ICON_TOK_RATE} {}{}{RESET}{} t/m{RESET}",
            t.tok_icon,
            t.tok,
            fmt_tok(tok_rate),
            t.label,
        );

        let bar_w = sparkline_bar_w(box_width, sess_cost, day_cost, tok_rate, extras.show_cost);
        let sparkline = if bar_w <= 0 {
            String::new()
        } else {
            // Always show the bottom half of the sparkline. When activity is
            // zero the bottom row renders a baseline of `▁` glyphs in the
            // theme's spark colour (typically red) so the sparkline area is
            // always visible; with history it shows the bar heights up to
            // `█` saturation.
            let history: Vec<i32> = if extras.spark_history.is_empty() {
                vec![0; bar_w as usize]
            } else {
                let mut h = extras.spark_history.clone();
                h.reverse();
                if h.len() > bar_w as usize {
                    h[h.len() - bar_w as usize..].to_vec()
                } else if h.len() < bar_w as usize {
                    let mut padded = vec![0; bar_w as usize - h.len()];
                    padded.extend_from_slice(&h);
                    padded
                } else {
                    h
                }
            };
            let (_top, bot) = self
                .gradient()
                .sparkline(&history, !extras.spark_history.is_empty());
            bot
        };

        // Compose: in   out   [cost   ]rate  sparkline
        let mut line = String::new();
        line.push_str(&in_cluster);
        line.push_str(&" ".repeat(GAP_BETWEEN as usize));
        line.push_str(&out_cluster);
        if extras.show_cost {
            line.push_str(&" ".repeat(GAP_BETWEEN as usize));
            line.push_str(&cost_cluster);
        }
        line.push_str(&" ".repeat(GAP_BETWEEN as usize));
        line.push_str(&rate_label);
        line.push_str(&" ".repeat(GAP_RATE_SPARK as usize));
        line.push_str(&sparkline);

        // `mark_col` for the seam decoration. The line starts at column 2
        // (after the leading-space pad). Fixed prefix up to start of
        // sparkline gives mark_col = 2 + (line-pre-spark visible width)
        // + (bar_w / 2).
        let mark_col = if bar_w > 0 {
            let pre_spark_w = visible_width(&line) as i32 - bar_w;
            2 + pre_spark_w + bar_w / 2
        } else {
            0
        };

        TokensCost { line, mark_col }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::strip_ansi;

    #[test]
    fn fmt_money_basics() {
        assert_eq!(fmt_money(0.0), "$0.00");
        assert_eq!(fmt_money(1.0), "$1.00");
        assert_eq!(fmt_money(1.235), "$1.24");
        assert_eq!(fmt_money(1234.5), "$1,234.50");
    }

    #[test]
    fn tokens_cost_single_line() {
        let r = Renderer::default();
        let result = r.tokens_cost(1, 2, 0.01, 0.02, 0, &TokensCostExtras::new(), 160);
        // Just one line, no newline.
        assert!(!result.line.contains('\n'));
    }

    #[test]
    fn tokens_cost_labels_present() {
        let r = Renderer::default();
        let result = r.tokens_cost(120_000, 3_400, 0.18, 1.42, 0, &TokensCostExtras::new(), 160);
        let plain = strip_ansi(&result.line);
        assert!(plain.contains("in"), "missing 'in' label: {plain}");
        assert!(plain.contains("out"), "missing 'out' label: {plain}");
        assert!(plain.contains("120.0K"), "missing in value: {plain}");
        assert!(plain.contains("3.4K"), "missing out value: {plain}");
    }

    #[test]
    fn tokens_cost_no_cache_read_parenthetical() {
        let r = Renderer::default();
        // cache-read is no longer a parameter — verify no `(NNN)` cluster
        // shows up between the icon and the rjust column.
        let result = r.tokens_cost(120_000, 3_400, 0.18, 1.42, 0, &TokensCostExtras::new(), 160);
        let plain = strip_ansi(&result.line);
        // No parenthesised digit cluster after the in icon.
        assert!(
            !plain.contains("(0)") && !plain.contains("(    0)"),
            "cache-read parenthetical leaked: {plain}",
        );
    }

    #[test]
    fn tokens_cost_consolidated_cost_cell() {
        let r = Renderer::default();
        let result = r.tokens_cost(1, 2, 0.18, 1.42, 0, &TokensCostExtras::new(), 160);
        let plain = strip_ansi(&result.line);
        assert!(plain.contains("$0.18 sess · $1.42 today"), "{plain}");
    }

    #[test]
    fn tokens_cost_hidden_cost_blanks_cluster() {
        let r = Renderer::default();
        let extras = TokensCostExtras {
            show_cost: false,
            ..Default::default()
        };
        let result = r.tokens_cost(1, 2, 0.42, 12.34, 0, &extras, 160);
        let plain = strip_ansi(&result.line);
        assert!(
            !plain.contains('$'),
            "no dollar sign when cost hidden: {plain}"
        );
        assert!(!plain.contains("0.42"), "session cost leaked: {plain}");
        assert!(!plain.contains("12.34"), "day cost leaked: {plain}");
    }

    #[test]
    fn tokens_cost_width_fills_box() {
        let r = Renderer::default();
        for box_width in [80i32, 100, 130, 160, 200] {
            let result = r.tokens_cost(120, 34, 0.1, 0.2, 0, &TokensCostExtras::new(), box_width);
            // The renderer doesn't pad to box_width — that's the layout's job
            // via border_line. But the line's visible width must equal what
            // sparkline_bar_w predicted plus the fixed clusters.
            let bar_w = sparkline_bar_w(box_width, 0.1, 0.2, 0, true);
            let predicted = box_width - 3;
            assert_eq!(
                visible_width(&result.line) as i32,
                predicted,
                "box_width={box_width} bar_w={bar_w}: line width should equal box - 3",
            );
        }
    }

    #[test]
    fn tokens_cost_sparkline_present_with_history() {
        let r = Renderer::default();
        let extras = TokensCostExtras {
            spark_history: vec![10, 20, 30, 40, 50, 60, 70, 80],
            ..TokensCostExtras::new()
        };
        let result = r.tokens_cost(1, 2, 0.01, 0.02, 1000, &extras, 160);
        let plain = strip_ansi(&result.line);
        // Sparkline glyphs come from the SPARK_* constants — they're in the
        // Symbols for Legacy Computing block.
        assert!(
            plain
                .chars()
                .any(|c| ('\u{1FB00}'..='\u{1FBFF}').contains(&c)),
            "expected sparkline glyphs: {plain}",
        );
    }

    #[test]
    fn sparkline_bar_w_matches_render() {
        let r = Renderer::default();
        for &show_cost in &[true, false] {
            for &box_width in &[80i32, 100, 130, 160, 200] {
                for &tok_rate in &[0u64, 12, 4567, 999_999] {
                    let predicted = sparkline_bar_w(box_width, 0.18, 1.42, tok_rate, show_cost);
                    let extras = TokensCostExtras {
                        show_cost,
                        ..TokensCostExtras::new()
                    };
                    let result = r.tokens_cost(120, 34, 0.18, 1.42, tok_rate, &extras, box_width);
                    let actual_line_w = visible_width(&result.line) as i32;
                    assert_eq!(
                        actual_line_w,
                        box_width - 3,
                        "show_cost={show_cost} box={box_width} tok_rate={tok_rate}: \
                         predicted bar_w={predicted}, line width mismatch",
                    );
                }
            }
        }
    }
}
