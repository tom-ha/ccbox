//! `SubagentsRow` — one row per running subagent (table layout).

use crate::layout::RowSpec;
use crate::width::visible_width;

use super::component::{Component, ComponentOutput, SeparatorPolicy};
use super::context::ComponentContext;

pub struct SubagentsRow;
pub static SUBAGENTS_ROW: SubagentsRow = SubagentsRow;

impl Component for SubagentsRow {
    fn id(&self) -> &'static str {
        "subagents-row"
    }

    fn is_visible(&self, ctx: &ComponentContext) -> bool {
        ctx.env.density.includes_subagents() && !ctx.data.running_subagents(ctx).agents.is_empty()
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        let r = ctx.renderer;
        let subagents = ctx.data.running_subagents(ctx);

        // Column width for the `type` cell, computed across all subagents
        // so the table aligns regardless of how many agents are running.
        let type_w = subagents
            .agents
            .iter()
            .map(|s| {
                let t = if s.agent_type.is_empty() {
                    "?"
                } else {
                    s.agent_type.as_str()
                };
                visible_width(t)
            })
            .max()
            .unwrap_or(1);

        let mut rows: Vec<RowSpec> = Vec::with_capacity(subagents.agents.len());
        for (idx, sub) in subagents.agents.iter().enumerate() {
            let row_text = r.subagent_row(sub, ctx.width, type_w, idx, ctx.now);
            rows.push(RowSpec::content(row_text));
        }

        ComponentOutput {
            rows,
            leading_separator: SeparatorPolicy::Dim,
            top_right_chip: String::new(),
            leading_left_chip: String::new(),
        }
    }
}
