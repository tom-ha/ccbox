use crate::data::waiting::{Kind, Marker};
use crate::glyphs::{BOLD, RESET};
use crate::layout::RowSpec;
use crate::render::sections::tokens_cost::fmt_reset;
use crate::render::Renderer;
use crate::width::visible_width;

use super::component::{Component, ComponentOutput, SeparatorPolicy};
use super::context::ComponentContext;

const REVERSE: &str = "\x1b[7m";

pub struct AttentionRow;
pub static ATTENTION_ROW: AttentionRow = AttentionRow;

pub fn attention_line(
    r: &Renderer,
    own: Option<&Marker>,
    others: &[&Marker],
    now: f64,
    available: usize,
) -> String {
    let t = r.theme;
    let ago = |m: &Marker| fmt_reset((now - m.since) as i64);
    let mut line = String::from(" ");
    if let Some(m) = own {
        if m.kind == Kind::YourTurn {
            line.push_str(&format!("{}⏸ your turn {}{RESET}", t.label, ago(m)));
        } else {
            line.push_str(&format!(
                "{}{BOLD}{REVERSE} ⏸ NEEDS YOU {RESET} {}{}{RESET} {}{}{RESET}",
                t.alert,
                t.alert,
                m.kind.label(),
                t.label,
                ago(m),
            ));
        }
    }
    if !others.is_empty() {
        if own.is_some() {
            line.push_str("   ");
        }
        let lead = if own.is_some() {
            "also waiting:"
        } else {
            "⏸ waiting:"
        };
        line.push_str(&format!("{}{lead}{RESET}", t.label));
        for (i, m) in others.iter().enumerate() {
            let sep = if i == 0 {
                " "
            } else {
                &format!(" {}·{RESET} ", t.label)
            };
            let (clr, weight) = if m.kind == Kind::YourTurn {
                (t.label, "")
            } else {
                (t.warn, BOLD)
            };
            let item = format!(
                "{sep}{clr}{weight}{}{RESET} {}{} {}{RESET}",
                m.name,
                t.label,
                m.kind.label(),
                ago(m),
            );
            let rest = others.len() - i - 1;
            let more = if rest > 0 {
                format!(" {}+{rest} more{RESET}", t.label)
            } else {
                String::new()
            };
            if visible_width(&line) + visible_width(&item) + visible_width(&more) > available {
                line.push_str(&format!(" {}+{} more{RESET}", t.label, others.len() - i));
                break;
            }
            line.push_str(&item);
        }
    }
    let pad = available.saturating_sub(visible_width(&line));
    format!("{line}{}", " ".repeat(pad))
}

impl Component for AttentionRow {
    fn id(&self) -> &'static str {
        "attention-row"
    }

    fn is_visible(&self, ctx: &ComponentContext) -> bool {
        !ctx.data.waiting(ctx).is_empty()
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        let all = ctx.data.waiting(ctx);
        let own = all
            .iter()
            .find(|(sid, _)| *sid == ctx.session.session_id)
            .map(|(_, m)| m);
        let others: Vec<&Marker> = all
            .iter()
            .filter(|(sid, _)| *sid != ctx.session.session_id)
            .map(|(_, m)| m)
            .collect();
        let available = (ctx.width - 2).max(0) as usize;
        ComponentOutput {
            rows: vec![RowSpec::content(attention_line(
                ctx.renderer,
                own,
                &others,
                ctx.now,
                available,
            ))],
            leading_separator: SeparatorPolicy::Dim,
            top_right_chip: String::new(),
            leading_left_chip: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ansi::strip_ansi;

    fn m(name: &str, kind: Kind, since: f64) -> Marker {
        Marker {
            kind,
            since,
            name: name.into(),
            pid: None,
            transcript_path: String::new(),
        }
    }

    fn plain(own: Option<&Marker>, others: &[&Marker], w: usize) -> String {
        let s = attention_line(&Renderer::default(), own, others, 1000.0, w);
        assert_eq!(visible_width(&s), w, "{:?}", strip_ansi(&s));
        strip_ansi(&s).trim_end().to_string()
    }

    #[test]
    fn own_session_gets_badge() {
        let me = m("me", Kind::Question, 820.0);
        assert_eq!(plain(Some(&me), &[], 80), "  ⏸ NEEDS YOU  question 3m");
    }

    #[test]
    fn others_listed_with_kind_and_age() {
        let a = m("api-fix", Kind::Permission, 940.0);
        let b = m("infra", Kind::YourTurn, 280.0);
        assert_eq!(
            plain(None, &[&a, &b], 100),
            " ⏸ waiting: api-fix permission 1m · infra your turn 12m"
        );
    }

    #[test]
    fn overflow_collapses_to_more() {
        let a = m("aaaaaaaaaaaa", Kind::Permission, 940.0);
        let b = m("bbbbbbbbbbbb", Kind::Permission, 940.0);
        let c = m("cccccccccccc", Kind::Permission, 940.0);
        let me = m("me", Kind::Question, 990.0);
        let s = plain(Some(&me), &[&a, &b, &c], 80);
        assert!(
            s.contains("also waiting: aaaaaaaaaaaa permission 1m"),
            "{s}"
        );
        assert!(s.ends_with("+2 more"), "{s}");
    }

    #[test]
    fn your_turn_is_quiet() {
        let r = Renderer::default();
        let me = m("me", Kind::YourTurn, 820.0);
        let s = attention_line(&r, Some(&me), &[], 1000.0, 60);
        assert!(!s.contains(REVERSE), "{s:?}");
        assert!(strip_ansi(&s).starts_with(" ⏸ your turn 3m"), "{s:?}");

        let idle = m("infra", Kind::YourTurn, 820.0);
        let s = attention_line(&r, None, &[&idle], 1000.0, 60);
        assert!(
            !s.contains(r.theme.warn),
            "idle entries use the dim label colour"
        );
    }
}
