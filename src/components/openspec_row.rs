//! `OpenspecRow` — one row per active openspec change.

use crate::glyphs::{BOLD, RESET};
use crate::layout::RowSpec;

use super::component::{Component, ComponentOutput, SeparatorPolicy};
use super::context::ComponentContext;

pub struct OpenspecRow;
pub static OPENSPEC_ROW: OpenspecRow = OpenspecRow;

impl Component for OpenspecRow {
    fn id(&self) -> &'static str {
        "openspec-row"
    }

    fn is_visible(&self, ctx: &ComponentContext) -> bool {
        ctx.env.density.includes_openspec() && !ctx.data.openspec(ctx).changes.is_empty()
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        let r = ctx.renderer;
        let changes = &ctx.data.openspec(ctx).changes;
        let title_cap = (ctx.width - 45).max(10);
        let max_name = changes
            .iter()
            .map(|(n, _, _)| n.chars().count() as i32)
            .max()
            .unwrap_or(25);
        let title_w = 40.min(title_cap).min(max_name);
        let rows: Vec<RowSpec> = changes
            .iter()
            .enumerate()
            .map(|(i, (name, d, t))| {
                RowSpec::content(r.openspec_bar(name, *d as i32, *t as i32, ctx.width, title_w, i))
            })
            .collect();
        let label = r.theme.label;
        let leading_left_chip = format!(" {label}{BOLD}OpenSpec{RESET} ");
        ComponentOutput {
            rows,
            leading_separator: SeparatorPolicy::Strong,
            top_right_chip: String::new(),
            leading_left_chip,
        }
    }
}
