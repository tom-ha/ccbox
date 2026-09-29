use crate::config::waiting_row_enabled;
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

fn event(kind: Kind) -> &'static str {
    match kind {
        Kind::Permission => "asked for permission",
        Kind::Question => "asked a question",
        Kind::YourTurn => "finished",
    }
}

/// Tries progressively shorter forms until one fits `available`: the full
/// list, fewer entries plus `+N more`, a count, this session's status alone,
/// the bare badge.
pub fn attention_line(
    r: &Renderer,
    own: Option<&Marker>,
    others: &[&Marker],
    now: f64,
    available: usize,
) -> String {
    let t = r.theme;
    let ago = |m: &Marker| match (now - m.since) as i64 {
        s if s < 60 => "just now".to_string(),
        s => format!("{} ago", fmt_reset(s)),
    };
    let (own_full, own_short) = match own {
        Some(m) if m.kind == Kind::YourTurn => (
            format!(
                "{}⏸ {} {}, your turn{RESET}",
                t.label,
                event(m.kind),
                ago(m)
            ),
            format!("{}⏸ your turn{RESET}", t.label),
        ),
        Some(m) => {
            let badge = format!("{}{BOLD}{REVERSE} ⏸ NEEDS YOU {RESET}", t.alert);
            (
                format!(
                    "{badge} {}{}{RESET} {}{}{RESET}",
                    t.alert,
                    event(m.kind),
                    t.label,
                    ago(m),
                ),
                badge,
            )
        }
        None => (String::new(), String::new()),
    };
    let join = |a: &str, b: &str| match (a.is_empty(), b.is_empty()) {
        (true, _) => format!(" {b}"),
        (_, true) => format!(" {a}"),
        _ => format!(" {a}   {b}"),
    };
    let fits = |line: &str| visible_width(line) <= available;

    let mut candidates = Vec::new();
    if !others.is_empty() {
        let lead = if own.is_some() {
            String::new()
        } else {
            format!("{}⏸{RESET} ", t.label)
        };
        let sep = format!(" {}·{RESET} ", t.label);
        let items: Vec<String> = others
            .iter()
            .map(|m| {
                let (clr, weight) = if m.kind == Kind::YourTurn {
                    (t.label, "")
                } else {
                    (t.warn, BOLD)
                };
                format!(
                    "{clr}{weight}{}{RESET} {}{} {}{RESET}",
                    m.name.as_deref().unwrap_or("another session"),
                    t.label,
                    event(m.kind),
                    ago(m),
                )
            })
            .collect();
        for shown in (1..=items.len()).rev() {
            let rest = items.len() - shown;
            let tail = if rest > 0 {
                format!(" {}+{rest} more{RESET}", t.label)
            } else {
                String::new()
            };
            let list = format!("{lead}{}{tail}", items[..shown].join(&sep));
            candidates.push(join(&own_full, &list));
        }
        let n = others.len();
        let count = if own.is_some() {
            format!("{}+{n} more waiting{RESET}", t.label)
        } else {
            let noun = if n == 1 { "session" } else { "sessions" };
            format!("{}⏸ {n} {noun} waiting{RESET}", t.label)
        };
        candidates.push(join(&own_full, &count));
        candidates.push(join(&own_short, &count));
    }
    if own.is_some() {
        candidates.push(join(&own_full, ""));
        candidates.push(join(&own_short, ""));
    }
    let line = candidates.into_iter().find(|c| fits(c)).unwrap_or_default();
    let pad = available.saturating_sub(visible_width(&line));
    format!("{line}{}", " ".repeat(pad))
}

impl Component for AttentionRow {
    fn id(&self) -> &'static str {
        "attention-row"
    }

    fn is_visible(&self, ctx: &ComponentContext) -> bool {
        waiting_row_enabled(ctx.env.toggles.show_waiting, ctx.env.show_waiting_override)
            && !ctx.data.waiting(ctx).is_empty()
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
        let available = (ctx.width - 3).max(0) as usize;
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
            name: Some(name.into()),
            pid: None,
            transcript_path: String::new(),
            agent_id: String::new(),
            tool_use_id: String::new(),
            tool_key: String::new(),
            scoped: false,
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
        assert_eq!(
            plain(Some(&me), &[], 80),
            "  ⏸ NEEDS YOU  asked a question 3m ago"
        );
    }

    #[test]
    fn others_listed_as_sentences() {
        let a = m("api-fix", Kind::Permission, 940.0);
        let b = m("infra", Kind::YourTurn, 280.0);
        assert_eq!(
            plain(None, &[&a, &b], 100),
            " ⏸ api-fix asked for permission 1m ago · infra finished 12m ago"
        );
    }

    #[test]
    fn unnamed_session_is_another_session() {
        let mut a = m("", Kind::YourTurn, 640.0);
        a.name = None;
        assert_eq!(plain(None, &[&a], 80), " ⏸ another session finished 6m ago");
    }

    #[test]
    fn under_a_minute_is_just_now() {
        let a = m("api-fix", Kind::Question, 990.0);
        assert_eq!(
            plain(None, &[&a], 80),
            " ⏸ api-fix asked a question just now"
        );
    }

    #[test]
    fn overflow_collapses_to_more() {
        let a = m("aaaaaaaaaaaa", Kind::Permission, 940.0);
        let b = m("bbbbbbbbbbbb", Kind::Permission, 940.0);
        let c = m("cccccccccccc", Kind::Permission, 940.0);
        let me = m("me", Kind::Question, 990.0);
        assert_eq!(
            plain(Some(&me), &[&a, &b, &c], 100),
            "  ⏸ NEEDS YOU  asked a question just now   \
             aaaaaaaaaaaa asked for permission 1m ago +2 more"
        );
    }

    #[test]
    fn narrow_widths_fall_back_to_shorter_forms() {
        let me = m("me", Kind::Permission, 940.0);
        let a = m("api-fix-with-a-long-name", Kind::Question, 940.0);
        assert_eq!(
            plain(Some(&me), &[&a], 60),
            "  ⏸ NEEDS YOU  asked for permission 1m ago   +1 more waiting"
        );
        assert_eq!(
            plain(Some(&me), &[&a], 59),
            "  ⏸ NEEDS YOU    +1 more waiting"
        );
        assert_eq!(plain(Some(&me), &[&a], 31), "  ⏸ NEEDS YOU");
        assert_eq!(plain(None, &[&a], 20), " ⏸ 1 session waiting");
        assert_eq!(plain(None, &[&a, &me], 21), " ⏸ 2 sessions waiting");
    }

    #[test]
    fn your_turn_is_quiet() {
        let r = Renderer::default();
        let me = m("me", Kind::YourTurn, 820.0);
        let s = attention_line(&r, Some(&me), &[], 1000.0, 60);
        assert!(!s.contains(REVERSE), "{s:?}");
        assert_eq!(strip_ansi(&s).trim_end(), " ⏸ finished 3m ago, your turn");

        let idle = m("infra", Kind::YourTurn, 820.0);
        let s = attention_line(&r, None, &[&idle], 1000.0, 60);
        assert!(
            !s.contains(r.theme.warn),
            "idle entries use the dim label colour"
        );
    }
}
