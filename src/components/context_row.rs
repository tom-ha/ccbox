//! `ContextRow` — the always-on `ctx` line.

use crate::consts::NARROW_WIDTH;
use crate::layout::RowSpec;

use super::component::{Component, ComponentOutput, SeparatorPolicy};
use super::context::ComponentContext;

pub struct ContextRow;
pub static CONTEXT_ROW: ContextRow = ContextRow;

impl Component for ContextRow {
    fn id(&self) -> &'static str {
        "context-row"
    }

    fn is_visible(&self, _ctx: &ComponentContext) -> bool {
        true
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        let r = ctx.renderer;
        let cw = &ctx.session.context_window;
        let line = if ctx.width < NARROW_WIDTH as i32 {
            r.context_line_compact(cw, ctx.width - 3)
        } else {
            r.context_line(cw, ctx.width - 3)
        };
        ComponentOutput {
            rows: vec![RowSpec::content(line)],
            leading_separator: SeparatorPolicy::Dim,
            top_right_chip: String::new(),
            leading_left_chip: String::new(),
        }
    }
}
